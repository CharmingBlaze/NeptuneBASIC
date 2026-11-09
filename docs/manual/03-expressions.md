# 3. Expressions and operators

[Contents](../MANUAL.md) · Previous: [2. Values and variables](02-values-and-variables.md) · Next: [4. Making decisions](04-decisions.md)

An **expression** is anything that works out to a value: `2 + 3`,
`score * 10`, `x > 0`, `Len(name)`. This chapter covers the operators
that combine values, and the maths commands.

- [Arithmetic](#arithmetic)
- [Order of operations](#order-of-operations)
- [Changing a variable](#changing-a-variable)
- [Comparisons](#comparisons)
- [Logic: And, Or, Not](#logic-and-or-not)
- [Choosing a value: IIf](#choosing-a-value-iif)
- [Maths commands](#maths-commands)
- [Random numbers](#random-numbers)
- [Bits](#bits)
- [Try it yourself](#try-it-yourself)

## Arithmetic

| Operator | Means | Example | Gives |
|---|---|---|---|
| `+` | add | `7 + 2` | `9` |
| `-` | subtract | `7 - 2` | `5` |
| `*` | multiply | `7 * 2` | `14` |
| `/` | divide | `7 / 2` | `3` (two ints: whole number) |
| | | `7 / 2.0` | `3.5` |
| `Mod` | remainder after dividing | `17 Mod 5` | `2` |
| `^` | to the power of | `2 ^ 10` | `1024.0` |
| `-` (in front) | negative | `-x` | |

When both sides of `/` are ints, the answer is an int with the fraction
dropped. Make either side a float (`7 / 2.0`, or `Float(a) / b`) to keep
it. `^` always gives a float.

`Mod` is useful for things that wrap round: `frame = (frame + 1) Mod 8`
counts 0, 1, ... 7, 0, 1, ...; `If n Mod 2 = 0` tests for an even number.

```basic
seconds = 125
Print seconds / 60 + " minutes and " + seconds Mod 60 + " seconds"
Print 7 / 2
Print 7 / 2.0
Print 2 ^ 10
```

Output:

```output
2 minutes and 5 seconds
3
3.5
1024.0
```

## Order of operations

As in school maths, `*` and `/` happen before `+` and `-`; brackets
change the order:

```basic
Print 2 + 3 * 4
Print (2 + 3) * 4
Print 10 - 2 - 3          ; left to right: (10 - 2) - 3
```

Output:

```output
14
20
5
```

From first to last:

| First | Operators |
|---|---|
| 1 | brackets `( )`, and commands like `Abs(x)` |
| 2 | `-` in front of a value, `~` (bitwise not) |
| 3 | `^` |
| 4 | `*`, `/`, `Mod` |
| 5 | `Shl`, `Shr`, `Sar` |
| 6 | `+`, `-` |
| 7 | comparisons: `=`, `<>`, `<`, `>`, `<=`, `>=` |
| 8 | `And`, `Or`, `Xor` |
| 9 | `Not` |
| 10 | `AndAlso`, `OrElse` |

When in doubt, add brackets: they cost nothing and make the meaning
clear to readers too.

## Changing a variable

`+=`, `-=`, `*=` and `/=` change a variable by a value:

```basic
score = 10
score += 5            ; the same as score = score + 5
score *= 2
score -= 1
Print score
```

Output:

```output
29
```

## Comparisons

| Operator | True when |
|---|---|
| `=` | equal |
| `<>` | not equal |
| `<`, `>` | less than, greater than |
| `<=`, `>=` | less or equal, greater or equal |

A comparison gives 1 (true) or 0 (false). Text is compared letter by
letter, so `"apple" < "banana"`, and capitals matter: `"abc" = "ABC"` is
false. (Use `Lower` on both to compare without capitals.)

```basic
age = 15
Print age >= 18
Print "apple" < "banana"
Print "abc" = "ABC"
Print Lower("abc") = Lower("ABC")
```

Output:

```output
0
1
0
1
```

`=` means "compare" inside an expression and "store" at the start of a
statement: `x = y = 3` stores 1 in `x` if `y` is 3, else 0.

## Logic: And, Or, Not

Combine conditions with:

| Operator | True when |
|---|---|
| `a And b` | both are true |
| `a Or b` | either is true |
| `a Xor b` | exactly one is true |
| `Not a` | a is false |
| `a AndAlso b` | both are true; `b` is not worked out if `a` is false |
| `a OrElse b` | either is true; `b` is not worked out if `a` is true |

```basic
hp = 30 : hasShield = True
Print hp > 0 And hasShield
Print hp > 50 Or hasShield
Print Not hasShield
```

Output:

```output
1
1
0
```

Two things to know:

- `And`, `Or` and `Xor` work on the **bits** of numbers. With true/false
  values (1 and 0, which comparisons give) that is exactly what you
  want. With other numbers it can surprise you: `1 And 2` is 0, because
  1 and 2 share no bits. `AndAlso` and `OrElse` treat any non-zero
  number as true: `1 AndAlso 2` is 1.
- `AndAlso` and `OrElse` stop early, which protects the second test:
  `If enemy <> Null AndAlso enemy\hp > 0` never looks at `enemy\hp` when
  there is no enemy.

`Not` comes after everything else, so `Not a = b` means `Not (a = b)`.

## Choosing a value: IIf

`IIf(condition, a, b)` gives `a` when the condition is true and `b`
when it is false. Only the chosen one is worked out.

```basic
lives = 1
Print "You have " + lives + IIf(lives = 1, " life", " lives")
```

Output:

```output
You have 1 life
```

## Maths commands

Angles are in **degrees** (a full turn is 360).

| Command | Gives |
|---|---|
| `Abs(x)` | x without its sign: `Abs(-3)` is 3 |
| `Sgn(x)` | -1, 0 or 1: the sign of x |
| `Sqr(x)` | the square root |
| `Sin(a)`, `Cos(a)`, `Tan(a)` | sine, cosine, tangent of an angle |
| `ASin(x)`, `ACos(x)`, `ATan(x)` | the angle for a sine, cosine or tangent |
| `ATan2(y, x)` | the angle of the point (x, y) from the origin, -180 to 180 |
| `Exp(x)`, `Log(x)`, `Log10(x)` | e to the power x, natural log, base-10 log |
| `Floor(x)`, `Ceil(x)` | rounded down, rounded up |
| `Int(x)` | rounded to the nearest whole number |
| `Min(a, b)`, `Max(a, b)` | the smaller, the larger (always a float: `Print Min(3, 5)` shows `3.0`) |
| `Clamp(x, low, high)` | x kept between low and high |
| `Lerp(a, b, t)` | the value a fraction t of the way from a to b |
| `Pi` | 3.141593 |

```basic
; the distance between two points, and the angle from one to the other
x1 = 0 : y1 = 0
x2 = 3 : y2 = 4
Print Sqr((x2 - x1) ^ 2 + (y2 - y1) ^ 2)
Print ATan2(1, 1)
Print Clamp(150, 0, 100)
Print Lerp(0, 10, 0.25)
```

Output:

```output
5.0
45.0
100.0
2.5
```

`Sin` and `Cos` are how games move things in circles and waves:

```basic
; a point on a circle of radius 10, a quarter of the way round
angle = 90
Print 10 * Cos(angle)
Print 10 * Sin(angle)
```

Output:

```output
-4.371139e-7
10.0
```

The first answer should be 0, and it is very nearly: -4.371139e-7 means
-0.0000004371139. Floats hold about 7 digits, so maths with them is
almost always a tiny bit off. It never matters on screen, but it means
you should not test floats with `=`: write `If Abs(a - b) < 0.001`
instead of `If a = b`.

## Random numbers

| Command | Gives |
|---|---|
| `Rand(low, high)` | a random whole number from low to high, both included |
| `Rand(high)` | a random whole number from 1 to high |
| `Rnd(low, high)` | a random float from low to high |
| `Rnd(high)` | a random float from 0 to high |
| `SeedRnd seed` | starts the random numbers from `seed`: the same seed gives the same numbers every run |

```basic
SeedRnd MilliSecs()             ; different numbers each time the program runs
dice = Rand(1, 6)
Print "You rolled " + dice
If Rnd(1) < 0.1 Then Print "A rare event (10% chance)!"
```

A fixed seed (`SeedRnd 1234`) is useful for testing, or for a level
generator that must make the same level every time.

## Bits

Ints are 32 bits. These work on the bits directly, for flags and packed
values:

| Operator | Does |
|---|---|
| `a And b`, `a Or b`, `a Xor b` | bitwise and, or, exclusive or |
| `~a` | flips every bit |
| `a Shl n` | shifts left by n bits (multiplies by 2 to the n) |
| `a Shr n` | shifts right, filling with 0 |
| `a Sar n` | shifts right, keeping the sign (divides by 2 to the n) |

```basic
Const FLAG_FIRE = 1, FLAG_ICE = 2, FLAG_POISON = 4
effects = FLAG_FIRE Or FLAG_POISON
If effects And FLAG_POISON Then Print "poisoned"
If (effects And FLAG_ICE) = 0 Then Print "not frozen"
Print 1 Shl 4
Print Hex(255) + " " + Bin(5)
```

Output:

```output
poisoned
not frozen
16
000000FF 00000000000000000000000000000101
```

Numbers can be written in hexadecimal with `$` (`$FF` is 255) and in
binary with `%` (`%101` is 5).

## Try it yourself

1. **Race clock.** Your best lap took 3725 seconds in a very long race. Print
   it as hours, minutes and seconds, like `1 h 2 m 5 s`.
   *Hint: `/` on two ints drops the fraction, and `Mod` gives what is left over.*
2. **Treasure radar.** The player is at (1, 1) and the treasure is at (7, 9).
   Print the distance between them, and then `Treasure is close!` if the
   distance is 12 or less, or `Treasure is far away.` if it is more.
   *Hint: use `Sqr` for the distance and `IIf` to choose between the two messages.*
3. **Inventory flags.** Use one number to remember which items you carry:
   `SWORD` is 1, `SHIELD` is 2 and `KEY` is 4. Start with the sword and the
   key, print whether you have the shield, pick up the shield, drop the
   sword, and print the number that holds your items now.
   *Hint: `Or` switches a flag on, and `And ~flag` switches it off.*

### Answers

1.

```basic
total = 3725
hours = total / 3600
minutes = total Mod 3600 / 60
seconds = total Mod 60
Print hours + " h " + minutes + " m " + seconds + " s"
```

```output
1 h 2 m 5 s
```

Dividing two ints gives a whole number, so `3725 / 3600` is 1 hour.
`Mod` finds what is left over: 125 seconds after the hour, which is 2
minutes with 5 seconds remaining. (`Mod` and `/` have the same priority, so
`total Mod 3600 / 60` takes the remainder first, then divides.)

2.

```basic
dx = 7 - 1
dy = 9 - 1
distance# = Sqr(dx ^ 2 + dy ^ 2)
Print "Distance: " + distance
Print IIf(distance <= 12, "Treasure is close!", "Treasure is far away.")
```

```output
Distance: 10.0
Treasure is close!
```

The two sides of the triangle are 6 and 8, so the distance is 10. The
condition is true, so `IIf` gives the first message.

3.

```basic
Const SWORD = 1, SHIELD = 2, KEY = 4
items = SWORD Or KEY
Print "Has shield: " + ((items And SHIELD) <> 0)
items = items Or SHIELD
items = items And ~SWORD
Print "Has shield: " + ((items And SHIELD) <> 0)
Print "Items number: " + items
```

```output
Has shield: 0
Has shield: 1
Items number: 6
```

Each item is its own bit, so `Or` can switch one on without touching the
others. `~SWORD` has every bit set except the sword's, so `And ~SWORD`
switches only the sword off. That leaves the shield (2) and the key (4), 6.

[Contents](../MANUAL.md) · Previous: [2. Values and variables](02-values-and-variables.md) · Next: [4. Making decisions](04-decisions.md)
