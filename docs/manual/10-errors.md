# 10. Errors and debugging

[Contents](../MANUAL.md) · Previous: [9. Types and objects](09-types.md) · Next: [11. Organising programs](11-organising.md)

Everyone's programs have mistakes. This chapter is about finding them and
dealing with things that go wrong while a program runs.

- [Mistakes found before the program runs](#mistakes-found-before-the-program-runs)
- [Errors while the program runs](#errors-while-the-program-runs)
- [Catching errors: Try and Catch](#catching-errors-try-and-catch)
- [Raising your own errors](#raising-your-own-errors)
- [Finding bugs](#finding-bugs)
- [Common mistakes](#common-mistakes)

## Mistakes found before the program runs

Before a program runs, NeptuneBASIC reads all of it and reports anything
it can't make sense of: a misspelled command, a missing `EndIf`, a
string where a number should be. Nothing runs until they are fixed.

```
error: not enough parameters for 'SetAlbedo' (expects 4 to 5)
 --> game.nb:12:1
   |
12 | SetAlbedo gold, 255
   | ^^^^^^^^^^^^^^^^^^^
```

Each message has the file, line and column, the line itself, and a
marker under the problem. Start with the **first** error: later ones are
often caused by it.

**Warnings** don't stop the program, but point at things that are
probably mistakes:

```
warning: variable 'scroe' is used but never assigned
 --> game.nb:30:9
```

(Here, a typo: `scroe` instead of `score`.) Read warnings; they are
usually right.

`neptune check game.nb` checks a program without running it. The VS Code
extension shows the same errors and warnings as you type.

## Errors while the program runs

Some problems can only be found while the program runs: dividing by
zero, using an array place that doesn't exist, using an object that has
been deleted, loading a file that isn't there. The program stops and
says what happened and where:

```basic
Function Divide(a, b)
    Return a / b
End Function

Print Divide(10, 2)
Print Divide(1, 0)
```

prints `5`, then stops with:

```
runtime error: Integer divide by zero
  at divide.nb:2:5 (in divide)
```

The last line says where it happened: line 2, column 5, inside the
function `Divide`. In a graphics program the message is also shown in a
message box, so it isn't lost when the window closes.

## Catching errors: Try and Catch

`Try ... Catch ... End Try` lets a program deal with an error itself
instead of stopping. If anything between `Try` and `Catch` causes an
error, the program jumps to `Catch`, with the message in the variable
named there:

```basic
scores = 0
Try
    Print 100 / scores
    Print "this line is skipped"
Catch problem$
    Print "Something went wrong: " + problem
End Try
Print "...but the program carries on"
```

Output:

```output
Something went wrong: Integer divide by zero
...but the program carries on
```

The error can happen in a function called from inside `Try`, however
deep: it still lands in the `Catch`. Use `Try` around things that can
fail for reasons outside your program, such as files and network
connections, and report the problem to the player in a friendly way.

## Raising your own errors

`Throw message` raises an error of your own. It can be caught by a
`Try` like any other:

```basic
Function LoadLevel(number)
    If number < 1 Or number > 10 Then Throw "there is no level " + number
    Print "Loading level " + number
End Function

Try
    LoadLevel(3)
    LoadLevel(42)
Catch why$
    Print "Could not load: " + why
End Try
```

Output:

```output
Loading level 3
Could not load: there is no level 42
```

`RuntimeError message` stops the program with your message, like a
runtime error, for problems the game can't recover from:

```basic
level = LoadMesh("levels/castle.glb")
If level = 0 Then RuntimeError "The castle level is missing. Please reinstall the game."
```

## Finding bugs

A **bug** is when the program runs but does the wrong thing. Some ways to
find one:

**Print what is happening.** `Print` (or `DebugLog`, which writes to the
console even in a graphics program) shows values while the program runs:

```basic
For i = 1 To 3
    damage = i * 7 Mod 5
    DebugLog "round " + i + ": damage " + damage
Next
```

**Shrink the problem.** Remove or comment out (put `;` in front of)
parts of the program until the bug disappears; the last part you removed
contains it.

**Check your assumptions.** Most bugs are a value that isn't what you
think it is. Print it and see.

**Use `Strict`.** At the top of a program, `Strict` makes every variable
need a declaration, which catches misspelled names
([chapter 2](02-values-and-variables.md#strict-mode)).

**Read the error's position.** A runtime error says which line and which
function; start there and work back to where the bad value came from.

## Common mistakes

| Symptom | Likely cause |
|---|---|
| a variable is always 0 | a misspelled name (look for the "never assigned" warning), or a main-program variable used in a function without `Global` |
| a division gives a whole number | both sides are ints: write `a / 2.0` or `Float(a) / b` |
| a float parameter loses its fraction | the parameter has no `#`: write `Function Move(speed#)` |
| text in an array comes back as 0 | the array has no `$`: write `Dim names$(10)` |
| `If x = 0.3` is never true | floats are approximate: `If Abs(x - 0.3) < 0.001` |
| a loop never ends | the condition never changes inside the loop, or the game loop is missing `Flip` |
| "Object does not exist" | a field of `Null`, or of an object that was deleted: test `If e <> Null` first |
| "array index out of bounds" | a position below 0 or past the size given to `Dim` |
| "function ... conflicts with a built-in command" | your function has the same name as a command: rename it |
| "variable ... is Int, but used with a Float type tag" | one variable used for two types: give the second use its own name |

[Contents](../MANUAL.md) · Previous: [9. Types and objects](09-types.md) · Next: [11. Organising programs](11-organising.md)
