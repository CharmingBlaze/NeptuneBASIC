#!/usr/bin/env python3
"""Writes a very large NeptuneBASIC program, to test the compiler on games
far bigger than any example.

    python tools/gen_big_program.py 8000 big.nb
    neptune check big.nb        (how long does it take to compile?)
    neptune run big.nb          (console program; prints how long it ran)

The first argument is the number of functions; each is about 25 lines, so
8000 functions make a 200,000-line program. The functions use what game
code uses: types and fields, globals, arrays, If/ElseIf, Select, For and
While loops, strings, and calls to each other.
"""
import random
import sys

FUNCTION = r"""Function Update{f}(a.Actor, dt#, level)
    Local dx# = a\x - {k97}, dz# = a\z + {k13}
    Local dist# = Sqr(dx * dx + dz * dz)
    If dist < {near} And a\hp > 0
        a\state = {k5}
        a\timer = a\timer + dt * speed{g}
        score{g} = score{g} + level
    ElseIf dist > 100
        a\state = 0
    Else
        For i = 0 To {loops}
            grid(i, level And 255) = grid(i, level And 255) + i * {k11}
        Next
    EndIf
    Select a\state
        Case 0 : a\x = a\x + dt
        Case 1 : a\z = a\z - dt * 2
        Case 2, 3 : a\hp = a\hp - 1
        Default : a\name = label{g} + " " + Str(level)
    End Select
    While a\timer > 10
        a\timer = a\timer - 10
        {again}
    Wend
    Return a\hp + Len(a\name) + Int(dist)
End Function
"""


def generate(n_funcs):
    random.seed(1)
    groups = max(1, n_funcs // 20)
    out = ["Type Actor", "    Field x#, y#, z#, hp, name$, state, timer#, target.Actor", "End Type"]
    for t in range(groups):
        out += [f"Type Thing{t}", "    Field a, b#, c$, d, e#", "End Type"]
        out.append(f'Global score{t}, speed{t}# = 1.5, label{t}$ = "thing {t}"')
    out += ["Dim grid(255, 255)", "Dim names$(1000)", ""]
    for f in range(n_funcs):
        again = r"a\hp = a\hp + 1"
        if f:
            again = f"If level > 0 Then Update{random.randrange(f)}(a, dt, level - 1)"
        out.append(
            FUNCTION.format(f=f, g=f % groups, k97=f % 97, k13=f % 13, k5=f % 5, k11=f % 11, near=5 + f % 7, loops=3 + f % 5, again=again)
        )
    out += ["a.Actor = New Actor", r"a\hp = 10", "total = 0", "t0 = MilliSecs()", "For frame = 1 To 20"]
    out += [f"    total = total + Update{i}(a, 0.016, 2)" for i in range(0, n_funcs, max(1, n_funcs // 200))]
    out += ["Next", 'Print "ran in " + (MilliSecs() - t0) + " ms, total " + total']
    return "\n".join(out) + "\n"


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    with open(sys.argv[2], "w", newline="\n") as file:
        file.write(generate(int(sys.argv[1])))
