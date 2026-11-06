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

pub mod commands;
pub mod input;
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
use neptune_vm::{base_env, Status, Vm};

use crate::input::Input;
use crate::world::{Host3D, World};

/// Compiler environment: standard library followed by the 3D runtime commands.
pub fn env() -> Env {
    let mut env = base_env();
    env.commands.extend(commands::COMMANDS.iter().map(|(s, _)| parse_sig(s)));
    env.constants.extend(input::constants());
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
    let mut vm = Vm::new(program);
    let mut world = World::default();
    let mut input = Input::default();
    // console phase
    loop {
        let mut host = Host3D::console(&mut world, &mut input);
        match vm.run(&mut host) {
            Ok(Status::Finished) => return Ok(()),
            Ok(Status::Yielded) if world.graphics_requested => break,
            Ok(Status::Yielded) => continue,
            Err(e) => return Err(RunError::Runtime(e.to_string())),
        }
    }
    // graphics phase
    let mut attrs = WindowAttributes::default();
    attrs.title = world.title.clone();
    attrs.resizable = true;
    attrs.inner_size = Some(fyrox::dpi::LogicalSize::new(world.width as f64, world.height as f64).into());
    let event_loop = EventLoop::new().map_err(|e| RunError::Runtime(format!("cannot create window: {e}")))?;
    let mut executor = Executor::from_params(
        Some(event_loop),
        GraphicsContextParams {
            window_attributes: attrs,
            vsync: true,
            msaa_sample_count: Some(4),
            graphics_server_constructor: Default::default(),
            named_objects: false,
        },
    );
    executor.set_resource_hot_reloading_enabled(false);
    let state = PluginState { vm: Some(vm), world, input, scene: Handle::NONE, error: None };
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
        world::setup_scene(&mut scene);
        self.state.borrow_mut().scene = context.scenes.add(scene);
        Ok(())
    }

    fn update(&mut self, context: &mut PluginContext) -> GameResult {
        let mut guard = self.state.borrow_mut();
        let st = &mut *guard;
        let Some(vm) = st.vm.as_mut() else {
            return Ok(());
        };
        if let GraphicsContext::Initialized(g) = &*context.graphics_context {
            let size = g.window.inner_size();
            st.world.width = size.width.max(1);
            st.world.height = size.height.max(1);
        }
        let Ok(scene) = context.scenes.try_get_mut(st.scene) else {
            return Ok(());
        };
        let result = {
            let mut host = Host3D::graphics(&mut st.world, &mut st.input, scene);
            vm.run(&mut host)
        };
        // apply window requests made by the program
        if let GraphicsContext::Initialized(g) = &*context.graphics_context {
            if let Some(t) = st.world.pending_title.take() {
                g.window.set_title(&t);
            }
            if let Some((w, h)) = st.world.pending_size.take() {
                let _ = g.window.request_inner_size(fyrox::dpi::LogicalSize::new(w as f64, h as f64));
            }
        }
        match result {
            Ok(Status::Yielded) => {}
            Ok(Status::Finished) => {
                st.vm = None;
                context.loop_controller.exit();
            }
            Err(e) => {
                eprintln!("{e}");
                st.error = Some(e.to_string());
                st.vm = None;
                context.loop_controller.exit();
            }
        }
        Ok(())
    }

    fn on_os_event(&mut self, event: &Event<()>, _context: PluginContext) -> GameResult {
        let mut guard = self.state.borrow_mut();
        let input = &mut guard.input;
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
                    input.mouse_x = position.x as f32;
                    input.mouse_y = position.y as f32;
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
