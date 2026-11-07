"""Checks the code examples in the documentation.

    python tools/check_doc_examples.py [--gpu] [--neptune PATH] [FILE ...]

Checked, unless FILEs are given:

* every ```basic block in docs/MANUAL.md, docs/manual/*.md,
  docs/LANGUAGE.md, docs/SHADERS.md and docs/shaders/*.md;
* every example in docs/examples/*.txt (the per-command examples that
  tools/gen_reference.py puts into docs/COMMANDS.md).

Each BASIC example must pass `neptune check` (command names, argument
counts and types). A ```basic block followed by an ```output block
(with only blank lines or an "Output:" line between them) is also run
with `neptune run`, and what it prints must match the output block
exactly.

With --gpu, every complete ```glsl and ```wgsl block (one with a fragment
stage) is also compiled on the GPU by a generated program run with
`neptune run`: shaders with LoadShader, post-processing effects (blocks
with nb_post) with LoadPostProcess.

Put <!-- nocheck --> on the line before a block to skip it (for example a
block that only shows part of a program or a shader).

docs/examples/*.txt holds one example per command:

    === CommandName
    ...code...
    === NextCommand
    ...code...
"""
import glob
import os
import re
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FENCE = re.compile(r"^```(\w+)\s*$")
EXAMPLE = re.compile(r"^=== (\w+)\s*$")


def blocks(path):
    """Yields (language, first line number, text, output or None) for each fenced block."""
    lines = open(path, encoding="utf-8").read().split("\n")
    found = []
    i = 0
    while i < len(lines):
        m = FENCE.match(lines[i])
        if not m:
            i += 1
            continue
        skip = i > 0 and "nocheck" in lines[i - 1]
        start = i + 1
        j = start
        while j < len(lines) and lines[j].strip() != "```":
            j += 1
        found.append([m.group(1), start + 1, "\n".join(lines[start:j]) + "\n", skip, i, j])
        i = j + 1
    for n, (lang, line, text, skip, _, end) in enumerate(found):
        if skip or lang == "output":
            continue
        output = None
        if n + 1 < len(found) and found[n + 1][0] == "output":
            between = lines[end + 1:found[n + 1][4]]
            if all(l.strip() in ("", "Output:") for l in between):
                output = found[n + 1][2]
        yield lang, line, text, output


def examples(path):
    """Yields ("basic", line, text, None) for each `=== Name` example."""
    lines = open(path, encoding="utf-8").read().split("\n")
    name, start, body = None, 0, []
    for i, l in enumerate(lines + ["=== END"]):
        m = EXAMPLE.match(l)
        if m:
            if name and any(b.strip() for b in body):
                yield "basic", start, "\n".join(body).rstrip() + "\n", None
            name, start, body = m.group(1), i + 2, []
        elif name:
            body.append(l)


def read_examples(path):
    """{name: text} for docs/examples/*.txt (used by gen_reference.py)."""
    out = {}
    lines = open(path, encoding="utf-8").read().split("\n")
    name, body = None, []
    for l in lines + ["=== END"]:
        m = EXAMPLE.match(l)
        if m:
            if name:
                out[name] = "\n".join(body).strip("\n")
            name, body = m.group(1), []
        elif name:
            body.append(l)
    return out


def neptune_cmd(args):
    if args:
        return [args]
    exe = os.path.join(ROOT, "target", "debug", "neptune" + (".exe" if os.name == "nt" else ""))
    if os.path.exists(exe):
        return [exe]
    return ["cargo", "run", "-q", "-p", "neptune-cli", "--"]


def default_files():
    d = os.path.join(ROOT, "docs")
    files = [os.path.join(d, "MANUAL.md"), os.path.join(d, "LANGUAGE.md"), os.path.join(d, "SHADERS.md")]
    files += sorted(glob.glob(os.path.join(d, "manual", "*.md")))
    files += sorted(glob.glob(os.path.join(d, "shaders", "*.md")))
    files += sorted(glob.glob(os.path.join(d, "examples", "*.txt")))
    return [f for f in files if os.path.exists(f)]


