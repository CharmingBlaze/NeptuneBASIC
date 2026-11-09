//! # neptune-runtime
//!
//! The NeptuneBASIC 3D runtime: BASIC commands mapped directly onto the
//! [Fyrox](https://fyrox.rs) scene graph. Every NeptuneBASIC entity *is* a
//! Fyrox node, so Fyrox features (PBR rendering, shadows, physics, sound,
//! animation, ...) can be exposed as further commands without a second
//! engine layer.
//!
//! Programs start in console mode. When they call `Graphics3D` the VM
//! yields, a Fyrox window is opened and execution continues inside the
//! engine loop, one BASIC frame (`RenderWorld`/`Flip`) per engine update.

pub mod ai;
pub mod anim;
pub mod behavior;
pub mod buffer;
pub mod classic;
pub mod collision;
pub mod commands;
pub mod draw2d;
pub mod erosion;
pub mod gamma;
pub mod image2d;
pub mod input;
pub mod joy;
pub mod json;
pub mod keyanim;
pub mod level;
pub mod material;
pub mod mesh;
pub mod mirror;
pub mod navquery;
pub mod model;
pub mod movie;
pub mod loading;
pub mod scenefile;
pub mod scenes;
pub mod tilemap;
pub mod pbr;
pub mod camerafx;
pub mod skybox;
pub mod debugdraw;
pub mod cloth;
pub mod decal;
pub mod lod;
pub mod particles;
pub mod shader;
pub mod shaderlib;
pub mod shadersrc;
pub mod wgsl;
pub mod postfx;
pub mod matcmds;
pub mod camera2d;
pub mod physics2d;
pub mod anim2d;
pub mod ui;
pub mod jointmotor;
pub mod ragdoll;
pub mod animblend;
pub mod proptrack;
pub mod gui;
pub mod assets;
pub mod gui_lists;
pub mod gui_dialogs;
pub mod gui_editors;
pub mod physics;
pub mod pick;
pub mod actions;
pub mod audiobus;
pub mod lightbake;
pub mod particles2d;
pub mod prefab;
pub mod screenfx;
pub mod tween;
pub mod probe;
pub mod primitive;
pub mod quality;
pub mod screenshot;
pub mod spritesheet;
pub mod tiled;
pub mod sound;
pub mod sprite;
pub mod terrain;
pub mod texture;
pub mod transform;
pub mod world;

use std::fmt;

use fyrox::{
    core::{pool::Handle, reflect::prelude::*, visitor::prelude::*},
    engine::{executor::Executor, GraphicsContext, GraphicsContextParams},
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::EventLoop,
    keyboard::PhysicalKey,
    plugin::{error::GameResult, Plugin, PluginContext},
    scene::Scene,
    window::WindowAttributes,
};
use neptune_lang::{compile, commands::parse_sig, Env, SourceLoader};
use neptune_vm::debug::{Control, DebugClient};
use neptune_vm::{base_env, Status, Vm};

use crate::input::Input;
use crate::world::{Host3D, World};

/// Lower-case names of every built-in command (standard library and
/// runtime, including modern aliases); plugins cannot replace these.
pub fn builtin_names() -> std::collections::HashSet<String> {
    let mut names: std::collections::HashSet<String> =
        neptune_vm::stdlib::COMMANDS.iter().map(|(s, _)| parse_sig(s).name).collect();
    names.extend(commands::table().iter().map(|(s, _)| parse_sig(s).name));
    names
}

/// Loads plugins from the usual folders (once) and returns any problems.
pub fn load_plugins() -> Vec<String> {
    let names = builtin_names();
    neptune_vm::plugin::load_default(&|n| names.contains(n))
}

/// Compiler environment: standard library followed by the 3D runtime commands.
pub fn env() -> Env {
    let mut env = base_env();
    env.commands.extend(commands::table().iter().map(|(s, _)| parse_sig(s)));
    env.constants.extend(input::constants());
    env.constants.extend(joy::constants());
    env.constants.extend(commands::constants());
    env
}

/// Errors reported by [`run_file`].
#[derive(Debug)]
pub enum RunError {
    Io(String),
    Compile(String),
    Runtime(String),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Io(s) | RunError::Compile(s) | RunError::Runtime(s) => f.write_str(s),
        }
    }
}

/// Compiles source and returns the program, or rendered diagnostics.
pub fn compile_source(name: &str, src: &str, loader: &mut dyn SourceLoader) -> Result<neptune_lang::Program, RunError> {
    let env = env();
    let c = compile(name, src, &env, loader);
    let warnings = c.render_diagnostics();
    match c.program {
        Some(p) => {
            if !warnings.is_empty() {
                eprint!("{warnings}");
            }
            Ok(p)
        }
        None => Err(RunError::Compile(warnings)),
    }
}

