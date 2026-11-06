//! 3D runtime commands. Signatures follow Blitz3D's `bbblitz3d.cpp` /
//! `bbgraphics.cpp` / `bbinput.cpp` declarations; NeptuneBASIC additions
//! are marked `(ext)`.

use fyrox::core::{algebra::Vector3, color::Color, math::Rect};
use fyrox::graph::SceneGraph;
use fyrox::scene::{
    camera::Camera,
    light::{directional::DirectionalLight, point::PointLight, spot::SpotLight, BaseLight},
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

/// Clear colours are written to the linear HDR frame; programs give sRGB.
fn srgb_to_linear(c: Color) -> Color {
    let f = |v: u8| {
        let x = v as f32 / 255.0;
        let l = if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
        (l * 255.0).round() as u8
    };
    Color::from_rgba(f(c.r), f(c.g), f(c.b), c.a)
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

fn mesh_extent(h: &mut Host3D, e: i32, axis: usize) -> Result<Value, String> {
    let tris = h.mesh_triangles(e)?;
    float(crate::model::bounds(&tris).map_or(0.0, |(lo, hi)| hi[axis] - lo[axis]))
}

fn nth_contact(h: &Host3D, a: &[Value]) -> Result<crate::collision::Contact, String> {
    let (e, i) = (a[0].int(), a[1].int());
    let list = &h.ent(e)?.contacts;
    usize::try_from(i - 1)
        .ok()
        .and_then(|i| list.get(i))
        .copied()
        .ok_or_else(|| format!("collision index {i} out of range (entity {e} has {} collisions)", list.len()))
}

fn contact(h: &Host3D, a: &[Value], f: impl Fn(&crate::collision::Contact) -> f32) -> Result<Value, String> {
    float(f(&nth_contact(h, a)?))
}

/// World-space collision shape of a destination entity for a method.
fn collision_shape(h: &mut Host3D, e: i32, method: i32) -> Result<crate::collision::Shape, String> {
    use crate::collision::{box_triangles, Shape};
    let w = h.world_of(e)?;
    let to_world = |p: Vector3<f32>| w.matrix.transform_point(&p.into()).coords;
    Ok(match method {
        1 => Shape::Sphere { center: w.position(), radius: h.ent(e)?.radius.0 },
        3 => {
            let b = match h.ent(e)?.col_box {
                Some(b) => b,
                None => {
                    // default to the mesh bounds (or a 2x2x2 box for non-meshes)
                    let tris = h.mesh_triangles(e)?;
                    match crate::model::bounds(&tris) {
                        Some((lo, hi)) => [lo.x, lo.y, lo.z, hi.x - lo.x, hi.y - lo.y, hi.z - lo.z],
                        None => [-1.0, -1.0, -1.0, 2.0, 2.0, 2.0],
                    }
                }
            };
            Shape::Triangles(box_triangles(b, to_world))
        }
        _ => Shape::Triangles(h.mesh_triangles(e)?.into_iter().map(|t| t.map(to_world)).collect()),
    })
}

/// `UpdateWorld`: moves every collision source from where it was at the
/// previous update to where it is now, resolving collisions on the way.
fn update_world(h: &mut Host3D) -> Result<(), String> {
    use crate::collision::{collide_and_slide, Obstacle, Shape};
    let rules = h.world.collision_rules.clone();
    let typed: Vec<(i32, i32, bool)> =
        h.entities().filter(|(_, e)| e.col_type != 0).map(|(i, e)| (i, e.col_type, e.hidden)).collect();
    for (_, e) in h.entities_mut() {
        e.contacts.clear();
    }
    let mut shape_cache: std::collections::HashMap<(i32, i32), Shape> = Default::default();
    for &(src, src_type, hidden) in &typed {
        let my_rules: Vec<_> = rules.iter().filter(|r| r.0 == src_type).collect();
        if my_rules.is_empty() || hidden {
            continue;
        }
        let now = h.world_of(src)?.position();
        let start = h.ent(src)?.last_pos.unwrap_or(now);
        if (now - start).norm_squared() < 1e-12 {
            continue;
        }
        let mut obstacles = Vec::new();
        for r in my_rules {
            for &(dst, dst_type, dst_hidden) in &typed {
                if dst == src || dst_type != r.1 || dst_hidden {
                    continue;
                }
                let shape = match shape_cache.get(&(dst, r.2)) {
                    Some(s) => s.clone(),
                    None => {
                        let s = collision_shape(h, dst, r.2)?;
                        shape_cache.insert((dst, r.2), s.clone());
                        s
                    }
                };
                obstacles.push(Obstacle { entity: dst, response: r.3, shape });
            }
        }
        let (rx, ry) = h.ent(src)?.radius;
        let (end, contacts) = collide_and_slide(start, now, Vector3::new(rx, ry, rx), &obstacles);
        if !contacts.is_empty() {
            set_world_position(h, src, end)?;
            // the source moved: cached shapes of it are stale
            shape_cache.retain(|k, _| k.0 != src);
        }
        h.ent_mut(src)?.contacts = contacts;
    }
    for &(e, _, _) in &typed {
        let p = h.world_of(e)?.position();
        h.ent_mut(e)?.last_pos = Some(p);
    }
    Ok(())
}

/// Picks along `origin .. origin + line` against pickable entities.
/// `exclude` entities are ignored; `obscurers_only` restricts to obscurers.
fn pick_line(
    h: &mut Host3D,
    origin: Vector3<f32>,
    line: Vector3<f32>,
    radius: f32,
    exclude: &[i32],
    obscurers_only: bool,
) -> Result<Option<crate::pick::PickHit>, String> {
    h.flush_meshes()?;
    let candidates: Vec<(i32, i32)> = h
        .entities()
        .filter(|(i, e)| e.pick_mode != 0 && !e.hidden && !exclude.contains(i) && (!obscurers_only || e.obscurer))
        .map(|(i, e)| (i, e.pick_mode))
        .collect();
    let mut targets = Vec::with_capacity(candidates.len());
    for (e, mode) in candidates {
        targets.push((e, collision_shape(h, e, mode)?));
    }
    Ok(crate::pick::pick(origin, line, radius, &targets))
}

fn picked(h: &mut Host3D, hit: Option<crate::pick::PickHit>) -> Result<Value, String> {
    h.world.picked = hit;
    int(hit.map_or(0, |p| p.entity))
}

/// Camera parameters for picking and projection.
struct CamParams {
    w: crate::transform::WorldXform,
    /// Half extents of the view: tan(half fov) for perspective, world units for orthographic.
    tx: f32,
    ty: f32,
    far: f32,
    ortho: bool,
}

fn camera_params(h: &mut Host3D, cam: i32) -> Result<CamParams, String> {
    let Kind::Camera { zoom, mode } = h.ent(cam)?.kind else { return Err(format!("Entity {cam} is not a camera")) };
    let far = camera_mut(h, cam)?.projection().z_far();
    let aspect = h.world.width as f32 / h.world.height.max(1) as f32;
    let tx = 1.0 / zoom.max(1e-6);
    Ok(CamParams { w: h.world_of(cam)?, tx, ty: tx / aspect, far, ortho: mode == 2 })
}

/// World-space ray through a screen pixel, as origin and full-length line.
fn camera_ray(h: &mut Host3D, cam: i32, sx: f32, sy: f32) -> Result<(Vector3<f32>, Vector3<f32>), String> {
    let c = camera_params(h, cam)?;
    let (sw, sh) = (h.world.width as f32, h.world.height as f32);
    let (nx, ny) = ((2.0 * sx / sw - 1.0) * c.tx, (1.0 - 2.0 * sy / sh) * c.ty);
    if c.ortho {
        let origin = c.w.position() + c.w.rot * Vector3::new(nx, ny, 0.0);
        Ok((origin, c.w.rot * Vector3::z() * c.far))
    } else {
        Ok((c.w.position(), c.w.rot * Vector3::new(nx, ny, 1.0) * c.far))
    }
}

/// Screen position of a world point, or `None` if it is behind the camera.
fn camera_project(h: &mut Host3D, cam: i32, p: Vector3<f32>) -> Result<Option<(f32, f32, f32)>, String> {
    let c = camera_params(h, cam)?;
    let v = c.w.rot.transpose() * (p - c.w.position());
    if v.z <= 1e-4 {
        return Ok(None);
    }
    let (sw, sh) = (h.world.width as f32, h.world.height as f32);
    let (px, py) = if c.ortho { (v.x / c.tx, v.y / c.ty) } else { (v.x / (v.z * c.tx), v.y / (v.z * c.ty)) };
    Ok(Some((sw / 2.0 * (1.0 + px), sh / 2.0 * (1.0 - py), v.z)))
}

fn add_image(h: &mut Host3D, img: Result<crate::image2d::Image2D, String>) -> Result<Value, String> {
    match img {
        Ok(mut img) => {
            if h.world.auto_mid_handle {
                img.handle = ((img.frame_w / 2) as f32, (img.frame_h / 2) as f32);
            }
            h.world.images.push(Some(img));
            int(h.world.images.len() as i32)
        }
        Err(e) => {
            eprintln!("{e}");
            int(0) // Blitz returns 0 when an image can't be loaded
        }
    }
}

/// `DrawImage`/`DrawBlock`; args are image, x, y, frame.
fn draw_image(h: &mut Host3D, a: &[Value], masked: bool, sub: Option<Rect<f32>>) -> Result<Value, String> {
    if h.image(a[0].int())?.dirty {
        h.image_mut(a[0].int())?.commit();
    }
    let target = h.current_buffer()?;
    if target != crate::buffer::Buffer::Screen {
        return blit_image(h, a, masked, sub, target);
    }
    let img = h.image(a[0].int())?;
    let uv = img.uv(a[3].int().max(0) as u32, sub)?;
    let (x, y) = img.top_left(a[1].float(), a[2].float());
    let (w, ht) = match sub {
        Some(r) => (r.w() * img.scale.0, r.h() * img.scale.1),
        None => img.size(),
    };
    let tex = if masked { img.masked.clone() } else { img.block.clone() };
    h.world.canvas.image(x, y, w, ht, uv, tex);
    VOID
}

/// Pushes changed fog settings into every classic material.
fn refresh_fog(h: &mut Host3D) -> Result<Value, String> {
    if h.world.classic.fog_mode != 0 {
        h.refresh_all_materials()?;
    }
    VOID
}

/// Changes a texture's settings and refreshes every entity that uses it.
fn modify_texture(h: &mut Host3D, t: i32, f: impl FnOnce(&mut crate::texture::TextureInfo)) -> Result<Value, String> {
    h.texture(t)?;
    if let Some(Some(info)) = h.world.textures.get_mut(t as usize - 1) {
        f(info);
    }
    let users: Vec<i32> = h.entities().filter(|(_, e)| e.textures.contains(&t) || e.normal_map == t).map(|(i, _)| i).collect();
    for e in users {
        h.refresh_material(e)?;
    }
    VOID
}

fn vfloat(h: &Host3D, a: &[Value], f: impl Fn(&crate::mesh::Vert) -> f32) -> Result<Value, String> {
    float(f(&h.vertex(a[0].int(), a[1].int())?))
}

fn coord_set(a: &[Value], i: usize) -> usize {
    a[i].int().clamp(0, 1) as usize
}

/// Sets one brush field and refreshes the surfaces painted with it.
fn modify_brush(h: &mut Host3D, b: i32, f: impl FnOnce(&mut crate::mesh::Brush)) -> Result<Value, String> {
    f(h.brush_mut(b)?);
    for e in h.brush_users(b) {
        h.refresh_material(e)?;
    }
    VOID
}

/// Copies a brush's settings onto an entity (`PaintEntity`/`PaintMesh`).
fn paint_entity(h: &mut Host3D, e: i32, b: i32) -> Result<Value, String> {
    let brush = h.brush(b)?.clone();
    let ent = h.ent_mut(e)?;
    ent.color = brush.color;
    ent.shininess = brush.shininess;
    ent.textures = brush.textures;
    ent.blend = brush.blend;
    ent.fx = brush.fx;
    h.refresh_material(e)?;
    VOID
}

/// Copies the surfaces of `src` into mesh `dst`.
fn append_surfaces(h: &mut Host3D, src: i32, dst: i32) -> Result<(), String> {
    for s in h.all_surfaces(src)? {
        let info = h.surface(s)?.clone();
        let ns = h.create_surface(dst, info.brush)?;
        h.edit_surface(ns, |d| {
            d.verts = info.verts;
            d.tris = info.tris;
        })?;
    }
    Ok(())
}

fn load_sound(h: &mut Host3D, path: &str, spatial: bool) -> Result<Value, String> {
    match crate::sound::load(path, false) {
        Ok((buffer, rate)) => {
            let info = crate::sound::SoundInfo { buffer, looping: false, volume: 1.0, pan: 0.0, pitch: 1.0, rate, spatial };
            int(h.add_sound(info))
        }
        Err(e) => {
            eprintln!("{e}");
            int(0) // Blitz returns 0 when a sound can't be loaded
        }
    }
}

fn with_channel(h: &mut Host3D, c: i32, f: impl FnOnce(&mut fyrox::scene::sound::Sound)) -> Result<Value, String> {
    if let Some(ch) = h.channel(c)? {
        f(ch);
    }
    VOID
}

fn body(h: &mut Host3D, e: i32, f: impl FnOnce(&mut fyrox::scene::rigidbody::RigidBody)) -> Result<Value, String> {
    f(h.body_mut(e)?);
    VOID
}

fn velocity(h: &mut Host3D, e: i32, angular: bool, axis: usize) -> Result<Value, String> {
    use crate::physics::{angular_to_fyrox, vec_to_fyrox};
    let b = h.body_mut(e)?;
    // both conversions are their own inverse
    let v = if angular { angular_to_fyrox(b.ang_vel()) } else { vec_to_fyrox(b.lin_vel()) };
    float(v[axis])
}

/// `DrawImage`/`DrawBlock` into an image or texture buffer.
fn blit_image(h: &mut Host3D, a: &[Value], masked: bool, sub: Option<Rect<f32>>, target: crate::buffer::Buffer) -> Result<Value, String> {
    let img = h.image(a[0].int())?;
    let frame = a[3].int().max(0) as u32;
    let (fx, fy, fw, fh) = img.frame_rect(frame)?;
    let (sx, sy, w, ht) = match sub {
        Some(r) => (fx + r.x() as i32, fy + r.y() as i32, r.w() as i32, r.h() as i32),
        None => (fx, fy, fw, fh),
    };
    let (x, y) = img.top_left(a[1].float(), a[2].float());
    let (ox, oy) = h.world.canvas.origin;
    let mask = masked.then(|| img.mask());
    let src = img.pixels().clone();
    h.with_target(target, |t| t.blit(&src, sx, sy, w, ht, (x + ox) as i32, (y + oy) as i32, mask))?;
    VOID
}

/// Runs a 2D shape on the current buffer: the screen layer or software pixels.
fn draw2d(h: &mut Host3D, screen: impl FnOnce(&mut crate::draw2d::Canvas2D), pixels: impl FnOnce(&mut crate::buffer::Target, image::Rgba<u8>, (i32, i32))) -> Result<Value, String> {
    let target = h.current_buffer()?;
    if target == crate::buffer::Buffer::Screen {
        h.scene()?;
        screen(&mut h.world.canvas);
    } else {
        let c = h.world.canvas.color;
        let color = image::Rgba([c.r, c.g, c.b, 255]);
        let (ox, oy) = h.world.canvas.origin;
        h.with_target(target, |t| pixels(t, color, (ox as i32, oy as i32)))?;
    }
    VOID
}

/// Resolves an optional buffer argument (0 = the current buffer).
fn buffer_arg(h: &Host3D, b: i32) -> Result<crate::buffer::Buffer, String> {
    if b == 0 {
        h.current_buffer()
    } else {
        crate::buffer::decode(b)
    }
}

/// `TileImage`/`TileBlock`: repeats an image over the whole screen.
/// Flags for loading a texture: the program's flags plus matching `TextureFilter`s.
fn texture_flags(h: &Host3D, file: &str, flags: i32) -> i32 {
    let name = file.to_lowercase();
    h.world.texture_filters.iter().filter(|(m, _)| name.contains(m.as_str())).fold(flags, |f, (_, add)| f | add)
}

fn tile_image(h: &mut Host3D, a: &[Value], masked: bool) -> Result<Value, String> {
    h.scene()?;
    let img = h.image(a[0].int())?;
    let uv = img.uv(a[3].int().max(0) as u32, None)?;
    let (w, ht) = img.size();
    if w < 1.0 || ht < 1.0 {
        return VOID;
    }
    let tex = if masked { img.masked.clone() } else { img.block.clone() };
    let (sw, sh) = (h.world.width as f32, h.world.height as f32);
    let (ox, oy) = (a[1].float().rem_euclid(w) - w, a[2].float().rem_euclid(ht) - ht);
    let mut y = oy;
    while y < sh {
        let mut x = ox;
        while x < sw {
            h.world.canvas.image(x - h.world.canvas.origin.0, y - h.world.canvas.origin.1, w, ht, uv, tex.clone());
            x += w;
        }
        y += ht;
    }
    VOID
}

fn copy_pixel(h: &mut Host3D, a: &[Value]) -> Result<Value, String> {
    let src = crate::buffer::decode(a[2].int())?;
    let dst = buffer_arg(h, a[5].int())?;
    if src == crate::buffer::Buffer::Screen {
        return Err("CopyPixel cannot read from the screen".into());
    }
    let (sx, sy, dx, dy) = (a[0].int(), a[1].int(), a[3].int(), a[4].int());
    let p = h.with_target(src, |t| t.get(sx, sy))?;
    if dst == crate::buffer::Buffer::Screen {
        let old = h.world.canvas.color;
        h.world.canvas.color = Color::opaque(p[0], p[1], p[2]);
        h.world.canvas.rect(dx as f32, dy as f32, 1.0, 1.0, true);
        h.world.canvas.color = old;
    } else {
        h.with_target(dst, |t| t.plot(dx, dy, p))?;
    }
    VOID
}

fn buffer_size(h: &mut Host3D, b: i32, width: bool) -> Result<Value, String> {
    let (w, ht) = match crate::buffer::decode(b)? {
        crate::buffer::Buffer::Screen => (h.world.width as i32, h.world.height as i32),
        other => h.with_target(other, |t| (t.w, t.h))?,
    };
    int(if width { w } else { ht })
}

fn write_pixel(h: &mut Host3D, a: &[Value]) -> Result<Value, String> {
    let b = buffer_arg(h, a[3].int())?;
    let (x, y, c) = (a[0].int(), a[1].int(), crate::buffer::argb_to_rgba(a[2].int()));
    if b == crate::buffer::Buffer::Screen {
        h.scene()?;
        let old = h.world.canvas.color;
        h.world.canvas.color = Color::opaque(c[0], c[1], c[2]);
        h.world.canvas.rect(x as f32, y as f32, 1.0, 1.0, true);
        h.world.canvas.color = old;
    } else {
        h.with_target(b, |t| t.plot(x, y, c))?;
    }
    VOID
}

/// Result of the last `TFormPoint`/`TFormVector`/`TFormNormal`.
fn tform(h: &mut Host3D, a: &[Value], kind: u8) -> Result<Value, String> {
    let v = v3(a, 0);
    let (src, dst) = (a[3].int(), a[4].int());
    let world = |h: &Host3D, e: i32| if e == 0 { Ok(crate::transform::WorldXform::identity()) } else { h.world_of(e) };
    let (ws, wd) = (world(h, src)?, world(h, dst)?);
    let out = match kind {
        // points: full affine transforms
        0 => {
            let p = ws.matrix.transform_point(&v.into()).coords;
            wd.to_local_point(&p)
        }
        // vectors: rotation and scale only
        1 => {
            let m = |w: &crate::transform::WorldXform| w.matrix.fixed_view::<3, 3>(0, 0).into_owned();
            let p = m(&ws) * v;
            m(&wd).try_inverse().map_or(p, |inv| inv * p)
        }
        // normals: the inverse-transpose, renormalised
        _ => {
            let m = |w: &crate::transform::WorldXform| w.matrix.fixed_view::<3, 3>(0, 0).into_owned();
            let to_world = m(&ws).try_inverse().map_or(v, |inv| inv.transpose() * v);
            let n = m(&wd).transpose() * to_world;
            n.try_normalize(1e-12).unwrap_or(n)
        }
    };
    h.world.tformed = out;
    VOID
}

/// `AlignToVector`: turns one local axis of an entity towards a world vector.
fn align_to_vector(h: &mut Host3D, a: &[Value]) -> Result<Value, String> {
    use fyrox::core::algebra::{Rotation3, Unit, UnitQuaternion};
    let e = a[0].int();
    let target = v3(a, 1);
    let Some(target) = target.try_normalize(1e-9) else { return VOID };
    let axis = match a[4].int() {
        1 => Vector3::x(),
        2 => Vector3::y(),
        _ => Vector3::z(),
    };
    let rate = a[5].float().clamp(0.0, 1.0);
    let rot = h.world_of(e)?.rot;
    let current = rot * axis;
    let delta = UnitQuaternion::rotation_between(&current, &target).unwrap_or_else(|| {
        // opposite directions: turn half a circle about any perpendicular axis
        let perp = current.cross(&Vector3::x()).try_normalize(1e-6).unwrap_or_else(|| current.cross(&Vector3::y()).normalize());
        UnitQuaternion::from_axis_angle(&Unit::new_normalize(perp), std::f32::consts::PI)
    });
    let delta = UnitQuaternion::identity().slerp(&delta, rate);
    let q = delta * UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(rot));
    set_world_rotation(h, e, q.to_rotation_matrix().into_inner())?;
    VOID
}

/// `EntityAutoFade` and `CaptureWorld` tweening, before each render.
fn before_render(h: &mut Host3D, tween: f32) -> Result<(), String> {
    // put tweened nodes back to their real transforms
    for e in std::mem::take(&mut h.world.tweened) {
        if h.ent(e).is_ok() {
            h.sync(e)?;
        }
    }
    if tween < 0.999 && !h.world.captured.is_empty() {
        use fyrox::core::algebra::{Rotation3, UnitQuaternion};
        let t = tween.clamp(0.0, 1.0);
        let captured: Vec<(i32, crate::transform::Xform)> = h.world.captured.iter().map(|(k, v)| (*k, *v)).collect();
        for (e, old) in captured {
            let Ok(ent) = h.ent(e) else { continue };
            if ent.phys.is_some() || ent.sprite.is_some() {
                continue;
            }
            let now = h.live_xf(ent);
            let q = |m: &fyrox::core::algebra::Matrix3<f32>| UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(*m));
            let mix = crate::transform::Xform {
                pos: old.pos.lerp(&now.pos, t),
                rot: q(&old.rot).slerp(&q(&now.rot), t).to_rotation_matrix().into_inner(),
                scale: old.scale.lerp(&now.scale, t),
            };
            let node = ent.node;
            let tr = h.graph()?[node].local_transform_mut();
            tr.set_position(mix.fyrox_position());
            tr.set_rotation(mix.fyrox_rotation());
            tr.set_scale(mix.scale);
            h.world.tweened.push(e);
        }
    }
    // auto-fade by distance from the first active camera
    let fading: Vec<(i32, (f32, f32))> = h.entities().filter_map(|(i, e)| e.auto_fade.map(|f| (i, f))).collect();
    if !fading.is_empty() {
        let cam = h.entities().find(|(_, e)| matches!(e.kind, Kind::Camera { mode, .. } if mode != 0)).map(|(i, _)| i);
        if let Some(cam) = cam {
            let cp = h.world_of(cam)?.position();
            for (e, (near, far)) in fading {
                let d = (h.world_of(e)?.position() - cp).norm();
                let k = if d <= near { 1.0 } else if d >= far { 0.0 } else { (far - d) / (far - near).max(1e-6) };
                let node = h.ent(e)?.node;
                let hidden = h.ent(e)?.hidden;
                h.graph()?[node].set_visibility(!hidden && k > 0.0);
                let base = h.ent(e)?.fade_base;
                let alpha = (base as f32 * k).round() as u8;
                if h.ent(e)?.color.a != alpha {
                    h.ent_mut(e)?.color.a = alpha;
                    h.refresh_material(e)?;
                }
            }
        }
    }
    Ok(())
}