def main():
    args = sys.argv[1:]
    gpu = "--gpu" in args
    exe = None
    if "--neptune" in args:
        exe = args[args.index("--neptune") + 1]
        args.remove(exe)
    files = [os.path.abspath(a) for a in args if not a.startswith("--")] or default_files()
    neptune = neptune_cmd(exe)
    tmp = tempfile.mkdtemp(prefix="nbdoc")
    jobs = []
    shaders = []
    for path in files:
        rel = os.path.relpath(path, ROOT).replace("\\", "/")
        found = examples(path) if path.endswith(".txt") else blocks(path)
        for lang, line, text, output in found:
            if lang == "basic":
                jobs.append((rel, line, text, output))
            elif lang in ("glsl", "wgsl") and gpu:
                if lang == "glsl" and "//! fragment" not in text and "void main" not in text:
                    continue
                if lang == "wgsl" and "@fragment" not in text:
                    continue
                shaders.append((rel, line, lang, text))

    def check(job):
        n, (rel, line, text, output) = job
        f = os.path.join(tmp, "example%d.nb" % n)
        with open(f, "w", encoding="utf-8") as out:
            out.write(text)
        r = subprocess.run(neptune + ["check", f], capture_output=True, text=True, cwd=ROOT)
        if r.returncode != 0:
            msg = (r.stdout + r.stderr).strip().replace(f, "example")
            return "FAIL %s:%d\n    %s" % (rel, line, msg.replace("\n", "\n    "))
        if output is not None:
            r = subprocess.run(neptune + ["run", f], capture_output=True, text=True, cwd=ROOT, timeout=60)
            got = r.stdout.replace("\r\n", "\n").rstrip("\n")
            want = output.rstrip("\n")
            if got != want:
                return "FAIL %s:%d: output differs\n    expected:\n      %s\n    got:\n      %s" % (
                    rel, line, want.replace("\n", "\n      "), (got + r.stderr.strip()).replace("\n", "\n      "))
        return None

    with ThreadPoolExecutor(max_workers=8) as pool:
        results = list(pool.map(check, enumerate(jobs)))
    failures = [r for r in results if r]
    for r in failures:
        print(r)
    ran = sum(1 for j in jobs if j[3] is not None)
    print("%d BASIC examples checked (%d run and their output compared), %d failed" % (len(jobs), ran, len(failures)))
    bad = len(failures)
    if gpu and shaders:
        prog = ["Graphics3D 320, 240", "Global bad = 0"]
        for n, (rel, line, lang, text) in enumerate(shaders):
            f = os.path.join(tmp, "shader%d.%s" % (n, lang)).replace("\\", "/")
            with open(f, "w", encoding="utf-8") as out:
                out.write(text)
            where = "%s:%d" % (rel, line)
            if "nb_post" in text:
                prog.append('e = LoadPostProcess("%s")' % f)
                prog.append('If e = 0 Then DebugLog "FAIL %s" : bad = bad + 1' % where)
                prog.append('If e <> 0 Then s = PostProcessShader(e) : If ShaderCompiled(s) = 0 Then DebugLog "FAIL %s " + ShaderError(s) : bad = bad + 1' % where)
            else:
                prog.append('s = LoadShader("%s")' % f)
                prog.append('If s = 0 Then DebugLog "FAIL %s" : bad = bad + 1' % where)
                prog.append('If s <> 0 Then If ShaderCompiled(s) = 0 Then DebugLog "FAIL %s " + ShaderError(s) : bad = bad + 1' % where)
        prog.append('DebugLog Str(%d) + " shaders compiled, " + Str(bad) + " failed"' % len(shaders))
        f = os.path.join(tmp, "shaders.nb")
        with open(f, "w", encoding="utf-8") as out:
            out.write("\n".join(prog) + "\n")
        r = subprocess.run(neptune + ["run", f], capture_output=True, text=True, cwd=ROOT)
        out = (r.stdout + r.stderr).strip()
        print("\n".join(l for l in out.split("\n") if "FAIL" in l or "compiled" in l or "rror" in l))
        if r.returncode != 0 or "FAIL" in out:
            bad += 1
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