/// Runs a compiled program: console mode first, then a Fyrox window once
/// the program calls `Graphics3D`.
pub fn run_program(program: neptune_lang::Program) -> Result<(), RunError> {
    run_vm(Vm::new(program), None)
}

/// Runs a VM, under a debugger if there is a client (see `neptune_vm::debug`).
pub fn run_vm(mut vm: Vm, mut debug: Option<Box<dyn DebugClient>>) -> Result<(), RunError> {
    let mut world = World::default();
    let mut input = Input::default();
    // console phase
    loop {
        let mut host = Host3D::console(&mut world, &mut input);
        match vm.run(&mut host) {
            Ok(Status::Finished) => return Ok(()),
            Ok(Status::Yielded) if world.graphics_requested => break,
            Ok(Status::Yielded) => continue,
            Ok(Status::Paused) => {
                let Some(client) = debug.as_mut() else { continue };
                // no window to keep alive yet, so the client may wait
                loop {
                    match client.service(&mut vm, true) {
                        Control::Run => break,
                        Control::Hold => continue,
                        Control::Quit => return Ok(()),
                    }
                }
            }
            Err(e) => return Err(RunError::Runtime(e.to_string())),
        }
    }
    // graphics phase
    fyrox::core::log::Log::set_log_info(false);
    let mut attrs = WindowAttributes::default();
    attrs.title = world.title.clone();
    attrs.resizable = true;
    attrs.inner_size = Some(fyrox::dpi::LogicalSize::new(world.width as f64, world.height as f64).into());
    if world.fullscreen {
        attrs.fullscreen = Some(fyrox::window::Fullscreen::Borderless(None));
    }
    // from now on Print/Write draw on the screen, like Blitz3D in graphics mode
    vm.core.output = neptune_vm::stdlib::Output::Shared(world.print_sink.clone());
    // `neptune run --uncapped` (benchmarks): no vsync and one game update
    // per rendered frame, so programs run as fast as the machine allows
    let uncapped = std::env::var("NEPTUNE_UNCAPPED").is_ok_and(|v| v != "0");
    let event_loop = EventLoop::new().map_err(|e| RunError::Runtime(format!("cannot create window: {e}")))?;
    let mut executor = Executor::from_params(
        Some(event_loop),
        GraphicsContextParams {
            window_attributes: attrs,
            vsync: !uncapped,
            msaa_sample_count: world.quality.msaa(),
            graphics_server_constructor: Default::default(),
            named_objects: false,
        },
    );
    executor.set_resource_hot_reloading_enabled(false);
    if uncapped {
        // Fyrox runs updates at a fixed rate (60 a second); a tiny step with
        // throttling on every frame makes it one update per frame instead
        executor.set_desired_update_rate(10_000.0);
        executor.set_throttle_threshold(0.0);
        executor.set_throttle_frame_interval(0);
    }
    let state = PluginState { vm: Some(vm), world, input, scene: Handle::NONE, error: None, overlay: Default::default(), scale: 1.0, debug, held: false };
    let state = std::rc::Rc::new(std::cell::RefCell::new(state));
    executor.add_plugin(NeptunePlugin { state: state.clone() });
    executor.run();
    let err = state.borrow_mut().error.take();
    match err {
        Some(e) => Err(RunError::Runtime(e)),
        None => Ok(()),
    }
}

struct PluginState {
    vm: Option<Vm>,
    world: World,
    input: Input,
    scene: Handle<Scene>,
    error: Option<String>,
    overlay: draw2d::Overlay,
    /// Window DPI scale factor (physical pixels per logical pixel).
    scale: f32,
    debug: Option<Box<dyn DebugClient>>,
    /// Stopped in the debugger: the window stays alive but the world is still.
    held: bool,
}

/// Fyrox plugin that owns the VM and executes it frame by frame.
/// The VM and program state live behind a shared cell so the plugin type
/// satisfies Fyrox's `Clone`/`Reflect` requirements without copying the VM.
#[derive(Visit, Reflect, Clone)]
pub struct NeptunePlugin {
    #[visit(skip)]
    #[reflect(hidden)]
    state: std::rc::Rc<std::cell::RefCell<PluginState>>,
}

impl fmt::Debug for NeptunePlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NeptunePlugin").finish_non_exhaustive()
    }
}

