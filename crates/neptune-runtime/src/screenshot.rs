//! Screenshots of the game window (`SaveBuffer BackBuffer(), file$`).
//!
//! Fyrox does not read the window's frame back, so on Windows the pixels are
//! copied from the screen area of the window's client rectangle with GDI
//! (the window is in front while the game runs). Other platforms report
//! that screenshots are not available.

use image::RgbaImage;

/// Copies a screen rectangle (physical pixels) into an image.
#[cfg(windows)]
pub fn capture(x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage, String> {
    use core::ffi::c_void;
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_ppm: i32,
        y_ppm: i32,
        clr_used: u32,
        clr_important: u32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, dc: *mut c_void) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
        fn CreateCompatibleBitmap(dc: *mut c_void, w: i32, h: i32) -> *mut c_void;
        fn SelectObject(dc: *mut c_void, obj: *mut c_void) -> *mut c_void;
        fn BitBlt(dst: *mut c_void, x: i32, y: i32, w: i32, h: i32, src: *mut c_void, sx: i32, sy: i32, rop: u32) -> i32;
        fn GetDIBits(dc: *mut c_void, bmp: *mut c_void, start: u32, lines: u32, bits: *mut c_void, info: *mut BitmapInfoHeader, usage: u32) -> i32;
        fn DeleteObject(obj: *mut c_void) -> i32;
        fn DeleteDC(dc: *mut c_void) -> i32;
    }
    const SRCCOPY: u32 = 0x00CC_0020;
    if w == 0 || h == 0 {
        return Err("window has no size".into());
    }
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    // SAFETY: plain GDI calls on handles created and released here; the
    // buffer is large enough for a 32-bit top-down DIB of w x h pixels.
    let ok = unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w as i32, h as i32);
        let old = SelectObject(mem, bmp);
        let blt = BitBlt(mem, 0, 0, w as i32, h as i32, screen, x, y, SRCCOPY);
        SelectObject(mem, old);
        let mut info = BitmapInfoHeader {
            size: std::mem::size_of::<BitmapInfoHeader>() as u32,
            width: w as i32,
            height: -(h as i32), // top-down rows
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            x_ppm: 0,
            y_ppm: 0,
            clr_used: 0,
            clr_important: 0,
        };
        let lines = GetDIBits(mem, bmp, 0, h, bgra.as_mut_ptr().cast(), &mut info, 0);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
        blt != 0 && lines == h as i32
    };
    if !ok {
        return Err("could not capture the screen".into());
    }
    for p in bgra.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        p[3] = 255;
    }
    RgbaImage::from_raw(w, h, bgra).ok_or_else(|| "bad capture size".to_string())
}

#[cfg(not(windows))]
pub fn capture(_x: i32, _y: i32, _w: u32, _h: u32) -> Result<RgbaImage, String> {
    Err("saving the screen is only available on Windows".into())
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
