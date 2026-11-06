// NeptuneBASIC for VS Code.
//
// Talks to `neptune lsp` (the NeptuneBASIC language server) for errors as you
// type, completion, hover, signature help, outline and go-to-definition. A
// tiny built-in LSP client keeps the extension free of npm dependencies.
// Also adds run (F5), run at low quality and build commands.
const vscode = require("vscode");
const { spawn, execFile } = require("child_process");
const path = require("path");

/** Parses `neptune check` output into diagnostics grouped by file (fallback when the server can't start). */
function parseDiagnostics(output, cwd) {
  const byFile = new Map();
  const re = /^(error|warning): (.*)\r?\n\s*--> (.+):(\d+):(\d+)/gm;
  let m;
  while ((m = re.exec(output)) !== null) {
    const [, level, message, file, line, col] = m;
    const full = path.isAbsolute(file) ? file : path.join(cwd, file);
    const pos = new vscode.Position(Number(line) - 1, Math.max(0, Number(col) - 1));
    const range = new vscode.Range(pos, pos.translate(0, 1));
    const sev = level === "error" ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning;
    const d = new vscode.Diagnostic(range, message, sev);
    d.source = "neptune";
    if (!byFile.has(full)) byFile.set(full, []);
    byFile.get(full).push(d);
  }
  return byFile;
}

function exe() {
  return vscode.workspace.getConfiguration("neptunebasic").get("executable") || "neptune";
}

/** Programs run from the workspace folder, so media paths resolve like `neptune run` in a terminal. */
function cwdFor(doc) {
  const ws = vscode.workspace.getWorkspaceFolder(doc.uri);
  return ws ? ws.uri.fsPath : path.dirname(doc.fileName);
}

/** Minimal JSON-RPC client for `neptune lsp` over stdio. */
class Client {
  constructor(command, onNotification) {
    this.nextId = 1;
    this.pending = new Map();
    this.buffer = Buffer.alloc(0);
    this.onNotification = onNotification;
    this.proc = spawn(command, ["lsp"], { stdio: ["pipe", "pipe", "ignore"] });
    this.ready = new Promise((resolve, reject) => {
      this.proc.on("error", reject);
      this.proc.on("spawn", resolve);
    });
    this.proc.stdout.on("data", (chunk) => this.receive(chunk));
    this.proc.on("exit", () => {
      for (const { reject } of this.pending.values()) reject(new Error("neptune lsp exited"));
      this.pending.clear();
    });
  }

  receive(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    for (;;) {
      const sep = this.buffer.indexOf("\r\n\r\n");
      if (sep < 0) return;
      const header = this.buffer.slice(0, sep).toString();
      const m = /Content-Length: *(\d+)/i.exec(header);
      const len = m ? Number(m[1]) : 0;
      if (this.buffer.length < sep + 4 + len) return;
      const body = this.buffer.slice(sep + 4, sep + 4 + len).toString("utf8");
      this.buffer = this.buffer.slice(sep + 4 + len);
      let msg;
      try {
        msg = JSON.parse(body);
      } catch {
        continue;
      }
      if (msg.id !== undefined && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(msg.error.message));
        else resolve(msg.result);
      } else if (msg.method) {
        this.onNotification(msg.method, msg.params);
      }
    }
  }

  write(msg) {
    const body = Buffer.from(JSON.stringify(Object.assign({ jsonrpc: "2.0" }, msg)), "utf8");
    this.proc.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.proc.stdin.write(body);
  }

  request(method, params) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.write({ id, method, params });
    });
  }

  notify(method, params) {
    this.write({ method, params });
  }

  stop() {
    try {
      this.request("shutdown", null).finally(() => this.notify("exit", null));
    } catch {
      this.proc.kill();
    }
  }
}

const toRange = (r) => new vscode.Range(r.start.line, r.start.character, r.end.line, r.end.character);
const docParams = (doc, pos) => ({
  textDocument: { uri: doc.uri.toString() },
  position: pos ? { line: pos.line, character: pos.character } : undefined,
});

