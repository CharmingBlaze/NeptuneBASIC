# Neptune's copy of Fyrox's editor

This folder is `fyroxed_base` 1.0.1, the scene editor of the
[Fyrox](https://github.com/FyroxEngine/Fyrox) game engine, copied from
crates.io so that Neptune Level Editor (`crates/neptune-editor`) can change
parts of it that it does not let a program change from outside.

It is Fyrox's work, under the MIT licence in `LICENSE`, which stays with it.
The rest of NeptuneBASIC is under the Zlib licence.

## Rules for changing it

* Change as little as possible. What can be done from `crates/neptune-editor`
  (colours, layout, keys, Play, the starter level) is done there.
* Mark every change with a comment that starts `// Neptune:` and says why,
  so the changes can be found with one search and carried over to a newer
  Fyrox:

      grep -rn "Neptune:" vendor/fyroxed_base/src

* To see every change, compare with the original:
  `cargo download fyroxed_base==1.0.1`, or the copy in
  `~/.cargo/registry/src/*/fyroxed_base-1.0.1`.
* It is not a member of the workspace (see `exclude` in the root
  `Cargo.toml`): `cargo test` and `cargo clippy` do not run on it.

## Changes so far

See the `// Neptune:` comments. In short:

* `src/neptune.rs` is new: Neptune's own code, kept apart from Fyrox's.
* Object list: names have no `(index:generation)` after them.
* Inspector: the engine's own settings are left out unless
  `neptune::set_full_inspector(true)` is called, the type is shown as `Mesh`
  and not its full Rust path, and the buttons that make Fyrox physics
  objects from a mesh show only in the full Inspector.
* Toolbar: the build (hammer) button is hidden, and Play and Stop are in the
  IDE's green and red.
* Panel titles: Objects, Level and Undo History (were World Viewer, Scene
  Preview and Command Stack).
* `AddNodeCommand::keeping_local_transform`, so that the starter level's
  objects stay where they were put.
