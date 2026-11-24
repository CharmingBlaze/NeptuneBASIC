//! The level the editor opens with when it is given none: a ground to stand
//! on, a sun and a place to start, so there is something to see, to press
//! Play on and to change. It is a new, unsaved scene.

use fyroxed_base::{
    fyrox::{
        asset::untyped::ResourceKind,
        core::{
            algebra::{Matrix4, UnitQuaternion, Vector3},
            color::Color,
            pool::Handle,
            uuid::Uuid,
        },
        material::{Material, MaterialResource},
        scene::{
            base::{BaseBuilder, Property, PropertyValue},
            light::{directional::DirectionalLightBuilder, BaseLightBuilder},
            mesh::{
                surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
                MeshBuilder,
            },
            node::Node,
            pivot::PivotBuilder,
            transform::TransformBuilder,
        },
    },
    scene::commands::graph::AddNodeCommand,
    Editor, Message,
};

fn base(name: &str, position: [f32; 3]) -> BaseBuilder {
    BaseBuilder::new().with_name(name).with_local_transform(TransformBuilder::new().with_local_position(Vector3::from(position)).build())
}

/// The starter level's objects. `col_Ground` is solid by its name and
/// `Spawn` is found by its `type` property, as `docs/EDITOR.md` explains.
pub fn objects() -> Vec<Node> {
    let mut grass = Material::standard();
    grass.set_property("diffuseColor", Color::opaque(96, 132, 88));
    let grass = MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, grass);
    let slab = SurfaceData::make_cube(Matrix4::new_nonuniform_scaling(&Vector3::new(30.0, 0.5, 30.0)));
    let ground = MeshBuilder::new(base("col_Ground", [0.0, -0.25, 0.0]))
        .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(slab)).with_material(grass).build()])
        .build_node();

    // down and a little to one side, so that shapes show their sides
    let tilt = UnitQuaternion::from_euler_angles(-40f32.to_radians(), 30f32.to_radians(), 0.0);
    let place = TransformBuilder::new().with_local_position(Vector3::new(0.0, 8.0, 0.0)).with_local_rotation(tilt).build();
    let sun = DirectionalLightBuilder::new(BaseLightBuilder::new(BaseBuilder::new().with_name("Sun").with_local_transform(place)).with_intensity(2.5))
        .build_node();

    let mut spawn = PivotBuilder::new(base("Spawn", [0.0, 1.0, -6.0])).build_node();
    spawn.set_properties(vec![Property { name: "type".into(), value: PropertyValue::String("spawn".into()) }]);

    vec![ground, sun, spawn]
}

/// Opens a new scene holding the starter level.
pub fn open(editor: &Editor) {
    editor.message_sender.send(Message::NewScene);
    for node in objects() {
        // keeping the place each object was given: the command would otherwise put it at the origin
        editor.message_sender.do_command(AddNodeCommand::new(node, Handle::NONE, false).keeping_local_transform());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_starter_level_can_be_played() {
        let nodes = objects();
        let names: Vec<&str> = nodes.iter().map(|n| n.name()).collect();
        assert_eq!(names, ["col_Ground", "Sun", "Spawn"]);
        let spawn = &nodes[2];
        assert_eq!(spawn.properties[0].name, "type");
        assert_eq!(spawn.properties[0].value, PropertyValue::String("spawn".into()));
    }
}
