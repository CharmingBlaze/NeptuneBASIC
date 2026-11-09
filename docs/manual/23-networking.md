# 23. Networking

[Contents](../MANUAL.md) · Previous: [22. Files and data](22-files.md) · Next: [24. User interfaces](24-gui.md)

Programs can talk to each other over a network or the internet: for
multiplayer games, high-score tables, or downloading data. NeptuneBASIC
has three levels:

| Level | Good for |
|---|---|
| **Multiplayer games** | players joining a game, sending each other messages; finds games on the local network for you |
| **TCP** | a reliable connection: everything arrives, in order |
| **UDP** | single messages, fast, but some may be lost: positions sent many times a second |

- [Multiplayer games](#multiplayer-games)
- [TCP connections](#tcp-connections)
- [UDP messages](#udp-messages)
- [Addresses](#addresses)
- [Tips for network games](#tips-for-network-games)
- [Try it yourself](#try-it-yourself)

## Multiplayer games

The multiplayer commands handle players, joining and leaving, and
messages. One computer **hosts** the game; the others **join** it.

```basic
Graphics 800, 600
result = StartNetGame()               ; join a game on the network, or host one
If result = 0 Then RuntimeError "Could not start a network game"
me = CreateNetPlayer("Player " + Rand(100))

While Not KeyHit(KEY_ESCAPE)
    ; tell everyone where the mouse is
    SendNetMsg 1, MouseX() + "," + MouseY(), me

    ; read what has arrived
    While RecvNetMsg()
        Select NetMsgType()
            Case 100 : DebugLog NetPlayerName(NetMsgFrom()) + " joined"
            Case 101 : DebugLog NetPlayerName(NetMsgFrom()) + " left"
            Case 1 : DebugLog NetPlayerName(NetMsgFrom()) + " is at " + NetMsgData()
        End Select
    Wend
    Cls
    Text 10, 10, IIf(result = 2, "Hosting", "Joined")
    Flip
Wend
StopNetGame
```

| Command | Does |
|---|---|
| `StartNetGame()` | joins a game on the local network, or hosts one if there is none: 1 joined, 2 hosting, 0 failed |
| `HostNetGame(name$)` | hosts a game |
| `JoinNetGame(name$, address$)` | joins a game; an empty address searches the local network |
| `CreateNetPlayer(name$)` | adds a player on this computer and returns its id |
| `SendNetMsg type, data$, from [, to]` | sends text to one player, or everyone (to 0) |
| `RecvNetMsg()` | true if a message has arrived; then read it with... |
| `NetMsgType()`, `NetMsgFrom()`, `NetMsgTo()`, `NetMsgData()` | ...its type, sender, receiver and text |
| `NetPlayerName(id)`, `NetPlayerLocal(id)` | about a player |
| `StopNetGame` | leaves the game |

Message types 1 to 99 are yours to choose (say 1 for positions, 2 for
chat, 3 for "I fired"). Type 100 means a player joined, 101 a player
left. Put several values in one message as text separated by commas,
and take it apart with the string commands
([chapter 7](07-text.md#splitting-a-line-into-words)).

## TCP connections

A TCP connection is a stream, so the file commands read and write it.
One side is the **server**, which waits for connections; the other is the
**client**, which connects. Here both are in one program, talking to
itself:

```basic
server = CreateTCPServer(8123)                  ; listen on port 8123
client = OpenTCPStream("127.0.0.1", 8123)       ; connect to this computer
connection = 0
While connection = 0
    connection = AcceptTCPStream(server)        ; the server accepts it
Wend

WriteLine client, "hello server"
Print "server got: " + ReadLine(connection)
WriteLine connection, "hello client"
Print "client got: " + ReadLine(client)

CloseTCPStream client
CloseTCPStream connection
CloseTCPServer server
```

Output:

```output
server got: hello server
client got: hello client
```

| Command | Does |
|---|---|
| `CreateTCPServer(port)` | starts listening |
| `AcceptTCPStream(server)` | the next client that has connected, or 0 |
| `OpenTCPStream(address$, port)` | connects to a server |
| `CloseTCPStream s`, `CloseTCPServer srv` | closes |
| `TCPTimeouts read_ms, accept_ms` | how long reads and accepts wait |
| `TCPStreamIP(s)`, `TCPStreamPort(s)` | who is at the other end |

A server checks for new clients each frame, and `ReadAvail(stream)`
says whether a client has sent something, so the game never waits.

## UDP messages

UDP sends separate messages. They are fast, but may arrive out of order
or not at all, so they suit information that is sent again and again,
such as positions.

```basic
udp = CreateUDPStream(9000)
; send: write the message, then send it
WriteFloat udp, 12.5
WriteFloat udp, 3.0
SendUDPMsg udp, HostIP(1), 9000
; receive: each RecvUDPMsg fetches one message to read
If RecvUDPMsg(udp)
    x# = ReadFloat(udp)
    y# = ReadFloat(udp)
EndIf
```

`UDPMsgIP` and `UDPMsgPort` say who sent the last message.

## Addresses

Addresses are kept as whole numbers. `DottedIP(ip)` shows one as
`"192.168.0.10"`. `CountHostIPs(name$)` looks up a computer's addresses
(an empty name for this computer) and `HostIP(i)` gives each one.

## Tips for network games

- **Send little, often.** A position a few times a second is plenty;
  smooth the movement in between on the other computers.
- **Never wait.** Check for messages each frame and carry on if there are
  none, so the game doesn't freeze when the network is slow.
- **One computer decides.** Let the host decide things like who was hit,
  so players can't disagree.
- **Test on two computers**, or two copies of the game on one, from the
  start: networking is much easier to build in than to add later.

## Try it yourself

You do not need a second computer for these: each program is both sides of the
conversation, talking to itself over the local network.

1. **Shouting parrot.** Start a TCP server on port 8231 and connect a client to
   it. The client sends three lines (`hello`, `how are you`, `goodbye`). For
   each one the server sends the same line back in capitals, and the client
   prints what it gets back.
   *Hint: look at the chapter's TCP example for the setup, then use a `For` loop with `WriteLine` and `ReadLine` on both ends.*
2. **Guessing game.** The server (port 8232) is thinking of the number 42. The
   client guesses 10, 70, 50 and then 42, sending each guess as a number with
   `WriteInt`. The server replies `higher`, `lower` or `correct!` as a line of
   text. Print each guess and its reply, and stop when the answer is correct.
   *Hint: the server reads a guess with `ReadInt` and compares it with the secret number.*
3. **Position messages.** Send three positions, (1.5, 2.0), (3.0, 4.0) and
   (4.5, 6.0), as UDP messages from one stream (port 9051) to another (port 9052)
   on this computer, and print each one as it arrives.
   *Hint: write the two floats, then `SendUDPMsg`; to receive, `RecvUDPMsg` fetches a message and `ReadFloat` takes it apart. `CountHostIPs("localhost")` then `HostIP(1)` gives this computer's address.*

### Answers

1.

```basic
server = CreateTCPServer(8231)
client = OpenTCPStream("127.0.0.1", 8231)
connection = 0
While connection = 0
    connection = AcceptTCPStream(server)
Wend

Dim line$(2)
line(0) = "hello"
line(1) = "how are you"
line(2) = "goodbye"

For i = 0 To 2
    WriteLine client, line(i)                      ; the client speaks
    heard$ = ReadLine(connection)                  ; the server hears it...
    WriteLine connection, Upper(heard)             ; ...and shouts it back
    Print "client got: " + ReadLine(client)
Next

CloseTCPStream client
CloseTCPStream connection
CloseTCPServer server
```

```output
client got: HELLO
client got: HOW ARE YOU
client got: GOODBYE
```

The server waits until the client's connection is accepted. After that, both
ends are streams, so `WriteLine` and `ReadLine` work on them like on a file:
what one side writes, the other side reads. Each time round the loop makes one
complete round trip: the client's line to the server, then the capitals back.

2.

```basic
SECRET = 42
server = CreateTCPServer(8232)
client = OpenTCPStream("127.0.0.1", 8232)
connection = 0
While connection = 0
    connection = AcceptTCPStream(server)
Wend

Dim guess(3)
guess(0) = 10 : guess(1) = 70 : guess(2) = 50 : guess(3) = 42

For i = 0 To 3
    WriteInt client, guess(i)

    ; the server's turn
    heard = ReadInt(connection)
    If heard < SECRET
        WriteLine connection, "higher"
    ElseIf heard > SECRET
        WriteLine connection, "lower"
    Else
        WriteLine connection, "correct!"
    EndIf

    reply$ = ReadLine(client)
    Print "Guess " + guess(i) + ": " + reply
    If reply = "correct!" Then Exit
Next

CloseTCPStream client
CloseTCPStream connection
CloseTCPServer server
```

```output
Guess 10: higher
Guess 70: lower
Guess 50: lower
Guess 42: correct!
```

This sends two kinds of thing over one connection: a number from the client
(`WriteInt`, which the server takes with `ReadInt`) and a line of text back
(`WriteLine` and `ReadLine`). Both sides must read in the same order and with
the same commands as the other side wrote. `Exit` leaves the loop once the
secret number is found.

3.

```basic
count = CountHostIPs("localhost")
address = HostIP(1)

sender = CreateUDPStream(9051)
receiver = CreateUDPStream(9052)

For i = 1 To 3
    WriteFloat sender, i * 1.5
    WriteFloat sender, i * 2
    SendUDPMsg sender, address, 9052
Next

received = 0
tries = 0
While received < 3 And tries < 200
    If RecvUDPMsg(receiver)
        x# = ReadFloat(receiver)
        y# = ReadFloat(receiver)
        Print "position " + x + ", " + y
        received = received + 1
    Else
        Delay 5                     ; nothing yet: wait a moment
        tries = tries + 1
    EndIf
Wend
Print "Received " + received + " of 3"
```

```output
position 1.5, 2.0
position 3.0, 4.0
position 4.5, 6.0
Received 3 of 3
```

Each message holds two floats. `RecvUDPMsg` returns 0 when nothing has arrived,
so the loop never waits forever: it tries up to 200 times, 5 milliseconds
apart, and then gives up. That is the way to read UDP, because a message
can be lost. On one computer they all arrive, in order, so all three
positions are printed.

[Contents](../MANUAL.md) · Previous: [22. Files and data](22-files.md) · Next: [24. User interfaces](24-gui.md)
