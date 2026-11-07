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

[Contents](../MANUAL.md) · Previous: [21. Building worlds](21-worlds.md) · Next: [23. Networking](23-networking.md)
