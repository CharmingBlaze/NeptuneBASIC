# 4. Making decisions

[Contents](../MANUAL.md) · Previous: [3. Expressions and operators](03-expressions.md) · Next: [5. Loops](05-loops.md)

Programs decide what to do with `If` and `Select`.

- [If on one line](#if-on-one-line)
- [If blocks](#if-blocks)
- [ElseIf](#elseif)
- [Select](#select)
- [Tips](#tips)
- [Try it yourself](#try-it-yourself)

## If on one line

```basic
hp = 0
If hp <= 0 Then Print "Game over"
If hp > 0 Then Print "Still alive" Else Print "Try again"
```

Output:

```output
Game over
Try again
```

`If condition Then statement` runs the statement only when the condition
is true. `Else` gives what to do otherwise. Several statements can follow
`Then`, separated by `:`, and all of them depend on the condition:

```basic
coins = 100
If coins >= 50 Then coins = coins - 50 : Print "Bought a sword"
Print coins
```

Output:

```output
Bought a sword
50
```

## If blocks

For more than a line or two, write the `If` on its own line and end it
with `EndIf` (or `End If`):

```basic
score = 1200
best = 1000
If score > best
    best = score
    Print "New best score!"
EndIf
Print best
```

Output:

```output
New best score!
1200
```

`Then` is optional on a block `If`. Everything between `If` and `EndIf`
runs when the condition is true. Indent the lines inside (with spaces or
a tab) so you can see at a glance what belongs to the `If`.

With `Else`:

```basic
age = 12
If age >= 18
    Print "Welcome"
Else
    Print "Sorry, this game is for grown-ups"
EndIf
```

Output:

```output
Sorry, this game is for grown-ups
```

## ElseIf

To choose between several possibilities, add `ElseIf` (or `Else If`)
parts. They are tested in order, and only the first true one runs:

```basic
temperature = 24
If temperature < 0
    Print "Freezing"
ElseIf temperature < 15
    Print "Cold"
ElseIf temperature < 25
    Print "Nice"
Else
    Print "Hot"
EndIf
```

Output:

```output
Nice
```

## Select

`Select` compares one value with several possibilities. It is easier to
read than a long `ElseIf` chain when you are checking one thing:

```basic
day = 6
Select day
    Case 1, 2, 3, 4, 5
        Print "A school day"
    Case 6, 7
        Print "The weekend!"
    Default
        Print "That's not a day"
End Select
```

Output:

```output
The weekend!
```

- `Select value` names the value to check.
- Each `Case` lists one or more values, separated by commas. The first
  `Case` that matches runs, and then the program carries on after
  `End Select`.
- `Default` runs when no `Case` matched. It is optional.

`Select` works with text too:

```basic
command = "north"
Select command
    Case "north", "n" : Print "You walk north."
    Case "south", "s" : Print "You walk south."
    Default : Print "I don't understand."
End Select
```

Output:

```output
You walk north.
```

To check ranges, use `Select True` with conditions as the cases: the
first condition that is true runs.

```basic
score = 75
Select True
    Case score >= 90 : Print "Grade A"
    Case score >= 70 : Print "Grade B"
    Case score >= 50 : Print "Grade C"
    Default : Print "Try again"
End Select
```

Output:

```output
Grade B
```

## Tips

- **Test the rarest case first** when several can be true at once: the
  first true one wins.
- **`=` compares** inside an `If`: `If x = 5` asks "is x 5?"; it does not
  change x.
- **Avoid comparing floats with `=`**: they are rarely exactly equal.
  Compare with a range: `If Abs(x - 5) < 0.01`.
- **Conditions are numbers**: `If lives` means `If lives <> 0`, and
  `If Not found` means `If found = 0`.
- **`IIf`** picks between two values in an expression
  ([chapter 3](03-expressions.md#choosing-a-value-iif)):
  `Print IIf(won, "You win", "You lose")`.

## Try it yourself

1. **The castle gate.** The guard lets you in only if your level is 5 or more
   *and* you carry the pass. Set `level = 7` and `hasPass = True`, and print
   `The gate opens.` or `The guard blocks your way.`
   *Hint: use an `If` block with `Else`, and join the two tests with `And`.*
2. **Damage report.** A hit does `damage = 18` points. Print `Miss` for 0,
   `Weak hit` for 1 to 9, `Good hit` for 10 to 19 and `Critical hit!` for 20
   or more.
   *Hint: `Select True` lets each `Case` be a condition, and the first true one wins.*
3. **Tiny adventure.** You stand in front of a locked door and have typed the
   command `"open"`, and you have `hasKey = False`. Use `Select` on the
   command: for `"open"`, say the door opens if you have the key and that it
   is locked if you do not; for `"look"` describe the room; anything else
   prints `I don't understand.`
   *Hint: you can put an `If` block inside a `Case`.*

### Answers

1.

```basic
level = 7
hasPass = True
If level >= 5 And hasPass
    Print "The gate opens."
Else
    Print "The guard blocks your way."
EndIf
```

```output
The gate opens.
```

`And` is true only when both sides are true. Level 7 is at least 5 and
`hasPass` is `True`, so the first part runs. Change either value and the
`Else` part runs instead.

2.

```basic
damage = 18
Select True
    Case damage = 0 : Print "Miss"
    Case damage < 10 : Print "Weak hit"
    Case damage < 20 : Print "Good hit"
    Default : Print "Critical hit!"
End Select
```

```output
Good hit
```

The cases are checked from the top, and the first true one runs. 18 is not 0
and not below 10, but it is below 20, so it prints `Good hit`. Anything left
over (20 or more) reaches `Default`.

3.

```basic
command = "open"
hasKey = False
Select command
    Case "open"
        If hasKey
            Print "The door swings open."
        Else
            Print "The door is locked."
        EndIf
    Case "look" : Print "You see a stone room with one wooden door."
    Default : Print "I don't understand."
End Select
```

```output
The door is locked.
```

`Select` works with text, so the `"open"` case is chosen. Inside it, an
ordinary `If` block checks the key. Try changing `hasKey` to `True`, or the
command to `"look"` or `"dance"`.

[Contents](../MANUAL.md) · Previous: [3. Expressions and operators](03-expressions.md) · Next: [5. Loops](05-loops.md)
