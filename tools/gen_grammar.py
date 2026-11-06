"""Regenerates editors/vscode/syntaxes/neptune.tmLanguage.json.

Run from the repository root after adding commands:
    cargo build && python tools/gen_grammar.py
It reads the command list from `neptune commands`, so highlighting always
matches the runtime.
"""
import json, subprocess, sys, os

exe = os.path.join("target", "debug", "neptune.exe" if os.name == "nt" else "neptune")
out = subprocess.run([exe, "commands"], capture_output=True, text=True, check=True).stdout
commands = sorted({line.split()[0].rstrip("%#$?") for line in out.splitlines() if line.strip()}, key=str.lower)

keywords = """Dim Goto Gosub Return Exit Continue If Then Else EndIf ElseIf While Wend For To Step Next
Function EndFunction Type EndType Each Local Global Field Const Select Case Default EndSelect Repeat
Until Forever Data Read Restore Include Import New Delete First Last Insert Before After End Stop As
Try Catch EndTry Throw Method EndMethod Strict Classic Enum EndEnum In""".split()
operators = "And Or Xor Not Shl Shr Sar AndAlso OrElse Mod".split()
constants = "True False Null Pi Self".split()
types = "Int Integer Float Single String Str Bool Boolean Variant Any Object Handle Abs Sgn".split()

def words(ws):
    return r"(?i)\b(" + "|".join(sorted(set(ws), key=lambda w: (-len(w), w))) + r")\b"

grammar = {
    "$schema": "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
    "name": "NeptuneBASIC",
    "scopeName": "source.neptune",
    "fileTypes": ["nb", "bb"],
    "patterns": [
        {"include": "#comments"},
        {"include": "#strings"},
        {"include": "#numbers"},
        {"name": "keyword.control.neptune", "match": r"(?i)\b(end\s+(if|function|type|select|try|method|enum)|else\s+if)\b"},
        {"name": "keyword.control.neptune", "match": words(keywords)},
        {"name": "keyword.operator.word.neptune", "match": words(operators)},
        {"name": "constant.language.neptune", "match": words(constants)},
        {"name": "storage.type.neptune", "match": words(types)},
        {"name": "entity.name.label.neptune", "match": r"^\s*\.[A-Za-z_][A-Za-z0-9_]*"},
        {"name": "support.function.neptune", "match": words(commands)},
        {"name": "constant.other.neptune", "match": r"\b[A-Z][A-Z0-9_]*_[A-Z0-9_]+\b"},
        {"name": "keyword.operator.neptune", "match": r"\+=|-=|\*=|/=|<>|<=|>=|[-+*/^=<>\]"},
    ],
    "repository": {
        "comments": {"patterns": [
            {"name": "comment.line.semicolon.neptune", "match": ";.*$"},
            {"name": "comment.line.quote.neptune", "match": "'.*$"},
        ]},
        "strings": {"patterns": [
            {"name": "string.interpolated.neptune", "begin": r'\$"', "end": '"',
             "patterns": [{"name": "constant.character.escape.neptune", "match": r'""|\{\{|\}\}'},
                          {"name": "meta.embedded.neptune", "begin": r"\{", "end": r"\}",
                           "patterns": [{"include": "$self"}]}]},
            {"name": "string.quoted.double.neptune", "begin": '"', "end": '"',
             "patterns": [{"name": "constant.character.escape.neptune", "match": '""'}]},
        ]},
        "numbers": {"patterns": [
            {"name": "constant.numeric.hex.neptune", "match": r"\$[0-9A-Fa-f]+\b"},
            {"name": "constant.numeric.binary.neptune", "match": r"%[01]+\b"},
            {"name": "constant.numeric.neptune", "match": r"\b[0-9]+(\.[0-9]*)?([eE][-+]?[0-9]+)?\b|\.[0-9]+\b"},
        ]},
    },
}
path = os.path.join("editors", "vscode", "syntaxes", "neptune.tmLanguage.json")
with open(path, "w", encoding="utf-8") as f:
    json.dump(grammar, f, indent=2)
print(f"wrote {path} with {len(commands)} commands")
