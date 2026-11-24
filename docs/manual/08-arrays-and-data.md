# 8. Arrays and data

[Contents](../MANUAL.md) · Previous: [7. Text](07-text.md) · Next: [9. Types and objects](09-types.md)

An **array** is a numbered row of values under one name: ten high
scores, the tiles of a map, the cards in a hand. Instead of `score1`,
`score2`, `score3`, you write `scores(1)`, `scores(2)`, `scores(i)`.

- [Dim arrays](#dim-arrays)
- [Grids: arrays with two or more sizes](#grids-arrays-with-two-or-more-sizes)
- [Fixed arrays](#fixed-arrays)
- [Going through an array](#going-through-an-array)
- [Mistakes with arrays](#mistakes-with-arrays)
- [Data lines](#data-lines)
- [Try it yourself](#try-it-yourself)

## Dim arrays

`Dim name(size)` makes an array with places numbered 0 to `size`:

```basic
Dim scores(4)              ; scores(0) to scores(4): five places
scores(0) = 120
scores(1) = 95
scores(4) = 300
Print scores(0) + scores(1)
Print scores(2)            ; never set: 0
```

Output:

```output
215
0
```

- Every place starts as 0 (or `""`).
- The type comes from the name, as for variables: `Dim names$(10)` holds
  strings, `Dim speeds#(10)` floats. Without a mark, an array holds
  whole numbers.
- `Dim` arrays can be used everywhere in the program, including inside
  functions.
- The size can be worked out while the program runs: `Dim map(width,
  height)`.
- `Dim` again with the same name makes a new array: the old values are
  gone. Use it to resize or clear an array.

```basic
Dim names$(2)
names(0) = "Ada"
names(1) = "Grace"
names(2) = "Alan"
For i = 0 To 2
    Print i + ": " + names(i)
Next
```

Output:

```output
0: Ada
1: Grace
2: Alan
```

## Grids: arrays with two or more sizes

An array can have two (or more) sizes, separated by commas. A
two-size array is a grid, perfect for boards and tile maps:

```basic
Const WIDTH = 4, HEIGHT = 2
Dim board(WIDTH, HEIGHT)
board(1, 1) = 1                ; a wall
board(3, 0) = 2                ; treasure
For y = 0 To HEIGHT
    row$ = ""
    For x = 0 To WIDTH
        Select board(x, y)
            Case 0 : row = row + "."
            Case 1 : row = row + "#"
            Case 2 : row = row + "$"
        End Select
    Next
    Print row
Next
```

Output:

```output
...$.
.#...
.....
```

## Fixed arrays

Square brackets make a **fixed array**: its size is part of its type and
never changes. Fixed arrays can be local to a function, and can be fields
of a [type](09-types.md).

```basic
Local v#[2]                    ; v[0] to v[2], floats
v[0] = 1.5
v[2] = 4.0
Print v[0] + v[2]
```

Output:

```output
5.5
```

| | `Dim a(n)` | `Local a[n]` |
|---|---|---|
| brackets | round | square |
| size | can be worked out at run time, and changed with `Dim` again | fixed, written as a number |
| where it lives | the whole program | the function it is in (or a type's field) |
| sizes | any number (`Dim a(10, 10, 4)`) | one |

## Going through an array

A `For` loop is the usual way to go through an array:

```basic
Dim temps#(6)
temps(0) = 12.5 : temps(1) = 14.0 : temps(2) = 13.0 : temps(3) = 17.5
temps(4) = 19.0 : temps(5) = 18.5 : temps(6) = 15.0

warmest# = temps(0)
total# = 0
For day = 0 To 6
    total = total + temps(day)
    If temps(day) > warmest Then warmest = temps(day)
Next
Print "Warmest: " + warmest
Print "Average: " + FormatFloat(total / 7, 1)
```

Output:

```output
Warmest: 19.0
Average: 15.6
```

For a fixed array, `For Each` gives each value in turn:

```basic
Local hand$[2]
hand[0] = "ace" : hand[1] = "king" : hand[2] = "seven"
For Each card In hand
    Print card
Next
```

Output:

```output
ace
king
seven
```

## Mistakes with arrays

**Using a place that does not exist** stops the program with an error
that says which number was wrong:

```basic
Dim scores(9)
Try
    scores(20) = 1
Catch problem$
    Print problem
End Try
```

Output:

```output
array index out of bounds (20 not in 0..9)
```

(`Try` and `Catch` are explained in [chapter 10](10-errors.md).)

**Off by one.** `Dim a(10)` has eleven places, 0 to 10. Many programs
use places 1 to 10 and ignore 0; that is fine, as long as you are
consistent.

**Text in a number array.** `Dim names(5)` (no `$`) holds numbers, and a
string stored in it becomes 0. Write `Dim names$(5)`.

## Data lines

`Data` lines store values in the program itself: level layouts,
enemy stats, dialogue. `Read` takes the next value, and `Restore label`
says where to start reading.

```basic
Restore enemies
For i = 1 To 3
    Read name$, hp, speed#
    Print name + ": " + hp + " hp, speed " + speed
Next
End

.enemies
Data "orc", 30, 1.5
Data "bat", 5, 4.0
Data "troll", 80, 0.75
```

Output:

```output
orc: 30 hp, speed 1.5
bat: 5 hp, speed 4.0
troll: 80 hp, speed 0.75
```

- A **label** is a name starting with `.` on a line of its own:
  `.enemies`.
- `Read` reads values in order, across `Data` lines, into variables of
  matching types.
- `End` stops the program before it reaches the data. (Data lines
  don't run, but anything else after them would.)

A level drawn with `Data`:

```basic
Dim level(5, 2)
Restore map
For y = 0 To 2
    Read row$
    For x = 0 To 5
        If Mid(row, x + 1, 1) = "#" Then level(x, y) = 1
    Next
Next
Print level(0, 0) + level(1, 1)
End

.map
Data "######"
Data "#....#"
Data "######"
```

Output:

```output
1
```

## Try it yourself

1. **High scores.** Keep five scores (120, 95, 300, 210, 150) in an array.
   Print the best score and its place number (the first place is number 1).
   *Hint: remember the best so far, and the place where you found it, while a `For` loop goes through the array.*
2. **Dungeon map.** Make a grid 10 across and 5 down, where `#` marks the
   edge walls and `.` is floor. Put an `@` for the player at column 2, row 1,
   and print the map.
   *Hint: a cell is on the edge when its x is 0 or 9, or its y is 0 or 4.*
3. **Shop list.** Store three items and their prices in `Data` lines:
   sword 50, shield 35 and potion 12. Read them, print them in lined-up
   columns, and print the total price.
   *Hint: `LSet` and `RSet` line up the columns, and `Read` can fill several variables at once.*

### Answers

1.

```basic
Dim scores(4)
scores(0) = 120
scores(1) = 95
scores(2) = 300
scores(3) = 210
scores(4) = 150

best = scores(0)
place = 1
For i = 1 To 4
    If scores(i) > best
        best = scores(i)
        place = i + 1
    EndIf
Next
Print "Best score: " + best + " (place " + place + ")"
```

```output
Best score: 300 (place 3)
```

The loop starts with the first score as the best so far and then looks at the
others. When one is bigger, it becomes the new best. Arrays count from 0, so
the place number is the index plus one.

2.

```basic
Const W = 10, H = 5
Dim map$(W - 1, H - 1)

For y = 0 To H - 1
    For x = 0 To W - 1
        If x = 0 Or x = W - 1 Or y = 0 Or y = H - 1
            map(x, y) = "#"
        Else
            map(x, y) = "."
        EndIf
    Next
Next
map(2, 1) = "@"

For y = 0 To H - 1
    row$ = ""
    For x = 0 To W - 1
        row = row + map(x, y)
    Next
    Print row
Next
```

```output
##########
#.@......#
#........#
#........#
##########
```

`Dim` with a `$` makes a grid of text, one character per cell. The first pair
of loops fills it, using `Or` to catch the four edges, and `map(2, 1)` is then
changed to the player. The second pair of loops joins each row into one line
and prints it.

3.

```basic
Restore items
total = 0
For i = 1 To 3
    Read name$, price
    Print LSet(name, 10) + RSet(price, 4)
    total = total + price
Next
Print String("-", 14)
Print LSet("Total", 10) + RSet(total, 4)
End

.items
Data "sword", 50
Data "shield", 35
Data "potion", 12
```

```output
sword       50
shield      35
potion      12
--------------
Total       97
```

`Read` takes the next two values from the `Data` lines each time round the
loop, a name into `name$` and a number into `price`. `End` stops the program
before it runs into the data lines. `LSet` and `RSet` pad the text so the
names and prices line up.

[Contents](../MANUAL.md) · Previous: [7. Text](07-text.md) · Next: [9. Types and objects](09-types.md)
