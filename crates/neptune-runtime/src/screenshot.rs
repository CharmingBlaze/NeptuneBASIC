//! Screenshots of the game window (`SaveBuffer BackBuffer(), file$`) on
//! every platform.
//!
//! A frame can only be read back reliably after it is drawn and before it is
//! shown: once presented, the window's buffers are undefined in OpenGL (black
//! or stale on many drivers, always so with vsync on some). Fyrox draws and
//! presents in one call, so its graphics server is wrapped ([`ShotServer`])
//! to get in between. `SaveBuffer` waits until the next frame has been read
//! (see [`service`]), so the picture is the frame the program last flipped.
//! Every piece of GL state touched is restored, so Fyrox's renderer is not
//! disturbed.

use std::cell::{Cell, RefCell};
use std::rc::Weak;
use std::time::{Duration, Instant};

use fyrox::engine::GraphicsContext;
use fyrox::graphics::buffer::{GpuBuffer, GpuBufferDescriptor};
use fyrox::graphics::error::FrameworkError;
use fyrox::graphics::framebuffer::{Attachment, GpuFrameBuffer};
use fyrox::graphics::geometry_buffer::{GpuGeometryBuffer, GpuGeometryBufferDescriptor};
use fyrox::graphics::gpu_program::{GpuProgram, GpuShader, ShaderKind, ShaderResourceDefinition};
use fyrox::graphics::gpu_texture::{GpuTexture, GpuTextureDescriptor};
use fyrox::graphics::query::GpuQuery;
use fyrox::graphics::read_buffer::GpuAsyncReadBuffer;
use fyrox::graphics::sampler::{GpuSampler, GpuSamplerDescriptor};
use fyrox::graphics::server::{GraphicsServer, ServerCapabilities, ServerMemoryUsage, SharedGraphicsServer};
use fyrox::graphics::stats::PipelineStatistics;
use fyrox::graphics::{PolygonFace, PolygonFillMode};
use fyrox::graphics_gl::server::GlGraphicsServer;
use fyrox::renderer::Renderer;
use glow::HasContext;
use image::RgbaImage;

use crate::world::World;

/// How long `SaveBuffer` waits for a frame to be drawn (a minimized or
/// hidden window draws none) before it gives up and returns 0.
const TIMEOUT: Duration = Duration::from_secs(2);

/// Serves a screenshot the program is waiting for; called before and after
/// the program runs each update. Asks for the next frame to be read, and
/// saves it to the file once it has been.
pub fn service(world: &mut World, since: &mut Option<Instant>, graphics: &mut GraphicsContext) {
    let Some(path) = world.pending_screenshot.clone() else {
        return;
    };
    let started = *since.get_or_insert_with(Instant::now);
    let late = started.elapsed() > TIMEOUT;
    let mut shot = None;
    if let GraphicsContext::Initialized(g) = graphics {
        let size = g.window.inner_size();
        let server = hook(&mut g.renderer);
        shot = server.shot.take();
        if shot.is_some() || late {
            server.want.set(None);
        } else {
            server.want.set(Some((size.width, size.height)));
        }
    }
    if shot.is_none() && late {
        shot = Some(Err("the window is not being drawn".into()));
    }
    if let Some(shot) = shot {
        let saved = shot.and_then(|img| save(&img, &path));
        if let Err(e) = &saved {
            eprintln!("SaveBuffer: {e}");
        }
        world.pending_screenshot = None;
        world.screenshot_saved = Some(saved.is_ok());
        *since = None;
    }
}

/// The renderer's graphics server as a [`ShotServer`], wrapping it first if
/// it is not one yet (a new renderer after the graphics context was remade).
fn hook(renderer: &mut Renderer) -> &ShotServer {
    if !renderer.server.as_any().is::<ShotServer>() {
        let inner = renderer.server.clone();
        renderer.server = std::rc::Rc::new(ShotServer { inner, want: Cell::new(None), shot: RefCell::new(None) });
    }
    renderer.server.as_any().downcast_ref::<ShotServer>().expect("just wrapped")
}

/// Fyrox's graphics server with one change: when a screenshot is wanted, the
/// finished frame is read just before it is presented.
struct ShotServer {
    inner: SharedGraphicsServer,
    /// Window size in physical pixels when the next frame is to be read.
    want: Cell<Option<(u32, u32)>>,
    /// The frame that was read, or why it could not be.
    shot: RefCell<Option<Result<RgbaImage, String>>>,
}

