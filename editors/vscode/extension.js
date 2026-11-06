// NeptuneBASIC for VS Code: diagnostics from `neptune check` and run/build commands.
const vscode = require("vscode");
const { execFile } = require("child_process");
const path = require("path");

/** Parses `neptune check` output into diagnostics grouped by file. */
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
    vscode.workspace.onDidSaveTextDocument((doc) => {
      if (vscode.workspace.getConfiguration("neptunebasic").get("checkOnSave")) check(doc, collection);
    }),
    vscode.workspace.onDidOpenTextDocument((doc) => check(doc, collection)),
    vscode.commands.registerCommand("neptunebasic.check", () => {
      const ed = vscode.window.activeTextEditor;
      if (ed) check(ed.document, collection);
    }),
    vscode.commands.registerCommand("neptunebasic.run", () => runInTerminal(["run"])),
    vscode.commands.registerCommand("neptunebasic.runLow", () => runInTerminal(["run", "--quality", "low"])),
    vscode.commands.registerCommand("neptunebasic.build", () => runInTerminal(["build"]))
  );
  for (const doc of vscode.workspace.textDocuments) check(doc, collection);
}

function deactivate() {}

module.exports = { activate, deactivate, parseDiagnostics };
