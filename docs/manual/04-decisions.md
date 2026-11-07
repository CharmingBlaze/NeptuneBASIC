# 4. Making decisions

[Contents](../MANUAL.md) · Previous: [3. Expressions and operators](03-expressions.md) · Next: [5. Loops](05-loops.md)

Programs decide what to do with `If` and `Select`.

- [If on one line](#if-on-one-line)
- [If blocks](#if-blocks)
- [ElseIf](#elseif)
- [Select](#select)
- [Tips](#tips)

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

[Contents](../MANUAL.md) · Previous: [3. Expressions and operators](03-expressions.md) · Next: [5. Loops](05-loops.md)