impl GraphicsServer for ShotServer {
    fn swap_buffers(&self) -> Result<(), FrameworkError> {
        if let Some((w, h)) = self.want.take() {
            *self.shot.borrow_mut() = Some(capture(&*self.inner, w, h));
        }
        self.inner.swap_buffers()
    }
    fn create_buffer(&self, desc: GpuBufferDescriptor) -> Result<GpuBuffer, FrameworkError> {
        self.inner.create_buffer(desc)
    }
    fn create_texture(&self, desc: GpuTextureDescriptor) -> Result<GpuTexture, FrameworkError> {
        self.inner.create_texture(desc)
    }
    fn create_sampler(&self, desc: GpuSamplerDescriptor) -> Result<GpuSampler, FrameworkError> {
        self.inner.create_sampler(desc)
    }
    fn create_frame_buffer(&self, depth: Option<Attachment>, color: Vec<Attachment>) -> Result<GpuFrameBuffer, FrameworkError> {
        self.inner.create_frame_buffer(depth, color)
    }
    fn back_buffer(&self) -> GpuFrameBuffer {
        self.inner.back_buffer()
    }
    fn create_query(&self) -> Result<GpuQuery, FrameworkError> {
        self.inner.create_query()
    }
    fn create_shader(&self, name: String, kind: ShaderKind, source: String, resources: &[ShaderResourceDefinition], line_offset: isize) -> Result<GpuShader, FrameworkError> {
        self.inner.create_shader(name, kind, source, resources, line_offset)
    }
    fn create_program(&self, name: &str, vs: String, vs_offset: isize, fs: String, fs_offset: isize, resources: &[ShaderResourceDefinition]) -> Result<GpuProgram, FrameworkError> {
        self.inner.create_program(name, vs, vs_offset, fs, fs_offset, resources)
    }
    fn create_program_from_shaders(&self, name: &str, vs: &GpuShader, fs: &GpuShader, resources: &[ShaderResourceDefinition]) -> Result<GpuProgram, FrameworkError> {
        self.inner.create_program_from_shaders(name, vs, fs, resources)
    }
    fn create_async_read_buffer(&self, name: &str, pixel_size: usize, pixel_count: usize) -> Result<GpuAsyncReadBuffer, FrameworkError> {
        self.inner.create_async_read_buffer(name, pixel_size, pixel_count)
    }
    fn create_geometry_buffer(&self, desc: GpuGeometryBufferDescriptor) -> Result<GpuGeometryBuffer, FrameworkError> {
        self.inner.create_geometry_buffer(desc)
    }
    fn weak(&self) -> Weak<dyn GraphicsServer> {
        self.inner.weak()
    }
    fn flush(&self) {
        self.inner.flush()
    }
    fn finish(&self) {
        self.inner.finish()
    }
    fn invalidate_resource_bindings_cache(&self) {
        self.inner.invalidate_resource_bindings_cache()
    }
    fn pipeline_statistics(&self) -> PipelineStatistics {
        self.inner.pipeline_statistics()
    }
    fn set_frame_size(&self, new_size: (u32, u32)) {
        self.inner.set_frame_size(new_size)
    }
    fn capabilities(&self) -> ServerCapabilities {
        self.inner.capabilities()
    }
    fn set_polygon_fill_mode(&self, face: PolygonFace, mode: PolygonFillMode) {
        self.inner.set_polygon_fill_mode(face, mode)
    }
    fn generate_mipmap(&self, texture: &GpuTexture) {
        self.inner.generate_mipmap(texture)
    }
    fn memory_usage(&self) -> ServerMemoryUsage {
        self.inner.memory_usage()
    }
    fn push_debug_group(&self, name: &str) {
        self.inner.push_debug_group(name)
    }
    fn pop_debug_group(&self) {
        self.inner.pop_debug_group()
    }
}

/// What the graphics card calls itself (OpenGL's renderer string), such as
/// "Intel(R) HD Graphics 4400".
pub fn card_name(server: &dyn GraphicsServer) -> Option<String> {
    let any = server.as_any();
    let gl_server = match any.downcast_ref::<ShotServer>() {
        Some(wrapped) => wrapped.inner.as_any().downcast_ref::<GlGraphicsServer>(),
        None => any.downcast_ref::<GlGraphicsServer>(),
    }?;
    // SAFETY: a plain GL query on the context Fyrox keeps current on this thread.
    Some(unsafe { gl_server.gl.get_parameter_string(glow::RENDERER) })
}

/// Reads the frame just drawn and not yet presented: the window's back
/// buffer (`w`×`h` physical pixels).
fn capture(server: &dyn GraphicsServer, w: u32, h: u32) -> Result<RgbaImage, String> {
    if w == 0 || h == 0 {
        return Err("window has no size".into());
    }
    let gl_server = server.as_any().downcast_ref::<GlGraphicsServer>().ok_or("screenshots need the OpenGL renderer")?;
    let gl = &gl_server.gl;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    // SAFETY: plain GL queries and reads on the current context, which Fyrox
    // keeps current on this thread; every binding changed here is restored.
    unsafe {
        let prev_fb = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
        let prev_pbo = gl.get_parameter_i32(glow::PIXEL_PACK_BUFFER_BINDING);
        let prev_align = gl.get_parameter_i32(glow::PACK_ALIGNMENT);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        // the read buffer belongs to the framebuffer just bound (the window)
        let prev_read = gl.get_parameter_i32(glow::READ_BUFFER);
        gl.read_buffer(glow::BACK);
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        // an error left by earlier drawing is not a failed read
        while gl.get_error() != glow::NO_ERROR {}
        gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut rgba)));
        let err = gl.get_error();
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, prev_align);
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, std::num::NonZeroU32::new(prev_pbo as u32).map(glow::NativeBuffer));
        gl.read_buffer(prev_read as u32);
        let fb = std::num::NonZeroU32::new(prev_fb as u32).map(glow::NativeFramebuffer);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, fb);
        if err != glow::NO_ERROR {
            return Err(format!("could not read the screen (GL error {err:#x})"));
        }
    }
    let mut img = RgbaImage::from_raw(w, h, rgba).ok_or("bad capture size")?;
    // GL rows start at the bottom
    image::imageops::flip_vertical_in_place(&mut img);
    for p in img.pixels_mut() {
        p[3] = 255;
    }
    Ok(img)
}

/// Saves an image; the format follows the file extension (png, bmp, jpg, tga).
pub fn save(img: &RgbaImage, path: &str) -> Result<(), String> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".bmp") {
        // formats without alpha
        image::DynamicImage::ImageRgba8(img.clone()).to_rgb8().save(path).map_err(|e| format!("cannot save '{path}': {e}"))
    } else {
        img.save(path).map_err(|e| format!("cannot save '{path}': {e}"))
    }
}