function startServer(context, collection) {
  const client = new Client(exe(), (method, params) => {
    if (method === "textDocument/publishDiagnostics") {
      const diags = params.diagnostics.map((d) => {
        const x = new vscode.Diagnostic(
          toRange(d.range),
          d.message,
          d.severity === 1 ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning
        );
        x.source = "neptune";
        return x;
      });
      collection.set(vscode.Uri.parse(params.uri), diags);
    }
  });
  const selector = { language: "neptunebasic" };
  const timers = new Map();
  const open = (doc) => {
    if (doc.languageId !== "neptunebasic") return;
    client.notify("textDocument/didOpen", {
      textDocument: { uri: doc.uri.toString(), languageId: "neptunebasic", version: doc.version, text: doc.getText() },
    });
  };
  const kinds = vscode.CompletionItemKind;
  const completionKind = { 3: kinds.Function, 6: kinds.Variable, 7: kinds.Class, 13: kinds.Enum, 14: kinds.Keyword, 18: kinds.Reference, 21: kinds.Constant };

  return client.ready.then(() => {
    client.request("initialize", { processId: process.pid, capabilities: {} }).then(() => {
      client.notify("initialized", {});
      vscode.workspace.textDocuments.forEach(open);
    });
    context.subscriptions.push(
      { dispose: () => client.stop() },
      vscode.workspace.onDidOpenTextDocument(open),
      vscode.workspace.onDidChangeTextDocument((e) => {
        const doc = e.document;
        if (doc.languageId !== "neptunebasic") return;
        // re-check shortly after typing stops
        clearTimeout(timers.get(doc.uri.toString()));
        timers.set(
          doc.uri.toString(),
          setTimeout(() => {
            client.notify("textDocument/didChange", {
              textDocument: { uri: doc.uri.toString(), version: doc.version },
              contentChanges: [{ text: doc.getText() }],
            });
          }, 250)
        );
      }),
      vscode.workspace.onDidCloseTextDocument((doc) => {
        if (doc.languageId === "neptunebasic") client.notify("textDocument/didClose", { textDocument: { uri: doc.uri.toString() } });
      }),
      vscode.languages.registerHoverProvider(selector, {
        provideHover: (doc, pos) =>
          client.request("textDocument/hover", docParams(doc, pos)).then((r) =>
            r ? new vscode.Hover(new vscode.MarkdownString(r.contents.value)) : null
          ),
      }),
      vscode.languages.registerCompletionItemProvider(selector, {
        provideCompletionItems: (doc, pos) =>
          client.request("textDocument/completion", docParams(doc, pos)).then((items) =>
            (items || []).map((i) => {
              const c = new vscode.CompletionItem(i.label, completionKind[i.kind] ?? kinds.Text);
              c.detail = i.detail;
              return c;
            })
          ),
      }),
      vscode.languages.registerSignatureHelpProvider(
        selector,
        {
          provideSignatureHelp: (doc, pos) =>
            client.request("textDocument/signatureHelp", docParams(doc, pos)).then((r) => {
              if (!r) return null;
              const help = new vscode.SignatureHelp();
              help.signatures = r.signatures.map((s) => {
                const info = new vscode.SignatureInformation(s.label);
                info.parameters = s.parameters.map((p) => new vscode.ParameterInformation(p.label));
                return info;
              });
              help.activeSignature = r.activeSignature;
              help.activeParameter = r.activeParameter;
              return help;
            }),
        },
        "(",
        ",",
        " "
      ),
      vscode.languages.registerDocumentSymbolProvider(selector, {
        provideDocumentSymbols: (doc) =>
          client.request("textDocument/documentSymbol", docParams(doc)).then((list) =>
            (list || []).map(
              (s) => new vscode.DocumentSymbol(s.name, "", s.kind - 1, toRange(s.range), toRange(s.selectionRange))
            )
          ),
      }),
      vscode.languages.registerDefinitionProvider(selector, {
        provideDefinition: (doc, pos) =>
          client.request("textDocument/definition", docParams(doc, pos)).then((r) =>
            r ? new vscode.Location(vscode.Uri.parse(r.uri), toRange(r.range)) : null
          ),
      })
    );
  });
}

/** Fallback: `neptune check` on open/save when the language server can't start. */
function check(doc, collection) {
  if (doc.languageId !== "neptunebasic" || doc.uri.scheme !== "file") return;
  const cwd = cwdFor(doc);
  const file = path.relative(cwd, doc.fileName);
  execFile(exe(), ["check", file], { cwd }, (err, stdout, stderr) => {
    if (err && err.code === "ENOENT") {
      vscode.window.showWarningMessage("NeptuneBASIC: cannot find the neptune executable; set neptunebasic.executable.");
      return;
    }
    collection.delete(doc.uri);
    for (const [f, diags] of parseDiagnostics(stdout + stderr, cwd)) {
      collection.set(vscode.Uri.file(f), diags);
    }
  });
}

function runInTerminal(args) {
  const doc = vscode.window.activeTextEditor && vscode.window.activeTextEditor.document;
  if (!doc || doc.languageId !== "neptunebasic") return;
  doc.save().then(() => {
    const cwd = cwdFor(doc);
    const term =
      vscode.window.terminals.find((t) => t.name === "NeptuneBASIC") ||
      vscode.window.createTerminal({ name: "NeptuneBASIC", cwd });
    term.show(true);
    const quote = (s) => (/\s/.test(s) ? `"${s}"` : s);
    const file = path.relative(cwd, doc.fileName);
    const prefix = process.platform === "win32" ? "& " : "";
    term.sendText(`${prefix}${quote(exe())} ${args.concat([quote(file)]).join(" ")}`);
  });
}

function activate(context) {
  const collection = vscode.languages.createDiagnosticCollection("neptunebasic");
  context.subscriptions.push(collection);
  context.subscriptions.push(
    vscode.commands.registerCommand("neptunebasic.check", () => {
      const ed = vscode.window.activeTextEditor;
      if (ed) check(ed.document, collection);
    }),
    vscode.commands.registerCommand("neptunebasic.run", () => runInTerminal(["run"])),
    vscode.commands.registerCommand("neptunebasic.runLow", () => runInTerminal(["run", "--quality", "low"])),
    vscode.commands.registerCommand("neptunebasic.build", () => runInTerminal(["build"]))
  );
  startServer(context, collection).catch(() => {
    // older neptune builds without `lsp`, or a wrong path: check on open/save instead
    context.subscriptions.push(
      vscode.workspace.onDidSaveTextDocument((doc) => {
        if (vscode.workspace.getConfiguration("neptunebasic").get("checkOnSave")) check(doc, collection);
      }),
      vscode.workspace.onDidOpenTextDocument((doc) => check(doc, collection))
    );
    for (const doc of vscode.workspace.textDocuments) check(doc, collection);
  });
}

function deactivate() {}

module.exports = { activate, deactivate, parseDiagnostics, Client };