impl Plugin for NeptunePlugin {
    fn init(&mut self, _scene_path: Option<&str>, context: PluginContext) -> GameResult {
        let mut scene = Scene::new();
        let pool = world::setup_scene(&mut scene);
        let mut st = self.state.borrow_mut();
        st.world.mirror_pool = pool;
        st.scene = context.scenes.add(scene);
        st.world.scenes = vec![Some(scenes::SceneSlot { handle: st.scene, mirror_pool: Vec::new() })];
        if context.user_interfaces.iter().next().is_none() {
            let size = fyrox::core::algebra::Vector2::new(st.world.width as f32, st.world.height as f32);
            context.user_interfaces.add(fyrox::gui::UserInterface::new(size));
        }
        // the 2D drawing layer first, so GUI widgets made later draw above it
        let scale = st.scale;
        let screen = fyrox::core::algebra::Vector2::new(st.world.width as f32, st.world.height as f32) * scale;
        st.overlay.show(context.user_interfaces.first_mut(), &[], scale, screen);
        Ok(())
    }

    fn update(&mut self, context: &mut PluginContext) -> GameResult {
        let mut guard = self.state.borrow_mut();
        let st = &mut *guard;
        let Some(vm) = st.vm.as_mut() else {
            return Ok(());
        };
        if let GraphicsContext::Initialized(g) = &*context.graphics_context {
            // Programs work in logical pixels, so `Graphics3D 1280,720` means
            // the same thing on a high-DPI screen.
            let scale = g.window.scale_factor();
            let size = g.window.inner_size().to_logical::<f64>(scale);
            st.world.width = (size.width.round() as u32).max(1);
            st.world.height = (size.height.round() as u32).max(1);
            st.scale = scale as f32;
        }
        // UpdateGamma: every camera grades its image through the gamma table
        if st.world.gamma_dirty {
            st.world.gamma_dirty = false;
            let identity = st.world.gamma.is_identity();
            let lut = if identity { None } else { st.world.gamma.lut().map_err(|e| eprintln!("{e}")).ok() };
            for scene in context.scenes.iter_mut() {
                for (handle, node) in scene.graph.pair_iter_mut() {
                    // cameras with their own colour table (CameraColorGrading) keep it
                    if st.world.graded_cameras.contains(&handle) {
                        continue;
                    }
                    if let Some(cam) = node.cast_mut::<fyrox::scene::camera::Camera>() {
                        if let Some(l) = &lut {
                            let _ = cam.set_color_grading_lut(Some(l.clone()));
                        }
                        cam.set_color_grading_enabled(lut.is_some());
                    }
                }
            }
        }
        // a screenshot asked for last frame: the frame now on screen
        if let Some(path) = st.world.pending_screenshot.take() {
            if let GraphicsContext::Initialized(g) = &*context.graphics_context {
                let size = g.window.inner_size();
                let r = screenshot::capture(g.renderer.graphics_server(), size.width, size.height).and_then(|img| screenshot::save(&img, &path));
                if let Err(e) = r {
                    eprintln!("SaveBuffer: {e}");
                }
            }
        }
        st.input.joys.poll();
        st.input.update_actions();
        if let GraphicsContext::Initialized(g) = &*context.graphics_context {
            st.world.tris_rendered = g.renderer.get_statistics().geometry.triangles_rendered as i32;
        }
        if context.scenes.try_get(st.scene).is_err() {
            return Ok(());
        }
        st.world.update_dt = context.dt;
        st.world.dpi = st.scale;
        // (ext) the post-processing render pass, once the renderer exists
        if !st.world.postfx.registered {
            if let GraphicsContext::Initialized(g) = &mut *context.graphics_context {
                let pass = postfx::PostPass::new(st.world.postfx.frame.clone(), st.world.postfx.failed.clone(), std::any::TypeId::of::<NeptunePlugin>());
                g.renderer.add_render_pass(std::rc::Rc::new(std::cell::RefCell::new(pass)));
                st.world.postfx.registered = true;
            }
        }
        let control = st.debug.as_mut().map_or(Control::Run, |c| c.service(vm, false));
        if control == Control::Quit {
            st.vm = None;
            context.loop_controller.exit();
            return Ok(());
        }
        let hold = control == Control::Hold;
        if hold != st.held {
            st.held = hold;
            // Fyrox steps physics by itself; bodies must not fall while stopped
            for scene in context.scenes.iter_mut() {
                scene.graph.physics.enabled.set_value_silent(!hold);
                scene.graph.physics2d.enabled.set_value_silent(!hold);
            }
        }
        let result = {
            let scale = st.scale;
            let ui = context.user_interfaces.first_mut();
            let gpu = match &*context.graphics_context {
                GraphicsContext::Initialized(g) => Some(g.renderer.graphics_server()),
                _ => None,
            };
            let mut host = Host3D::graphics(&mut st.world, &mut st.input, &mut *context.scenes, context.resource_manager.clone()).with_ui(ui, scale).with_gpu(gpu);
            // models and textures loading in the background
            if let Err(e) = loading::poll(&mut host) {
                eprintln!("{e}");
            }
            // (ext) shader time, hot reloading and automatic parameters
            if let Err(e) = host.update_shaders() {
                eprintln!("{e}");
            }
            let r = if hold { Ok(Status::Paused) } else { vm.run(&mut host) };
            if let Err(e) = host.update_postfx() {
                eprintln!("{e}");
            }
            r
        };
        // show the 2D frame published by Flip
        if let Some(items) = st.world.canvas.published.take() {
            let screen = fyrox::core::algebra::Vector2::new(st.world.width as f32, st.world.height as f32) * st.scale;
            st.overlay.show(context.user_interfaces.first_mut(), &items, st.scale, screen);
        }
        // the retained GUI fills the window, in logical pixels
        let logical = fyrox::core::algebra::Vector2::new(st.world.width as f32, st.world.height as f32);
        st.world.gui.sync_root(context.user_interfaces.first_mut(), logical, st.scale);
        // apply renderer quality changes (GraphicsQuality / PostEffect)
        if st.world.quality_dirty {
            if let GraphicsContext::Initialized(g) = &mut *context.graphics_context {
                st.world.quality_dirty = false;
                let _ = g.renderer.set_quality_settings(&st.world.quality.settings());
            }
        }
        // apply window requests made by the program
        if let GraphicsContext::Initialized(g) = &*context.graphics_context {
            if let Some(t) = st.world.pending_title.take() {
                g.window.set_title(&t);
            }
            if let Some((w, h)) = st.world.pending_size.take() {
                let _ = g.window.request_inner_size(fyrox::dpi::LogicalSize::new(w as f64, h as f64));
            }
            if let Some(on) = st.world.pending_fullscreen.take() {
                g.window.set_fullscreen(on.then_some(fyrox::window::Fullscreen::Borderless(None)));
            }
            if let Some(visible) = st.world.pending_cursor_visible.take() {
                g.window.set_cursor_visible(visible);
            }
            if let Some((x, y)) = st.world.pending_cursor_pos.take() {
                let _ = g.window.set_cursor_position(fyrox::dpi::LogicalPosition::new(x as f64, y as f64));
            }
            if let Some(grab) = st.world.pending_grab.take() {
                use fyrox::window::CursorGrabMode;
                let mode = if grab { CursorGrabMode::Locked } else { CursorGrabMode::None };
                // not every platform can lock; confining is the next best thing
                if g.window.set_cursor_grab(mode).is_err() && grab {
                    let _ = g.window.set_cursor_grab(CursorGrabMode::Confined);
                }
            }
        }
        match result {
            Ok(Status::Yielded | Status::Paused) => {}
            Ok(Status::Finished) => {
                st.vm = None;
                context.loop_controller.exit();
            }
            Err(e) => {
                st.error = Some(e.to_string());
                st.vm = None;
                context.loop_controller.exit();
            }
        }
        Ok(())
    }

