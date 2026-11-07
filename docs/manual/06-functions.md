# 6. Functions

[Contents](../MANUAL.md) · Previous: [5. Loops](05-loops.md) · Next: [7. Text](07-text.md)

A **function** is a piece of program with a name. You write it once and
use it as often as you like, just like the built-in commands. Functions
keep programs short, and let you think about one job at a time:
`MovePlayer()`, `DrawScore()`, `SpawnEnemy(x, y)`.

- [Writing a function](#writing-a-function)
- [Parameters](#parameters)
- [Returning a value](#returning-a-value)
- [Default values](#default-values)
- [Variables inside functions](#variables-inside-functions)
- [Functions that call themselves](#functions-that-call-themselves)
- [Objects as parameters](#objects-as-parameters)
- [Designing with functions](#designing-with-functions)

## Writing a function

```basic
Function SayHello()
    Print "Hello!"
    Print "Welcome to the game."
End Function

SayHello()
SayHello()
```

Output:

```output
Hello!
Welcome to the game.
Hello!
Welcome to the game.
```

- `Function SayHello()` starts the function, and `End Function` ends it.
- The lines in between run each time the function is **called**:
  `SayHello()`.
- A function can be written anywhere in the file, before or after the
  places that call it. Many programs put them all at the end.
- Function names follow the same rules as variable names, and can't be
  the name of a command NeptuneBASIC already has (`neptune check` tells
  you if one is).

## Parameters

**Parameters** are values you give a function when you call it. Inside,
they work like variables:

```basic
Function Welcome(name$, times)
    For i = 1 To times
        Print "Hi, " + name + "!"
    Next
End Function

Welcome("Ada", 2)
Welcome("Grace", 1)
```

Output:

```output
Hi, Ada!
Hi, Ada!
Hi, Grace!
```

Give each parameter a [type mark](02-values-and-variables.md#type-marks)
unless it is a whole number: a parameter without one is an **Int**. That
matters, because a float passed to an int parameter is rounded:

```basic
Function HalfInt(n)          ; n is an Int
    Return n / 2.0
End Function
Function Half(n#)            ; n is a Float
    Return n / 2
End Function

Print HalfInt(5.5)
Print Half(5.5)
```

Output:

```output
3.0
2.75
```

(`HalfInt(5.5)` rounds 5.5 to 6 on the way in.)

Parameters are **copies**: changing one inside the function does not
change the variable it came from.

```basic
Function AddOne(n)
    n = n + 1
    Return n
End Function

x = 5
y = AddOne(x)
Print x
Print y
```

Output:

```output
5
6
```

## Returning a value

`Return value` ends the function and gives back a value, so the function
can be used in an expression like `Len` or `Sqr`:

```basic
Function Square(n)
    Return n * n
End Function

Function Distance#(x1#, y1#, x2#, y2#)
    Return Sqr((x2 - x1) ^ 2 + (y2 - y1) ^ 2)
End Function

Print Square(7)
Print Square(3) + Square(4)
Print Distance(0, 0, 3, 4)
```

Output:

```output
49
25
5.0
```

The function's type comes from what it returns. Mark it to be sure: a
`$` after the name (`Function Shout$(text$)`) returns text, a `#` a
float; or write `As Float` after the brackets.

```basic
Function Shout$(text$)
    Return Upper(text) + "!"
End Function

Function Area(w#, h#) As Float
    Return w * h
End Function

Print Shout("watch out")
Print Area(2, 3.5)
```

Output:

```output
WATCH OUT!
7.0
```

`Return` with no value just leaves the function early:

```basic
Function ShowHealth(hp)
    If hp <= 0 Then Return       ; nothing to show
    Print "Health: " + hp
End Function

ShowHealth(0)
ShowHealth(75)
```

Output:

```output
Health: 75
```

A function that returns a value can still be called on its own line; the
value is thrown away.

## Default values

A parameter can have a default, used when the call leaves it out.
Parameters with defaults go last.

```basic
Function Hit(damage, critical = False)
    If critical Then damage = damage * 2
    Print "Hit for " + damage
End Function

Hit(10)
Hit(10, True)
```

Output:

```output
Hit for 10
Hit for 20
```

## Variables inside functions

Variables used inside a function belong to it. They start fresh every
call and vanish when it ends; they don't clash with variables of the
same name elsewhere. To use a variable from outside, it must be
`Global` (see [chapter 2](02-values-and-variables.md#local-and-global-variables)):

```basic
Global score = 0

Function AddPoints(points)
    score = score + points        ; the global score
    bonus = points / 10           ; a variable of AddPoints only
    score = score + bonus
End Function

AddPoints(50)
AddPoints(100)
Print score
```

Output:

```output
165
```

## Functions that call themselves

A function can call itself. This is called **recursion**, and suits
problems made of smaller copies of themselves:

```basic
; 5! = 5 x 4 x 3 x 2 x 1
Function Factorial(n)
    If n <= 1 Then Return 1
    Return n * Factorial(n - 1)
End Function

Print Factorial(5)
Print Factorial(10)
```

Output:

```output
120
3628800
```

Every recursive function needs a case that does not call itself
(`If n <= 1 Then Return 1`), or it never stops.

## Objects as parameters

Objects ([chapter 9](09-types.md)) can be passed to functions and
returned from them. Unlike numbers, an object is not copied: the
function works on the same object, so changes stay.

```basic
Type Enemy
    Field name$
    Field hp
End Type

Function MakeEnemy.Enemy(name$, hp)
    e.Enemy = New Enemy
    e\name = name
    e\hp = hp
    Return e
End Function

Function Hurt(e.Enemy, amount)
    e\hp = e\hp - amount
End Function

orc.Enemy = MakeEnemy("orc", 30)
Hurt(orc, 12)
Print orc\name + " has " + orc\hp + " hp"
```

Output:

```output
orc has 18 hp
```

`Function MakeEnemy.Enemy(...)` says the function returns an `Enemy`.

## Designing with functions

- **One job each.** A function called `UpdateEnemies` should update
  enemies, and not also draw the score.
- **Name them with a verb**: `LoadLevel`, `DrawMap`, `IsOnGround`.
- **Pass in what they need**, and return what they work out, rather than
  sharing lots of globals. Such functions are easy to test on their own.
- **Short is good.** When a function grows past a screenful, look for a
  part that could be a function of its own.

A typical game is a few dozen functions called from a short main loop:

```basic
Global playerX = 400, playerY = 300

Function UpdatePlayer()
    If KeyDown(KEY_LEFT) Then playerX = playerX - 4
    If KeyDown(KEY_RIGHT) Then playerX = playerX + 4
End Function

Function DrawPlayer()
    Color 80, 200, 255
    Oval playerX - 16, playerY - 16, 32, 32
End Function

Graphics 800, 600
SetBuffer BackBuffer()
While Not KeyHit(KEY_ESCAPE)
    Cls
    UpdatePlayer()
    DrawPlayer()
    Flip
Wend
```

[Contents](../MANUAL.md) · Previous: [5. Loops](05-loops.md) · Next: [7. Text](07-text.md)
