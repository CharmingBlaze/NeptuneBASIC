//! What the Neptune panel changes on an object: its name prefix (`col_`,
//! `hit_`) and the properties `LoadLevelFile` understands (`type`,
//! `physics`, `shape`, `mass`, `prefab`). The same rules as the Blender
//! add-on's buttons (`tools/blender/neptune_levels.py`) and as
//! `neptune-runtime`'s `level.rs`, kept apart from the panel's widgets so
//! they can be tested without a window.

use fyroxed_base::fyrox::scene::base::{Property, PropertyValue};

/// How an object collides, given by the start of its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Solidity {
    /// No prefix: drawn, not collided with.
    No,
    /// `col_`: drawn and collided with.
    Solid,
    /// `hit_`: collided with, never drawn.
    Invisible,
}

impl Solidity {
    pub const ALL: [Solidity; 3] = [Solidity::No, Solidity::Solid, Solidity::Invisible];

    pub fn label(self) -> &'static str {
        match self {
            Solidity::No => "Not Solid",
            Solidity::Solid => "Solid",
            Solidity::Invisible => "Invisible",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Solidity::No => "",
            Solidity::Solid => "col_",
            Solidity::Invisible => "hit_",
        }
    }
}

/// The choices for `physics`; the first one means no property.
pub const PHYSICS: [&str; 4] = ["none", "static", "dynamic", "kinematic"];
/// The choices for `shape`; the first one means no property.
pub const SHAPES: [&str; 7] = ["auto", "box", "sphere", "capsule", "cylinder", "mesh", "hull"];

/// What an object's name says about collisions. Letter case does not matter,
/// as in `level.rs`.
pub fn solid_of(name: &str) -> Solidity {
    let head = name.get(..4).unwrap_or("");
    if head.eq_ignore_ascii_case("col_") {
        Solidity::Solid
    } else if head.eq_ignore_ascii_case("hit_") {
        Solidity::Invisible
    } else {
        Solidity::No
    }
}

/// The name without its `col_` or `hit_`.
pub fn plain_name(name: &str) -> &str {
    match solid_of(name) {
        Solidity::No => name,
        _ => &name[4..],
    }
}

/// The name with the prefix for `solid` in place of any it had.
pub fn with_solid(name: &str, solid: Solidity) -> String {
    format!("{}{}", solid.prefix(), plain_name(name))
}

/// A property as text, as `LoadLevelFile` reads it; empty when missing.
pub fn text(props: &[Property], key: &str) -> String {
    let Some(p) = props.iter().find(|p| p.name.eq_ignore_ascii_case(key)) else {
        return String::new();
    };
    match &p.value {
        PropertyValue::String(s) => s.clone(),
        PropertyValue::I64(n) => n.to_string(),
        PropertyValue::U64(n) => n.to_string(),
        PropertyValue::I32(n) => n.to_string(),
        PropertyValue::U32(n) => n.to_string(),
        PropertyValue::I16(n) => n.to_string(),
        PropertyValue::U16(n) => n.to_string(),
        PropertyValue::I8(n) => n.to_string(),
        PropertyValue::U8(n) => n.to_string(),
        PropertyValue::F32(n) => n.to_string(),
        PropertyValue::F64(n) => n.to_string(),
        PropertyValue::NodeHandle(_) | PropertyValue::Handle(_) => String::new(),
    }
}

/// The properties with `key` set to `value`, kept in its place if it was
/// there; `None` takes it out.
pub fn with_value(props: &[Property], key: &str, value: Option<PropertyValue>) -> Vec<Property> {
    let mut out: Vec<Property> = Vec::with_capacity(props.len() + 1);
    let mut placed = false;
    for p in props {
        if p.name.eq_ignore_ascii_case(key) {
            if let (false, Some(v)) = (placed, &value) {
                out.push(Property { name: key.into(), value: v.clone() });
                placed = true;
            }
        } else {
            out.push(p.clone());
        }
    }
    if let (false, Some(v)) = (placed, value) {
        out.push(Property { name: key.into(), value: v });
    }
    out
}

/// A text property; empty text takes it out.
pub fn with_text(props: &[Property], key: &str, value: &str) -> Vec<Property> {
    let value = value.trim();
    with_value(props, key, (!value.is_empty()).then(|| PropertyValue::String(value.into())))
}

