"""Checks the code examples in the shader manual.

    python tools/check_doc_examples.py [--gpu] [--neptune PATH]

Every ```basic block in docs/SHADERS.md and docs/shaders/*.md is written to
a file and checked with `neptune check` (it must compile: command names,
argument counts and types).

With --gpu, every complete ```glsl and ```wgsl block (one with a fragment
stage) is also compiled on the GPU by a generated program run with
`neptune run`: shaders with LoadShader, post-processing effects (blocks
with nb_post) with LoadPostProcess.

Put <!-- nocheck --> on the line before a block to skip it (for example a
block that only shows part of a program or a shader).
"""
import glob
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FENCE = re.compile(r"^```(\w+)\s*$")


def blocks(path):
    """Yields (language, first line number, text) for each fenced block."""
    lines = open(path, encoding="utf-8").read().split("\n")
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
        if not skip:
            yield m.group(1), start + 1, "\n".join(lines[start:j]) + "\n"
        i = j + 1


def neptune_cmd(args):
    if args:
        return [args]
    exe = os.path.join(ROOT, "target", "debug", "neptune" + (".exe" if os.name == "nt" else ""))
    if os.path.exists(exe):
        return [exe]
    return ["cargo", "run", "-q", "-p", "neptune-cli", "--"]


def main():
    gpu = "--gpu" in sys.argv
    exe = None
    if "--neptune" in sys.argv:
        exe = sys.argv[sys.argv.index("--neptune") + 1]
    neptune = neptune_cmd(exe)
    files = [os.path.join(ROOT, "docs", "SHADERS.md")] + sorted(glob.glob(os.path.join(ROOT, "docs", "shaders", "*.md")))
    tmp = tempfile.mkdtemp(prefix="nbdoc")
    failures = 0
    checked = 0
    shaders = []
    for path in files:
        rel = os.path.relpath(path, ROOT).replace("\\", "/")
        for lang, line, text in blocks(path):
            if lang == "basic":
                f = os.path.join(tmp, "example%d.nb" % checked)
                with open(f, "w", encoding="utf-8") as out:
                    out.write(text)
                r = subprocess.run(neptune + ["check", f], capture_output=True, text=True, cwd=ROOT)
                checked += 1
                if r.returncode != 0:
                    failures += 1
                    msg = (r.stdout + r.stderr).strip().replace(f, "example")
                    print("FAIL %s:%d\n    %s" % (rel, line, msg.replace("\n", "\n    ")))
            elif lang in ("glsl", "wgsl") and gpu:
                if lang == "glsl" and "//! fragment" not in text and "void main" not in text:
                    continue
                if lang == "wgsl" and "@fragment" not in text:
                    continue
                shaders.append((rel, line, lang, text))
    print("%d BASIC examples checked, %d failed" % (checked, failures))
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
            failures += 1
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
