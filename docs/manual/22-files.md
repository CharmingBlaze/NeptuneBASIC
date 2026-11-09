# 22. Files and data

[Contents](../MANUAL.md) · Previous: [21. Building worlds](21-worlds.md) · Next: [23. Networking](23-networking.md)

Games save progress, settings and high scores, and read level files and
configuration. This chapter covers files and folders, reading and
writing text and numbers, and **banks**, blocks of raw bytes.

- [Writing and reading a text file](#writing-and-reading-a-text-file)
- [Streams](#streams)
- [Numbers in files](#numbers-in-files)
- [A save file](#a-save-file)
- [Files and folders](#files-and-folders)
- [Banks](#banks)
- [Try it yourself](#try-it-yourself)

## Writing and reading a text file

```basic
path$ = SystemProperty("tempdir") + "scores.txt"

out = WriteFile(path)               ; create the file (or empty it)
WriteLine out, "Ada 1200"
WriteLine out, "Grace 950"
CloseFile out

source = ReadFile(path)                 ; open it to read
While Not Eof(source)                   ; until the end of the file
    Print ReadLine(source)
Wend
CloseFile source
```

Output:

```output
Ada 1200
Grace 950
```

- `WriteFile(path$)` creates a file (emptying it if it exists) and
  returns a **stream** to write to; `ReadFile(path$)` opens one to read;
  `OpenFile(path$)` opens one to read and write. Each returns 0 if the
  file can't be opened.
- `WriteLine stream, text$` writes a line; `ReadLine(stream)` reads one.
- `Eof(stream)` is true at the end of the file.
- `CloseFile stream` finishes with it. Always close files you write, or
  the end of what you wrote may be lost.

Paths are relative to the current folder (the one the game was started
from) unless they start from a drive or `/`. `SystemProperty("tempdir")`,
`"homedir"` and `"appdir"` give useful folders.

## Streams

A **stream** is anything you can read from or write to in order: a file,
a network connection ([chapter 23](23-networking.md)). The same commands
work on all of them.

| Command | Reads or writes |
|---|---|
| `ReadLine(s)`, `WriteLine s, text$` | a line of text |
| `ReadString(s)`, `WriteString s, text$` | text with its length stored first (can contain line breaks) |
| `ReadByte(s)`, `WriteByte s, n` | one byte, 0-255 |
| `ReadShort(s)`, `WriteShort s, n` | a whole number from 0 to 65535 (2 bytes) |
| `ReadInt(s)`, `WriteInt s, n` | any Int (4 bytes) |
| `ReadFloat(s)`, `WriteFloat s, n` | any Float (4 bytes) |
| `Eof(s)` | true at the end |
| `ReadAvail(s)` | how many bytes can be read now |
| `FilePos(s)`, `SeekFile(s, pos)` | where in a file the next read or write happens, and moving it |

## Numbers in files

Text files are easy to read and edit by hand. **Binary** files store
numbers as their bytes: smaller and faster, and they read back exactly.
Read things back in the same order and with the same commands they were
written with:

```basic
path$ = SystemProperty("tempdir") + "data.bin"
out = WriteFile(path)
WriteInt out, 42
WriteFloat out, 3.5
WriteString out, "two" + Chr(10) + "lines"
CloseFile out

source = ReadFile(path)
Print ReadInt(source)
Print ReadFloat(source)
Print ReadString(source)
CloseFile source
```

Output:

```output
42
3.5
two
lines
```

## A save file

```basic
Global playerName$ = "Ada"
Global level = 3
Global score = 4200
Global volume# = 0.8

Function SaveGame(path$)
    f = WriteFile(path)
    If f = 0 Then Return False
    WriteLine f, "SAVE1"                 ; a marker, so old or wrong files are noticed
    WriteLine f, playerName
    WriteInt f, level
    WriteInt f, score
    WriteFloat f, volume
    CloseFile f
    Return True
End Function

Function LoadGame(path$)
    If FileType(path) <> 1 Then Return False     ; no save yet
    f = ReadFile(path)
    If ReadLine(f) <> "SAVE1" Then CloseFile f : Return False
    playerName = ReadLine(f)
    level = ReadInt(f)
    score = ReadInt(f)
    volume = ReadFloat(f)
    CloseFile f
    Return True
End Function

path$ = SystemProperty("tempdir") + "mygame.sav"
SaveGame(path)
level = 1 : score = 0                 ; pretend we started again
If LoadGame(path) Then Print "Welcome back, " + playerName + ": level " + level + ", score " + score
```

Output:

```output
Welcome back, Ada: level 3, score 4200
```

## Files and folders

| Command | Does |
|---|---|
| `FileType(path$)` | 1 a file, 2 a folder, 0 nothing there |
| `FileSize(path$)` | its size in bytes |
| `CopyFile from$, to$`, `DeleteFile path$` | copies, deletes |
| `CreateDir path$`, `DeleteDir path$` | makes, removes (an empty) folder |
| `CurrentDir()`, `ChangeDir path$` | the current folder |
| `ReadDir(path$)`, `NextFile(dir)`, `CloseDir dir` | lists a folder |

```basic
; list the files in the examples folder
dir = ReadDir("examples")
Repeat
    name$ = NextFile(dir)
    If name = "" Then Exit
    If FileType("examples/" + name) = 1 Then DebugLog name
Forever
CloseDir dir
```

## Banks

A **bank** is a block of memory, a row of bytes you can put numbers into
at any position. Banks are for binary formats, network messages, and
passing data to plugins.

```basic
bank = CreateBank(12)              ; 12 bytes, all zero
PokeInt bank, 0, 1234              ; an Int at byte 0 (it takes 4 bytes)
PokeFloat bank, 4, 2.5             ; a Float at byte 4
PokeByte bank, 8, 255              ; a byte at byte 8
Print PeekInt(bank, 0)
Print PeekFloat(bank, 4)
Print PeekByte(bank, 8)
Print BankSize(bank)
FreeBank bank
```

Output:

```output
1234
2.5
255
12
```

| Command | Does |
|---|---|
| `CreateBank(size)`, `FreeBank b`, `ResizeBank b, size`, `BankSize(b)` | makes, frees, resizes, measures |
| `PokeByte/Short/Int/Float b, offset, value` | writes a value at a byte position |
| `PeekByte/Short/Int/Float(b, offset)` | reads one |
| `CopyBank from, from_offset, to, to_offset, count` | copies bytes |
| `ReadBytes(stream, b, offset, count)`, `WriteBytes(stream, b, offset, count)` | moves bytes between a bank and a stream |

## Try it yourself

Each program works in the temporary folder, and deletes its file at the end.

1. **Hall of fame.** Write a file that holds three players and their scores
   (Ada 1200, Grace 950 and Alan 1530), each name and score on a line of its
   own. Read it back, print each player, and then say who is the champion.
   *Hint: read lines in pairs inside a `While Not Eof(...)` loop, and use `Int` to turn the score line into a number.*
2. **Backpack.** Save a backpack in a binary file: the number of items first,
   then for each item its name (`WriteString`), how many you have (`WriteInt`)
   and its price (`WriteFloat`). Read the backpack back and print each item
   with the value of the stack, and the total value of everything.
   *Hint: writing the count first tells the reader how many items to read.*
3. **Map in a bank.** Make a bank of 24 bytes for a map 6 across and 4 down.
   Put 1 in the bytes on the edge of the map (walls) and 0 inside. Write the
   bank to a file, check the file is 24 bytes long, read it into a second bank,
   and print the second bank as a picture with `#` for 1 and `.` for 0.
   *Hint: the byte for column x of row y is at position `y * 6 + x`.*

### Answers

1.

```basic
path$ = SystemProperty("tempdir") + "halloffame.txt"

out = WriteFile(path)
WriteLine out, "Ada"
WriteLine out, "1200"
WriteLine out, "Grace"
WriteLine out, "950"
WriteLine out, "Alan"
WriteLine out, "1530"
CloseFile out

source = ReadFile(path)
bestName$ = ""
bestScore = -1
While Not Eof(source)
    name$ = ReadLine(source)
    score = Int(ReadLine(source))
    Print name + ": " + score
    If score > bestScore
        bestScore = score
        bestName = name
    EndIf
Wend
CloseFile source
Print "The champion is " + bestName + "!"
DeleteFile path
```

```output
Ada: 1200
Grace: 950
Alan: 1530
The champion is Alan!
```

Every line in a text file is text, so the scores are read as text and turned
into numbers with `Int`. The loop reads a name and then a score each time,
and `Eof` is true after the last line. Starting `bestScore` at -1 means that
the first player always becomes the champion until someone beats them.

2.

```basic
path$ = SystemProperty("tempdir") + "backpack.bin"

out = WriteFile(path)
WriteInt out, 3
WriteString out, "potion"
WriteInt out, 4
WriteFloat out, 2.5
WriteString out, "arrow"
WriteInt out, 20
WriteFloat out, 0.25
WriteString out, "gem"
WriteInt out, 2
WriteFloat out, 50
CloseFile out

source = ReadFile(path)
count = ReadInt(source)
total# = 0
For i = 1 To count
    name$ = ReadString(source)
    amount = ReadInt(source)
    price# = ReadFloat(source)
    value# = amount * price
    Print name + " x" + amount + " = " + value
    total = total + value
Next
CloseFile source
Print "Total: " + total
DeleteFile path
```

```output
potion x4 = 10.0
arrow x20 = 5.0
gem x2 = 100.0
Total: 115.0
```

A binary file has no lines to find the end of, so the reader must read the
same things in the same order as the writer wrote them. Writing the number
of items first lets the loop know how many to read. Floats like 2.5 and 0.25
are stored exactly, so the totals print without surprises.

3.

```basic
Const W = 6, H = 4
path$ = SystemProperty("tempdir") + "map.bin"

map = CreateBank(W * H)
For y = 0 To H - 1
    For x = 0 To W - 1
        If x = 0 Or x = W - 1 Or y = 0 Or y = H - 1
            PokeByte map, y * W + x, 1
        EndIf
    Next
Next

out = WriteFile(path)
WriteBytes out, map, 0, W * H
CloseFile out
Print "File size: " + FileSize(path)

copy = CreateBank(W * H)
source = ReadFile(path)
ReadBytes source, copy, 0, W * H
CloseFile source

For y = 0 To H - 1
    row$ = ""
    For x = 0 To W - 1
        row = row + IIf(PeekByte(copy, y * W + x) = 1, "#", ".")
    Next
    Print row
Next

FreeBank map
FreeBank copy
DeleteFile path
```

```output
File size: 24
######
#....#
#....#
######
```

A new bank starts with every byte 0, so only the walls need to be set. A map
cell at column `x` of row `y` lives at byte `y * W + x`: the rows are laid one
after another. `WriteBytes` and `ReadBytes` move the whole block between the
bank and the file in one go, and the file is exactly one byte per cell, 24
bytes.

[Contents](../MANUAL.md) · Previous: [21. Building worlds](21-worlds.md) · Next: [23. Networking](23-networking.md)
