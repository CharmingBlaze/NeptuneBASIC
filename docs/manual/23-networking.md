# 23. Networking

[Contents](../MANUAL.md) · Previous: [22. Files and data](22-files.md) · Next: [24. User interfaces](24-gui.md)

Programs can talk to each other over a network or the internet: for
multiplayer games, high-score tables, or downloading data. NeptuneBASIC
has three levels:

| Level | Good for |
|---|---|
| **Multiplayer games** | a whole game on several computers, on a local network or over the internet, with entities that move on everyone's screen |
| **TCP** | a reliable connection: everything arrives, in order |
| **UDP** | single messages, fast, but some may be lost: positions sent many times a second |

- [Multiplayer games](#multiplayer-games)
- [Blitz3D's multiplayer commands](#blitz3ds-multiplayer-commands)
- [TCP connections](#tcp-connections)
- [UDP messages](#udp-messages)
- [Addresses](#addresses)
- [Tips for network games](#tips-for-network-games)
- [Try it yourself](#try-it-yourself)

## Multiplayer games

This is a complete multiplayer program. Everyone who runs it is in the same
world, and sees everyone else's cube move:

```basic
Graphics3D 800, 600, 32, 2
If NetPlay("Cube Chase") = 0 Then RuntimeError NetError()

camera = CreateCamera()
PositionEntity camera, 0, 12, -12
RotateEntity camera, 45, 0, 0
light = CreateLight()
ground = CreatePlane()

NetTemplate "player", CreateCube()      ; what a player looks like
me = NetSpawn("player", 0, 1, 0)        ; appears on every computer

While Not KeyHit(KEY_ESCAPE)
    TranslateEntity me, (KeyDown(KEY_D) - KeyDown(KEY_A)) * 0.1, 0, (KeyDown(KEY_W) - KeyDown(KEY_S)) * 0.1
    UpdateWorld
    RenderWorld
    Flip
Wend
```

`NetPlay` looks for a game of that name on the local network, and starts one
if there is none. `NetSpawn` makes an entity that belongs to you. You move it
with the commands you already know, and it glides smoothly on every other
computer. When a player leaves, their entities go too.

You do not need a second computer. This starts two copies, side by side:

```bash
neptune run --players 2 mygame.nb
```

[`examples/games/coinrush.nb`](../../examples/games/coinrush.nb) is a whole
game built this way, in about 150 lines.

### Playing with friends over the internet

The host calls `NetInvite()`, which returns a long code. Send it to a friend,
and their game joins with it:

```basic
If NetJoin(CommandLine()) = 0 Then RuntimeError NetError()
```

Nobody has to open a port on their router. Connections are encrypted, and
computers talk directly when they can. When a router will not allow that, the
traffic goes through a relay server, which cannot read it.

The free public relays are for development, and they limit how much can be
sent. Before you release a game, run your own relay (the `iroh-relay` program
is free) and point your game at it with `NetRelay`.

### Sharing things

Three commands cover most games:

| Command | Good for |
|---|---|
| `NetSet "score", 5` and `NetGet("score")` | things that have a current value: scores, the round, the map. Anyone who joins later gets them too. |
| `NetSend "boom", "12,40"` | things that happen: an explosion, a chat line. Each one always arrives, in order. |
| `NetSpawn("kart")` | things that move |

Read what has happened once a frame:

```basic
While NetRecv()
    Select NetEvent()
        Case "join" : Print NetData() + " joined"
        Case "leave" : Print NetPlayerName(NetFrom()) + " left"
        Case "boom" : Print "Explosion at " + NetData()
        Case "spawn" : EntityColor NetEntity(), 255, 80, 80
        Case "lost" : RuntimeError "The game has ended: " + NetData()
    End Select
Wend
```

Besides your own events there are `"join"`, `"leave"`, `"spawn"` and
`"despawn"` (an entity from another computer appeared or went) and `"lost"`
(the connection to the host has gone).

### Let one computer decide

If two players grab the same coin at the same moment, somebody has to say who
got it. Let that be the host: check `NetIsHost()`, make the decision there, and
tell everybody with `NetSet` or `NetSend`. The host can see every player's
entity, because they are all copies on its screen.

### Characters that answer instantly

`NetSpawn` trusts each computer about what it owns, which is simple but lets a
modified game move as fast as it likes. A [character
controller](../CHARACTERS.md#in-a-multiplayer-game) on a template changes that:
the host decides where characters really are, and yet your own still answers the
moment you press a key.

```basic
hero = CreateCube()
pattern = CreateCharController(hero, 0.4, 1.8)
NetTemplate "hero", hero

me = NetSpawn("hero", 0, 1, 0)
player = NetCharacter(me)
CharMove player, 1, 0, 0        ; then CharJump, UpdateChar, as usual
```

### Shooting at things that move

Other players are always drawn a moment in the past, because their positions
take time to arrive. If you line up a moving target and fire, it has really
moved on, so a check on the host would say you missed. `NetFire` tells the host
when you fired *in terms of what you could see*. `NetHit` puts everyone back
there for an instant, checks the shot, and puts them back.

```basic
; on whoever fires
If MouseHit(1) Then NetFire "shot", camera, 200

; on the host, in the NetRecv loop
If NetEvent() = "shot"
    target = NetHit()
    If target <> 0 Then NetSend "hit", "10", NetOwner(target)
EndIf
```

`NetHit` works like `LinePick`, so walls with `EntityPickMode` stop shots, and
`PickedX()` and the rest say where it landed. Give what you can hit an
`EntityPickMode` before `NetTemplate`, and the copies will have it too. Players
never hit their own entities.

The host looks back at most half a second, so on a worse connection than that
players have to aim ahead. It also believes where the shot came from and which
way it went. If that matters for your game, check it on the host.

### When it goes wrong

`NetPlay`, `NetCreate` and `NetJoin` return 0 if they fail, and `NetError()` says
why in words you can show the player.

Before you release a game, try it on a bad connection. This adds 150
milliseconds of delay and loses a tenth of the messages that are allowed to be
lost:

```basic
NetSimulate 150, 10
```

| Command | Does |
|---|---|
| `NetPlay(game$ [, name$])` | joins a game on the local network, or hosts one: 1 joined, 2 hosting, 0 failed |
| `NetCreate(game$ [, name$])` | hosts a new game |
| `NetJoin(invite$ [, name$])` | joins with an invite, from anywhere |
| `NetInvite()` | the code that lets others join |
| `NetError()` | why the game could not start or was lost |
| `NetIsHost()`, `NetMe()`, `NetHostPlayer()` | this computer's role, your player, the host's player |
| `NetCountPlayers()`, `NetPlayerAt(i)`, `NetPlayerName(id)` | who is playing |
| `NetPing()` | round trip to the host, in milliseconds |
| `NetSet key$, value$`, `NetGet(key$ [, default$])` | shared values |
| `NetSend event$ [, data$ [, to]]` | tells everyone, or one player |
| `NetRecv()` | true if something happened; then `NetEvent()`, `NetData()`, `NetFrom()`, `NetEntity()` |
| `NetTemplate name$, entity` | what copies called `name$` look like |
| `NetSpawn(template$ [, x#, y#, z#])` | an entity that belongs to you, seen by everyone |
| `NetIsMine(entity)`, `NetOwner(entity)` | who an entity belongs to |
| `NetCharacter(entity)`, `NetCorrections(character)` | the character controller of a networked entity, and how often the host has corrected it |
| `NetFire event$, entity [, range#, data$]` | fires along an entity's forward direction |
| `NetHit([radius#])` | on the host: what the last `NetFire` hit |
| `NetRate per_second` | how often positions are sent (20 unless you change it) |
| `NetSimulate lag_ms, loss_percent` | pretends the network is bad |
| `NetRelay url$` | uses your own relay server |
| `StopNetGame` | leaves the game |

## Blitz3D's multiplayer commands

Programs written for Blitz3D use these, and they work the same way. They are
also the place to start if you want to build your own message formats.

```basic
Graphics 800, 600
result = StartNetGame()               ; join a game on the network, or host one
If result = 0 Then RuntimeError "Could not start a network game"
me = CreateNetPlayer("Player " + Rand(100))

While Not KeyHit(KEY_ESCAPE)
    ; tell everyone where the mouse is; 0, 0 = to everyone, and it may be lost
    SendNetMsg 1, MouseX() + "," + MouseY(), me, 0, 0

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
| `StartNetGame()` | joins another copy of this program on the local network, or hosts: 1 joined, 2 hosting, 0 failed |
| `HostNetGame(name$)` | hosts a game |
| `JoinNetGame(name$, address$)` | joins a game; the address can be an IP address, an invite, or empty to search the local network |
| `CreateNetPlayer(name$)` | adds a player on this computer and returns its id |
| `SendNetMsg type, data$, from [, to [, reliable]]` | sends text to one player, or everyone (to 0); with reliable 0 it is faster but may be lost |
| `RecvNetMsg()` | true if a message has arrived; then read it with... |
| `NetMsgType()`, `NetMsgFrom()`, `NetMsgTo()`, `NetMsgData()` | ...its type, sender, receiver and text |
| `NetPlayerName(id)`, `NetPlayerLocal(id)` | about a player |
| `StopNetGame` | leaves the game |

Message types 1 to 99 are yours to choose (say 1 for positions, 2 for
chat, 3 for "I fired"). Type 100 means a player joined, 101 a player
left, and 200 that the game was lost. Put several values in one message as
text separated by commas, and take it apart with the string commands
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
  smooth the movement in between on the other computers. `NetSpawn` does both
  for you.
- **Never wait.** Check for messages each frame and carry on if there are
  none, so the game doesn't freeze when the network is slow.
- **One computer decides.** Let the host decide things like who was hit,
  so players can't disagree.
- **Test with two copies from the start** (`neptune run --players 2`), and with
  `NetSimulate` to see a bad connection: networking is much easier to build in
  than to add later.

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
