//! 3D runtime commands. Signatures follow Blitz3D's `bbblitz3d.cpp` /
//! `bbgraphics.cpp` / `bbinput.cpp` declarations; NeptuneBASIC additions
//! are marked `(ext)`.

use fyrox::core::{algebra::Vector3, color::Color, math::Rect};
use fyrox::graph::SceneGraph;
use fyrox::scene::{
    camera::{Camera, Projection},
    light::{directional::DirectionalLight, point::PointLight, spot::SpotLight, BaseLight},
    mesh::RenderPath,
};
use neptune_lang::commands::ConstVal;
use neptune_vm::{NativeCtx, Value};

use crate::draw2d::{load_font, FontInfo};
use crate::transform::{angles, orthonormalize, rotation, vector_pitch, vector_yaw};
use crate::world::{camera_node, light_node, mesh_node, pivot_node, primitive, Host3D, Kind, LightKind};

pub type Cmd = for<'a, 'b> fn(&'b mut Host3D<'a>, &[Value], &mut NativeCtx) -> Result<Value, String>;

const VOID: Result<Value, String> = Ok(Value::Null);

fn int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}
fn float(v: f32) -> Result<Value, String> {
    Ok(Value::Float(v))
}
fn v3(a: &[Value], i: usize) -> Vector3<f32> {
    Vector3::new(a[i].float(), a[i + 1].float(), a[i + 2].float())
}
fn color(a: &[Value], i: usize) -> Color {
    let c = |v: &Value| v.float().clamp(0.0, 255.0) as u8;
    Color::opaque(c(&a[i]), c(&a[i + 1]), c(&a[i + 2]))
}

fn create(h: &mut Host3D, kind: &str, segments: i32, solid: bool, parent: i32) -> Result<Value, String> {
    h.scene()?;
    let name = match kind {
        "cube" => "Cube",
        "sphere" => "Sphere",
        "cylinder" => "Cylinder",
        "cone" => "Cone",
        _ => "Plane",
    };
    let id = h.add_entity(mesh_node(primitive(kind, segments, solid), name), Kind::Mesh, parent)?;
    int(id)
}

fn set_world_position(h: &mut Host3D, e: i32, p: Vector3<f32>) -> Result<(), String> {
    let local = h.parent_world(e)?.to_local_point(&p);
    h.set_xf(e, |xf| xf.pos = local)
}

fn set_world_rotation(h: &mut Host3D, e: i32, r: fyrox::core::algebra::Matrix3<f32>) -> Result<(), String> {
    let pw = h.parent_world(e)?;
    let local = pw.rot.transpose() * r;
    h.set_xf(e, |xf| xf.rot = orthonormalize(&local))
}

fn look_at(h: &mut Host3D, e: i32, target: Vector3<f32>, roll: f32) -> Result<Value, String> {
    let v = target - h.world_of(e)?.position();
    if v.norm_squared() > 1e-12 {
        set_world_rotation(h, e, rotation(vector_pitch(&v), vector_yaw(&v), roll))?;
    }
    VOID
}

fn coord(h: &mut Host3D, a: &[Value], axis: usize) -> Result<Value, String> {
    let e = a[0].int();
    let p = if a[1].int() != 0 { h.world_of(e)?.position() } else { h.ent(e)?.xf.pos };
    float(p[axis])
}

fn angle(h: &mut Host3D, a: &[Value], which: usize) -> Result<Value, String> {
    let e = a[0].int();
    let r = if a[1].int() != 0 { h.world_of(e)?.rot } else { h.ent(e)?.xf.rot };
    let (p, y, rl) = angles(&r);
    float([p, y, rl][which])
}

fn light_of(h: &mut Host3D, e: i32) -> Result<(fyrox::core::pool::Handle<fyrox::scene::node::Node>, LightKind), String> {
    match h.ent(e)?.kind {
        Kind::Light { kind, light } => Ok((light, kind)),
        _ => Err(format!("Entity {e} is not a light")),
    }
}

