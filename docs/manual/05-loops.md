# 5. Loops

[Contents](../MANUAL.md) · Previous: [4. Making decisions](04-decisions.md) · Next: [6. Functions](06-functions.md)

A loop runs the same lines again and again: once for each enemy, once
for each frame of a game, until the player quits.

- [For: counting](#for-counting)
- [While: as long as something is true](#while-as-long-as-something-is-true)
- [Repeat: until something is true](#repeat-until-something-is-true)
- [For Each: every item](#for-each-every-item)
- [Leaving a loop early](#leaving-a-loop-early)
- [Loops inside loops](#loops-inside-loops)
- [Which loop?](#which-loop)

## For: counting

```basic
For i = 1 To 5
    Print "Lap " + i
Next
```

Output:

```output
Lap 1
Lap 2
Lap 3
Lap 4
Lap 5
```

`For i = 1 To 5` sets `i` to 1, runs the lines up to `Next`, adds 1 to
`i`, and repeats while `i` is 5 or less. The variable `i` is the **loop
counter**; any name will do.

`Step` changes how much is added each time. It can be negative, to count
down, or a fraction, with a float counter:

```basic
For countdown = 10 To 0 Step -5
    Print countdown
Next
For t# = 0 To 1 Step 0.5
    Print t
Next
```

Output:

```output
10
5
0
0.0
0.5
1.0
```

If the start is already past the end (`For i = 5 To 1` without a
negative step), the loop runs no times at all.

A `For` loop adds up a total:

```basic
total = 0
For n = 1 To 100
    total = total + n
Next
Print "1 + 2 + ... + 100 = " + total
```

Output:

```output
1 + 2 + ... + 100 = 5050
```

## While: as long as something is true

`While condition ... Wend` repeats as long as the condition is true. The
condition is tested before each time round, so if it is false at the
start the loop never runs.

```basic
money = 100
years = 0
While money < 200
    money = money * 1.1      ; 10% interest a year
    years = years + 1
Wend
Print "Doubled after " + years + " years"
```

Output:

```output
Doubled after 8 years
```

Every game has a `While` loop at its heart:

```basic
Graphics 800, 600
While Not KeyHit(KEY_ESCAPE)     ; until the player presses Escape
    Cls
    Text 10, 10, "Press Escape to quit"
    Flip
Wend
```

## Repeat: until something is true

`Repeat ... Until condition` runs the lines first and then tests, so it
always runs at least once. It stops when the condition becomes true.

```basic
tries = 0
Repeat
    roll = Rand(1, 6)
    tries = tries + 1
Until roll = 6
Print "Rolled a six after " + tries + " tries"
```

`Repeat ... Forever` never stops by itself: leave it with `Exit` (below).

## For Each: every item

`For Each` goes through every item of an array, or every object of a
type ([chapter 9](09-types.md)):

```basic
Local fruit$[2]
fruit[0] = "apple" : fruit[1] = "pear" : fruit[2] = "plum"
For Each f In fruit
    Print f
Next
```

Output:

```output
apple
pear
plum
```

```basic
Type Enemy
    Field name$
End Type
e.Enemy = New Enemy : e\name = "orc"
e.Enemy = New Enemy : e\name = "bat"
For Each e In Enemy
    Print e\name
Next
```

Output:

```output
orc
bat
```

## Leaving a loop early

`Exit` leaves the loop at once; the program carries on after its `Next`,
`Wend` or `Until`. `Continue` skips the rest of this time round and goes
on with the next.

```basic
For i = 1 To 10
    If i Mod 2 = 0 Then Continue     ; skip even numbers
    If i > 7 Then Exit               ; stop after 7
    Print i
Next
```

Output:

```output
1
3
5
7
```

```basic
; keep asking until the answer is right
Repeat
    answer = Input("What is 6 x 7? ")
    If Int(answer) = 42 Then Exit
    Print "Not quite, try again."
Forever
Print "Correct!"
```

## Loops inside loops

A loop can contain another loop. The inner loop runs completely each time
the outer one goes round once:

```basic
For row = 1 To 3
    line$ = ""
    For col = 1 To 4
        line = line + (row * col) + " "
    Next
    Print line
Next
```

Output:

```output
1 2 3 4 
2 4 6 8 
3 6 9 12 
```

`Exit` and `Continue` act on the innermost loop they are in.

## Which loop?

| You know... | Use |
|---|---|
| how many times | `For` |
| a condition to keep going | `While` |
| a condition to stop, and it must run at least once | `Repeat ... Until` |
| you want every item of an array or type | `For Each` |
| it should run until something inside decides | `Repeat ... Forever` with `Exit` |

A loop that never ends freezes the program. In a game loop, make sure
something (a key, the window closing) can end it, and that each time
round calls `Flip`, which also gives the window a chance to respond.

[Contents](../MANUAL.md) · Previous: [4. Making decisions](04-decisions.md) · Next: [6. Functions](06-functions.md)
