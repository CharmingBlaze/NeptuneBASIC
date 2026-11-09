# 9. Types and objects

[Contents](../MANUAL.md) · Previous: [8. Arrays and data](08-arrays-and-data.md) · Next: [10. Errors and debugging](10-errors.md)

A game is full of things: bullets, enemies, coins, particles. Each one
has several values that belong together (a position, a speed, health).
A **type** describes a kind of thing; an **object** is one of them.

- [Defining a type](#defining-a-type)
- [Making and using objects](#making-and-using-objects)
- [Every object of a type](#every-object-of-a-type)
- [Deleting objects](#deleting-objects)
- [Methods](#methods)
- [Objects that refer to objects](#objects-that-refer-to-objects)
- [Null: no object](#null-no-object)
- [The order of objects](#the-order-of-objects)
- [Handles](#handles)
- [Enums](#enums)
- [A complete example](#a-complete-example)
- [Try it yourself](#try-it-yourself)

## Defining a type

```basic
Type Enemy
    Field name$
    Field x#, y#
    Field hp = 100          ; a field can have a starting value
End Type
```

`Type Enemy ... End Type` defines a new kind of value called `Enemy`.
Each `Field` is a value every enemy has. Fields have types, marked like
variables (`$`, `#`, or none for an Int), and can have a starting value.

## Making and using objects

`New Enemy` makes an object. Store it in a variable marked with the type
name (`.Enemy`), and reach its fields with `\`:

```basic
Type Enemy
    Field name$
    Field x#, y#
    Field hp = 100
End Type

orc.Enemy = New Enemy        ; a new enemy, with hp 100
orc\name = "orc"
orc\x = 50
orc\hp = orc\hp - 30
Print orc\name + " at " + orc\x + " has " + orc\hp + " hp"
```

Output:

```output
orc at 50.0 has 70 hp
```

The `.Enemy` is only needed the first time a variable is used; after
that, `orc` is known to hold an `Enemy`.

A variable doesn't hold the object itself, but a **reference** to it.
Two variables can refer to the same object:

```basic
Type Box
    Field size
End Type
a.Box = New Box
b.Box = a            ; b refers to the same box as a
b\size = 5
Print a\size
```

Output:

```output
5
```

## Every object of a type

NeptuneBASIC keeps a list of every object of each type. `For Each` goes
through it, which is how games update all their enemies, bullets and
particles every frame:

```basic
Type Coin
    Field x, value
End Type

For i = 1 To 4
    c.Coin = New Coin
    c\x = i * 100
    c\value = i * 5
Next

total = 0
For Each c In Coin
    total = total + c\value
Next
Print "Coins are worth " + total
```

Output:

```output
Coins are worth 50
```

`For c.Coin = Each Coin` is another way to write the same loop.

## Deleting objects

`Delete object` removes an object. It is gone from the type's list, and
every variable that referred to it now holds `Null`. `Delete Each Type`
removes them all.

Deleting the object a `For Each` loop is on is safe, and is the usual way
to remove things that are finished:

```basic
Type Bullet
    Field y
End Type

For i = 1 To 5
    b.Bullet = New Bullet
    b\y = i * 150
Next

; remove the bullets that have gone off the bottom of the screen
For Each b In Bullet
    If b\y > 600 Then Delete b
Next

count = 0
For Each b In Bullet : count = count + 1 : Next
Print count + " bullets left"
Delete Each Bullet
```

Output:

```output
4 bullets left
```

## Methods

A **method** is a function that belongs to a type. Inside it, `Self` is
the object it was called on:

```basic
Type Ship
    Field name$
    Field hp = 100

    Method Damage(amount)
        Self\hp = Self\hp - amount
        If Self\hp < 0 Then Self\hp = 0
    End Method

    Method Alive()
        Return Self\hp > 0
    End Method
End Type

hawk.Ship = New Ship
hawk\name = "Hawk"
hawk\Damage(30)
Print hawk\name + ": " + hawk\hp
hawk\Damage(500)
If Not hawk\Alive() Then Print hawk\name + " is destroyed"
```

Output:

```output
Hawk: 70
Hawk is destroyed
```

Methods keep the code that works on a thing next to its fields, which
makes types easy to understand and reuse.

## Objects that refer to objects

A field can hold an object, linking things together: a bullet knows who
fired it, an item knows who carries it.

```basic
Type Player
    Field name$
    Field score
End Type

Type Bullet
    Field owner.Player
End Type

ada.Player = New Player
ada\name = "Ada"
shot.Bullet = New Bullet
shot\owner = ada

; the bullet hit something: reward its owner
shot\owner\score = shot\owner\score + 10
Print shot\owner\name + " scores " + ada\score
```

Output:

```output
Ada scores 10
```

## Null: no object

A type variable that refers to nothing holds `Null`. New variables are
`Null`, and so are variables whose object was deleted. Reading a field
of `Null` is an error, so test first:

```basic
Type Enemy
    Field hp
End Type

target.Enemy = Null
If target = Null Then Print "no target"
target = New Enemy
If target <> Null Then Print "target hp " + target\hp
```

Output:

```output
no target
target hp 0
```

## The order of objects

Objects are kept in the order they were made. These move around the
list:

| Write | Gives |
|---|---|
| `First Enemy`, `Last Enemy` | the first and last object of the type (`Null` if there is none) |
| `After e`, `Before e` | the next and previous object (`Null` at the end) |
| `Insert e Before other` / `Insert e After other` | moves `e` in the list |

```basic
Type Card
    Field value
End Type

For v = 1 To 3
    c.Card = New Card : c\value = v
Next

top.Card = First Card
nxt.Card = After top
Print top\value + " then " + nxt\value
bottom.Card = Last Card
Insert bottom Before top          ; move the last card to the top
top = First Card
Print "now " + top\value + " is first"
```

Output:

```output
1 then 2
now 3 is first
```

The order is the order `For Each` uses, so it can be used to draw things
back to front.

## Handles

`Handle(object)` turns an object into a number, and
`Object.Type(number)` turns the number back. Use them to store an object
where only numbers fit: in an array, a file, or the name of a 3D entity.

```basic
Type Monster
    Field name$
End Type

m.Monster = New Monster
m\name = "slime"
id = Handle(m)
again.Monster = Object.Monster(id)
Print again\name
```

Output:

```output
slime
```

## Enums

An **enum** gives names to a set of related numbers, like the states a
character can be in. Each name is one more than the one before, starting
at 0, unless you give it a value.

```basic
Enum State
    Idle             ; 0
    Walking          ; 1
    Jumping          ; 2
    Dead = 10        ; 10
End Enum

Local now As State = State.Walking
Select now
    Case State.Idle : Print "standing still"
    Case State.Walking : Print "walking"
    Case State.Jumping : Print "in the air"
End Select
Print State.Dead
```

Output:

```output
walking
10
```

Enums make programs read like what they mean: `If state = State.Dead`
instead of `If state = 10`.

## A complete example

A small particle system, the kind used for sparks and explosions:

```basic
Type Spark
    Field x#, y#
    Field dx#, dy#
    Field life

    Method Move()
        Self\x = Self\x + Self\dx
        Self\y = Self\y + Self\dy
        Self\dy = Self\dy + 0.2          ; gravity
        Self\life = Self\life - 1
    End Method
End Type

Function Explode(x#, y#, count)
    For i = 1 To count
        s.Spark = New Spark
        s\x = x : s\y = y
        s\dx = Rnd(-3, 3) : s\dy = Rnd(-5, 0)
        s\life = Rand(20, 40)
    Next
End Function

Graphics 800, 600
SetBuffer BackBuffer()
While Not KeyHit(KEY_ESCAPE)
    If MouseHit(1) Then Explode(MouseX(), MouseY(), 50)
    Cls
    Color 255, 200, 80
    For Each s In Spark
        s\Move()
        If s\life <= 0
            Delete s
        Else
            Plot s\x, s\y
        EndIf
    Next
    Flip
Wend
```

Click to make sparks. Every spark is an object; each frame, every spark
moves, and the ones whose life has run out are deleted.

## Try it yourself

1. **Pocket dragon.** Define a type `Pet` with a `name$` and a `hunger` that
   starts at 5. Make a pet called `Ember`, feed it twice (each meal takes 2
   off its hunger) and print `Ember has hunger 1`.
   *Hint: give `hunger` its starting value on the `Field` line.*
2. **Loot bag.** Make four `Item` objects: a gem worth 90, a coin worth 5, a
   crown worth 250 and a button worth 1. Delete every item worth less than 10,
   then print the names of the items that are left, and their total value.
   *Hint: `For Each` and `Delete` work well together.*
3. **Arena.** Define a type `Fighter` with a name, `hp` and `power`, and a
   method `Attack(target.Fighter)` that takes `power` away from the target's
   `hp`. Let the Hero (30 hp, power 8) and the Goblin (20 hp, power 5) take
   turns attacking, the Hero first, until one has 0 hp or less. Print who won
   and how many rounds it took.
   *Hint: use a `Repeat` loop, and inside the method `Self` is the fighter that attacks.*

### Answers

1.

```basic
Type Pet
    Field name$
    Field hunger = 5
End Type

pet.Pet = New Pet
pet\name = "Ember"
For meal = 1 To 2
    pet\hunger = pet\hunger - 2
Next
Print pet\name + " has hunger " + pet\hunger
```

```output
Ember has hunger 1
```

A field can have a starting value, so every new `Pet` begins with hunger 5.
Two meals take off 4, leaving 1.

2.

```basic
Type Item
    Field name$
    Field value
End Type

Function AddItem(name$, value)
    i.Item = New Item
    i\name = name
    i\value = value
End Function

AddItem("gem", 90)
AddItem("coin", 5)
AddItem("crown", 250)
AddItem("button", 1)

For Each i In Item
    If i\value < 10 Then Delete i
Next

total = 0
For Each i In Item
    Print i\name
    total = total + i\value
Next
Print "Total value: " + total
```

```output
gem
crown
Total value: 340
```

Deleting the object that `For Each` is on is safe, so the coin and the button
are removed while the loop goes on. The second loop only sees the objects
that are left, in the order they were made.

3.

```basic
Type Fighter
    Field name$
    Field hp
    Field power

    Method Attack(target.Fighter)
        target\hp = target\hp - Self\power
    End Method
End Type

hero.Fighter = New Fighter
hero\name = "Hero" : hero\hp = 30 : hero\power = 8
goblin.Fighter = New Fighter
goblin\name = "Goblin" : goblin\hp = 20 : goblin\power = 5

rounds = 0
Repeat
    rounds = rounds + 1
    hero\Attack(goblin)
    If goblin\hp <= 0 Then Exit
    goblin\Attack(hero)
Until hero\hp <= 0

If hero\hp > 0
    Print "Hero wins after " + rounds + " rounds"
Else
    Print "Goblin wins after " + rounds + " rounds"
EndIf
```

```output
Hero wins after 3 rounds
```

Objects are passed by reference, so `Attack` changes the real `hp` of the
fighter it is given. The Hero takes the first swing each round and the loop
leaves with `Exit` as soon as the Goblin is down, so the Goblin does not get
a last blow. The Hero needs three hits (8, 16, 24) to bring the Goblin down.

[Contents](../MANUAL.md) · Previous: [8. Arrays and data](08-arrays-and-data.md) · Next: [10. Errors and debugging](10-errors.md)