    fn on_ui_message(&mut self, _context: &mut PluginContext, message: &fyrox::gui::message::UiMessage, _ui: fyrox::core::pool::Handle<fyrox::gui::UserInterface>) -> GameResult {
        // clicks, edits and selections of the program's GUI widgets
        self.state.borrow_mut().world.gui.on_message(message);
        Ok(())
    }

    fn on_os_event(&mut self, event: &Event<()>, _context: PluginContext) -> GameResult {
        let mut guard = self.state.borrow_mut();
        let scale = guard.scale.max(0.01);
        let input = &mut guard.input;
        if let Event::DeviceEvent { event: fyrox::event::DeviceEvent::MouseMotion { delta }, .. } = event {
            input.mouse_motion(delta.0 as f32 / scale, delta.1 as f32 / scale);
        }
        if let Event::WindowEvent { event, .. } = event {
            match event {
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        input.key_event(code, event.state == ElementState::Pressed);
                    }
                    if event.state == ElementState::Pressed {
                        if let Some(text) = &event.text {
                            for c in text.chars() {
                                input.char_event(c);
                            }
                        }
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    input.mouse_x = position.x as f32 / scale;
                    input.mouse_y = position.y as f32 / scale;
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    let b = match button {
                        MouseButton::Left => 1,
                        MouseButton::Right => 2,
                        MouseButton::Middle => 3,
                        _ => 0,
                    };
                    input.mouse_button(b, *state == ElementState::Pressed);
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    input.mouse_z += match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y,
                        MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                    };
                }
                WindowEvent::Focused(false) => input.flush_keys(),
                _ => {}
            }
        }
        Ok(())
    }
}