/// World-space triangles of an entity's meshes.
fn world_triangles(h: &mut Host3D, e: i32) -> Result<Vec<[Vector3<f32>; 3]>, String> {
    let w = h.world_of(e)?;
    let tris = h.mesh_triangles(e)?;
    Ok(tris.into_iter().map(|t| t.map(|p| w.matrix.transform_point(&p.into()).coords)).collect())
}

/// Does segment `p..q` cross triangle `t`?
fn segment_hits_triangle(p: Vector3<f32>, q: Vector3<f32>, t: &[Vector3<f32>; 3]) -> bool {
    let d = q - p;
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let pv = d.cross(&e2);
    let det = e1.dot(&pv);
    if det.abs() < 1e-12 {
        return false;
    }
    let inv = 1.0 / det;
    let tv = p - t[0];
    let u = tv.dot(&pv) * inv;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let qv = tv.cross(&e1);
    let v = d.dot(&qv) * inv;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let s = e2.dot(&qv) * inv;
    (0.0..=1.0).contains(&s)
}

/// `MeshesIntersect`: does any triangle edge of one mesh pass through the other?
pub fn triangles_intersect(a: &[[Vector3<f32>; 3]], b: &[[Vector3<f32>; 3]]) -> bool {
    let bounds = |t: &[[Vector3<f32>; 3]]| crate::model::bounds(t);
    let (Some((alo, ahi)), Some((blo, bhi))) = (bounds(a), bounds(b)) else { return false };
    if (0..3).any(|i| ahi[i] < blo[i] || bhi[i] < alo[i]) {
        return false;
    }
    let edges_cross = |x: &[[Vector3<f32>; 3]], y: &[[Vector3<f32>; 3]]| {
        x.iter().any(|t| (0..3).any(|i| {
            let (p, q) = (t[i], t[(i + 1) % 3]);
            y.iter().any(|u| segment_hits_triangle(p, q, u))
        }))
    };
    edges_cross(a, b) || edges_cross(b, a)
}

/// `LightMesh`: adds a light's colour to vertex colours (fullbright-free baked lighting).
fn light_mesh(h: &mut Host3D, a: &[Value]) -> Result<Value, String> {
    let m = a[0].int();
    let col = [a[1].float() / 255.0, a[2].float() / 255.0, a[3].float() / 255.0];
    let (range, light) = (a[4].float(), v3(a, 5));
    for s in h.all_surfaces(m)? {
        h.edit_surface(s, |info| {
            for v in &mut info.verts {
                let k = if range <= 0.0 {
                    1.0
                } else {
                    let to = light - v.pos;
                    let d = to.norm();
                    let facing = if d > 1e-6 { (v.normal.dot(&(to / d))).max(0.0) } else { 1.0 };
                    (1.0 - d / range).max(0.0) * facing
                };
                for c in 0..3 {
                    v.color[c] = (v.color[c] + col[c] * k).min(1.0);
                }
            }
        })?;
    }
    VOID
}

fn font_of(h: &Host3D, f: i32) -> Result<FontInfo, String> {
    usize::try_from(f - 1)
        .ok()
        .and_then(|i| h.world.canvas.fonts.get(i))
        .and_then(Clone::clone)
        .ok_or_else(|| format!("Font {f} does not exist"))
}

/// Common screen sizes offered by `CountGfxModes` and friends. Windows can
/// be any size, so these are the usual choices rather than a hardware list.
const GFX_MODES: &[(i32, i32)] = &[
    (640, 480), (800, 600), (1024, 768), (1152, 864), (1280, 720), (1280, 800), (1280, 1024),
    (1366, 768), (1440, 900), (1600, 900), (1680, 1050), (1920, 1080), (1920, 1200), (2560, 1440),
    (2560, 1600), (3440, 1440), (3840, 2160),
];

fn gfx_mode(i: i32) -> Option<(i32, i32)> {
    usize::try_from(i - 1).ok().and_then(|i| GFX_MODES.get(i)).copied()
}

