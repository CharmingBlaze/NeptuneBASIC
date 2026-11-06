//! Movies (`OpenMovie`, `DrawMovie`, `MoviePlaying`, ...).
//!
//! Blitz3D played video through DirectShow. NeptuneBASIC plays, on every
//! platform and without extra libraries:
//!
//! * animated GIF files;
//! * Motion-JPEG AVI files (each frame a JPEG; uncompressed 24/32-bit AVI
//!   frames work too) – the classic format for game cut-scenes, which tools
//!   such as `ffmpeg -i in.mp4 -c:v mjpeg -q:v 3 out.avi` produce;
//! * a folder of numbered images (`OpenMovie "intro/"`), played at 30 frames
//!   a second.
//!
//! Movies play once in real time from `OpenMovie`; `DrawMovie` shows the
//! current frame (decoded on demand) as a 2D image. Sound tracks are not
//! played: use `PlayMusic` alongside.

use std::time::Instant;

use fyrox::{
    asset::untyped::ResourceKind,
    core::uuid::Uuid,
    resource::texture::{Texture, TextureKind, TextureMagnificationFilter, TextureMinificationFilter, TexturePixelKind, TextureResource},
};
use image::{AnimationDecoder, RgbaImage};

enum Frames {
    /// Decoded frames with their display times in seconds.
    Decoded(Vec<(RgbaImage, f32)>),
    /// AVI frames, decoded when shown.
    Avi { frames: Vec<AviFrame>, frame_time: f32 },
    /// Image files, loaded when shown.
    Files { paths: Vec<std::path::PathBuf>, frame_time: f32 },
}

enum AviFrame {
    Jpeg(Vec<u8>),
    /// Uncompressed DIB: bottom-up rows of BGR or BGRA.
    Raw { data: Vec<u8>, bpp: u32 },
}

pub struct Movie {
    frames: Frames,
    pub width: u32,
    pub height: u32,
    start: Instant,
    shown: Option<usize>,
    pub texture: TextureResource,
}

impl std::fmt::Debug for Movie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Movie({}x{}, {} frames)", self.width, self.height, self.count())
    }
}

fn texture(w: u32, h: u32) -> TextureResource {
    let mut tex = Texture::from_bytes(TextureKind::Rectangle { width: w, height: h }, TexturePixelKind::RGBA8, vec![0; (w * h * 4) as usize])
        .expect("RGBA buffer matches its size");
    tex.set_minification_filter(TextureMinificationFilter::Linear);
    tex.set_magnification_filter(TextureMagnificationFilter::Linear);
    TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, tex)
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4).map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()))
}

/// An AVI video frame: its bytes, whether it is a JPEG, and bits per pixel.
pub type AviChunk = (Vec<u8>, bool, u32);

/// Reads an AVI file: frame size, frame time and the video frames.
pub fn parse_avi(data: &[u8]) -> Result<(u32, u32, f32, Vec<AviChunk>), String> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"AVI " {
        return Err("not an AVI file".into());
    }
    let (mut usec, mut w, mut h, mut bpp) = (33_333u32, 0u32, 0u32, 24u32);
    let mut frames = Vec::new();
    // walk chunks; LIST chunks are entered
    let mut stack = vec![(12usize, data.len())];
    while let Some((mut i, end)) = stack.pop() {
        while i + 8 <= end {
            let id = &data[i..i + 4];
            let size = u32_at(data, i + 4) as usize;
            let body = i + 8;
            let next = (body + size + (size & 1)).min(end);
            if id == b"LIST" || id == b"RIFF" {
                stack.push((next, end));
                stack.push((body + 4, (body + size).min(end)));
                break;
            }
            match id {
                b"avih" => {
                    usec = u32_at(data, body).max(1);
                    w = u32_at(data, body + 32);
                    h = u32_at(data, body + 36);
                }
                b"strf" if bpp == 24 && size >= 16 => {
                    // BITMAPINFOHEADER of the video stream
                    let bits = data.get(body + 14..body + 16).map_or(24, |b| u16::from_le_bytes([b[0], b[1]]) as u32);
                    if bits == 24 || bits == 32 {
                        bpp = bits;
                    }
                }
                _ if id[2..4] == *b"dc" || id[2..4] == *b"db" => {
                    let bytes = data[body..(body + size).min(data.len())].to_vec();
                    let jpeg = bytes.starts_with(&[0xFF, 0xD8]);
                    if !bytes.is_empty() {
                        frames.push((bytes, jpeg, bpp));
                    }
                }
                _ => {}
            }
            i = next;
        }
    }
    if frames.is_empty() {
        return Err("the AVI file has no video frames".into());
    }
    Ok((w, h, usec as f32 / 1_000_000.0, frames))
}

