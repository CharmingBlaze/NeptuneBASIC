"""NeptuneBASIC level tools for Blender.

Adds a "Neptune" tab to the 3D view's sidebar (press N) for marking objects
as solid, giving them the properties LoadLevelFile understands, and exporting the
level with the right settings.

Install: Edit > Preferences > Add-ons > the arrow at the top right >
Install from Disk, then choose this file. See docs/BLENDER.md.
"""

import os

import bpy

bl_info = {
    "name": "NeptuneBASIC Level Tools",
    "author": "NeptuneBASIC",
    "version": (1, 0, 0),
    "blender": (4, 2, 0),
    "location": "View3D > Sidebar > Neptune",
    "description": "Mark solid objects, set level properties and export levels for LoadLevelFile",
    "category": "Import-Export",
}

SOLID_PREFIXES = ("col_", "hit_")


def plain_name(name):
    """The object's name without a solid prefix."""
    return name[4:] if name[:4].lower() in SOLID_PREFIXES else name


def export_level(filepath, selected_only=False):
    """Writes a level with every setting LoadLevelFile depends on."""
    os.makedirs(os.path.dirname(filepath) or ".", exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=filepath,
        export_format="GLB",
        use_selection=selected_only,
        export_extras=True,
        export_lights=True,
        export_cameras=True,
        export_yup=True,
        export_apply=True,
        export_import_convert_lighting_mode="SPEC",
    )


class NEPTUNE_OT_solid(bpy.types.Operator):
    """Make the selected objects solid, or not, by their name prefix"""

    bl_idname = "neptune.solid"
    bl_label = "Set Solid"
    bl_options = {"REGISTER", "UNDO"}

    mode: bpy.props.EnumProperty(
        items=[
            ("col_", "Solid", "Seen and collided with (col_ prefix)"),
            ("hit_", "Invisible", "Collided with but never drawn (hit_ prefix)"),
            ("", "Not Solid", "Remove the prefix"),
        ]
    )

    def execute(self, context):
        for obj in context.selected_objects:
            obj.name = self.mode + plain_name(obj.name)
        return {"FINISHED"}


class NEPTUNE_OT_properties(bpy.types.Operator):
    """Give the selected objects a type, physics or a prefab"""

    bl_idname = "neptune.properties"
    bl_label = "Set Level Properties"
    bl_options = {"REGISTER", "UNDO"}

    type_name: bpy.props.StringProperty(name="Type", description="Read in the game with EntityProperty(object, \"type\")")
    physics: bpy.props.EnumProperty(
        name="Physics",
        items=[
            ("none", "None", "No rigid body"),
            ("static", "Static", "Never moves"),
            ("dynamic", "Dynamic", "Falls and can be pushed"),
            ("kinematic", "Kinematic", "Moved by the program"),
        ],
    )
    shape: bpy.props.EnumProperty(
        name="Shape",
        items=[(s, s.capitalize(), "") for s in ("auto", "box", "sphere", "capsule", "cylinder", "mesh", "hull")],
    )
    mass: bpy.props.FloatProperty(name="Mass", default=1.0, min=0.0)
    prefab: bpy.props.StringProperty(name="Prefab", description="Model to place here, beside the level or from the game folder")

    def invoke(self, context, event):
        return context.window_manager.invoke_props_dialog(self)

    def execute(self, context):
        for obj in context.selected_objects:
            if self.type_name:
                obj["type"] = self.type_name
            if self.prefab:
                obj["prefab"] = self.prefab
            if self.physics == "none":
                for key in ("physics", "shape", "mass"):
                    obj.pop(key, None)
            else:
                obj["physics"] = self.physics
                obj["shape"] = self.shape
                obj["mass"] = self.mass
        return {"FINISHED"}


class NEPTUNE_OT_export(bpy.types.Operator):
    """Export the level as a .glb file for LoadLevelFile"""

    bl_idname = "neptune.export"
    bl_label = "Export Level"

    def execute(self, context):
        path = bpy.path.abspath(context.scene.neptune_export_path)
        if not path or path.startswith("//"):
            self.report({"ERROR"}, "Save the .blend file first, or choose a full export path")
            return {"CANCELLED"}
        export_level(path)
        self.report({"INFO"}, "Exported " + path)
        return {"FINISHED"}


class NEPTUNE_PT_panel(bpy.types.Panel):
    bl_label = "NeptuneBASIC Level"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Neptune"

    def draw(self, context):
        layout = self.layout
        row = layout.row(align=True)
        row.operator("neptune.solid", text="Solid").mode = "col_"
        row.operator("neptune.solid", text="Invisible").mode = "hit_"
        row.operator("neptune.solid", text="Not Solid").mode = ""
        layout.operator("neptune.properties", text="Set Properties...")
        obj = context.active_object
        if obj is not None:
            box = layout.box()
            box.label(text=obj.name, icon="OBJECT_DATA")
            keys = [k for k in obj.keys() if not k.startswith("_") and k not in ("cycles", "cycles_visibility")]
            for key in keys:
                box.prop(obj, '["%s"]' % key, text=key)
            if not keys:
                box.label(text="No properties")
        layout.separator()
        layout.prop(context.scene, "neptune_export_path", text="File")
        layout.operator("neptune.export", icon="EXPORT")


CLASSES = (NEPTUNE_OT_solid, NEPTUNE_OT_properties, NEPTUNE_OT_export, NEPTUNE_PT_panel)


def register():
    for cls in CLASSES:
        bpy.utils.register_class(cls)
    bpy.types.Scene.neptune_export_path = bpy.props.StringProperty(
        name="Export Path", default="//level.glb", subtype="FILE_PATH", description="Where Export Level writes"
    )


def unregister():
    del bpy.types.Scene.neptune_export_path
    for cls in reversed(CLASSES):
        bpy.utils.unregister_class(cls)


if __name__ == "__main__":
    register()
