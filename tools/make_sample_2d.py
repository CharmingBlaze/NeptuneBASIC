"""Writes the sample art for examples/tiledmap.nb into examples/media/tiled:
a tileset, a Tiled map and a sprite sheet in Aseprite's format. Standard
library only.

Open meadow.tmx in the Tiled editor (https://www.mapeditor.org) to change the
level. Running this script again overwrites it.
"""

import json
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "examples", "media", "tiled")
TILE = 32


class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [(0, 0, 0, 0)] * (w * h)

    def rect(self, x, y, w, h, c):
        c = (*c, 255) if len(c) == 3 else c
        for j in range(max(y, 0), min(y + h, self.h)):
            for i in range(max(x, 0), min(x + w, self.w)):
                self.px[j * self.w + i] = c

    def save(self, path):
        raw = b"".join(b"\0" + bytes(v for p in self.px[y * self.w : (y + 1) * self.w] for v in p) for y in range(self.h))

        def chunk(kind, data):
            return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

        header = struct.pack(">IIBBBBB", self.w, self.h, 8, 6, 0, 0, 0)
        with open(path, "wb") as f:
            f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


GRASS, DIRT, STONE, BUSH, FLOWER, COIN, SPIKE, CLOUD = 1, 2, 3, 4, 5, 6, 7, 8
FLIP_H = 0x80000000


