"""Builds examples/media/courtyard.glb, the level of examples/level.nb.

Run with:  blender --background --factory-startup --python tools/blender/make_sample_level.py

It also saves examples/media/courtyard.blend so you can open the level in
Blender, change it and export it again from the Neptune tab.
"""

import math
import os
import sys

import bpy

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import neptune_levels  # noqa: E402

MEDIA = os.path.join(HERE, "..", "..", "examples", "media")

bpy.ops.wm.read_factory_settings(use_empty=True)
neptune_levels.register()


def material(name, color, emission=None, roughness=0.8, metallic=0.0):
    mat = bpy.data.materials.get(name)
    if mat is None:
        mat = bpy.data.materials.new(name)
        mat.use_nodes = True
        node = mat.node_tree.nodes["Principled BSDF"]
        node.inputs["Base Color"].default_value = (*color, 1)
        node.inputs["Roughness"].default_value = roughness
        node.inputs["Metallic"].default_value = metallic
        if emission:
            node.inputs["Emission Color"].default_value = (*emission, 1)
            node.inputs["Emission Strength"].default_value = 2.0
    return mat


STONE = ("Stone", (0.42, 0.40, 0.37))
GRASS = ("Grass", (0.18, 0.32, 0.12))
BRICK = ("Brick", (0.45, 0.17, 0.11))
WOOD = ("Wood", (0.45, 0.28, 0.12))
GOLD = ("Gold", (1.0, 0.72, 0.15), (1.0, 0.6, 0.05), 0.3)
GATE = ("Gate", (0.1, 0.5, 0.9), (0.1, 0.5, 1.0))


def box(name, location, size, mat, rotation=(0, 0, 0)):
    bpy.ops.mesh.primitive_cube_add(size=1, location=location, rotation=rotation)
    obj = bpy.context.object
    obj.name = name
    obj.scale = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(material(*mat))
    return obj


def empty(name, location, rotation=(0, 0, 0)):
    bpy.ops.object.empty_add(location=location, rotation=rotation)
    obj = bpy.context.object
    obj.name = name
    return obj


# the ground and the walls of the courtyard: solid, by their names
box("col_Ground", (0, 0, -0.25), (32, 32, 0.5), GRASS)
for i, (x, y, w, d) in enumerate([(0, 15.5, 32, 1), (0, -15.5, 32, 1), (15.5, 0, 1, 30), (-15.5, 0, 1, 30)]):
    box("col_Wall.%03d" % i, (x, y, 2), (w, d, 4), BRICK)

# pillars, copied the way a level designer would: the prefix stays
for i, (x, y) in enumerate([(-7, -7), (7, -7), (-7, 7), (7, 7)]):
    box("col_Pillar.%03d" % i, (x, y, 2), (1.2, 1.2, 4), STONE)

# a ramp up to a platform
length = 8.0
height = 3.0
angle = math.atan2(height, length)
box("col_Ramp", (-3, 11, height / 2 - 0.15), (6, math.hypot(length, height), 0.3), STONE, (angle, 0, 0))
box("col_Platform", (-3, 13.5 + length / 2 - 2.5, height - 0.25), (6, 3, 0.5), STONE)

# an invisible wall keeps the player away from the gate's back: solid, never drawn
box("hit_GateFence", (10, 13.8, 2), (6, 0.4, 4), STONE)

# crates that fall and can be pushed: custom properties do this
for i, (x, y, z) in enumerate([(4, -3, 0.6), (4, -3, 1.8), (5.3, -3, 0.6), (-4, 3, 2.5)]):
    crate = box("Crate.%03d" % i, (x, y, z), (1.1, 1.1, 1.1), WOOD)
    crate["type"] = "crate"
    crate["physics"] = "dynamic"
    crate["shape"] = "box"
    crate["mass"] = 2.0
    crate["bounce"] = 0.1

# gems for the program to find: it reads type and points
GEMS = [(-11, -11, 1), (11, -11, 1), (-11, 3, 1), (0, 0, 1), (11, 6, 1), (-3, 13.5, 4)]
for i, (x, y, z) in enumerate(GEMS):
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1, radius=0.4, location=(x, y, z))
    gem = bpy.context.object
    gem.name = "Gem.%03d" % i
    gem.data.materials.append(material(*GOLD))
    gem["type"] = "gem"
    gem["points"] = 50 if z > 2 else 10

gate = box("Exit", (10, 14.6, 1.5), (3, 0.4, 3), GATE)
gate["type"] = "exit"

# where the player starts: an empty
start = empty("Start", (0, -12, 1.2))
start["type"] = "spawn"

# a model placed from another file
fox = empty("Fox", (-10, -3, 0), (0, 0, math.radians(120)))
fox.scale = (0.025, 0.025, 0.025)
fox["type"] = "npc"
fox["prefab"] = "Fox.glb"

# lights
bpy.ops.object.light_add(type="SUN", location=(0, 0, 20), rotation=(math.radians(50), 0, math.radians(35)))
sun = bpy.context.object
sun.name = "Sun"
sun.data.energy = 3.0
sun.data.color = (1.0, 0.93, 0.82)

for i, (x, y) in enumerate([(-7, -7), (7, -7), (-7, 7), (7, 7)]):
    bpy.ops.object.light_add(type="POINT", location=(x, y, 5))
    lamp = bpy.context.object
    lamp.name = "Lamp.%03d" % i
    lamp.data.energy = 600
    lamp.data.color = (1.0, 0.55, 0.25)
    lamp["flicker"] = 1

bpy.ops.object.light_add(type="SPOT", location=(10, 8, 7), rotation=(math.radians(50), 0, 0))
spot = bpy.context.object
spot.name = "GateSpot"
spot.data.energy = 1500
spot.data.color = (0.5, 0.7, 1.0)
spot.data.spot_size = math.radians(50)
spot.data.spot_blend = 0.4

# a view of the whole level: the game can switch to it
bpy.ops.object.camera_add(location=(0, -24, 16), rotation=(math.radians(58), 0, 0))
bpy.context.object.name = "Overview"
bpy.context.object.data.lens = 28

neptune_levels.export_level(os.path.join(MEDIA, "courtyard.glb"))
bpy.context.scene.neptune_export_path = "//courtyard.glb"
bpy.ops.wm.save_as_mainfile(filepath=os.path.join(MEDIA, "courtyard.blend"), compress=True)
