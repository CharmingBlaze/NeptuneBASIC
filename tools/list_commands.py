"""Lists every command with its signature and the section it is in.

Reads the command tables in source order (`COMMANDS` in the VM's stdlib.rs
and the runtime's commands.rs, plus runtime `ALIASES`) and the `// ----`
section comments above the entries. Prints tab-separated lines:

    section<TAB>name<TAB>signature<TAB>alias-of

Used by gen_reference.py; run it alone to see the raw list.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCES = [
    ("crates/neptune-vm/src/stdlib.rs", "Core"),
    ("crates/neptune-runtime/src/commands.rs", "Graphics"),
]
# an entry is the signature then a closure or a function name
ENTRY = re.compile(r'^\s*\("([^"\\]*(?:\\.[^"\\]*)*)",\s*(?:\||[a-z_])')
SECTION = re.compile(r'^\s*//\s*-{2,}\s*(.+?)\s*$')
ALIAS = re.compile(r'^\s*\("(\w+)",\s*"(\w+)"\),')
NAME = re.compile(r'^[%#$?]?([A-Za-z_][A-Za-z0-9_]*)')


def commands():
    out = []
    for path, default in SOURCES:
        section = default
        in_aliases = False
        with open(os.path.join(ROOT, path), encoding="utf-8") as f:
            for line in f:
                if "pub static ALIASES" in line:
                    in_aliases = True
                    section = "Modern names (aliases)"
                    continue
                if in_aliases:
                    if line.startswith("];"):
                        in_aliases = False
                        continue
                    m = ALIAS.match(line)
                    if m:
                        out.append((section, m.group(1), "", m.group(2)))
                    continue
                m = SECTION.match(line)
                if m:
                    section = m.group(1)
                    continue
                m = ENTRY.match(line)
                if m:
                    sig = m.group(1).replace('\\"', '"')
                    n = NAME.match(sig)
                    if n:
                        out.append((section, n.group(1), sig, ""))
    return out


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    for row in commands():
        sys.stdout.write("\t".join(row) + "\n")
