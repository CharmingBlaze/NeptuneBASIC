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

[Contents](../MANUAL.md) · Previous: [22. Files and data](22-files.md) · Next: [24. User interfaces](24-gui.md)
