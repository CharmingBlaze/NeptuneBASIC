//! Blender's keys for the tools every level designer uses.
//!
//! G, R and S pick the move, rotate and scale tools, W the select tool and X
//! deletes, as in Blender. The editor moves its camera on W A S D whether or
//! not a mouse button is held, which would make S both "scale" and "go
//! back", so the camera moves on the arrow keys (Page Up and Page Down for
//! up and down) while the right mouse button looks around.
//!
//! The keys are settings: these replace Fyrox's defaults, and keys someone
//! has changed in the editor's settings are left alone.

use fyroxed_base::{
    fyrox::gui::{
        key::{HotKey, KeyBinding},
        message::{KeyCode, KeyboardModifiers},
    },
    settings::keys::KeyBindings,
};

/// Neptune's keys: Fyrox's, with Blender's where the two differ.
pub fn blender() -> KeyBindings {
    let key = HotKey::from_key_code;
    let held = KeyBinding::from_key_code;
    KeyBindings {
        move_forward: held(KeyCode::ArrowUp),
        move_back: held(KeyCode::ArrowDown),
        move_left: held(KeyCode::ArrowLeft),
        move_right: held(KeyCode::ArrowRight),
        move_up: held(KeyCode::PageUp),
        move_down: held(KeyCode::PageDown),
        redo: HotKey::Some { code: KeyCode::KeyZ, modifiers: KeyboardModifiers { control: true, shift: true, ..Default::default() } },
        enable_select_mode: key(KeyCode::KeyW),
        enable_move_mode: key(KeyCode::KeyG),
        enable_rotate_mode: key(KeyCode::KeyR),
        enable_scale_mode: key(KeyCode::KeyS),
        remove_selection: key(KeyCode::KeyX),
        ..KeyBindings::default()
    }
}

/// Gives the settings Neptune's keys unless someone has chosen their own.
pub fn apply(bindings: &mut KeyBindings) {
    if *bindings == KeyBindings::default() {
        *bindings = blender();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_are_on_blenders_keys() {
        let keys = blender();
        assert_eq!(keys.enable_move_mode, HotKey::from_key_code(KeyCode::KeyG));
        assert_eq!(keys.enable_rotate_mode, HotKey::from_key_code(KeyCode::KeyR));
        assert_eq!(keys.enable_scale_mode, HotKey::from_key_code(KeyCode::KeyS));
        assert_eq!(keys.remove_selection, HotKey::from_key_code(KeyCode::KeyX));
        // what Blender shares with every other program stays
        assert_eq!(keys.undo, HotKey::ctrl_key(KeyCode::KeyZ));
        assert_eq!(keys.save_scene, HotKey::ctrl_key(KeyCode::KeyS));
    }

    #[test]
    fn no_tool_key_also_moves_the_camera() {
        let keys = blender();
        let camera = [&keys.move_forward, &keys.move_back, &keys.move_left, &keys.move_right, &keys.move_up, &keys.move_down];
        for letter in [KeyCode::KeyW, KeyCode::KeyG, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyX] {
            assert!(camera.iter().all(|held| **held != KeyBinding::from_key_code(letter)), "{letter:?}");
        }
    }

    #[test]
    fn chosen_keys_are_kept() {
        let mut fresh = KeyBindings::default();
        apply(&mut fresh);
        assert_eq!(fresh, blender());
        let mut own = KeyBindings { enable_move_mode: HotKey::from_key_code(KeyCode::KeyM), ..KeyBindings::default() };
        let before = own.clone();
        apply(&mut own);
        assert_eq!(own, before);
    }
}