fn graphics_mode(h: &mut Host3D, a: &[Value], ctx: &mut NativeCtx) -> Result<Value, String> {
    let (w, ht) = (a[0].int().max(1) as u32, a[1].int().max(1) as u32);
    // Blitz modes: 0 auto (windowed here), 1 fullscreen, 2 windowed, 3 scaled window
    let fullscreen = a[3].int() == 1;
    if h.has_graphics() {
        if fullscreen != h.world.fullscreen {
            h.world.pending_fullscreen = Some(fullscreen);
        }
    }
    h.world.fullscreen = fullscreen;
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
        // (ext) short light names
        ("DIRECTIONAL".into(), ConstVal::Int(1)),
        ("POINT".into(), ConstVal::Int(2)),
        ("SPOT".into(), ConstVal::Int(3)),
        ("BLEND_ALPHA".into(), ConstVal::Int(1)),
        ("BLEND_MULTIPLY".into(), ConstVal::Int(2)),
        ("BLEND_ADD".into(), ConstVal::Int(3)),
        ("FX_FULLBRIGHT".into(), ConstVal::Int(1)),
        ("FX_VERTEXCOLOR".into(), ConstVal::Int(2)),
        ("FX_FLAT".into(), ConstVal::Int(4)),
        ("FX_NOFOG".into(), ConstVal::Int(8)),
        ("FX_TWOSIDED".into(), ConstVal::Int(16)),
        ("FX_FORCEALPHA".into(), ConstVal::Int(32)),
        ("QUALITY_LOW".into(), ConstVal::Int(0)),
        ("QUALITY_MEDIUM".into(), ConstVal::Int(1)),
        ("QUALITY_HIGH".into(), ConstVal::Int(2)),
        ("QUALITY_ULTRA".into(), ConstVal::Int(3)),
        ("EFFECT_BLOOM".into(), ConstVal::Int(1)),
        ("EFFECT_SSAO".into(), ConstVal::Int(2)),
        ("EFFECT_FXAA".into(), ConstVal::Int(3)),
        ("EFFECT_SHADOWS".into(), ConstVal::Int(4)),
        ("EFFECT_LIGHT_SCATTER".into(), ConstVal::Int(5)),
        ("PHYS_DYNAMIC".into(), ConstVal::Int(1)),
        ("PHYS_STATIC".into(), ConstVal::Int(2)),
        ("PHYS_KINEMATIC".into(), ConstVal::Int(3)),
        ("SHAPE_AUTO".into(), ConstVal::Int(0)),
        ("SHAPE_BOX".into(), ConstVal::Int(1)),
        ("SHAPE_SPHERE".into(), ConstVal::Int(2)),
        ("SHAPE_CAPSULE".into(), ConstVal::Int(3)),
        ("SHAPE_CYLINDER".into(), ConstVal::Int(4)),
        ("SHAPE_MESH".into(), ConstVal::Int(5)),
        ("SHAPE_HULL".into(), ConstVal::Int(6)),
        ("PROJ_NONE".into(), ConstVal::Int(0)),
        ("PROJ_PERSPECTIVE".into(), ConstVal::Int(1)),
        ("PROJ_ORTHO".into(), ConstVal::Int(2)),
    ]
}

/// Modern names for classic commands (ext). Each maps a new name onto the
/// signature of an existing command, so both spellings behave identically.
/// Only the command name differs; parameters and defaults are the target's.
pub static ALIASES: &[(&str, &str)] = &[
    ("SetWindowTitle", "AppTitle"),
    ("LoadModel", "LoadMesh"),
    ("LoadAnimModel", "LoadAnimMesh"),
    ("CopyModel", "CopyEntity"),
    ("UpdatePhysics", "UpdateWorld"),
    ("SetEntityPosition", "PositionEntity"),
    ("SetCameraPosition", "PositionEntity"),
    ("SetLightPosition", "PositionEntity"),
    ("SetEntityRotation", "RotateEntity"),
    ("SetCameraRotation", "RotateEntity"),
    ("SetEntityScale", "ScaleEntity"),
    ("SetEntityColor", "EntityColor"),
    ("SetEntityAlpha", "EntityAlpha"),
    ("SetEntityTexture", "EntityTexture"),
    ("SetEntityShininess", "EntityShininess"),
    ("SetEntityBlend", "EntityBlend"),
    ("SetEntityFX", "EntityFX"),
    ("SetEntityParent", "EntityParent"),
    ("SetEntityName", "NameEntity"),
    ("GetEntityName", "EntityName"),
    ("GetEntityX", "EntityX"),
    ("GetEntityY", "EntityY"),
    ("GetEntityZ", "EntityZ"),
    ("GetEntityPitch", "EntityPitch"),
    ("GetEntityYaw", "EntityYaw"),
    ("GetEntityRoll", "EntityRoll"),
    ("GetEntityDistance", "EntityDistance"),
    ("SetCameraZoom", "CameraZoom"),
    ("SetCameraRange", "CameraRange"),
    ("SetCameraViewport", "CameraViewport"),
    ("SetCameraClearColor", "CameraClsColor"),
    ("SetCameraProjection", "CameraProjMode"),
    ("SetCameraFog", "CameraFogMode"),
    ("SetCameraFogColor", "CameraFogColor"),
    ("SetCameraFogRange", "CameraFogRange"),
    ("SetLightColor", "LightColor"),
    ("SetLightRange", "LightRange"),
    ("SetLightCone", "LightConeAngles"),
    ("SetAmbientLight", "AmbientLight"),
    ("SetCollisionType", "EntityType"),
    ("SetCollisionRadius", "EntityRadius"),
    ("SetCollisionBox", "EntityBox"),
    ("SetPickMode", "EntityPickMode"),
    ("SetTextureScale", "ScaleTexture"),
    ("SetTexturePosition", "PositionTexture"),
    ("SetTextureRotation", "RotateTexture"),
    ("SetFont", "SetFont"),
    ("DrawText", "Text"),
    ("SetColor", "Color"),
    ("SetClearColor", "ClsColor"),
    ("ClearScreen", "Cls"),
    ("DrawRect", "Rect"),
    ("DrawOval", "Oval"),
    ("DrawLine", "Line"),
    ("DrawPixel", "Plot"),
];