def make_tileset():
    c = Canvas(TILE * 4, TILE * 2)
    at = lambda n: ((n - 1) % 4 * TILE, (n - 1) // 4 * TILE)

    x, y = at(GRASS)
    c.rect(x, y, TILE, TILE, (122, 82, 48))
    c.rect(x + 6, y + 18, 5, 4, (98, 64, 38))
    c.rect(x + 21, y + 24, 6, 4, (98, 64, 38))
    c.rect(x, y, TILE, 9, (92, 176, 72))
    for i in range(0, TILE, 6):
        c.rect(x + i, y + 9, 3, 3, (92, 176, 72))

    x, y = at(DIRT)
    c.rect(x, y, TILE, TILE, (122, 82, 48))
    c.rect(x + 8, y + 7, 6, 4, (98, 64, 38))
    c.rect(x + 22, y + 19, 5, 5, (98, 64, 38))

    x, y = at(STONE)
    c.rect(x, y, TILE, TILE, (130, 134, 145))
    c.rect(x, y + 15, TILE, 2, (0, 0, 0))
    c.rect(x + 15, y, 2, 15, (0, 0, 0))
    c.rect(x + 7, y + 17, 2, 15, (0, 0, 0))

    x, y = at(BUSH)
    c.rect(x + 3, y + 20, 26, 12, (46, 128, 60))
    c.rect(x + 9, y + 12, 20, 10, (58, 150, 72))
    c.rect(x + 22, y + 8, 7, 6, (58, 150, 72))

    x, y = at(FLOWER)
    c.rect(x + 15, y + 18, 2, 14, (60, 140, 60))
    c.rect(x + 11, y + 10, 10, 10, (240, 120, 160))
    c.rect(x + 14, y + 13, 4, 4, (255, 225, 90))

    x, y = at(COIN)
    c.rect(x + 9, y + 6, 14, 20, (250, 200, 50))
    c.rect(x + 6, y + 9, 20, 14, (250, 200, 50))
    c.rect(x + 13, y + 10, 5, 12, (255, 235, 140))

    x, y = at(SPIKE)
    for i in range(4):
        c.rect(x + 2 + i * 7, y + 22, 7, 10, (190, 195, 205))
        c.rect(x + 4 + i * 7, y + 14, 3, 8, (225, 228, 235))

    x, y = at(CLOUD)
    c.rect(x + 2, y + 14, 28, 10, (250, 250, 255))
    c.rect(x + 9, y + 7, 15, 9, (250, 250, 255))
    c.save(os.path.join(OUT, "tiles.png"))


def csv(rows):
    return ",\n".join(",".join(str(v) for v in row) for row in rows)


def make_map():
    width, height = 60, 15
    ground = [[0] * width for _ in range(height)]
    decor = [[0] * width for _ in range(height)]
    sky = [[0] * width for _ in range(height)]

    pits = {18, 19, 20, 37, 38}
    for x in range(width):
        if x in pits:
            continue
        ground[12][x] = GRASS
        ground[13][x] = DIRT
        ground[14][x] = DIRT
    for sx, sy, length in [(10, 9, 5), (22, 8, 6), (31, 10, 4), (42, 8, 5), (50, 10, 4)]:
        for x in range(sx, sx + length):
            ground[sy][x] = STONE
    for y in range(12):
        ground[y][0] = STONE
        ground[y][width - 1] = STONE

    for x, flip in [(4, 0), (9, FLIP_H), (15, 0), (28, FLIP_H), (35, 0), (46, FLIP_H), (55, 0)]:
        decor[11][x] = BUSH | flip
    for x in (6, 13, 26, 33, 44, 53):
        decor[11][x] = FLOWER
    for x, y in [(5, 2), (6, 2), (17, 4), (18, 4), (19, 4), (30, 2), (31, 2), (40, 3), (41, 3), (52, 2)]:
        sky[y][x] = CLOUD

    def obj(i, name, kind, x, y, extra="", body=""):
        return f'  <object id="{i}" name="{name}" type="{kind}" x="{x}" y="{y}"{extra}>{body}</object>\n'

    things = obj(1, "Start", "spawn", 3 * TILE, 11 * TILE, "", "<point/>")
    next_id = 2
    for x, y, value in [(12, 8, 10), (24, 7, 10), (26, 7, 10), (32, 9, 10), (44, 7, 10), (52, 9, 50), (19, 8, 25), (38, 9, 25)]:
        prop = f'<properties><property name="value" type="int" value="{value}"/></properties>'
        # a tile object is placed by its bottom-left corner
        things += obj(next_id, "Coin", "coin", x * TILE, (y + 1) * TILE, f' gid="{COIN}" width="{TILE}" height="{TILE}"', prop)
        next_id += 1
    for x in (18, 37):
        things += obj(next_id, "Pit", "hazard", x * TILE, 14 * TILE, f' width="{TILE * (3 if x == 18 else 2)}" height="{TILE}"')
        next_id += 1
    things += obj(next_id, "Goal", "goal", 57 * TILE, 9 * TILE, f' width="{TILE}" height="{TILE * 3}"')

    tmx = f"""<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" tiledversion="1.11.0" orientation="orthogonal" renderorder="right-down" width="{width}" height="{height}" tilewidth="{TILE}" tileheight="{TILE}" infinite="0">
 <properties>
  <property name="title" value="Meadow Run"/>
 </properties>
 <tileset firstgid="1" source="tiles.tsx"/>
 <layer id="1" name="Sky" width="{width}" height="{height}">
  <data encoding="csv">
{csv(sky)}
</data>
 </layer>
 <layer id="2" name="Ground" width="{width}" height="{height}">
  <data encoding="csv">
{csv(ground)}
</data>
 </layer>
 <layer id="3" name="Decor" width="{width}" height="{height}">
  <data encoding="csv">
{csv(decor)}
</data>
 </layer>
 <objectgroup id="4" name="Things">
{things} </objectgroup>
</map>
"""
    tsx = f"""<?xml version="1.0" encoding="UTF-8"?>
<tileset version="1.10" tiledversion="1.11.0" name="tiles" tilewidth="{TILE}" tileheight="{TILE}" tilecount="8" columns="4">
 <image source="tiles.png" width="{TILE * 4}" height="{TILE * 2}"/>
</tileset>
"""
    with open(os.path.join(OUT, "meadow.tmx"), "w", encoding="utf-8", newline="\n") as f:
        f.write(tmx)
    with open(os.path.join(OUT, "tiles.tsx"), "w", encoding="utf-8", newline="\n") as f:
        f.write(tsx)


def draw_hero(c, x, y, legs, arm):
    c.rect(x + 7, y + 1, 11, 10, (250, 205, 150))
    c.rect(x + 6, y, 13, 4, (150, 80, 40))
    c.rect(x + 15, y + 5, 2, 2, (30, 30, 50))
    c.rect(x + 6, y + 11, 13, 11, (60, 120, 230))
    c.rect(x + 9 + arm, y + 13, 4, 8, (250, 205, 150))
    c.rect(x + 7 + legs, y + 22, 5, 9, (40, 40, 70))
    c.rect(x + 13 - legs, y + 22, 5, 9, (40, 40, 70))


def make_sheet():
    w, h = 24, 32
    frames = [(0, 0, 500), (0, 1, 500), (-3, -2, 90), (-1, 0, 90), (3, 2, 90), (1, 0, 90), (-3, 3, 200)]
    c = Canvas(w * len(frames), h)
    data = {}
    for i, (legs, arm, ms) in enumerate(frames):
        draw_hero(c, i * w, 0, legs, arm)
        data[f"hero {i}.aseprite"] = {
            "frame": {"x": i * w, "y": 0, "w": w, "h": h},
            "rotated": False,
            "trimmed": False,
            "spriteSourceSize": {"x": 0, "y": 0, "w": w, "h": h},
            "sourceSize": {"w": w, "h": h},
            "duration": ms,
        }
    c.save(os.path.join(OUT, "hero.png"))
    tags = [("idle", 0, 1), ("run", 2, 5), ("jump", 6, 6)]
    meta = {
        "app": "https://www.aseprite.org/",
        "image": "hero.png",
        "format": "RGBA8888",
        "size": {"w": c.w, "h": c.h},
        "scale": "1",
        "frameTags": [{"name": n, "from": a, "to": b, "direction": "forward"} for n, a, b in tags],
    }
    with open(os.path.join(OUT, "hero.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump({"frames": data, "meta": meta}, f, indent=1)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    make_tileset()
    make_map()
    make_sheet()
