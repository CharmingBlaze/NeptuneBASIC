# 7. Text

[Contents](../MANUAL.md) · Previous: [6. Functions](06-functions.md) · Next: [8. Arrays and data](08-arrays-and-data.md)

Text values are called **strings** (a string of characters). Games use
them for names, messages, scores on screen, and reading and writing
files.

- [Writing strings](#writing-strings)
- [Joining strings](#joining-strings)
- [Putting values into text](#putting-values-into-text)
- [Taking strings apart](#taking-strings-apart)
- [Searching and replacing](#searching-and-replacing)
- [Changing case and spaces](#changing-case-and-spaces)
- [Characters and codes](#characters-and-codes)
- [Numbers as text](#numbers-as-text)
- [Splitting a line into words](#splitting-a-line-into-words)

## Writing strings

A string is written between double quotes. `""` is the empty string.
To put a double quote inside a string, use `Chr(34)`:

```basic
greeting = "Hello"
empty = ""
quote = "She said " + Chr(34) + "run!" + Chr(34)
Print greeting
Print quote
Print Len(empty)
```

Output:

```output
Hello
She said "run!"
0
```

## Joining strings

`+` joins strings. A number joined to a string becomes text:

```basic
name = "Ada"
level = 3
Print "Player " + name + " reached level " + level
```

Output:

```output
Player Ada reached level 3
```

`+` works left to right, so where the numbers are matters:

```basic
Print "a" + 1 + 2       ; "a1", then "a12"
Print 1 + 2 + "a"       ; 3, then "3a"
```

Output:

```output
a12
3a
```

Add brackets to do the sum first: `"total " + (1 + 2)`.

## Putting values into text

A string written `$"..."` can hold expressions in `{ }`, which are worked
out and put into the text. This is easier to read than many `+`s:

```basic
name = "Ada"
hp = 7
maxHp = 10
Print $"{name}: {hp}/{maxHp} HP"
Print $"{hp * 100 / maxHp}% health"
Print $"Braces are written {{like this}}"
```

Output:

```output
Ada: 7/10 HP
70% health
Braces are written {like this}
```

## Taking strings apart

Positions in a string count from 1.

| Command | Gives | Example | Result |
|---|---|---|---|
| `Len(s)` | the number of characters | `Len("hello")` | `5` |
| `Left(s, n)` | the first n characters | `Left("hello", 2)` | `"he"` |
| `Right(s, n)` | the last n characters | `Right("hello", 3)` | `"llo"` |
| `Mid(s, start, n)` | n characters from position start | `Mid("hello", 2, 3)` | `"ell"` |
| `Mid(s, start)` | everything from position start | `Mid("hello", 3)` | `"llo"` |

```basic
word = "Neptune"
For i = 1 To Len(word)
    Print Mid(word, i, 1)       ; one letter at a time
Next
```

Output:

```output
N
e
p
t
u
n
e
```

## Searching and replacing

| Command | Gives |
|---|---|
| `Instr(s, find)` | the position of `find` in `s`, or 0 if it is not there |
| `Instr(s, find, from)` | the same, searching from position `from` |
| `Replace(s, from, to)` | `s` with every `from` replaced by `to` |

```basic
sentence = "the cat sat on the mat"
Print Instr(sentence, "cat")
Print Instr(sentence, "dog")
Print Instr(sentence, "the", 2)        ; the second "the"
Print Replace(sentence, "the", "a")
If Instr(sentence, "mat") Then Print "found a mat"
```

Output:

```output
5
0
16
a cat sat on a mat
found a mat
```

## Changing case and spaces

| Command | Gives |
|---|---|
| `Upper(s)`, `Lower(s)` | all capitals, all small letters |
| `Trim(s)` | `s` without spaces (and tabs and line breaks) at either end |
| `LSet(s, n)` | `s` made exactly n characters, padded with spaces on the right |
| `RSet(s, n)` | `s` made exactly n characters, padded on the left |
| `String(s, n)` | `s` repeated n times |

```basic
typed = "  Ada  "
name = Trim(typed)
Print "[" + name + "]"
Print Upper(Left(name, 1)) + Lower(Mid(name, 2))
Print String("=", 12)
; a score table with lined-up columns
Print LSet("Ada", 8) + RSet("1200", 6)
Print LSet("Grace", 8) + RSet("950", 6)
```

Output:

```output
[Ada]
Ada
============
Ada       1200
Grace      950
```

## Characters and codes

Each character has a number, its **character code**: 65 is `A`, 97 is
`a`, 48 is `0`, 32 is a space.

| Command | Gives |
|---|---|
| `Asc(s)` | the code of the first character of `s` (-1 for an empty string) |
| `Chr(code)` | the one-character string for a code |

```basic
Print Asc("A")
Print Chr(66)
; shift each letter one along the alphabet
secret = ""
For i = 1 To Len("HAL")
    secret = secret + Chr(Asc(Mid("HAL", i, 1)) + 1)
Next
Print secret
```

Output:

```output
65
B
IBM
```

## Numbers as text

Numbers turn into text by themselves when joined to a string. To control
how they look:

| Command | Gives |
|---|---|
| `Str(n)` | the number as text |
| `FormatFloat(n, digits)` | the number with exactly `digits` digits after the point |
| `Hex(n)`, `Bin(n)` | the number in hexadecimal (8 digits) or binary (32 digits) |
| `Int(s)`, `Float(s)` | the number at the start of a string |

```basic
time# = 83.4567
Print "Time: " + FormatFloat(time, 2) + "s"
Print Int("42 apples") + 1
Print Hex(255)
```

Output:

```output
Time: 83.46s
43
000000FF
```

## Splitting a line into words

There is no built-in "split", but `Instr` and `Mid` make one. This
function returns word number `n` of a line, counting words separated by
spaces:

```basic
Function Word$(line$, n)
    count = 0
    start = 1
    line = Trim(line) + " "
    While start <= Len(line)
        space = Instr(line, " ", start)
        If space > start
            count = count + 1
            If count = n Then Return Mid(line, start, space - start)
        EndIf
        start = space + 1
    Wend
    Return ""
End Function

Print Word("take the red key", 3)
Print Word("go north", 1)
```

Output:

```output
red
go
```

[Contents](../MANUAL.md) · Previous: [6. Functions](06-functions.md) · Next: [8. Arrays and data](08-arrays-and-data.md)
