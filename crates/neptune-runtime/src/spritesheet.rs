//! Sprite sheets from Aseprite and TexturePacker (ext): `LoadSheetImage`.
//!
//! Both tools write a picture and a JSON file saying where each frame is.
//! Frames may be packed tightly and trimmed of their empty edges, so they
//! are laid back out as equal frames: the result is an ordinary animation
//! image. Aseprite's frame tags and frame times are kept for `Anim2DPlayTag`.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use image::{imageops, RgbaImage};

use crate::json::{self, Json};

#[derive(Debug, Clone, PartialEq)]
pub struct SheetFrame {
    /// Where the frame is in the picture.
    pub rect: (u32, u32, u32, u32),
    /// Where it goes in its whole frame (trimmed frames lost their edges).
    pub offset: (u32, u32),
    pub ms: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetTag {
    pub name: String,
    pub first: i32,
    pub last: i32,
    /// 1 loops forwards or in reverse, 2 ping-pong.
    pub mode: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sheet {
    pub image: String,
    pub frame_w: u32,
    pub frame_h: u32,
    pub frames: Vec<SheetFrame>,
    pub tags: Vec<SheetTag>,
}

/// What an image keeps from its sheet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SheetInfo {
    pub tags: Vec<SheetTag>,
    pub frame_ms: Vec<u32>,
}

/// Orders `walk 2` before `walk 10`. JSON objects do not keep their order
/// here, and Aseprite's default output is an object.
fn natural(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let run = |s: &[u8], mut k: usize| {
                let start = k;
                while k < s.len() && s[k].is_ascii_digit() {
                    k += 1;
                }
                (start, k)
            };
            let ((s1, e1), (s2, e2)) = (run(a, i), run(b, j));
            let number = |s: &[u8]| s.iter().fold(0u64, |n, d| n.saturating_mul(10).saturating_add((d - b'0') as u64));
            match number(&a[s1..e1]).cmp(&number(&b[s2..e2])) {
                Ordering::Equal => {}
                other => return other,
            }
            (i, j) = (e1, e2);
        } else {
            match a[i].cmp(&b[j]) {
                Ordering::Equal => {}
                other => return other,
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

fn uint(v: &Json) -> u32 {
    v.num().map_or(0, |n| n.max(0.0) as u32)
}

/// Reads the JSON of a sheet.
pub fn parse_sheet(text: &str) -> Result<Sheet, String> {
    let doc = json::parse(text)?;
    let list: Vec<&Json> = match doc.get("frames") {
        Json::Arr(a) => a.iter().collect(),
        Json::Obj(m) => {
            let mut named: Vec<(&String, &Json)> = m.iter().collect();
            named.sort_by(|a, b| natural(a.0, b.0));
            named.into_iter().map(|(_, f)| f).collect()
        }
        _ => return Err("not a sprite sheet: it has no frames".into()),
    };
    if list.is_empty() {
        return Err("the sheet has no frames".into());
    }
    let mut sheet = Sheet { image: doc.at("meta.image").str().unwrap_or("").to_string(), ..Sheet::default() };
    if sheet.image.is_empty() {
        return Err("the sheet does not name its image".into());
    }
    for f in list {
        if matches!(f.get("rotated"), Json::Bool(true)) {
            return Err("rotated frames are not supported; turn off rotation when exporting".into());
        }
        let r = f.get("frame");
        let rect = (uint(r.get("x")), uint(r.get("y")), uint(r.get("w")), uint(r.get("h")));
        let source = f.get("sourceSize");
        let (w, h) = if source.is_null() { (rect.2, rect.3) } else { (uint(source.get("w")), uint(source.get("h"))) };
        sheet.frame_w = sheet.frame_w.max(w);
        sheet.frame_h = sheet.frame_h.max(h);
        let at = f.get("spriteSourceSize");
        sheet.frames.push(SheetFrame { rect, offset: (uint(at.get("x")), uint(at.get("y"))), ms: f.get("duration").num().map_or(100, |d| d.max(1.0) as u32) });
    }
    let count = sheet.frames.len() as u32;
    for t in doc.at("meta.frameTags").arr() {
        let (from, to) = (uint(t.get("from")), uint(t.get("to")));
        if from >= count || to >= count {
            return Err(format!("tag '{}' goes past the last frame", t.get("name").str().unwrap_or("")));
        }
        let direction = t.get("direction").str().unwrap_or("forward");
        let (first, last) = if direction == "reverse" { (to, from) } else { (from, to) };
        sheet.tags.push(SheetTag { name: t.get("name").str().unwrap_or("").to_string(), first: first as i32, last: last as i32, mode: if direction.starts_with("pingpong") { 2 } else { 1 } });
    }
    Ok(sheet)
}

/// Equal frames side by side in rows, each put back where it was trimmed from.
pub fn lay_out(sheet: &Sheet, picture: &RgbaImage) -> Result<(RgbaImage, u32), String> {
    let n = sheet.frames.len() as u32;
    let columns = (n as f32).sqrt().ceil() as u32;
    let mut out = RgbaImage::new(columns * sheet.frame_w, n.div_ceil(columns) * sheet.frame_h);
    for (i, f) in sheet.frames.iter().enumerate() {
        let (x, y, w, h) = f.rect;
        if x + w > picture.width() || y + h > picture.height() {
            return Err(format!("frame {i} is outside the picture"));
        }
        let i = i as u32;
        let cell = ((i % columns) * sheet.frame_w + f.offset.0, (i / columns) * sheet.frame_h + f.offset.1);
        imageops::replace(&mut out, &*imageops::crop_imm(picture, x, y, w, h), cell.0 as i64, cell.1 as i64);
    }
    Ok((out, n))
}

/// The picture a sheet names, from the sheet's own folder.
pub fn image_path(json_file: &str, sheet: &Sheet) -> PathBuf {
    Path::new(json_file).parent().unwrap_or(Path::new("")).join(&sheet.image)
}

/// `LoadSheetImage`.
pub fn load(path: &str) -> Result<crate::image2d::Image2D, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("LoadSheetImage: cannot read {path}: {e}"))?;
    let sheet = parse_sheet(&text).map_err(|e| format!("LoadSheetImage: {path}: {e}"))?;
    let file = image_path(path, &sheet);
    let picture = image::open(&file).map_err(|e| format!("LoadSheetImage: cannot load {}: {e}", file.display()))?.to_rgba8();
    let (pixels, frames) = lay_out(&sheet, &picture).map_err(|e| format!("LoadSheetImage: {path}: {e}"))?;
    let mut img = crate::image2d::Image2D::new(pixels, sheet.frame_w, sheet.frame_h, 0, frames)?;
    img.path = path.replace('\\', "/");
    img.use_alpha_only();
    img.sheet = Some(Box::new(SheetInfo { tags: sheet.tags, frame_ms: sheet.frames.iter().map(|f| f.ms).collect() }));
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASEPRITE: &str = r#"{"frames":{
      "hero 10.aseprite":{"frame":{"x":0,"y":0,"w":2,"h":2},"rotated":false,"trimmed":false,"spriteSourceSize":{"x":0,"y":0,"w":2,"h":2},"sourceSize":{"w":2,"h":2},"duration":300},
      "hero 2.aseprite":{"frame":{"x":2,"y":0,"w":1,"h":1},"rotated":false,"trimmed":true,"spriteSourceSize":{"x":1,"y":1,"w":1,"h":1},"sourceSize":{"w":2,"h":2},"duration":50}},
      "meta":{"image":"hero.png","frameTags":[{"name":"idle","from":0,"to":1,"direction":"forward"},{"name":"back","from":0,"to":1,"direction":"reverse"},{"name":"swing","from":0,"to":1,"direction":"pingpong"}]}}"#;

    #[test]
    fn hash_frames_are_in_number_order() {
        let s = parse_sheet(ASEPRITE).unwrap();
        assert_eq!(s.frames[0].ms, 50, "frame 2 before frame 10");
        assert_eq!(s.frames[1].ms, 300);
        assert_eq!((s.frame_w, s.frame_h), (2, 2));
        assert_eq!(s.image, "hero.png");
    }

    #[test]
    fn tags() {
        let s = parse_sheet(ASEPRITE).unwrap();
        assert_eq!(s.tags[0], SheetTag { name: "idle".into(), first: 0, last: 1, mode: 1 });
        assert_eq!((s.tags[1].first, s.tags[1].last), (1, 0), "reverse");
        assert_eq!(s.tags[2].mode, 2);
    }

    #[test]
    fn array_frames_and_no_extras() {
        let s = parse_sheet(r#"{"frames":[{"frame":{"x":0,"y":0,"w":4,"h":3}}],"meta":{"image":"a.png"}}"#).unwrap();
        assert_eq!((s.frame_w, s.frame_h, s.frames[0].ms), (4, 3, 100));
    }

    #[test]
    fn trimmed_frames_go_back_in_place() {
        let s = parse_sheet(ASEPRITE).unwrap();
        let mut pic = RgbaImage::new(3, 2);
        pic.put_pixel(2, 0, image::Rgba([9, 9, 9, 255]));
        pic.put_pixel(0, 0, image::Rgba([7, 7, 7, 255]));
        let (out, n) = lay_out(&s, &pic).unwrap();
        assert_eq!((n, out.dimensions()), (2, (4, 2)));
        // frame 0 is the trimmed one, moved to (1,1) of its cell
        assert_eq!(out.get_pixel(1, 1).0[0], 9);
        assert_eq!(out.get_pixel(0, 0).0[3], 0);
        // frame 1 is whole, in the next cell
        assert_eq!(out.get_pixel(2, 0).0[0], 7);
    }

    #[test]
    fn problems_are_reported() {
        assert!(parse_sheet(r#"{"hello":1}"#).unwrap_err().contains("no frames"));
        assert!(parse_sheet(r#"{"frames":[{"frame":{"x":0,"y":0,"w":1,"h":1}}],"meta":{}}"#).unwrap_err().contains("image"));
        assert!(parse_sheet(r#"{"frames":[{"rotated":true,"frame":{"x":0,"y":0,"w":1,"h":1}}],"meta":{"image":"a"}}"#).unwrap_err().contains("rotation"));
        let bad = r#"{"frames":[{"frame":{"x":0,"y":0,"w":1,"h":1}}],"meta":{"image":"a","frameTags":[{"name":"x","from":0,"to":9}]}}"#;
        assert!(parse_sheet(bad).unwrap_err().contains("past"));
        let s = parse_sheet(r#"{"frames":[{"frame":{"x":5,"y":0,"w":1,"h":1}}],"meta":{"image":"a"}}"#).unwrap();
        assert!(lay_out(&s, &RgbaImage::new(2, 2)).is_err());
    }
}