fn base_light_mut(
    graph: &mut fyrox::scene::graph::Graph,
    light: fyrox::core::pool::Handle<fyrox::scene::node::Node>,
    kind: LightKind,
) -> Option<&mut BaseLight> {
    let node = &mut graph[light];
    match kind {
        LightKind::Directional => node.cast_mut::<DirectionalLight>().map(|l| l.base_light_mut()),
        LightKind::Point => node.cast_mut::<PointLight>().map(|l| l.base_light_mut()),
        LightKind::Spot => node.cast_mut::<SpotLight>().map(|l| l.base_light_mut()),
    }
}

fn camera_mut<'h>(h: &'h mut Host3D, e: i32) -> Result<&'h mut Camera, String> {
    let node = match h.ent(e)?.kind {
        Kind::Camera { .. } => h.ent(e)?.node,
        _ => return Err(format!("Entity {e} is not a camera")),
    };
    h.graph()?[node].cast_mut::<Camera>().ok_or_else(|| "camera node missing".to_string())
}

fn font_of(h: &Host3D, f: i32) -> Result<FontInfo, String> {
    usize::try_from(f - 1)
        .ok()
        .and_then(|i| h.world.canvas.fonts.get(i))
        .and_then(Clone::clone)
        .ok_or_else(|| format!("Font {f} does not exist"))
}

fn graphics_mode(h: &mut Host3D, a: &[Value], ctx: &mut NativeCtx) -> Result<Value, String> {
    let (w, ht) = (a[0].int().max(1) as u32, a[1].int().max(1) as u32);
    if h.has_graphics() {
        h.world.pending_size = Some((w, ht));
    } else {
        h.world.width = w;
        h.world.height = ht;
        h.world.graphics_requested = true;
        ctx.request_yield();
    }
    VOID
}

/// Runtime constants (`LIGHT_*`, ...). Key constants live in [`crate::input`].
pub fn constants() -> Vec<(String, ConstVal)> {
    vec![
        ("LIGHT_DIRECTIONAL".into(), ConstVal::Int(1)),
        ("LIGHT_POINT".into(), ConstVal::Int(2)),
        ("LIGHT_SPOT".into(), ConstVal::Int(3)),
        ("PROJ_NONE".into(), ConstVal::Int(0)),
        ("PROJ_PERSPECTIVE".into(), ConstVal::Int(1)),
    ]
}

