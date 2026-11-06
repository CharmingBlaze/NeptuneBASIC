//! Screenshots of the game window (`SaveBuffer BackBuffer(), file$`) on
//! every platform.
//!
//! The frame on screen is read from the front buffer of the window through
//! Fyrox's own OpenGL context, at the start of the update after the program
//! asked (when the asked-for frame has just been shown). Every piece of GL
//! state touched is restored, so Fyrox's renderer is not disturbed.

use fyrox::graphics::server::GraphicsServer;
use fyrox::graphics_gl::server::GlGraphicsServer;
use glow::HasContext;
use image::RgbaImage;

/// Reads the window's front buffer (`w`×`h` physical pixels).
pub fn capture(server: &dyn GraphicsServer, w: u32, h: u32) -> Result<RgbaImage, String> {
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
        let prev_read = gl.get_parameter_i32(glow::READ_BUFFER);
        let prev_align = gl.get_parameter_i32(glow::PACK_ALIGNMENT);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        gl.read_buffer(glow::FRONT);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut rgba)));
        let err = gl.get_error();
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, prev_align);
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