impl Movie {
    pub fn open(path: &str) -> Result<Movie, String> {
        let p = std::path::Path::new(path);
        let (frames, w, h) = if p.is_dir() {
            let mut paths: Vec<_> = std::fs::read_dir(p)
                .map_err(|e| format!("cannot read '{path}': {e}"))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|f| f.extension().is_some_and(|x| ["png", "jpg", "jpeg", "bmp", "tga", "gif"].contains(&x.to_string_lossy().to_lowercase().as_str())))
                .collect();
            paths.sort();
            let first = paths.first().ok_or_else(|| format!("no images in '{path}'"))?;
            let (w, h) = image::image_dimensions(first).map_err(|e| e.to_string())?;
            (Frames::Files { paths, frame_time: 1.0 / 30.0 }, w, h)
        } else {
            let data = std::fs::read(p).map_err(|e| format!("cannot read '{path}': {e}"))?;
            if data.starts_with(b"GIF8") {
                let dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
                let frames: Vec<(RgbaImage, f32)> = dec
                    .into_frames()
                    .collect_frames()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|f| {
                        let (n, d) = f.delay().numer_denom_ms();
                        // browsers treat a missing delay as 100 ms
                        let secs = if n == 0 { 0.1 } else { n as f32 / d.max(1) as f32 / 1000.0 };
                        (f.into_buffer(), secs)
                    })
                    .collect();
                let (w, h) = frames.first().map(|f| f.0.dimensions()).ok_or("empty GIF")?;
                (Frames::Decoded(frames), w, h)
            } else if data.starts_with(b"RIFF") {
                let (mut w, mut h, frame_time, raw) = parse_avi(&data)?;
                let frames: Vec<AviFrame> = raw
                    .into_iter()
                    .map(|(bytes, jpeg, bpp)| if jpeg { AviFrame::Jpeg(bytes) } else { AviFrame::Raw { data: bytes, bpp } })
                    .collect();
                if w == 0 || h == 0 {
                    if let Some(AviFrame::Jpeg(j)) = frames.first() {
                        let img = image::load_from_memory(j).map_err(|e| e.to_string())?;
                        (w, h) = (img.width(), img.height());
                    }
                }
                (Frames::Avi { frames, frame_time }, w, h)
            } else {
                return Err(format!("'{path}' is not a movie this version can play (GIF, Motion-JPEG AVI or a folder of images)"));
            }
        };
        if w == 0 || h == 0 {
            return Err(format!("'{path}' has no frame size"));
        }
        Ok(Movie { frames, width: w, height: h, start: Instant::now(), shown: None, texture: texture(w, h) })
    }

    pub fn count(&self) -> usize {
        match &self.frames {
            Frames::Decoded(f) => f.len(),
            Frames::Avi { frames, .. } => frames.len(),
            Frames::Files { paths, .. } => paths.len(),
        }
    }

    /// Frame to show after `t` seconds, or `None` once the movie has ended.
    pub fn frame_at(&self, t: f32) -> Option<usize> {
        match &self.frames {
            Frames::Decoded(f) => {
                let mut acc = 0.0;
                for (i, (_, d)) in f.iter().enumerate() {
                    acc += d;
                    if t < acc {
                        return Some(i);
                    }
                }
                None
            }
            Frames::Avi { frame_time, .. } | Frames::Files { frame_time, .. } => {
                let i = (t / frame_time.max(1e-4)) as usize;
                (i < self.count()).then_some(i)
            }
        }
    }

    pub fn playing(&self) -> bool {
        self.frame_at(self.start.elapsed().as_secs_f32()).is_some()
    }

    fn decode(&self, i: usize) -> Result<RgbaImage, String> {
        let img = match &self.frames {
            Frames::Decoded(f) => f[i].0.clone(),
            Frames::Files { paths, .. } => image::open(&paths[i]).map_err(|e| e.to_string())?.to_rgba8(),
            Frames::Avi { frames, .. } => match &frames[i] {
                AviFrame::Jpeg(j) => image::load_from_memory(j).map_err(|e| e.to_string())?.to_rgba8(),
                AviFrame::Raw { data, bpp } => {
                    let (w, h) = (self.width, self.height);
                    let px = (*bpp / 8) as usize;
                    let stride = (w as usize * px).div_ceil(4) * 4;
                    RgbaImage::from_fn(w, h, |x, y| {
                        // rows are stored bottom-up, BGR(A)
                        let o = (h - 1 - y) as usize * stride + x as usize * px;
                        let c = |k: usize| data.get(o + k).copied().unwrap_or(0);
                        image::Rgba([c(2), c(1), c(0), 255])
                    })
                }
            },
        };
        // frames of another size are fitted to the movie size
        Ok(if img.dimensions() != (self.width, self.height) {
            image::imageops::resize(&img, self.width, self.height, image::imageops::FilterType::Triangle)
        } else {
            img
        })
    }

    /// Uploads the frame due now (if it changed); false once the movie ended.
    pub fn update(&mut self) -> Result<bool, String> {
        let Some(i) = self.frame_at(self.start.elapsed().as_secs_f32()).or_else(|| self.count().checked_sub(1).filter(|_| self.shown.is_none())) else {
            return Ok(false);
        };
        if self.shown != Some(i) {
            let img = self.decode(i)?;
            let tex = self.texture.data_ref();
            let mut t = tex;
            let mut m = t.modify();
            let bytes = m.data_mut();
            if bytes.len() == img.as_raw().len() {
                bytes.copy_from_slice(img.as_raw());
            }
            self.shown = Some(i);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(r: u8) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(8, 4, image::Rgb([r, 0, 0]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg).unwrap();
        out
    }

    fn chunk(id: &[u8], body: &[u8]) -> Vec<u8> {
        let mut c = id.to_vec();
        c.extend_from_slice(&(body.len() as u32).to_le_bytes());
        c.extend_from_slice(body);
        if body.len() % 2 == 1 {
            c.push(0);
        }
        c
    }

    fn list(kind: &[u8], items: &[Vec<u8>]) -> Vec<u8> {
        let mut body = kind.to_vec();
        for i in items {
            body.extend_from_slice(i);
        }
        chunk(b"LIST", &body)
    }

    /// A minimal Motion-JPEG AVI: 25 fps, 8x4, three frames.
    fn avi() -> Vec<u8> {
        let mut avih = vec![0u8; 56];
        avih[0..4].copy_from_slice(&40_000u32.to_le_bytes());
        avih[32..36].copy_from_slice(&8u32.to_le_bytes());
        avih[36..40].copy_from_slice(&4u32.to_le_bytes());
        let hdrl = list(b"hdrl", &[chunk(b"avih", &avih)]);
        let movi = list(b"movi", &[chunk(b"00dc", &jpeg(250)), chunk(b"00dc", &jpeg(120)), chunk(b"00dc", &jpeg(10))]);
        let mut body = b"AVI ".to_vec();
        body.extend(hdrl);
        body.extend(movi);
        chunk(b"RIFF", &body)
    }

    #[test]
    fn reads_motion_jpeg_avi() {
        let (w, h, ft, frames) = parse_avi(&avi()).unwrap();
        assert_eq!((w, h, frames.len()), (8, 4, 3));
        assert!((ft - 0.04).abs() < 1e-6);
        assert!(frames.iter().all(|f| f.1));
        assert!(parse_avi(b"RIFF....WAVE").is_err());
    }

    #[test]
    fn timing_and_decoding() {
        let dir = std::env::temp_dir().join(format!("neptune-movie-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.avi");
        std::fs::write(&path, avi()).unwrap();
        let m = Movie::open(path.to_str().unwrap()).unwrap();
        assert_eq!((m.width, m.height, m.count()), (8, 4, 3));
        assert_eq!(m.frame_at(0.0), Some(0));
        assert_eq!(m.frame_at(0.05), Some(1));
        assert_eq!(m.frame_at(0.2), None);
        let f1 = m.decode(1).unwrap();
        assert!((f1.get_pixel(4, 2)[0] as i32 - 120).abs() < 20);
        assert!(Movie::open(dir.join("missing.avi").to_str().unwrap()).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