/// The full command table: [`COMMANDS`] followed by [`ALIASES`].
pub fn table() -> &'static [(String, Cmd)] {
    static TABLE: std::sync::OnceLock<Vec<(String, Cmd)>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t: Vec<(String, Cmd)> = COMMANDS.iter().map(|(s, f)| (s.to_string(), *f)).collect();
        for (alias, target) in ALIASES {
            if alias.eq_ignore_ascii_case(target) {
                continue;
            }
            let (sig, f) = COMMANDS
                .iter()
                .find(|(s, _)| neptune_lang::commands::parse_sig(s).name.eq_ignore_ascii_case(target))
                .unwrap_or_else(|| panic!("alias target {target} is not a command"));
            let target_sig = neptune_lang::commands::parse_sig(sig);
            let at = sig
                .to_ascii_lowercase()
                .find(&target_sig.name.to_ascii_lowercase())
                .unwrap_or_else(|| panic!("cannot find name {} in {sig}", target_sig.name));
            let new_sig = format!("{}{}{}", &sig[..at], alias, &sig[at + target_sig.name.len()..]);
            t.push((new_sig, *f));
        }
        t
    })
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
    // (ext) renderer quality presets and effects, for old and new GPUs
    ("GraphicsQuality%level", |h, a, _| {
        let l = a[0].int();
        if !(0..=3).contains(&l) {
            return Err(format!("GraphicsQuality level must be 0 (low) to 3 (ultra), got {l}"));
        }
        h.world.quality = crate::quality::Quality { level: l, ..Default::default() };
        h.world.quality_dirty = true;
        VOID
    }),
    ("%GetGraphicsQuality", |h, _, _| int(h.world.quality.level)),
    ("PostEffect%effect%enable=1", |h, a, _| {
        h.world.quality.set_effect(a[0].int(), a[1].int() != 0)?;
        h.world.quality_dirty = true;
        VOID
    }),
    // ---- graphics modes and drivers (bbgraphics.cpp)
    ("%GraphicsDepth", |_, _, _| int(32)),
    ("%CountGfxModes", |_, _, _| int(GFX_MODES.len() as i32)),
    ("%CountGfxModes3D", |_, _, _| int(GFX_MODES.len() as i32)),
    ("%GfxModeWidth%mode", |_, a, _| int(gfx_mode(a[0].int()).map_or(0, |m| m.0))),
    ("%GfxModeHeight%mode", |_, a, _| int(gfx_mode(a[0].int()).map_or(0, |m| m.1))),
    ("%GfxModeDepth%mode", |_, a, _| int(if gfx_mode(a[0].int()).is_some() { 32 } else { 0 })),
    // any size works in a window; report the listed sizes for fullscreen-style checks
    ("%GfxModeExists%width%height%depth=0", |_, a, _| int(GFX_MODES.contains(&(a[0].int(), a[1].int())) as i32)),
    ("%GfxMode3DExists%width%height%depth=0", |_, a, _| int(GFX_MODES.contains(&(a[0].int(), a[1].int())) as i32)),
    ("%CountGfxDrivers", |_, _, _| int(1)),
    ("$GfxDriverName%driver", |_, a, _| Ok(Value::Str(if a[0].int() == 1 { "OpenGL 3.3 (Fyrox)" } else { "" }.into()))),
    ("SetGfxDriver%driver", |_, _, _| VOID),
    ("%Windowed3D", |_, _, _| int(1)),
    ("%GfxDriverCaps3D", |_, _, _| int(110)),
    ("%HWTexUnits", |_, _, _| int(crate::classic::MAX_LAYERS as i32)),
    ("HWMultiTex%enable", |_, _, _| VOID),
    ("Dither%enable", |_, _, _| VOID),
    ("AntiAlias%enable", |h, a, _| {
        h.world.quality.set_effect(crate::quality::EFFECT_FXAA, a[0].int() != 0)?;
        h.world.quality_dirty = true;
        VOID
    }),
    // video memory is not reported by OpenGL 3.3; a generous figure keeps old checks happy
    ("%TotalVidMem", |_, _, _| int(1 << 30)),
    ("%AvailVidMem", |_, _, _| int(1 << 30)),
    // Flip already waits for the display (vsync)
    ("VWait%frames=1", |_, _, _| VOID),
    ("%ScanLine", |_, _, _| int(0)),
    ("%TrisRendered", |h, _, _| int(h.world.tris_rendered)),
    // (ext) switch between a window and borderless fullscreen at any time
    ("SetFullscreen%enable", |h, a, _| {
        let on = a[0].int() != 0;
        if h.world.fullscreen != on {
            h.world.fullscreen = on;
            h.world.pending_fullscreen = Some(on);
        }
        VOID
    }),
    ("%IsFullscreen", |h, _, _| int(h.world.fullscreen as i32)), // (ext)
    // ---- pointer and waiting input (bbinput.cpp)
    ("HidePointer", |h, _, _| {
        h.world.pending_cursor_visible = Some(false);
        VOID
    }),
    ("ShowPointer", |h, _, _| {
        h.world.pending_cursor_visible = Some(true);
        VOID
    }),
    ("MoveMouse%x%y", |h, a, _| {
        let (x, y) = (a[0].float(), a[1].float());
        h.input.moved_to(x, y);
        h.world.pending_cursor_pos = Some((x, y));
        VOID
    }),
    // (ext) lock the pointer to the window for mouse-look (MouseXSpeed keeps working)
    ("CaptureMouse%enable", |h, a, _| {
        let on = a[0].int() != 0;
        h.world.pending_grab = Some(on);
        h.world.pending_cursor_visible = Some(!on);
        VOID
    }),
    ("%MouseZSpeed", |h, _, _| int(h.input.mouse_speed_z() as i32)),
    ("%GetMouse", |h, _, _| int(h.input.get_mouse())),
    ("%WaitMouse", |h, _, ctx| {
        h.scene()?;
        let b = h.input.get_mouse();
        if b == 0 {
            ctx.request_retry();
        }
        int(b)
    }),
    ("%WaitKey", |h, _, ctx| {
        h.scene()?;
        let k = h.input.any_key_hit();
        if k == 0 {
            ctx.request_retry();
        }
        int(k)
    }),
    // ---- joysticks and gamepads (bbinput.cpp), port 0 is the first controller
    ("%JoyType%port=0", |h, a, _| int(h.input.joys.kind(a[0].int()))),
    ("%CountJoys", |h, _, _| int((0..16).filter(|&p| h.input.joys.kind(p) != 0).count() as i32)), // (ext)
    ("%JoyDown%button%port=0", |h, a, _| int(h.input.joys.down(a[0].int(), a[1].int()) as i32)),
    ("%JoyHit%button%port=0", |h, a, _| int(h.input.joys.hit(a[0].int(), a[1].int()))),
    ("%GetJoy%port=0", |h, a, _| int(h.input.joys.get(a[0].int()))),
    ("%WaitJoy%port=0", |h, a, ctx| {
        h.scene()?;
        let b = h.input.joys.get(a[0].int());
        if b == 0 {
            ctx.request_retry();
        }
        int(b)
    }),
    ("FlushJoy", |h, _, _| {
        h.input.joys.flush();
        VOID
    }),
    ("#JoyX%port=0", |h, a, _| float(h.input.joys.value(a[0].int(), "x"))),
    ("#JoyY%port=0", |h, a, _| float(h.input.joys.value(a[0].int(), "y"))),
    ("#JoyZ%port=0", |h, a, _| float(h.input.joys.value(a[0].int(), "z"))),
    ("#JoyU%port=0", |h, a, _| float(h.input.joys.value(a[0].int(), "u"))),
    ("#JoyV%port=0", |h, a, _| float(h.input.joys.value(a[0].int(), "v"))),
    ("#JoyPitch%port=0", |h, a, _| float(h.input.joys.value(a[0].int(), "pitch"))),
    ("#JoyYaw%port=0", |h, a, _| float(h.input.joys.value(a[0].int(), "yaw"))),
    ("#JoyRoll%port=0", |h, a, _| float(h.input.joys.value(a[0].int(), "roll"))),
    ("%JoyHat%port=0", |h, a, _| int(h.input.joys.hat(a[0].int()))),
    ("%JoyXDir%port=0", |h, a, _| int(crate::joy::dir(h.input.joys.value(a[0].int(), "x")))),
    ("%JoyYDir%port=0", |h, a, _| int(crate::joy::dir(h.input.joys.value(a[0].int(), "y")))),
    ("%JoyZDir%port=0", |h, a, _| int(crate::joy::dir(h.input.joys.value(a[0].int(), "z")))),
    ("%JoyUDir%port=0", |h, a, _| int(crate::joy::dir(h.input.joys.value(a[0].int(), "u")))),
    ("%JoyVDir%port=0", |h, a, _| int(crate::joy::dir(h.input.joys.value(a[0].int(), "v")))),
    // ---- text cursor, viewport, pixels and buffer files
    ("Locate%x%y", |h, a, _| {
        h.drain_print();
        h.world.cursor = (a[0].float(), a[1].float());
        VOID
    }),
    ("Viewport%x%y%width%height", |h, a, _| {
        h.scene()?;
        let (x, y, w, ht) = (a[0].float(), a[1].float(), a[2].float(), a[3].float());
        let whole = x <= 0.0 && y <= 0.0 && w >= h.world.width as f32 && ht >= h.world.height as f32;
        let rect = (!whole).then(|| Rect::new(x, y, w.max(0.0), ht.max(0.0)));
        h.world.canvas.items.push(crate::draw2d::DrawOp::Clip { rect });
        VOID
    }),
    ("GetColor%x%y", |h, a, _| {
        let target = h.current_buffer()?;
        if target != crate::buffer::Buffer::Screen {
            let (x, y) = (a[0].int(), a[1].int());
            let p = h.with_target(target, |t| t.get(x, y))?;
            h.world.canvas.color = Color::opaque(p[0], p[1], p[2]);
        }
        VOID
    }),
    ("CopyPixel%src_x%src_y%src_buffer%dest_x%dest_y%dest_buffer=0", |h, a, _| copy_pixel(h, a)),
    ("CopyPixelFast%src_x%src_y%src_buffer%dest_x%dest_y%dest_buffer=0", |h, a, _| copy_pixel(h, a)),
    ("%BufferWidth%buffer", |h, a, _| buffer_size(h, a[0].int(), true)),
    ("%BufferHeight%buffer", |h, a, _| buffer_size(h, a[0].int(), false)),
    ("%SaveBuffer%buffer$file", |h, a, _| {
        let path = a[1].str().to_string();
        match crate::buffer::decode(a[0].int())? {
            crate::buffer::Buffer::Screen => {
                // saved at the start of the next frame, when this frame is on screen
                h.world.pending_screenshot = Some(path);
                int(1)
            }
            b => {
                let img = h.with_target(b, |t| {
                    let mut out = image::RgbaImage::new(t.w as u32, t.h as u32);
                    for y in 0..t.h {
                        for x in 0..t.w {
                            out.put_pixel(x as u32, y as u32, t.get(x, y));
                        }
                    }
                    out
                })?;
                int(crate::screenshot::save(&img, &path).is_ok() as i32)
            }
        }
    }),
    ("%LoadBuffer%buffer$file", |h, a, _| {
        let b = crate::buffer::decode(a[0].int())?;
        let Ok(src) = image::open(&*a[1].str()) else { return int(0) };
        let src = src.to_rgba8();
        if b == crate::buffer::Buffer::Screen {
            let img = crate::image2d::Image2D::new(src, 0, 0, 0, 1)?;
            let (w, ht) = (h.world.width as f32, h.world.height as f32);
            h.world.canvas.image(0.0, 0.0, w, ht, Rect::new(0.0, 0.0, 1.0, 1.0), img.block.clone());
        } else {
            let (w, ht) = (src.width() as i32, src.height() as i32);
            h.with_target(b, |t| t.blit(&src, 0, 0, w, ht, 0, 0, None))?;
        }
        int(1)
    }),
    ("%SaveImage%image$file%frame=0", |h, a, _| {
        let img = h.image(a[0].int())?;
        let (x, y, w, ht) = img.frame_rect(a[2].int().max(0) as u32)?;
        let frame = image::imageops::crop_imm(img.pixels(), x as u32, y as u32, w as u32, ht as u32).to_image();
        int(crate::screenshot::save(&frame, &a[1].str()).is_ok() as i32)
    }),
    ("%GraphicsWidth", |h, _, _| int(h.world.width as i32)),
    ("%GraphicsHeight", |h, _, _| int(h.world.height as i32)),
    ("UpdateWorld#elapsed_time=1", |h, a, _| {
        h.scene()?;
        h.flush_meshes()?;
        h.update_animations(a[0].float())?;
        update_world(h)?;
        VOID
    }),
    ("RenderWorld#tween=1", |h, a, ctx| {
        h.scene()?;
        before_render(h, a[0].float())?;
        h.flush_meshes()?;
        h.face_sprites()?;
        h.commit_buffers()?;
        // A program that renders repeatedly without Flip still needs frames shown.
        if h.world.renders > h.world.renders_at_flip {
            h.world.canvas.flip();
            h.world.renders_at_flip = h.world.renders;
            ctx.request_yield();
        }
        // the 3D render covers whatever 2D was drawn before it
        h.world.canvas.items.clear();
        h.world.cursor = (0.0, 0.0);
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
        h.commit_buffers()?;
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
    // ---- custom meshes (Blitz3D mesh/surface commands)
    ("%CreateMesh%brush=0%parent=0", |h, a, _| {
        h.scene()?;
        let id = h.add_entity(crate::mesh::empty_mesh_node(), Kind::Mesh, a[1].int())?;
        if a[0].int() != 0 {
            paint_entity(h, id, a[0].int())?;
        }
        int(id)
    }),
    ("%CreateSurface%mesh%brush=0", |h, a, _| int(h.create_surface(a[0].int(), a[1].int())?)),
    ("%CountSurfaces%mesh", |h, a, _| int(h.count_surfaces(a[0].int())? as i32)),
    ("%GetSurface%mesh%index", |h, a, _| {
        let (m, i) = (a[0].int(), a[1].int());
        let n = h.count_surfaces(m)?;
        if i < 1 || i as usize > n {
            return Err(format!("GetSurface index {i} out of range (mesh has {n} surfaces)"));
        }
        int(h.surface_handle(m, i as usize - 1)?)
    }),
    ("%AddVertex%surface#x#y#z#u=0#v=0#w=1", |h, a, _| {
        let uv = [a[4].float(), a[5].float(), a[6].float()];
        let v = crate::mesh::Vert { pos: v3(a, 1), uv: [uv, uv], ..Default::default() };
        int(h.edit_surface(a[0].int(), |s| {
            s.verts.push(v);
            s.verts.len() as i32 - 1
        })?)
    }),
    ("%AddTriangle%surface%v0%v1%v2", |h, a, _| {
        let s = a[0].int();
        let n = h.surface(s)?.verts.len() as i32;
        let t = [a[1].int(), a[2].int(), a[3].int()];
        if let Some(bad) = t.iter().find(|&&i| i < 0 || i >= n) {
            return Err(format!("AddTriangle vertex {bad} out of range (surface has {n} vertices)"));
        }
        int(h.edit_surface(s, |info| {
            info.tris.push(t.map(|i| i as u32));
            info.tris.len() as i32 - 1
        })?)
    }),
    ("ClearSurface%surface%clear_vertices=1%clear_triangles=1", |h, a, _| {
        let (cv, ct) = (a[1].int() != 0, a[2].int() != 0);
        h.edit_surface(a[0].int(), |s| {
            if cv {
                s.verts.clear();
                s.tris.clear();
            }
            if ct {
                s.tris.clear();
            }
        })?;
        VOID
    }),
    ("%CountVertices%surface", |h, a, _| int(h.surface(a[0].int())?.verts.len() as i32)),
    ("%CountTriangles%surface", |h, a, _| int(h.surface(a[0].int())?.tris.len() as i32)),
    ("%TriangleVertex%surface%triangle%corner", |h, a, _| {
        let s = h.surface(a[0].int())?;
        let (t, c) = (a[1].int(), a[2].int());
        let tri = usize::try_from(t).ok().and_then(|t| s.tris.get(t)).ok_or_else(|| format!("triangle {t} out of range"))?;
        let c = usize::try_from(c).ok().filter(|&c| c < 3).ok_or_else(|| format!("triangle corner must be 0-2, got {c}"))?;
        int(tri[c] as i32)
    }),
    ("VertexCoords%surface%index#x#y#z", |h, a, _| {
        let p = v3(a, 2);
        h.edit_vertex(a[0].int(), a[1].int(), |v| v.pos = p)?;
        VOID
    }),
    ("VertexNormal%surface%index#nx#ny#nz", |h, a, _| {
        let n = v3(a, 2);
        h.edit_vertex(a[0].int(), a[1].int(), |v| v.normal = n)?;
        VOID
    }),
    ("VertexColor%surface%index#red#green#blue#alpha=1", |h, a, _| {
        let c = [a[2].float() / 255.0, a[3].float() / 255.0, a[4].float() / 255.0, a[5].float()];
        h.edit_vertex(a[0].int(), a[1].int(), |v| v.color = c)?;
        VOID
    }),
    ("VertexTexCoords%surface%index#u#v#w=1%coord_set=0", |h, a, _| {
        let set = coord_set(a, 5);
        let uv = [a[2].float(), a[3].float(), a[4].float()];
        h.edit_vertex(a[0].int(), a[1].int(), |v| v.uv[set] = uv)?;
        VOID
    }),
    ("#VertexX%surface%index", |h, a, _| vfloat(h, a, |v| v.pos.x)),
    ("#VertexY%surface%index", |h, a, _| vfloat(h, a, |v| v.pos.y)),
    ("#VertexZ%surface%index", |h, a, _| vfloat(h, a, |v| v.pos.z)),
    ("#VertexNX%surface%index", |h, a, _| vfloat(h, a, |v| v.normal.x)),
    ("#VertexNY%surface%index", |h, a, _| vfloat(h, a, |v| v.normal.y)),
    ("#VertexNZ%surface%index", |h, a, _| vfloat(h, a, |v| v.normal.z)),
    ("#VertexRed%surface%index", |h, a, _| vfloat(h, a, |v| v.color[0] * 255.0)),
    ("#VertexGreen%surface%index", |h, a, _| vfloat(h, a, |v| v.color[1] * 255.0)),
    ("#VertexBlue%surface%index", |h, a, _| vfloat(h, a, |v| v.color[2] * 255.0)),
    ("#VertexAlpha%surface%index", |h, a, _| vfloat(h, a, |v| v.color[3])),
    ("#VertexU%surface%index%coord_set=0", |h, a, _| {
        let set = coord_set(a, 2);
        vfloat(h, a, |v| v.uv[set][0])
    }),
    ("#VertexV%surface%index%coord_set=0", |h, a, _| {
        let set = coord_set(a, 2);
        vfloat(h, a, |v| v.uv[set][1])
    }),
    ("#VertexW%surface%index%coord_set=0", |h, a, _| {
        let set = coord_set(a, 2);
        vfloat(h, a, |v| v.uv[set][2])
    }),
    ("UpdateNormals%mesh", |h, a, _| {
        for s in h.all_surfaces(a[0].int())? {
            h.edit_surface(s, |info| crate::mesh::smooth_normals(&mut info.verts, &info.tris))?;
        }
        VOID
    }),
    ("FlipMesh%mesh", |h, a, _| {
        for s in h.all_surfaces(a[0].int())? {
            h.edit_surface(s, |info| {
                for t in &mut info.tris {
                    t.swap(1, 2);
                }
                for v in &mut info.verts {
                    v.normal = -v.normal;
                }
            })?;
        }
        VOID
    }),
    ("ScaleMesh%mesh#x_scale#y_scale#z_scale", |h, a, _| {
        let s = v3(a, 1);
        let inv = Vector3::new(1.0 / s.x, 1.0 / s.y, 1.0 / s.z);
        h.transform_mesh(a[0].int(), |v| {
            v.pos.component_mul_assign(&s);
            v.normal = v.normal.component_mul(&inv).try_normalize(1e-12).unwrap_or(v.normal);
        })?;
        VOID
    }),
    ("PositionMesh%mesh#x#y#z", |h, a, _| {
        let d = v3(a, 1);
        h.transform_mesh(a[0].int(), |v| v.pos += d)?;
        VOID
    }),
    ("RotateMesh%mesh#pitch#yaw#roll", |h, a, _| {
        h.rotate_mesh(a[0].int(), rotation(a[1].float(), a[2].float(), a[3].float()))?;
        VOID
    }),
    ("FitMesh%mesh#x#y#z#width#height#depth%uniform=0", |h, a, _| {
        let m = a[0].int();
        let Some((lo, hi)) = h.mesh_bounds(m)? else { return VOID };
        let size = hi - lo;
        let target = v3(a, 4);
        let fit = |s: f32, t: f32| if s > 1e-9 { t / s } else { 1.0 };
        let mut scale = Vector3::new(fit(size.x, target.x), fit(size.y, target.y), fit(size.z, target.z));
        if a[7].int() != 0 {
            let s = scale.x.min(scale.y).min(scale.z);
            scale = Vector3::new(s, s, s);
        }
        // centred within the box when scaled uniformly
        let offset = v3(a, 1) + (target - size.component_mul(&scale)) / 2.0;
        h.transform_mesh(m, |v| v.pos = (v.pos - lo).component_mul(&scale) + offset)?;
        VOID
    }),
    ("AddMesh%source_mesh%dest_mesh", |h, a, _| {
        append_surfaces(h, a[0].int(), a[1].int())?;
        VOID
    }),
    ("%CopyMesh%mesh%parent=0", |h, a, _| {
        h.scene()?;
        let new = h.add_entity(crate::mesh::empty_mesh_node(), Kind::Mesh, a[1].int())?;
        append_surfaces(h, a[0].int(), new)?;
        int(new)
    }),
    // ---- brushes
    ("%CreateBrush#red=255#green=255#blue=255", |h, a, _| {
        let b = crate::mesh::Brush { color: color(a, 0), ..Default::default() };
        h.world.brushes.push(Some(b));
        int(h.world.brushes.len() as i32)
    }),
    ("%LoadBrush$file%flags=1#u_scale=1#v_scale=1", |h, a, _| match crate::texture::load(&a[0].str(), a[1].int()) {
        Ok(mut t) => {
            t.scale_u = a[2].float();
            t.scale_v = a[3].float();
            h.world.textures.push(Some(t));
            let mut b = crate::mesh::Brush::default();
            b.textures[0] = h.world.textures.len() as i32;
            h.world.brushes.push(Some(b));
            int(h.world.brushes.len() as i32)
        }
        Err(_) => int(0),
    }),
    ("FreeBrush%brush", |h, a, _| {
        h.brush(a[0].int())?;
        h.world.brushes[a[0].int() as usize - 1] = None;
        VOID
    }),
    ("BrushColor%brush#red#green#blue", |h, a, _| {
        let c = color(a, 1);
        modify_brush(h, a[0].int(), |b| b.color = Color::from_rgba(c.r, c.g, c.b, b.color.a))
    }),
    ("BrushAlpha%brush#alpha", |h, a, _| {
        let al = (a[1].float().clamp(0.0, 1.0) * 255.0) as u8;
        modify_brush(h, a[0].int(), |b| b.color.a = al)
    }),
    ("BrushShininess%brush#shininess", |h, a, _| {
        let s = a[1].float().clamp(0.0, 1.0);
        modify_brush(h, a[0].int(), |b| b.shininess = s)
    }),
    ("BrushTexture%brush%texture%frame=0%index=0", |h, a, _| {
        let (t, i) = (a[1].int(), a[3].int());
        if t != 0 {
            h.texture(t)?;
        }
        if !(0..crate::classic::MAX_LAYERS as i32).contains(&i) {
            return Err(format!("BrushTexture index must be 0-{}, got {i}", crate::classic::MAX_LAYERS - 1));
        }
        modify_brush(h, a[0].int(), |b| b.textures[i as usize] = t)
    }),
    ("BrushBlend%brush%blend", |h, a, _| {
        let m = a[1].int().clamp(1, 3);
        modify_brush(h, a[0].int(), |b| b.blend = m)
    }),
    ("BrushFX%brush%fx", |h, a, _| {
        let f = a[1].int() & 63;
        modify_brush(h, a[0].int(), |b| b.fx = f)
    }),
    ("PaintSurface%surface%brush", |h, a, _| {
        let (s, b) = (a[0].int(), a[1].int());
        if b != 0 {
            h.brush(b)?;
        }
        let e = {
            let info = h.surface_mut(s)?;
            info.brush = b;
            info.entity
        };
        h.refresh_material(e)?;
        VOID
    }),
    ("PaintEntity%entity%brush", |h, a, _| paint_entity(h, a[0].int(), a[1].int())),
    ("PaintMesh%mesh%brush", |h, a, _| paint_entity(h, a[0].int(), a[1].int())),
    ("%GetSurfaceBrush%surface", |h, a, _| {
        // Blitz returns a new brush copied from the surface
        let b = h.surface(a[0].int())?.brush;
        let brush = if b != 0 { h.brush(b)?.clone() } else { crate::mesh::Brush::default() };
        h.world.brushes.push(Some(brush));
        int(h.world.brushes.len() as i32)
    }),
    ("%GetEntityBrush%entity", |h, a, _| {
        let e = h.ent(a[0].int())?;
        let brush = crate::mesh::Brush { color: e.color, shininess: e.shininess, textures: e.textures, blend: e.blend, fx: e.fx };
        h.world.brushes.push(Some(brush));
        int(h.world.brushes.len() as i32)
    }),
    // ---- sprites
    ("%CreateSprite%parent=0", |h, a, _| int(h.create_sprite(a[0].int())?)),
    ("%LoadSprite$file%texture_flags=1%parent=0", |h, a, _| {
        h.scene()?;
        match crate::texture::load(&a[0].str(), a[1].int()) {
            Ok(t) => {
                h.world.textures.push(Some(t));
                let tex = h.world.textures.len() as i32;
                let s = h.create_sprite(a[2].int())?;
                h.ent_mut(s)?.textures[0] = tex;
                h.refresh_material(s)?;
                int(s)
            }
            Err(e) => {
                eprintln!("LoadSprite: {e}");
                int(0)
            }
        }
    }),
    ("SpriteViewMode%sprite%view_mode", |h, a, _| {
        let m = a[1].int();
        if !(1..=4).contains(&m) {
            return Err(format!("SpriteViewMode mode must be 1-4, got {m}"));
        }
        h.sprite_mut(a[0].int())?.mode = m;
        if m == 2 {
            // free sprites use the entity rotation again
            h.sync(a[0].int())?;
        }
        VOID
    }),
    ("RotateSprite%sprite#angle", |h, a, _| {
        h.sprite_mut(a[0].int())?.angle = a[1].float();
        VOID
    }),
    ("ScaleSprite%sprite#x_scale#y_scale", |h, a, _| {
        let e = a[0].int();
        h.sprite_mut(e)?.scale = (a[1].float(), a[2].float());
        h.update_sprite(e)?;
        VOID
    }),
    ("HandleSprite%sprite#x_handle#y_handle", |h, a, _| {
        let e = a[0].int();
        h.sprite_mut(e)?.handle = (a[1].float(), a[2].float());
        h.update_sprite(e)?;
        VOID
    }),
    // ---- terrains
    ("%CreateTerrain%grid_size%parent=0", |h, a, _| int(h.create_terrain(a[0].int(), a[1].int())?)),
    ("%LoadTerrain$file%parent=0", |h, a, _| {
        h.scene()?;
        int(h.load_terrain(&a[0].str(), a[1].int())?)
    }),
    ("%TerrainSize%terrain", |h, a, _| int(h.terrain(a[0].int())?.size as i32)),
    ("#TerrainHeight%terrain%grid_x%grid_z", |h, a, _| float(h.terrain(a[0].int())?.height(a[1].int(), a[2].int()))),
    ("ModifyTerrain%terrain%grid_x%grid_z#height%realtime=0", |h, a, _| {
        h.terrain_mut(a[0].int())?.set_height(a[1].int(), a[2].int(), a[3].float())?;
        VOID
    }),
    ("#TerrainY%terrain#x#y#z", |h, a, _| float(h.terrain_y(a[0].int(), v3(a, 1))?)),
    ("#TerrainX%terrain#x#y#z", |h, a, _| {
        h.terrain(a[0].int())?;
        float(a[1].float())
    }),
    ("#TerrainZ%terrain#x#y#z", |h, a, _| {
        h.terrain(a[0].int())?;
        float(a[3].float())
    }),
    // level of detail and shading are handled by the GPU; accepted for compatibility
    ("TerrainDetail%terrain%detail_level%vertex_morph=0", |h, a, _| {
        h.terrain(a[0].int())?;
        VOID
    }),
    ("TerrainShading%terrain%enable", |h, a, _| {
        h.terrain(a[0].int())?;
        VOID
    }),
    // ---- maths on entities and vectors (bbblitz3d.cpp)
    ("TFormPoint#x#y#z%source_entity%dest_entity", |h, a, _| tform(h, a, 0)),
    ("TFormVector#x#y#z%source_entity%dest_entity", |h, a, _| tform(h, a, 1)),
    ("TFormNormal#x#y#z%source_entity%dest_entity", |h, a, _| tform(h, a, 2)),
    ("#TFormedX", |h, _, _| float(h.world.tformed.x)),
    ("#TFormedY", |h, _, _| float(h.world.tformed.y)),
    ("#TFormedZ", |h, _, _| float(h.world.tformed.z)),
    ("#VectorYaw#x#y#z", |_, a, _| float(vector_yaw(&v3(a, 0)))),
    ("#VectorPitch#x#y#z", |_, a, _| float(vector_pitch(&v3(a, 0)))),
    ("#DeltaYaw%src_entity%dest_entity", |h, a, _| {
        let (s, d) = (h.world_of(a[0].int())?, h.world_of(a[1].int())?);
        let local = s.rot.transpose() * (d.position() - s.position());
        // Blitz: the yaw to turn src towards dest, in -180..180
        float(vector_yaw(&local))
    }),
    ("#DeltaPitch%src_entity%dest_entity", |h, a, _| {
        let (s, d) = (h.world_of(a[0].int())?, h.world_of(a[1].int())?);
        let local = s.rot.transpose() * (d.position() - s.position());
        float(vector_pitch(&local))
    }),
    ("AlignToVector%entity#vector_x#vector_y#vector_z%axis#rate=1", |h, a, _| align_to_vector(h, a)),
    ("#GetMatElement%entity%row%column", |h, a, _| {
        let w = h.world_of(a[0].int())?;
        let (r, c) = (a[1].int(), a[2].int());
        if !(0..=3).contains(&r) || !(0..=2).contains(&c) {
            return Err(format!("GetMatElement row must be 0-3 and column 0-2, got {r}, {c}"));
        }
        // rows 0-2 are the scaled axes, row 3 the position (Blitz row-vector layout)
        let v = if r == 3 { w.position() } else { w.rot.column(r as usize) * w.scale[r as usize] };
        float(v[c as usize])
    }),
    ("$EntityClass%entity", |h, a, _| {
        let e = h.ent(a[0].int())?;
        let class = match e.kind {
            Kind::Camera { .. } => "Camera",
            Kind::Light { .. } => "Light",
            Kind::Pivot => "Pivot",
            Kind::Mesh if e.sprite.is_some() => "Sprite",
            Kind::Mesh if e.terrain.is_some() => "Terrain",
            Kind::Mesh => "Mesh",
        };
        Ok(Value::Str(class.into()))
    }),
    ("EntityAutoFade%entity#near#far", |h, a, _| {
        let e = h.ent_mut(a[0].int())?;
        let (n, f) = (a[1].float(), a[2].float());
        if f <= 0.0 {
            e.auto_fade = None;
            e.color.a = e.fade_base;
        } else {
            if e.auto_fade.is_none() {
                e.fade_base = e.color.a;
            }
            e.auto_fade = Some((n.min(f), f.max(n)));
        }
        let id = a[0].int();
        h.refresh_material(id)?;
        VOID
    }),
    ("CaptureWorld", |h, _, _| {
        h.scene()?;
        let all: Vec<(i32, crate::transform::Xform)> = h.entities().map(|(i, e)| (i, h.live_xf(e))).collect();
        h.world.captured = all.into_iter().collect();
        VOID
    }),
    ("ClearWorld%entities=1%brushes=1%textures=1", |h, a, _| {
        h.scene()?;
        if a[0].int() != 0 {
            let roots: Vec<i32> = h.entities().filter(|(_, e)| e.parent == 0).map(|(i, _)| i).collect();
            for e in roots {
                h.free_entity(e)?;
            }
            h.world.captured.clear();
        }
        if a[1].int() != 0 {
            h.world.brushes.clear();
        }
        if a[2].int() != 0 {
            h.world.textures.clear();
        }
        VOID
    }),
    ("CameraClsMode%camera%cls_color%cls_zbuffer", |h, a, _| {
        // Fyrox always clears each camera's view; keep the setting for programs that query it
        camera_mut(h, a[0].int())?;
        VOID
    }),
    ("%MeshesIntersect%mesh_a%mesh_b", |h, a, _| {
        let ta = world_triangles(h, a[0].int())?;
        let tb = world_triangles(h, a[1].int())?;
        int(triangles_intersect(&ta, &tb) as i32)
    }),
    ("%CollisionSurface%entity%index", |h, a, _| int((nth_contact(h, a)?.triangle >= 0) as i32)),
    ("%CollisionTriangle%entity%index", |h, a, _| int(nth_contact(h, a)?.triangle.max(0))),
    ("%FindSurface%mesh%brush", |h, a, _| {
        let (m, b) = (a[0].int(), a[1].int());
        h.ent(m)?;
        let found = h.world.surfaces.iter().position(|s| s.as_ref().is_some_and(|s| s.entity == m && s.brush == b));
        int(found.map_or(0, |i| i as i32 + 1))
    }),
    ("%GetBrushTexture%brush%index=0", |h, a, _| {
        let i = a[1].int();
        let b = h.brush(a[0].int())?;
        int(usize::try_from(i).ok().and_then(|i| b.textures.get(i)).copied().unwrap_or(0))
    }),
    ("LightMesh%mesh#red#green#blue#range=0#light_x=0#light_y=0#light_z=0", |h, a, _| light_mesh(h, a)),
    ("%LoadAnimTexture$file%flags%frame_width%frame_height%first_frame%frame_count", |h, a, _| {
        let flags = texture_flags(h, &a[0].str(), a[1].int());
        match crate::texture::load(&a[0].str(), flags) {
            Ok(mut t) => {
                let n = |i: usize| a[i].int().max(0) as u32;
                let (fw, fh) = (n(2).max(1).min(t.width), n(3).max(1).min(t.height));
                let available = (t.width / fw) * (t.height / fh);
                t.frame_w = fw;
                t.frame_h = fh;
                t.first_frame = n(4).min(available.saturating_sub(1));
                t.frames = n(5).clamp(1, available - t.first_frame);
                h.world.textures.push(Some(t));
                int(h.world.textures.len() as i32)
            }
            Err(_) => int(0),
        }
    }),
    ("TextureFilter$match_text%flags", |h, a, _| {
        h.world.texture_filters.push((a[0].str().to_lowercase(), a[1].int()));
        VOID
    }),
    ("ClearTextureFilters", |h, _, _| {
        h.world.texture_filters.clear();
        VOID
    }),
    // ---- keyframe animation of any entity
    ("%AddAnimSeq%entity%length", |h, a, _| int(h.add_anim_seq(a[0].int(), a[1].float())?)),
    ("SetAnimKey%entity%frame%pos_key=1%rot_key=1%scale_key=1", |h, a, _| {
        h.set_anim_key(a[0].int(), a[1].int(), a[2].int() != 0, a[3].int() != 0, a[4].int() != 0)?;
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
        let id = h.add_entity(camera_node(), Kind::Camera { zoom: 1.0, mode: 1 }, a[0].int())?;
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
    ("%LoadMesh$file%parent=0", |h, a, _| int(crate::model::load_mesh(h, &a[0].str(), a[1].int())?)),
    ("%LoadAnimMesh$file%parent=0", |h, a, _| int(crate::model::load_mesh(h, &a[0].str(), a[1].int())?)),
    ("%CopyEntity%entity%parent=0", |h, a, _| int(h.copy_entity(a[0].int(), a[1].int())?)),
    ("#MeshWidth%mesh", |h, a, _| mesh_extent(h, a[0].int(), 0)),
    ("#MeshHeight%mesh", |h, a, _| mesh_extent(h, a[0].int(), 1)),
    ("#MeshDepth%mesh", |h, a, _| mesh_extent(h, a[0].int(), 2)),
    ("%CountMeshTriangles%mesh", |h, a, _| int(h.mesh_triangles(a[0].int())?.len() as i32)), // (ext)
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
    ("%CountChildren%entity", |h, a, _| int(h.node_children(a[0].int())?.len() as i32)),
    ("%GetChild%entity%index", |h, a, _| {
        let kids = h.node_children(a[0].int())?;
        let i = a[1].int();
        int(if i >= 1 { kids.get(i as usize - 1).copied().unwrap_or(0) } else { 0 })
    }),
    ("%FindChild%entity$name", |h, a, _| int(h.find_child(a[0].int(), &a[1].str())?)),
    // ---- animation
    ("Animate%entity%mode=1#speed=1%sequence=0#transition=0", |h, a, _| {
        h.animate(a[0].int(), a[1].int(), a[2].float(), a[3].int(), a[4].float())?;
        VOID
    }),
    ("SetAnimTime%entity#time%sequence=0", |h, a, _| {
        h.set_anim_time(a[0].int(), a[1].float(), a[2].int())?;
        VOID
    }),
    ("%Animating%entity", |h, a, _| int(h.ent(a[0].int())?.anim.as_ref().is_some_and(|s| s.mode != 0) as i32)),
    ("#AnimTime%entity", |h, a, _| float(h.ent(a[0].int())?.anim.as_ref().map_or(0.0, |s| s.time))),
    ("%AnimSeq%entity", |h, a, _| int(h.ent(a[0].int())?.anim.as_ref().map_or(0, |s| s.seq as i32))),
    ("%AnimLength%entity", |h, a, _| {
        let e = a[0].int();
        let Some(seq) = h.ent(e)?.anim.as_ref().map(|s| s.seq) else { return int(0) };
        int(h.seq_length(e, seq)?.round() as i32)
    }),
    ("%ExtractAnimSeq%entity%first_frame%last_frame%anim_seq=0", |h, a, _| {
        int(h.extract_seq(a[0].int(), a[1].float(), a[2].float(), a[3].int())?)
    }),
    // (ext)
    ("%CountAnimSeqs%entity", |h, a, _| {
        let e = a[0].int();
        if h.ent(e)?.anim.is_none() {
            return int(0);
        }
        int(h.seq_count(e)? as i32)
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
        h.refresh_material(e)?;
        VOID
    }),
    ("EntityAlpha%entity#alpha", |h, a, _| {
        let e = a[0].int();
        let alpha = (a[1].float().clamp(0.0, 1.0) * 255.0) as u8;
        // with auto-fade on, this is the alpha at close range
        h.ent_mut(e)?.fade_base = alpha;
        let c = h.ent(e)?.color;
        let c = Color::from_rgba(c.r, c.g, c.b, alpha);
        h.ent_mut(e)?.color = c;
        h.refresh_material(e)?;
        VOID
    }),
    // ---- textures
    ("%LoadTexture$file%flags=1", |h, a, _| {
        let flags = texture_flags(h, &a[0].str(), a[1].int());
        match crate::texture::load(&a[0].str(), flags) {
            Ok(t) => {
                h.world.textures.push(Some(t));
                int(h.world.textures.len() as i32)
            }
            Err(_) => int(0), // Blitz returns 0 when the file can't be loaded
        }
    }),
    ("%CreateTexture%width%height%flags=1%frames=1", |h, a, _| {
        let t = crate::texture::TextureInfo::blank(a[0].int().max(0) as u32, a[1].int().max(0) as u32, a[2].int())?;
        h.world.textures.push(Some(t));
        int(h.world.textures.len() as i32)
    }),
    ("FreeTexture%texture", |h, a, _| {
        let t = a[0].int();
        h.texture(t)?;
        h.world.textures[t as usize - 1] = None;
        // entities keep showing the texture until re-textured, like Blitz
        VOID
    }),
    ("%TextureWidth%texture", |h, a, _| int(h.texture(a[0].int())?.width as i32)),
    ("%TextureHeight%texture", |h, a, _| int(h.texture(a[0].int())?.height as i32)),
    ("$TextureName%texture", |h, a, _| Ok(Value::Str(h.texture(a[0].int())?.path.as_str().into()))),
    ("ScaleTexture%texture#u_scale#v_scale", |h, a, _| {
        let (u, v) = (a[1].float(), a[2].float());
        if u == 0.0 || v == 0.0 {
            return Err("ScaleTexture scale must not be zero".into());
        }
        modify_texture(h, a[0].int(), |t| {
            t.scale_u = u;
            t.scale_v = v;
        })
    }),
    ("PositionTexture%texture#u_position#v_position", |h, a, _| {
        let (u, v) = (a[1].float(), a[2].float());
        modify_texture(h, a[0].int(), |t| {
            t.pos_u = u;
            t.pos_v = v;
        })
    }),
    ("RotateTexture%texture#angle", |h, a, _| {
        let r = a[1].float();
        modify_texture(h, a[0].int(), |t| t.rotation = r)
    }),
    ("TextureBlend%texture%blend", |h, a, _| {
        let b = a[1].int();
        if !(0..=5).contains(&b) {
            return Err(format!("TextureBlend mode must be 0-5, got {b}"));
        }
        modify_texture(h, a[0].int(), |t| t.blend = b)
    }),
    ("TextureCoords%texture%coords", |h, a, _| {
        let c = a[1].int().clamp(0, 1);
        modify_texture(h, a[0].int(), |t| t.coords = c)
    }),
    ("EntityTexture%entity%texture%frame=0%index=0", |h, a, _| {
        let (e, t, index) = (a[0].int(), a[1].int(), a[3].int());
        if !(0..crate::classic::MAX_LAYERS as i32).contains(&index) {
            return Err(format!("EntityTexture index must be 0-{}, got {index}", crate::classic::MAX_LAYERS - 1));
        }
        if t != 0 {
            h.texture(t)?;
        }
        h.ent_mut(e)?.textures[index as usize] = t;
        h.ent_mut(e)?.tex_frames[index as usize] = a[2].int();
        h.refresh_material(e)?;
        VOID
    }),
    // (ext) normal map for per-pixel lighting detail
    ("EntityNormalMap%entity%texture", |h, a, _| {
        let (e, t) = (a[0].int(), a[1].int());
        if t != 0 {
            h.texture(t)?;
        }
        h.ent_mut(e)?.normal_map = t;
        h.refresh_material(e)?;
        VOID
    }),
    ("EntityShininess%entity#shininess", |h, a, _| {
        let e = a[0].int();
        h.ent_mut(e)?.shininess = a[1].float().clamp(0.0, 1.0);
        h.refresh_material(e)?;
        VOID
    }),
    ("EntityBlend%entity%blend", |h, a, _| {
        let (e, b) = (a[0].int(), a[1].int());
        if !(1..=3).contains(&b) {
            return Err(format!("EntityBlend mode must be 1 (alpha), 2 (multiply) or 3 (add), got {b}"));
        }
        h.ent_mut(e)?.blend = b;
        h.refresh_material(e)?;
        VOID
    }),
    ("EntityFX%entity%fx", |h, a, _| {
        let e = a[0].int();
        h.ent_mut(e)?.fx = a[1].int() & 63;
        h.refresh_material(e)?;
        VOID
    }),
    // Blitz draws non-zero orders after the main geometry; here they go to the forward pass
    ("EntityOrder%entity%order", |h, a, _| {
        let e = a[0].int();
        h.ent_mut(e)?.order = a[1].int();
        h.refresh_material(e)?;
        VOID
    }),
    // ---- cameras
    ("CameraClsColor%camera#red#green#blue", |h, a, _| {
        camera_mut(h, a[0].int())?;
        let c = color(a, 1);
        h.scene()?.rendering_options.get_value_mut_silent().clear_color = Some(srgb_to_linear(c));
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
        if let Kind::Camera { zoom, .. } = &mut h.ent_mut(e)?.kind {
            *zoom = a[1].float();
        }
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
        let (e, m) = (a[0].int(), a[1].int());
        if !(0..=2).contains(&m) {
            return Err(format!("CameraProjMode mode must be 0, 1 or 2, got {m}"));
        }
        camera_mut(h, e)?.set_enabled(m != 0);
        if let Kind::Camera { mode, .. } = &mut h.ent_mut(e)?.kind {
            *mode = m;
        }
        h.update_camera_fov(e)?;
        VOID
    }),
    ("CameraFogMode%camera%mode", |h, a, _| {
        camera_mut(h, a[0].int())?;
        h.world.classic.fog_mode = a[1].int().clamp(0, 1);
        h.refresh_all_materials()?;
        VOID
    }),
    ("CameraFogColor%camera#red#green#blue", |h, a, _| {
        camera_mut(h, a[0].int())?;
        h.world.classic.fog_color = color(a, 1);
        refresh_fog(h)
    }),
    ("CameraFogRange%camera#near#far", |h, a, _| {
        camera_mut(h, a[0].int())?;
        h.world.classic.fog_range = (a[1].float(), a[2].float());
        refresh_fog(h)
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
    // (ext) aims a light (or any entity) along a direction vector
    ("SetLightDirection%light#x#y#z", |h, a, _| {
        let e = a[0].int();
        let d = v3(a, 1);
        if d.norm_squared() > 1e-12 {
            set_world_rotation(h, e, rotation(vector_pitch(&d), vector_yaw(&d), 0.0))?;
        }
        VOID
    }),
    ("SetEntityDirection%entity#x#y#z", |h, a, _| {
        let e = a[0].int();
        let d = v3(a, 1);
        if d.norm_squared() > 1e-12 {
            set_world_rotation(h, e, rotation(vector_pitch(&d), vector_yaw(&d), 0.0))?;
        }
        VOID
    }),
    // (ext) real-time shadows per light
    ("LightShadows%light%enable", |h, a, _| {
        let (light, _) = light_of(h, a[0].int())?;
        h.graph()?[light].set_cast_shadows(a[1].int() != 0);
        VOID
    }),
    // ---- sound (bbaudio.cpp)
    ("%LoadSound$file", |h, a, _| {
        h.scene()?;
        load_sound(h, &a[0].str(), false)
    }),
    ("%Load3DSound$file", |h, a, _| {
        h.scene()?;
        load_sound(h, &a[0].str(), true)
    }),
    ("FreeSound%sound", |h, a, _| {
        h.sound(a[0].int())?;
        h.world.sounds[a[0].int() as usize - 1] = None;
        VOID
    }),
    ("LoopSound%sound", |h, a, _| {
        h.sound_mut(a[0].int())?.looping = true;
        VOID
    }),
    ("SoundVolume%sound#volume", |h, a, _| {
        h.sound_mut(a[0].int())?.volume = a[1].float().max(0.0);
        VOID
    }),
    ("SoundPan%sound#pan", |h, a, _| {
        h.sound_mut(a[0].int())?.pan = a[1].float().clamp(-1.0, 1.0);
        VOID
    }),
    ("SoundPitch%sound%pitch", |h, a, _| {
        let s = h.sound_mut(a[0].int())?;
        s.pitch = crate::sound::pitch_from_hz(a[1].float(), s.rate);
        VOID
    }),
    ("%PlaySound%sound", |h, a, _| {
        let info = h.sound(a[0].int())?.clone();
        int(h.play(&info, 0)?)
    }),
    ("%PlayMusic$file%mode=0", |h, a, _| {
        h.scene()?;
        match crate::sound::load(&a[0].str(), true) {
            Ok((buffer, rate)) => {
                let info = crate::sound::SoundInfo { buffer, looping: a[1].int() != 0, volume: 1.0, pan: 0.0, pitch: 1.0, rate, spatial: false };
                int(h.play(&info, 0)?)
            }
            Err(e) => {
                eprintln!("{e}");
                int(0)
            }
        }
    }),
    ("StopChannel%channel", |h, a, _| {
        h.stop_channel(a[0].int())?;
        VOID
    }),
    ("PauseChannel%channel", |h, a, _| with_channel(h, a[0].int(), |c| c.pause())),
    ("ResumeChannel%channel", |h, a, _| with_channel(h, a[0].int(), |c| c.play())),
    ("ChannelVolume%channel#volume", |h, a, _| {
        let v = a[1].float().max(0.0);
        with_channel(h, a[0].int(), |c| {
            c.set_gain(v);
        })
    }),
    ("ChannelPan%channel#pan", |h, a, _| {
        let p = a[1].float().clamp(-1.0, 1.0);
        with_channel(h, a[0].int(), |c| {
            c.set_panning(p);
        })
    }),
    ("ChannelPitch%channel%pitch", |h, a, _| {
        let hz = a[1].float();
        with_channel(h, a[0].int(), |c| {
            let rate = c.buffer().map_or(44100.0, |b| match &*b.data_ref() {
                fyrox::scene::sound::SoundBuffer::Generic(g) => g.sample_rate() as f64,
                fyrox::scene::sound::SoundBuffer::Streaming(s) => s.sample_rate() as f64,
            });
            c.set_pitch(crate::sound::pitch_from_hz(hz, rate));
        })
    }),
    ("%ChannelPlaying%channel", |h, a, _| {
        let playing = h.channel(a[0].int())?.is_some_and(|c| c.status() == fyrox::scene::sound::Status::Playing);
        int(playing as i32)
    }),
    ("%CreateListener%parent#rolloff_factor=1#doppler_scale=1#distance_scale=1", |h, a, _| {
        h.scene()?;
        // the listener is a pivot entity so programs can move it like Blitz
        let p = h.add_entity(pivot_node(), Kind::Pivot, a[0].int())?;
        h.create_listener(p, a[1].float(), a[3].float())?;
        int(p)
    }),
    ("%EmitSound%sound%entity", |h, a, _| {
        let info = h.sound(a[0].int())?.clone();
        int(h.play(&info, a[1].int())?)
    }),
    // ---- physics (ext), Rapier through Fyrox
    ("EntityPhysics%entity%body_type=1%shape=0#mass=1", |h, a, _| {
        h.scene()?;
        h.enable_physics(a[0].int(), a[1].int(), a[2].int(), a[3].float())?;
        VOID
    }),
    ("PhysicsGravity#x#y#z", |h, a, _| {
        let g = crate::physics::vec_to_fyrox(v3(a, 0));
        h.scene()?.graph.physics.gravity.set_value_and_mark_modified(g);
        VOID
    }),
    ("EntityMass%entity#mass", |h, a, _| {
        let m = a[1].float().max(1e-4);
        body(h, a[0].int(), |b| {
            b.set_mass(m);
        })
    }),
    ("EntityFriction%entity#friction", |h, a, _| {
        h.collider_mut(a[0].int())?.set_friction(a[1].float().max(0.0));
        VOID
    }),
    ("EntityBounce%entity#bounce", |h, a, _| {
        h.collider_mut(a[0].int())?.set_restitution(a[1].float().clamp(0.0, 1.0));
        VOID
    }),
    ("EntityDamping%entity#linear#angular", |h, a, _| {
        let (l, an) = (a[1].float().max(0.0), a[2].float().max(0.0));
        body(h, a[0].int(), |b| {
            b.set_lin_damping(l);
            b.set_ang_damping(an);
        })
    }),
    ("EntityGravityScale%entity#scale", |h, a, _| {
        let s = a[1].float();
        body(h, a[0].int(), |b| {
            b.set_gravity_scale(s);
        })
    }),
    ("LockRotation%entity%x%y%z", |h, a, _| {
        // mirroring keeps each axis an axis, so locks map one to one
        let (x, y, z) = (a[1].int() != 0, a[2].int() != 0, a[3].int() != 0);
        body(h, a[0].int(), |b| {
            b.lock_x_rotations(x);
            b.lock_y_rotations(y);
            b.lock_z_rotations(z);
        })
    }),
    ("ApplyForce%entity#x#y#z", |h, a, _| {
        let f = crate::physics::vec_to_fyrox(v3(a, 1));
        body(h, a[0].int(), |b| b.apply_force(f))
    }),
    ("ApplyImpulse%entity#x#y#z", |h, a, _| {
        let f = crate::physics::vec_to_fyrox(v3(a, 1));
        body(h, a[0].int(), |b| b.apply_impulse(f))
    }),
    ("ApplyImpulseAt%entity#x#y#z#point_x#point_y#point_z", |h, a, _| {
        let (f, p) = (crate::physics::vec_to_fyrox(v3(a, 1)), crate::physics::vec_to_fyrox(v3(a, 4)));
        body(h, a[0].int(), |b| b.apply_impulse_at_point(f, p))
    }),
    ("ApplyTorque%entity#x#y#z", |h, a, _| {
        let t = crate::physics::angular_to_fyrox(v3(a, 1));
        body(h, a[0].int(), |b| b.apply_torque(t))
    }),
    ("ApplyTorqueImpulse%entity#x#y#z", |h, a, _| {
        let t = crate::physics::angular_to_fyrox(v3(a, 1));
        body(h, a[0].int(), |b| b.apply_torque_impulse(t))
    }),
    ("SetVelocity%entity#x#y#z", |h, a, _| {
        let v = crate::physics::vec_to_fyrox(v3(a, 1));
        body(h, a[0].int(), |b| {
            b.set_lin_vel(v);
        })
    }),
    ("SetAngularVelocity%entity#x#y#z", |h, a, _| {
        let v = crate::physics::angular_to_fyrox(v3(a, 1));
        body(h, a[0].int(), |b| {
            b.set_ang_vel(v);
        })
    }),
    ("#VelocityX%entity", |h, a, _| velocity(h, a[0].int(), false, 0)),
    ("#VelocityY%entity", |h, a, _| velocity(h, a[0].int(), false, 1)),
    ("#VelocityZ%entity", |h, a, _| velocity(h, a[0].int(), false, 2)),
    ("#AngularVelocityX%entity", |h, a, _| velocity(h, a[0].int(), true, 0)),
    ("#AngularVelocityY%entity", |h, a, _| velocity(h, a[0].int(), true, 1)),
    ("#AngularVelocityZ%entity", |h, a, _| velocity(h, a[0].int(), true, 2)),
    ("%EntityTouching%entity%other", |h, a, _| int(h.touching(a[0].int())?.contains(&a[1].int()) as i32)),
    ("%CountContacts%entity", |h, a, _| int(h.touching(a[0].int())?.len() as i32)),
    ("%ContactEntity%entity%index", |h, a, _| {
        let list = h.touching(a[0].int())?;
        let i = a[1].int();
        int(if i >= 1 { list.get(i as usize - 1).copied().unwrap_or(0) } else { 0 })
    }),
    ("%PhysicsRayCast#x#y#z#dx#dy#dz", |h, a, _| {
        let hit = h.physics_ray(v3(a, 0), v3(a, 3))?;
        picked(h, hit)
    }),
    ("%CreateBallJoint%entity1%entity2#x#y#z", |h, a, _| {
        int(h.create_joint(a[0].int(), a[1].int(), 1, v3(a, 2), Vector3::x())?)
    }),
    ("%CreateHingeJoint%entity1%entity2#x#y#z#axis_x#axis_y#axis_z", |h, a, _| {
        int(h.create_joint(a[0].int(), a[1].int(), 2, v3(a, 2), v3(a, 5))?)
    }),
    ("%CreateFixedJoint%entity1%entity2", |h, a, _| {
        let p = h.world_of(a[1].int())?.position();
        int(h.create_joint(a[0].int(), a[1].int(), 3, p, Vector3::x())?)
    }),
    ("FreeJoint%joint", |h, a, _| {
        h.free_joint(a[0].int())?;
        VOID
    }),
    // ---- 2D (bbgraphics.cpp)
    ("Text%x%y$string%center_x=0%center_y=0", |h, a, _| {
        let target = h.current_buffer()?;
        if target != crate::buffer::Buffer::Screen {
            let font = h.world.canvas.font.clone();
            let s = a[2].str().to_string();
            let mut x = a[0].float() + h.world.canvas.origin.0;
            let mut y = a[1].float() + h.world.canvas.origin.1;
            if a[3].int() != 0 {
                x -= (font.width(&s) / 2.0).floor();
            }
            if a[4].int() != 0 {
                y -= (font.height() / 2.0).floor();
            }
            let c = h.world.canvas.color;
            h.with_target(target, |t| t.text(&font, x as i32, y as i32, &s, image::Rgba([c.r, c.g, c.b, 255])))?;
            return VOID;
        }
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
        h.world.cursor = (0.0, 0.0);
        let target = h.current_buffer()?;
        if target != crate::buffer::Buffer::Screen {
            let c = h.world.canvas.cls_color;
            h.with_target(target, |t| {
                let (w, ht) = (t.w, t.h);
                t.rect(0, 0, w, ht, true, image::Rgba([c.r, c.g, c.b, 255]));
            })?;
            return VOID;
        }
        h.world.canvas.items.clear();
        // without a camera, Cls is what defines the background
        if !h.entities().any(|(_, e)| matches!(e.kind, Kind::Camera { .. })) {
            let c = h.world.canvas.cls_color;
            h.scene()?.rendering_options.get_value_mut_silent().clear_color = Some(srgb_to_linear(c));
        }
        VOID
    }),
    ("Origin%x%y", |h, a, _| {
        h.world.canvas.origin = (a[0].float(), a[1].float());
        VOID
    }),
    ("Rect%x%y%width%height%solid=1", |h, a, _| {
        let (x, y, w, ht, solid) = (a[0].float(), a[1].float(), a[2].float(), a[3].float(), a[4].int() != 0);
        draw2d(h, |c| c.rect(x, y, w, ht, solid), |t, col, (ox, oy)| t.rect(x as i32 + ox, y as i32 + oy, w as i32, ht as i32, solid, col))
    }),
    ("Oval%x%y%width%height%solid=1", |h, a, _| {
        let (x, y, w, ht, solid) = (a[0].float(), a[1].float(), a[2].float(), a[3].float(), a[4].int() != 0);
        draw2d(h, |c| c.oval(x, y, w, ht, solid), |t, col, (ox, oy)| t.oval(x as i32 + ox, y as i32 + oy, w as i32, ht as i32, solid, col))
    }),
    ("Line%x1%y1%x2%y2", |h, a, _| {
        let (x1, y1, x2, y2) = (a[0].float(), a[1].float(), a[2].float(), a[3].float());
        draw2d(h, |c| c.line(x1, y1, x2, y2), |t, col, (ox, oy)| t.line(x1 as i32 + ox, y1 as i32 + oy, x2 as i32 + ox, y2 as i32 + oy, col))
    }),
    ("Plot%x%y", |h, a, _| {
        let (x, y) = (a[0].float(), a[1].float());
        draw2d(h, |c| c.rect(x, y, 1.0, 1.0, true), |t, col, (ox, oy)| t.plot(x as i32 + ox, y as i32 + oy, col))
    }),
    // ---- buffers and pixels
    ("%BackBuffer", |_, _, _| int(crate::buffer::BACK)),
    ("%FrontBuffer", |_, _, _| int(crate::buffer::FRONT)),
    ("%GraphicsBuffer", |h, _, _| int(h.world.buffer)),
    ("%ImageBuffer%image%frame=0", |h, a, _| {
        let (i, f) = (a[0].int(), a[1].int());
        h.image(i)?.frame_rect(f.max(0) as u32)?;
        int(crate::buffer::image_buffer(i, f))
    }),
    ("%TextureBuffer%texture%frame=0", |h, a, _| {
        h.texture(a[0].int())?;
        int(crate::buffer::texture_buffer(a[0].int()))
    }),
    ("SetBuffer%buffer", |h, a, _| {
        let b = a[0].int();
        crate::buffer::decode(b)?;
        h.world.buffer = b;
        VOID
    }),
    ("LockBuffer%buffer=0", |h, a, _| {
        buffer_arg(h, a[0].int())?;
        VOID
    }),
    ("UnlockBuffer%buffer=0", |h, a, _| {
        buffer_arg(h, a[0].int())?;
        h.commit_buffers()?;
        VOID
    }),
    ("%ReadPixel%x%y%buffer=0", |h, a, _| {
        let b = buffer_arg(h, a[2].int())?;
        if b == crate::buffer::Buffer::Screen {
            return int(0); // the screen cannot be read back
        }
        let (x, y) = (a[0].int(), a[1].int());
        int(crate::buffer::rgba_to_argb(h.with_target(b, |t| t.get(x, y))?))
    }),
    ("%ReadPixelFast%x%y%buffer=0", |h, a, _| {
        let b = buffer_arg(h, a[2].int())?;
        if b == crate::buffer::Buffer::Screen {
            return int(0);
        }
        let (x, y) = (a[0].int(), a[1].int());
        int(crate::buffer::rgba_to_argb(h.with_target(b, |t| t.get(x, y))?))
    }),
    ("WritePixel%x%y%argb%buffer=0", |h, a, _| write_pixel(h, a)),
    ("WritePixelFast%x%y%argb%buffer=0", |h, a, _| write_pixel(h, a)),
    ("CopyRect%src_x%src_y%width%height%dest_x%dest_y%src_buffer=0%dest_buffer=0", |h, a, _| {
        let (src, dst) = (buffer_arg(h, a[6].int())?, buffer_arg(h, a[7].int())?);
        if src == crate::buffer::Buffer::Screen {
            return Err("CopyRect cannot read from the screen; copy from an image or texture buffer".into());
        }
        let n = |i: usize| a[i].int();
        let block = h.with_target(src, |t| {
            let mut out = image::RgbaImage::new(n(2).max(0) as u32, n(3).max(0) as u32);
            for y in 0..n(3).max(0) {
                for x in 0..n(2).max(0) {
                    out.put_pixel(x as u32, y as u32, t.get(n(0) + x, n(1) + y));
                }
            }
            out
        })?;
        if dst == crate::buffer::Buffer::Screen {
            // show it on the screen layer through a temporary image
            let img = crate::image2d::Image2D::new(block, 0, 0, 0, 1)?;
            let (w, ht) = img.size();
            h.world.canvas.image(n(4) as f32, n(5) as f32, w, ht, Rect::new(0.0, 0.0, 1.0, 1.0), img.block.clone());
        } else {
            let (w, ht) = (block.width() as i32, block.height() as i32);
            h.with_target(dst, |t| t.blit(&block, 0, 0, w, ht, n(4), n(5), None))?;
        }
        VOID
    }),
    // ---- images (bbgraphics.cpp)
    ("%LoadImage$file", |h, a, _| add_image(h, crate::image2d::Image2D::load(&a[0].str(), 0, 0, 0, 1))),
    ("%LoadAnimImage$file%width%height%first%count", |h, a, _| {
        let n = |i: usize| a[i].int().max(0) as u32;
        add_image(h, crate::image2d::Image2D::load(&a[0].str(), n(1), n(2), n(3), n(4)))
    }),
    ("FreeImage%image", |h, a, _| {
        h.image(a[0].int())?;
        h.world.images[a[0].int() as usize - 1] = None;
        VOID
    }),
    ("DrawImage%image%x%y%frame=0", |h, a, _| draw_image(h, a, true, None)),
    ("DrawBlock%image%x%y%frame=0", |h, a, _| draw_image(h, a, false, None)),
    ("DrawImageRect%image%x%y%rect_x%rect_y%rect_width%rect_height%frame=0", |h, a, _| {
        let r = Rect::new(a[3].float(), a[4].float(), a[5].float(), a[6].float());
        let args = [a[0].clone(), a[1].clone(), a[2].clone(), a[7].clone()];
        draw_image(h, &args, true, Some(r))
    }),
    ("DrawBlockRect%image%x%y%rect_x%rect_y%rect_width%rect_height%frame=0", |h, a, _| {
        let r = Rect::new(a[3].float(), a[4].float(), a[5].float(), a[6].float());
        let args = [a[0].clone(), a[1].clone(), a[2].clone(), a[7].clone()];
        draw_image(h, &args, false, Some(r))
    }),
    ("%CreateImage%width%height%frames=1", |h, a, _| {
        add_image(h, crate::image2d::Image2D::blank(a[0].int().max(0) as u32, a[1].int().max(0) as u32, a[2].int().max(1) as u32))
    }),
    ("%CopyImage%image", |h, a, _| {
        let c = h.image(a[0].int())?.duplicate();
        h.world.images.push(Some(c));
        int(h.world.images.len() as i32)
    }),
    ("GrabImage%image%x%y%frame=0", |h, a, _| {
        let src = h.current_buffer()?;
        if src == crate::buffer::Buffer::Screen {
            return Err("GrabImage cannot read the screen; SetBuffer to an image or texture buffer first".into());
        }
        let (i, x, y, f) = (a[0].int(), a[1].int(), a[2].int(), a[3].int().max(0) as u32);
        let (_, _, w, ht) = h.image(i)?.frame_rect(f)?;
        let block = h.with_target(src, |t| {
            let mut out = image::RgbaImage::new(w as u32, ht as u32);
            for yy in 0..ht {
                for xx in 0..w {
                    out.put_pixel(xx as u32, yy as u32, t.get(x + xx, y + yy));
                }
            }
            out
        })?;
        h.with_target(crate::buffer::Buffer::Image { id: i, frame: f }, |t| t.blit(&block, 0, 0, w, ht, 0, 0, None))?;
        VOID
    }),
    ("RotateImage%image#angle", |h, a, _| {
        h.image_mut(a[0].int())?.rotate(a[1].float())?;
        VOID
    }),
    ("TileImage%image%x=0%y=0%frame=0", |h, a, _| tile_image(h, a, true)),
    ("TileBlock%image%x=0%y=0%frame=0", |h, a, _| tile_image(h, a, false)),
    ("%ImageRectCollide%image%x%y%frame%rect_x%rect_y%rect_width%rect_height", |h, a, _| {
        let img = h.image(a[0].int())?;
        let (l, t) = (a[1].float() - img.handle.0, a[2].float() - img.handle.1);
        let frame = a[3].int().max(0) as u32;
        let (rx, ry, rw, rh) = (a[4].float(), a[5].float(), a[6].float(), a[7].float());
        let mut hit = false;
        'outer: for y in ry.max(t).floor() as i32..(ry + rh).min(t + img.frame_h as f32).ceil() as i32 {
            for x in rx.max(l).floor() as i32..(rx + rw).min(l + img.frame_w as f32).ceil() as i32 {
                if img.solid(frame, x - l as i32, y - t as i32) {
                    hit = true;
                    break 'outer;
                }
            }
        }
        int(hit as i32)
    }),
    ("%ImageWidth%image", |h, a, _| int(h.image(a[0].int())?.size().0.round() as i32)),
    ("%ImageHeight%image", |h, a, _| int(h.image(a[0].int())?.size().1.round() as i32)),
    ("%ImageXHandle%image", |h, a, _| int(h.image(a[0].int())?.handle.0 as i32)),
    ("%ImageYHandle%image", |h, a, _| int(h.image(a[0].int())?.handle.1 as i32)),
    ("%ImageFrames%image", |h, a, _| int(h.image(a[0].int())?.frames as i32)), // (ext)
    ("HandleImage%image%x%y", |h, a, _| {
        h.image_mut(a[0].int())?.handle = (a[1].float(), a[2].float());
        VOID
    }),
    ("MidHandle%image", |h, a, _| {
        let img = h.image_mut(a[0].int())?;
        img.handle = ((img.frame_w / 2) as f32, (img.frame_h / 2) as f32);
        VOID
    }),
    ("AutoMidHandle%enable", |h, a, _| {
        h.world.auto_mid_handle = a[0].int() != 0;
        VOID
    }),
    ("MaskImage%image%red%green%blue", |h, a, _| {
        let c = |i: usize| a[i].int().clamp(0, 255) as u8;
        let mask = [c(1), c(2), c(3)];
        h.image_mut(a[0].int())?.set_mask(mask);
        VOID
    }),
    ("ScaleImage%image#x_scale#y_scale", |h, a, _| {
        h.image_mut(a[0].int())?.scale = (a[1].float(), a[2].float());
        VOID
    }),
    ("ResizeImage%image#width#height", |h, a, _| {
        let img = h.image_mut(a[0].int())?;
        img.scale = (a[1].float() / img.frame_w as f32, a[2].float() / img.frame_h as f32);
        VOID
    }),
    ("%RectsOverlap%x1%y1%width1%height1%x2%y2%width2%height2", |_, a, _| {
        let f = |i: usize| a[i].float();
        int(crate::image2d::rects_overlap((f(0), f(1), f(2), f(3)), (f(4), f(5), f(6), f(7))) as i32)
    }),
    ("%ImagesOverlap%image1%x1%y1%image2%x2%y2", |h, a, _| {
        let r = |img: &crate::image2d::Image2D, x: f32, y: f32| {
            let (l, t) = img.top_left(x, y);
            let (w, ht) = img.size();
            (l, t, w, ht)
        };
        let ra = r(h.image(a[0].int())?, a[1].float(), a[2].float());
        let rb = r(h.image(a[3].int())?, a[4].float(), a[5].float());
        int(crate::image2d::rects_overlap(ra, rb) as i32)
    }),
    ("%ImagesCollide%image1%x1%y1%frame1%image2%x2%y2%frame2", |h, a, _| {
        let (ia, ib) = (h.image(a[0].int())?, h.image(a[4].int())?);
        let hit = crate::image2d::images_collide(
            ia, a[1].float(), a[2].float(), a[3].int().max(0) as u32,
            ib, a[5].float(), a[6].float(), a[7].int().max(0) as u32,
        );
        int(hit as i32)
    }),
    ("%ImageRectOverlap%image%x%y%rect_x%rect_y%rect_width%rect_height", |h, a, _| {
        let img = h.image(a[0].int())?;
        let (l, t) = img.top_left(a[1].float(), a[2].float());
        let (w, ht) = img.size();
        int(crate::image2d::rects_overlap((l, t, w, ht), (a[3].float(), a[4].float(), a[5].float(), a[6].float())) as i32)
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
    // ---- collisions
    ("EntityType%entity%collision_type%recursive=0", |h, a, _| {
        let (e, t) = (a[0].int(), a[1].int());
        let mut stack = vec![e];
        while let Some(i) = stack.pop() {
            let pos = h.world_of(i)?.position();
            let ent = h.ent_mut(i)?;
            ent.col_type = t;
            ent.last_pos = Some(pos);
            if a[2].int() != 0 {
                stack.extend(ent.children.iter().map(|&c| c as i32));
            }
        }
        VOID
    }),
    ("%GetEntityType%entity", |h, a, _| int(h.ent(a[0].int())?.col_type)),
    ("EntityRadius%entity#x_radius#y_radius=0", |h, a, _| {
        let rx = a[1].float().abs();
        let ry = if a[2].float() == 0.0 { rx } else { a[2].float().abs() };
        h.ent_mut(a[0].int())?.radius = (rx.max(1e-4), ry.max(1e-4));
        VOID
    }),
    ("EntityBox%entity#x#y#z#width#height#depth", |h, a, _| {
        let b = [a[1].float(), a[2].float(), a[3].float(), a[4].float(), a[5].float(), a[6].float()];
        h.ent_mut(a[0].int())?.col_box = Some(b);
        VOID
    }),
    ("Collisions%src_type%dest_type%method%response", |h, a, _| {
        let (src, dst, method) = (a[0].int(), a[1].int(), a[2].int());
        if !(1..=3).contains(&method) {
            return Err(format!("Collisions method must be 1 (sphere), 2 (polygon) or 3 (box), got {method}"));
        }
        let response = crate::collision::Response::from_blitz(a[3].int());
        let rules = &mut h.world.collision_rules;
        // a later rule for the same pair replaces the earlier one
        rules.retain(|r| !(r.0 == src && r.1 == dst));
        rules.push((src, dst, method, response));
        VOID
    }),
    ("ClearCollisions", |h, _, _| {
        h.world.collision_rules.clear();
        VOID
    }),
    ("ResetEntity%entity", |h, a, _| {
        let e = a[0].int();
        let pos = h.world_of(e)?.position();
        let ent = h.ent_mut(e)?;
        ent.last_pos = Some(pos);
        ent.contacts.clear();
        VOID
    }),
    ("%EntityCollided%entity%type", |h, a, _| {
        let t = a[1].int();
        let contacts = h.ent(a[0].int())?.contacts.clone();
        for c in contacts {
            if h.ent(c.entity).is_ok_and(|e| e.col_type == t) {
                return int(c.entity);
            }
        }
        int(0)
    }),
    ("%CountCollisions%entity", |h, a, _| int(h.ent(a[0].int())?.contacts.len() as i32)),
    ("#CollisionX%entity%index", |h, a, _| contact(h, a, |c| c.point.x)),
    ("#CollisionY%entity%index", |h, a, _| contact(h, a, |c| c.point.y)),
    ("#CollisionZ%entity%index", |h, a, _| contact(h, a, |c| c.point.z)),
    ("#CollisionNX%entity%index", |h, a, _| contact(h, a, |c| c.normal.x)),
    ("#CollisionNY%entity%index", |h, a, _| contact(h, a, |c| c.normal.y)),
    ("#CollisionNZ%entity%index", |h, a, _| contact(h, a, |c| c.normal.z)),
    ("#CollisionTime%entity%index", |h, a, _| contact(h, a, |c| c.time)),
    ("%CollisionEntity%entity%index", |h, a, _| int(nth_contact(h, a)?.entity)),
    // ---- picking
    ("EntityPickMode%entity%pick_geometry%obscurer=1", |h, a, _| {
        let mode = a[1].int();
        if !(0..=3).contains(&mode) {
            return Err(format!("EntityPickMode geometry must be 0-3, got {mode}"));
        }
        let e = h.ent_mut(a[0].int())?;
        e.pick_mode = mode;
        e.obscurer = a[2].int() != 0;
        VOID
    }),
    ("%LinePick#x#y#z#dx#dy#dz#radius=0", |h, a, _| {
        let hit = pick_line(h, v3(a, 0), v3(a, 3), a[6].float(), &[], false)?;
        picked(h, hit)
    }),
    ("%EntityPick%entity#range", |h, a, _| {
        let e = a[0].int();
        let w = h.world_of(e)?;
        let dir = w.rot * Vector3::z() * a[1].float();
        let hit = pick_line(h, w.position(), dir, 0.0, &[e], false)?;
        picked(h, hit)
    }),
    ("%CameraPick%camera#viewport_x#viewport_y", |h, a, _| {
        let cam = a[0].int();
        let (origin, dir) = camera_ray(h, cam, a[1].float(), a[2].float())?;
        let hit = pick_line(h, origin, dir, 0.0, &[cam], false)?;
        picked(h, hit)
    }),
    ("#PickedX", |h, _, _| float(h.world.picked.map_or(0.0, |p| p.point.x))),
    ("#PickedY", |h, _, _| float(h.world.picked.map_or(0.0, |p| p.point.y))),
    ("#PickedZ", |h, _, _| float(h.world.picked.map_or(0.0, |p| p.point.z))),
    ("#PickedNX", |h, _, _| float(h.world.picked.map_or(0.0, |p| p.normal.x))),
    ("#PickedNY", |h, _, _| float(h.world.picked.map_or(0.0, |p| p.normal.y))),
    ("#PickedNZ", |h, _, _| float(h.world.picked.map_or(0.0, |p| p.normal.z))),
    ("#PickedTime", |h, _, _| float(h.world.picked.map_or(0.0, |p| p.time))),
    ("%PickedEntity", |h, _, _| int(h.world.picked.map_or(0, |p| p.entity))),
    ("%PickedTriangle", |h, _, _| int(h.world.picked.map_or(0, |p| p.triangle.max(0)))),
    ("%PickedSurface", |h, _, _| int(h.world.picked.map_or(0, |p| (p.triangle >= 0) as i32))),
    ("CameraProject%camera#x#y#z", |h, a, _| {
        h.world.projected = camera_project(h, a[0].int(), v3(a, 1))?.unwrap_or((0.0, 0.0, 0.0));
        VOID
    }),
    ("#ProjectedX", |h, _, _| float(h.world.projected.0)),
    ("#ProjectedY", |h, _, _| float(h.world.projected.1)),
    ("#ProjectedZ", |h, _, _| float(h.world.projected.2)),
    ("%EntityInView%entity%camera", |h, a, _| {
        let p = h.world_of(a[0].int())?.position();
        let (w, ht) = (h.world.width as f32, h.world.height as f32);
        let inside = camera_project(h, a[1].int(), p)?.is_some_and(|(x, y, _)| x >= 0.0 && y >= 0.0 && x < w && y < ht);
        int(inside as i32)
    }),
    ("%EntityVisible%src_entity%dest_entity", |h, a, _| {
        let (s, d) = (a[0].int(), a[1].int());
        let (p, q) = (h.world_of(s)?.position(), h.world_of(d)?.position());
        let hit = pick_line(h, p, q - p, 0.0, &[s, d], true)?;
        int(hit.is_none() as i32)
    }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_intersection() {
        let v = |x: f32, y: f32, z: f32| Vector3::new(x, y, z);
        // a triangle in the XY plane and one crossing it along Z
        let a = [[v(-1., -1., 0.), v(1., -1., 0.), v(0., 1., 0.)]];
        let b = [[v(0., 0., -1.), v(0., 0., 1.), v(0.2, 0.5, 0.)]];
        assert!(triangles_intersect(&a, &b));
        // moved away: no contact
        let c = [[v(5., 0., -1.), v(5., 0., 1.), v(5.2, 0.5, 0.)]];
        assert!(!triangles_intersect(&a, &c));
        assert!(!triangles_intersect(&a, &[]));
    }

    #[test]
    fn aliases_resolve_and_names_are_unique() {
        let t = table();
        assert!(t.len() > COMMANDS.len());
        let mut names: Vec<String> =
            t.iter().map(|(s, _)| neptune_lang::commands::parse_sig(s).name.to_lowercase()).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate command names");
    }
}