/// Index into `PHYSICS` of the object's `physics` property (`0` when it has
/// none or one the list does not know).
pub fn physics_index(props: &[Property]) -> usize {
    let t = text(props, "physics").to_ascii_lowercase();
    match t.as_str() {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        _ => PHYSICS.iter().position(|p| *p == t).unwrap_or(0),
    }
}

/// Index into `SHAPES` of the object's `shape` property.
pub fn shape_index(props: &[Property]) -> usize {
    let t = text(props, "shape").to_ascii_lowercase();
    SHAPES.iter().position(|s| *s == t).unwrap_or(0)
}

/// Sets `physics`. "none" also takes out `shape` and `mass`, which mean
/// nothing without it, as the Blender add-on does.
pub fn with_physics(props: &[Property], index: usize) -> Vec<Property> {
    if index == 0 || index >= PHYSICS.len() {
        let out = with_value(props, "physics", None);
        let out = with_value(&out, "shape", None);
        return with_value(&out, "mass", None);
    }
    with_text(props, "physics", PHYSICS[index])
}

/// Sets `shape`; "auto" is the default, so it takes the property out.
pub fn with_shape(props: &[Property], index: usize) -> Vec<Property> {
    match SHAPES.get(index) {
        Some(s) if index > 0 => with_text(props, "shape", s),
        _ => with_value(props, "shape", None),
    }
}

/// Sets `mass` from what was typed: empty takes it out (a mass of 1), a
/// number that is not one or below zero leaves the properties unchanged.
pub fn with_mass(props: &[Property], typed: &str) -> Option<Vec<Property>> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Some(with_value(props, "mass", None));
    }
    let mass: f32 = typed.parse().ok().filter(|m: &f32| m.is_finite() && *m >= 0.0)?;
    Some(with_value(props, "mass", Some(PropertyValue::F32(mass))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(name: &str, value: &str) -> Property {
        Property { name: name.into(), value: PropertyValue::String(value.into()) }
    }

    #[test]
    fn solid_comes_from_the_name() {
        assert_eq!(solid_of("col_Wall"), Solidity::Solid);
        assert_eq!(solid_of("HIT_Floor"), Solidity::Invisible);
        assert_eq!(solid_of("Wall"), Solidity::No);
        assert_eq!(solid_of("co"), Solidity::No);
        assert_eq!(solid_of("é"), Solidity::No);
        assert_eq!(with_solid("col_Wall", Solidity::Invisible), "hit_Wall");
        assert_eq!(with_solid("Wall", Solidity::Solid), "col_Wall");
        assert_eq!(with_solid("hit_Wall", Solidity::No), "Wall");
    }

    #[test]
    fn properties_are_set_in_place_and_taken_out() {
        let props = vec![s("type", "coin"), s("value", "5")];
        let set = with_text(&props, "type", "gem");
        assert_eq!(set, vec![s("type", "gem"), s("value", "5")]);
        assert_eq!(with_text(&props, "type", "  "), vec![s("value", "5")]);
        assert_eq!(with_text(&props, "prefab", "tree.glb"), vec![s("type", "coin"), s("value", "5"), s("prefab", "tree.glb")]);
        assert_eq!(text(&props, "TYPE"), "coin");
        assert_eq!(text(&props, "prefab"), "");
    }

    #[test]
    fn physics_shape_and_mass() {
        let props = with_physics(&[], 2);
        assert_eq!(physics_index(&props), 2);
        let props = with_shape(&props, 1);
        assert_eq!(shape_index(&props), 1);
        let props = with_mass(&props, "5").unwrap();
        assert_eq!(text(&props, "mass"), "5");
        assert!(with_mass(&props, "heavy").is_none());
        assert!(with_mass(&props, "-1").is_none());
        assert_eq!(with_shape(&props, 0).len(), 2);
        // none takes the others with it
        assert!(with_physics(&props, 0).is_empty());
        assert_eq!(physics_index(&[s("physics", "1")]), 1);
        assert_eq!(physics_index(&[s("physics", "Dynamic")]), 2);
        assert_eq!(physics_index(&[s("physics", "wobbly")]), 0);
    }
}