/// Command table. Order defines native ids (after the standard library).
pub static COMMANDS: &[(&str, Cmd)] = &[
    // ---- graphics / frame
    ("Graphics3D%width%height%depth=0%mode=0", graphics_mode),
    ("Graphics%width%height%depth=0%mode=0", graphics_mode),
    ("EndGraphics", |_, _, _| VOID),
    ("AppTitle$title$close_prompt=\"\"", |h, a, _| {
        let t = a[0].str().to_string();
        if h.has_graphics() {
            h.world.pending_title = Some(t.clone());
        }
        h.world.title = t;
        VOID
    }),
    ("%GraphicsWidth", |h, _, _| int(h.world.width as i32)),
    ("%GraphicsHeight", |h, _, _| int(h.world.height as i32)),
    ("UpdateWorld#elapsed_time=1", |h, _, _| {
        h.scene()?;
        VOID
    }),
    ("RenderWorld#tween=1", |h, _, ctx| {
        h.scene()?;
        // A program that renders repeatedly without Flip still needs frames shown.
        if h.world.renders > h.world.renders_at_flip {
            h.world.canvas.flip();
            h.world.renders_at_flip = h.world.renders;
            ctx.request_yield();
        }
        // the 3D render covers whatever 2D was drawn before it
        h.world.canvas.items.clear();
        // keep camera aspect in sync with the window
        let cams: Vec<i32> = h.entities().filter(|(_, e)| matches!(e.kind, Kind::Camera { .. })).map(|(i, _)| i).collect();
        for c in cams {
            h.update_camera_fov(c)?;
        }
        h.world.renders += 1;
        VOID
    }),
    ("Flip%vwait=1", |h, _, ctx| {
        h.scene()?;
        // Flip ends the frame: publish the 2D layer and let the engine render.
        h.world.canvas.flip();
        h.world.renders_at_flip = h.world.renders;
        ctx.request_yield();
        VOID
    }),
    ("AmbientLight#red#green#blue", |h, a, _| {
        let c = color(a, 0);
        h.world.ambient = c;
        h.scene()?.rendering_options.get_value_mut_silent().ambient_lighting_color = c;
        VOID
    }),
    ("WireFrame%enable", |h, a, _| {
        use fyrox::graphics::PolygonFillMode;
        h.scene()?.rendering_options.get_value_mut_silent().polygon_rasterization_mode =
            if a[0].int() != 0 { PolygonFillMode::Line } else { PolygonFillMode::Fill };
        VOID
    }),
    // ---- entity creation
    ("%CreatePivot%parent=0", |h, a, _| {
        h.scene()?;
        int(h.add_entity(pivot_node(), Kind::Pivot, a[0].int())?)
    }),
    ("%CreateCube%parent=0", |h, a, _| create(h, "cube", 8, true, a[0].int())),
    ("%CreateSphere%segments=8%parent=0", |h, a, _| create(h, "sphere", a[0].int(), true, a[1].int())),
    ("%CreateCylinder%segments=8%solid=1%parent=0", |h, a, _| create(h, "cylinder", a[0].int(), a[1].int() != 0, a[2].int())),
    ("%CreateCone%segments=8%solid=1%parent=0", |h, a, _| create(h, "cone", a[0].int(), a[1].int() != 0, a[2].int())),
    ("%CreatePlane%segments=1%parent=0", |h, a, _| create(h, "plane", a[0].int(), true, a[1].int())),
    ("%CreateCamera%parent=0", |h, a, _| {
        h.scene()?;
        let id = h.add_entity(camera_node(), Kind::Camera { zoom: 1.0 }, a[0].int())?;
        h.update_camera_fov(id)?;
        int(id)
    }),
    ("%CreateLight%type=1%parent=0", |h, a, _| {
        h.scene()?;
        let kind = match a[0].int() {
            2 => LightKind::Point,
            3 => LightKind::Spot,
            _ => LightKind::Directional,
        };
        let light = h.graph()?.add_node(light_node(kind));
        let id = h.add_entity(pivot_node(), Kind::Light { kind, light }, a[1].int())?;
        let pivot = h.ent(id)?.node;
        h.graph()?.link_nodes(light, pivot);
        int(id)
    }),
    ("FreeEntity%entity", |h, a, _| {
        h.free_entity(a[0].int())?;
        VOID
    }),
    // ---- hierarchy
    ("EntityParent%entity%parent%global=1", |h, a, _| {
        h.set_parent(a[0].int(), a[1].int(), a[2].int() != 0)?;
        VOID
    }),
    ("%GetParent%entity", |h, a, _| int(h.ent(a[0].int())?.parent as i32)),
    ("%CountChildren%entity", |h, a, _| int(h.ent(a[0].int())?.children.len() as i32)),
    ("%GetChild%entity%index", |h, a, _| {
        let e = h.ent(a[0].int())?;
        let i = a[1].int();
        int(if i >= 1 { e.children.get(i as usize - 1).map_or(0, |&c| c as i32) } else { 0 })
    }),
    // ---- transforms
    ("PositionEntity%entity#x#y#z%global=0", |h, a, _| {
        let (e, p) = (a[0].int(), v3(a, 1));
        if a[4].int() != 0 {
            set_world_position(h, e, p)?;
        } else {
            h.set_xf(e, |xf| xf.pos = p)?;
        }
        VOID
    }),
    ("MoveEntity%entity#x#y#z", |h, a, _| {
        let d = v3(a, 1);
        h.set_xf(a[0].int(), |xf| xf.pos += xf.rot * d)?;
        VOID
    }),
    ("TranslateEntity%entity#x#y#z%global=0", |h, a, _| {
        let (e, d) = (a[0].int(), v3(a, 1));
        if a[4].int() != 0 {
            let p = h.world_of(e)?.position() + d;
            set_world_position(h, e, p)?;
        } else {
            h.set_xf(e, |xf| xf.pos += d)?;
        }
        VOID
    }),
    ("RotateEntity%entity#pitch#yaw#roll%global=0", |h, a, _| {
        let (e, r) = (a[0].int(), rotation(a[1].float(), a[2].float(), a[3].float()));
        if a[4].int() != 0 {
            set_world_rotation(h, e, r)?;
        } else {
            h.set_xf(e, |xf| xf.rot = r)?;
        }
        VOID
    }),
    ("TurnEntity%entity#pitch#yaw#roll%global=0", |h, a, _| {
        let (e, r) = (a[0].int(), rotation(a[1].float(), a[2].float(), a[3].float()));
        if a[4].int() != 0 {
            let w = h.world_of(e)?.rot;
            set_world_rotation(h, e, r * w)?;
        } else {
            h.set_xf(e, |xf| xf.rot = orthonormalize(&(xf.rot * r)))?;
        }
        VOID
    }),
    ("ScaleEntity%entity#x_scale#y_scale#z_scale%global=0", |h, a, _| {
        let (e, s) = (a[0].int(), v3(a, 1));
        let s = if a[4].int() != 0 { s.component_div(&h.parent_world(e)?.scale) } else { s };
        h.set_xf(e, |xf| xf.scale = s)?;
        VOID
    }),
    ("PointEntity%entity%target#roll=0", |h, a, _| {
        let t = h.world_of(a[1].int())?.position();
        look_at(h, a[0].int(), t, a[2].float())
    }),
    // (ext) point an entity at a world position
    ("LookAt%entity#x#y#z#roll=0", |h, a, _| look_at(h, a[0].int(), v3(a, 1), a[4].float())),
    ("CameraLookAt%camera#x#y#z#roll=0", |h, a, _| look_at(h, a[0].int(), v3(a, 1), a[4].float())),
    ("#EntityX%entity%global=0", |h, a, _| coord(h, a, 0)),
    ("#EntityY%entity%global=0", |h, a, _| coord(h, a, 1)),
    ("#EntityZ%entity%global=0", |h, a, _| coord(h, a, 2)),
    ("#EntityPitch%entity%global=0", |h, a, _| angle(h, a, 0)),
    ("#EntityYaw%entity%global=0", |h, a, _| angle(h, a, 1)),
    ("#EntityRoll%entity%global=0", |h, a, _| angle(h, a, 2)),
    ("#EntityDistance%source_entity%destination_entity", |h, a, _| {
        let p = h.world_of(a[0].int())?.position();
        let q = h.world_of(a[1].int())?.position();
        float((p - q).norm())
    }),
    ("NameEntity%entity$name", |h, a, _| {
        h.ent_mut(a[0].int())?.name = a[1].str().to_string();
        VOID
    }),
    ("$EntityName%entity", |h, a, _| Ok(Value::Str(h.ent(a[0].int())?.name.as_str().into()))),
    ("HideEntity%entity", |h, a, _| {
        let e = a[0].int();
        h.ent_mut(e)?.hidden = true;
        let n = h.ent(e)?.node;
        h.graph()?[n].set_visibility(false);
        VOID
    }),
    ("ShowEntity%entity", |h, a, _| {
        let e = a[0].int();
        h.ent_mut(e)?.hidden = false;
        let n = h.ent(e)?.node;
        h.graph()?[n].set_visibility(true);
        VOID
    }),
    ("%EntityHidden%entity", |h, a, _| int(h.ent(a[0].int())?.hidden as i32)), // (ext)
    // ---- materials
    ("EntityColor%entity#red#green#blue", |h, a, _| {
        let e = a[0].int();
        let c = color(a, 1);
        let alpha = h.ent(e)?.color.a;
        let c = Color::from_rgba(c.r, c.g, c.b, alpha);
        h.ent_mut(e)?.color = c;
        h.with_materials(e, |m| m.set_property("diffuseColor", c))?;
        VOID
    }),
    ("EntityAlpha%entity#alpha", |h, a, _| {
        let e = a[0].int();
        let alpha = (a[1].float().clamp(0.0, 1.0) * 255.0) as u8;
        let c = h.ent(e)?.color;
        let c = Color::from_rgba(c.r, c.g, c.b, alpha);
        h.ent_mut(e)?.color = c;
        h.with_materials(e, |m| m.set_property("diffuseColor", c))?;
        let n = h.ent(e)?.node;
        if let Some(mesh) = h.graph()?[n].cast_mut::<fyrox::scene::mesh::Mesh>() {
            mesh.set_render_path(if alpha < 255 { RenderPath::Forward } else { RenderPath::Deferred });
        }
        VOID
    }),
    // ---- cameras
    ("CameraClsColor%camera#red#green#blue", |h, a, _| {
        camera_mut(h, a[0].int())?;
        let c = color(a, 1);
        h.scene()?.rendering_options.get_value_mut_silent().clear_color = Some(c);
        VOID
    }),
    ("CameraRange%camera#near#far", |h, a, _| {
        let cam = camera_mut(h, a[0].int())?;
        let (n, f) = (a[1].float().max(0.0001), a[2].float());
        cam.projection_mut().set_z_near(n);
        cam.projection_mut().set_z_far(f.max(n + 0.001));
        VOID
    }),
    ("CameraZoom%camera#zoom", |h, a, _| {
        let e = a[0].int();
        camera_mut(h, e)?;
        h.ent_mut(e)?.kind = Kind::Camera { zoom: a[1].float() };
        h.update_camera_fov(e)?;
        VOID
    }),
    ("CameraViewport%camera%x%y%width%height", |h, a, _| {
        let (w, ht) = (h.world.width.max(1) as f32, h.world.height.max(1) as f32);
        let r = Rect::new(a[1].float() / w, a[2].float() / ht, a[3].float() / w, a[4].float() / ht);
        camera_mut(h, a[0].int())?.set_viewport(r);
        VOID
    }),
    ("CameraProjMode%camera%mode", |h, a, _| {
        let cam = camera_mut(h, a[0].int())?;
        match a[1].int() {
            0 => {
                cam.set_enabled(false);
            }
            1 => {
                cam.set_enabled(true);
                if !matches!(cam.projection(), Projection::Perspective(_)) {
                    cam.set_projection(Projection::Perspective(Default::default()));
                }
            }
            m => return Err(format!("CameraProjMode {m} is not supported yet (use 0 or 1)")),
        }
        VOID
    }),
    // ---- lights
    ("LightColor%light#red#green#blue", |h, a, _| {
        let (light, kind) = light_of(h, a[0].int())?;
        // Blitz allows values above 255 for over-bright lights
        let peak = a[1].float().max(a[2].float()).max(a[3].float()).max(1.0);
        let scale = if peak > 255.0 { 255.0 / peak } else { 1.0 };
        let c = |v: &Value| (v.float().max(0.0) * scale) as u8;
        let col = Color::opaque(c(&a[1]), c(&a[2]), c(&a[3]));
        if let Some(l) = base_light_mut(h.graph()?, light, kind) {
            l.set_color(col);
            l.set_intensity((peak / 255.0).max(1.0));
        }
        VOID
    }),
    ("LightRange%light#range", |h, a, _| {
        let (light, kind) = light_of(h, a[0].int())?;
        let r = a[1].float().max(0.0);
        let graph = h.graph()?;
        match kind {
            LightKind::Point => {
                if let Some(p) = graph[light].cast_mut::<PointLight>() {
                    p.set_radius(r);
                }
            }
            LightKind::Spot => {
                if let Some(s) = graph[light].cast_mut::<SpotLight>() {
                    s.set_distance(r);
                }
            }
            LightKind::Directional => {}
        }
        VOID
    }),
    ("LightConeAngles%light#inner_angle#outer_angle", |h, a, _| {
        let (light, _) = light_of(h, a[0].int())?;
        let (inner, outer) = (a[1].float().max(0.0), a[2].float().max(0.0));
        if let Some(s) = h.graph()?[light].cast_mut::<SpotLight>() {
            s.set_hotspot_cone_angle(inner.to_radians());
            s.set_falloff_angle_delta((outer - inner).max(0.0).to_radians());
        }
        VOID
    }),
    // (ext) real-time shadows per light
    ("LightShadows%light%enable", |h, a, _| {
        let (light, _) = light_of(h, a[0].int())?;
        h.graph()?[light].set_cast_shadows(a[1].int() != 0);
        VOID
    }),
    // ---- 2D (bbgraphics.cpp)
    ("Text%x%y$string%center_x=0%center_y=0", |h, a, _| {
        h.scene()?;
        h.world.canvas.text(a[0].float(), a[1].float(), &a[2].str(), a[3].int() != 0, a[4].int() != 0);
        VOID
    }),
    ("Color%red%green%blue", |h, a, _| {
        h.world.canvas.color = color(a, 0);
        VOID
    }),
    ("%ColorRed", |h, _, _| int(h.world.canvas.color.r as i32)),
    ("%ColorGreen", |h, _, _| int(h.world.canvas.color.g as i32)),
    ("%ColorBlue", |h, _, _| int(h.world.canvas.color.b as i32)),
    ("ClsColor%red%green%blue", |h, a, _| {
        h.world.canvas.cls_color = color(a, 0);
        VOID
    }),
    ("Cls", |h, _, _| {
        h.world.canvas.items.clear();
        // without a camera, Cls is what defines the background
        if !h.entities().any(|(_, e)| matches!(e.kind, Kind::Camera { .. })) {
            let c = h.world.canvas.cls_color;
            h.scene()?.rendering_options.get_value_mut_silent().clear_color = Some(c);
        }
        VOID
    }),
    ("%LoadFont$fontname%height=12%bold=0%italic=0%underline=0", |h, a, _| {
        match load_font(&a[0].str(), a[1].float(), a[2].int() != 0, a[3].int() != 0) {
            Ok(f) => {
                let fonts = &mut h.world.canvas.fonts;
                fonts.push(Some(f));
                int(fonts.len() as i32)
            }
            Err(_) => int(0), // Blitz returns 0 when a font can't be loaded
        }
    }),
    ("SetFont%font", |h, a, _| {
        // (ext) font 0 selects the built-in font
        let f = if a[0].int() == 0 { FontInfo::default_font() } else { font_of(h, a[0].int())? };
        h.world.canvas.font = f;
        VOID
    }),
    ("FreeFont%font", |h, a, _| {
        font_of(h, a[0].int())?;
        h.world.canvas.fonts[a[0].int() as usize - 1] = None;
        VOID
    }),
    ("%FontWidth", |h, _, _| int(h.world.canvas.font.width("W").ceil() as i32)),
    ("%FontHeight", |h, _, _| int(h.world.canvas.font.height() as i32)),
    ("%StringWidth$string", |h, a, _| int(h.world.canvas.font.width(&a[0].str()).ceil() as i32)),
    ("%StringHeight$string", |h, _, _| int(h.world.canvas.font.height() as i32)),
    // ---- input (bbinput.cpp)
    ("%KeyDown%key", |h, a, _| int(h.input.key_down(a[0].int()) as i32)),
    ("%KeyHit%key", |h, a, _| int(h.input.key_hit(a[0].int()))),
    ("%GetKey", |h, _, _| int(h.input.get_key())),
    ("FlushKeys", |h, _, _| {
        h.input.flush_keys();
        VOID
    }),
    ("%MouseX", |h, _, _| int(h.input.mouse_x as i32)),
    ("%MouseY", |h, _, _| int(h.input.mouse_y as i32)),
    ("%MouseZ", |h, _, _| int(h.input.mouse_z as i32)),
    ("%MouseDown%button", |h, a, _| int(h.input.mouse_down(a[0].int()) as i32)),
    ("%MouseHit%button", |h, a, _| int(h.input.mouse_hit(a[0].int()))),
    ("%MouseXSpeed", |h, _, _| int(h.input.mouse_speed_x() as i32)),
    ("%MouseYSpeed", |h, _, _| int(h.input.mouse_speed_y() as i32)),
    ("FlushMouse", |h, _, _| {
        h.input.flush_mouse();
        VOID
    }),
];
