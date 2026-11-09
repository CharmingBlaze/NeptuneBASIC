//! Maps made in the Tiled editor (ext): `LoadTiledMap`.
//!
//! Reads Tiled's two file formats, `.tmx` (XML) and `.tmj` (JSON), with
//! embedded or external tilesets. Every layer becomes an ordinary tile map,
//! so `DrawTileMap`, `TileAt`, `TileMapBody2D` and the rest work on it.
//! All tilesets are cut into one animation image; a tile drawn flipped or
//! rotated gets a frame of its own there, so tile maps need no flip support.
//! Objects keep their names, types, shapes and custom properties.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

use image::{imageops, RgbaImage};

use crate::json::{self, Json};
use crate::tilemap::{TileMap, EMPTY};
use crate::world::Host3D;

const FLIP_H: u32 = 0x8000_0000;
const FLIP_V: u32 = 0x4000_0000;
const FLIP_D: u32 = 0x2000_0000;
/// The four high bits of a tile number are flags (the fourth is for hexagons).
const GID_MASK: u32 = 0x0FFF_FFFF;

pub const SHAPE_RECT: i32 = 0;
pub const SHAPE_ELLIPSE: i32 = 1;
pub const SHAPE_POINT: i32 = 2;
pub const SHAPE_POLYGON: i32 = 3;
pub const SHAPE_POLYLINE: i32 = 4;
pub const SHAPE_TILE: i32 = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct TilesetDef {
    pub first_gid: u32,
    pub name: String,
    pub tile_w: u32,
    pub tile_h: u32,
    pub count: u32,
    pub columns: u32,
    pub margin: u32,
    pub spacing: u32,
    /// Image file, from the game folder.
    pub image: PathBuf,
    pub transparent: Option<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerDef {
    pub name: String,
    pub width: i32,
    pub height: i32,
    /// Raw tile numbers, flags included; 0 is empty.
    pub gids: Vec<u32>,
    pub visible: bool,
    pub offset: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDef {
    pub name: String,
    pub kind: String,
    pub layer: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rotation: f32,
    pub shape: i32,
    pub gid: u32,
    /// Polygon and polyline corners, from the map's top-left corner.
    pub points: Vec<(f32, f32)>,
    pub props: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapDef {
    pub width: i32,
    pub height: i32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub tilesets: Vec<TilesetDef>,
    pub layers: Vec<LayerDef>,
    pub objects: Vec<ObjectDef>,
    pub props: Vec<(String, String)>,
}

/// A loaded map.
#[derive(Debug, Clone)]
pub struct TiledMap {
    pub image: i32,
    pub layers: Vec<TiledLayer>,
    pub objects: Vec<i32>,
    pub props: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct TiledLayer {
    pub name: String,
    pub map: i32,
    pub visible: bool,
    pub offset: (f32, f32),
}

/// An object of a loaded map; `tile` is its frame, or -1.
#[derive(Debug, Clone)]
pub struct TiledObject {
    pub def: ObjectDef,
    pub tile: i32,
}

// ---- XML -----------------------------------------------------------------

#[derive(Debug, Default)]
struct Xml {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Xml>,
    text: String,
}

impl Xml {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    fn child(&self, name: &str) -> Option<&Xml> {
        self.children.iter().find(|c| c.name == name)
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let entity = &tail[1..end];
        let number = entity.strip_prefix("#x").map(|h| u32::from_str_radix(h, 16).ok()).or_else(|| entity.strip_prefix('#').map(|d| d.parse().ok()));
        match (entity, number) {
            ("amp", _) => out.push('&'),
            ("lt", _) => out.push('<'),
            ("gt", _) => out.push('>'),
            ("quot", _) => out.push('"'),
            ("apos", _) => out.push('\''),
            (_, Some(Some(n))) => out.push(char::from_u32(n).unwrap_or('\u{fffd}')),
            _ => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Attributes of a start tag's text after its name.
fn parse_attrs(mut s: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    loop {
        s = s.trim_start();
        if s.is_empty() {
            return Ok(out);
        }
        let eq = s.find('=').ok_or("bad XML attribute")?;
        let name = s[..eq].trim().to_string();
        s = s[eq + 1..].trim_start();
        let quote = s.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or("bad XML attribute")?;
        s = &s[1..];
        let end = s.find(quote).ok_or("unterminated XML attribute")?;
        out.push((name, unescape(&s[..end])));
        s = &s[end + 1..];
    }
}

/// The small part of XML that Tiled writes.
fn parse_xml(text: &str) -> Result<Xml, String> {
    let mut stack: Vec<Xml> = vec![Xml::default()];
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        stack.last_mut().ok_or("bad XML")?.text.push_str(&unescape(&rest[..i]));
        let tag = &rest[i..];
        if let Some(body) = tag.strip_prefix("<!--") {
            rest = &body[body.find("-->").ok_or("unterminated XML comment")? + 3..];
        } else if let Some(body) = tag.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").ok_or("unterminated CDATA")?;
            stack.last_mut().ok_or("bad XML")?.text.push_str(&body[..end]);
            rest = &body[end + 3..];
        } else if tag.starts_with("<?") || tag.starts_with("<!") {
            rest = &tag[tag.find('>').ok_or("bad XML")? + 1..];
        } else if tag.starts_with("</") {
            rest = &tag[tag.find('>').ok_or("bad XML")? + 1..];
            let done = stack.pop().ok_or("bad XML")?;
            stack.last_mut().ok_or("unbalanced XML")?.children.push(done);
        } else {
            let mut end = 1;
            let mut quote = None;
            let bytes = tag.as_bytes();
            loop {
                let c = *bytes.get(end).ok_or("unterminated XML tag")?;
                match quote {
                    Some(q) if c == q => quote = None,
                    Some(_) => {}
                    None if c == b'"' || c == b'\'' => quote = Some(c),
                    None if c == b'>' => break,
                    None => {}
                }
                end += 1;
            }
            let inner = &tag[1..end];
            let (inner, closed) = match inner.strip_suffix('/') {
                Some(s) => (s, true),
                None => (inner, false),
            };
            let split = inner.find(char::is_whitespace).unwrap_or(inner.len());
            let node = Xml { name: inner[..split].to_string(), attrs: parse_attrs(&inner[split..])?, ..Xml::default() };
            if closed {
                stack.last_mut().ok_or("bad XML")?.children.push(node);
            } else {
                stack.push(node);
            }
            rest = &tag[end + 1..];
        }
    }
    let mut top = stack.pop().ok_or("bad XML")?;
    if !stack.is_empty() {
        return Err("XML ends inside a tag".into());
    }
    if top.children.is_empty() {
        return Err("no XML element found".into());
    }
    Ok(top.children.remove(0))
}

// ---- .tmx and .tsx become the same shape as .tmj and .tsj -----------------

const NUMBER_ATTRS: &[&str] =
    &["width", "height", "tilewidth", "tileheight", "firstgid", "tilecount", "columns", "margin", "spacing", "offsetx", "offsety", "x", "y", "rotation", "gid", "id", "infinite"];

fn attrs_json(x: &Xml) -> BTreeMap<String, Json> {
    x.attrs
        .iter()
        .map(|(k, v)| {
            let value = if k == "visible" {
                Json::Bool(v != "0")
            } else if NUMBER_ATTRS.contains(&k.as_str()) {
                v.parse().map(Json::Num).unwrap_or_else(|_| Json::Str(v.clone()))
            } else {
                Json::Str(v.clone())
            };
            (k.clone(), value)
        })
        .collect()
}

fn properties_json(x: &Xml) -> Json {
    let list = x.child("properties").map_or(&[][..], |p| &p.children);
    Json::Arr(
        list.iter()
            .map(|p| {
                let value = p.attr("value").map_or_else(|| p.text.clone(), str::to_string);
                let is_bool = p.attr("type") == Some("bool");
                let value = if is_bool { Json::Bool(value == "true") } else { Json::Str(value) };
                Json::Obj(BTreeMap::from([("name".to_string(), Json::Str(p.attr("name").unwrap_or("").to_string())), ("value".to_string(), value)]))
            })
            .collect(),
    )
}

fn tileset_json(x: &Xml) -> Json {
    let mut m = attrs_json(x);
    if let Some(image) = x.child("image") {
        m.insert("image".into(), Json::Str(image.attr("source").unwrap_or("").to_string()));
        if let Some(t) = image.attr("trans") {
            m.insert("transparentcolor".into(), Json::Str(t.to_string()));
        }
    }
    Json::Obj(m)
}

fn points_json(text: &str) -> Json {
    Json::Arr(
        text.split_whitespace()
            .filter_map(|pair| pair.split_once(','))
            .map(|(x, y)| {
                let n = |s: &str| Json::Num(s.parse().unwrap_or(0.0));
                Json::Obj(BTreeMap::from([("x".to_string(), n(x)), ("y".to_string(), n(y))]))
            })
            .collect(),
    )
}

fn object_json(x: &Xml) -> Json {
    let mut m = attrs_json(x);
    m.insert("properties".into(), properties_json(x));
    if x.child("point").is_some() {
        m.insert("point".into(), Json::Bool(true));
    }
    if x.child("ellipse").is_some() {
        m.insert("ellipse".into(), Json::Bool(true));
    }
    for kind in ["polygon", "polyline"] {
        if let Some(p) = x.child(kind) {
            m.insert(kind.into(), points_json(p.attr("points").unwrap_or("")));
        }
    }
    Json::Obj(m)
}

fn layers_json(x: &Xml) -> Result<Json, String> {
    let mut out = Vec::new();
    for c in &x.children {
        let mut m = attrs_json(c);
        match c.name.as_str() {
            "layer" => {
                m.insert("type".into(), Json::Str("tilelayer".into()));
                let data = c.child("data").ok_or("a layer has no data")?;
                if data.attr("encoding") != Some("csv") {
                    return Err(UNSUPPORTED_DATA.into());
                }
                let gids = data.text.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| s.parse().map(Json::Num).map_err(|_| format!("'{s}' is not a tile number")));
                m.insert("data".into(), Json::Arr(gids.collect::<Result<_, _>>()?));
            }
            "objectgroup" => {
                m.insert("type".into(), Json::Str("objectgroup".into()));
                m.insert("objects".into(), Json::Arr(c.children.iter().filter(|o| o.name == "object").map(object_json).collect()));
            }
            "group" => {
                m.insert("type".into(), Json::Str("group".into()));
                m.insert("layers".into(), layers_json(c)?);
            }
            _ => continue,
        }
        out.push(Json::Obj(m));
    }
    Ok(Json::Arr(out))
}

const UNSUPPORTED_DATA: &str = "set Tile Layer Format to CSV in Tiled's map properties";

fn tmx_to_json(x: &Xml) -> Result<Json, String> {
    if x.name != "map" {
        return Err(format!("expected a Tiled map, found <{}>", x.name));
    }
    let mut m = attrs_json(x);
    m.insert("tilesets".into(), Json::Arr(x.children.iter().filter(|c| c.name == "tileset").map(tileset_json).collect()));
    m.insert("layers".into(), layers_json(x)?);
    m.insert("properties".into(), properties_json(x));
    Ok(Json::Obj(m))
}

// ---- reading --------------------------------------------------------------

/// Removes `.` and `..` from a path, so packaged files stay inside the game.
fn tidy(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir if matches!(out.components().next_back(), Some(Component::Normal(_))) => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn beside(file: &Path, name: &str) -> PathBuf {
    tidy(&file.parent().unwrap_or(Path::new("")).join(name))
}

fn uint(v: &Json) -> u32 {
    v.num().map_or(0, |n| n as u32)
}

fn real(v: &Json) -> f32 {
    v.num().unwrap_or(0.0) as f32
}

fn text(v: &Json) -> String {
    v.str().unwrap_or("").to_string()
}

fn props(v: &Json) -> Vec<(String, String)> {
    v.get("properties").arr().iter().map(|p| (text(p.get("name")), crate::level::prop_text(p.get("value")))).collect()
}

fn color(s: &str) -> Option<[u8; 3]> {
    let h = s.trim_start_matches('#');
    let h = if h.len() == 8 { &h[2..] } else { h };
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some([byte(0)?, byte(2)?, byte(4)?])
}

type ReadFile<'a> = &'a dyn Fn(&Path) -> Result<String, String>;

fn parse_tileset(entry: &Json, map_file: &Path, read: ReadFile) -> Result<TilesetDef, String> {
    let first_gid = uint(entry.get("firstgid"));
    let (doc, file) = match entry.get("source").str() {
        Some(source) => {
            let file = beside(map_file, source);
            let body = read(&file)?;
            let doc = if body.trim_start().starts_with('<') { tileset_json(&parse_xml(&body)?) } else { json::parse(&body)? };
            (doc, file)
        }
        None => (entry.clone(), map_file.to_path_buf()),
    };
    let name = text(doc.get("name"));
    let image = doc.get("image").str().filter(|s| !s.is_empty());
    let Some(image) = image else { return Err(format!("tileset '{name}' is a collection of images; use a tileset based on one image")) };
    Ok(TilesetDef {
        first_gid,
        tile_w: uint(doc.get("tilewidth")),
        tile_h: uint(doc.get("tileheight")),
        count: uint(doc.get("tilecount")),
        columns: uint(doc.get("columns")),
        margin: uint(doc.get("margin")),
        spacing: uint(doc.get("spacing")),
        image: beside(&file, image),
        transparent: doc.get("transparentcolor").str().and_then(color),
        name,
    })
}

fn parse_object(o: &Json, layer: &str, offset: (f32, f32)) -> ObjectDef {
    let (x, mut y) = (real(o.get("x")) + offset.0, real(o.get("y")) + offset.1);
    let (w, h) = (real(o.get("width")), real(o.get("height")));
    let gid = uint(o.get("gid"));
    let is = |k: &str| matches!(o.get(k), Json::Bool(true));
    let (shape, list) = if gid != 0 {
        (SHAPE_TILE, &[][..])
    } else if is("point") {
        (SHAPE_POINT, &[][..])
    } else if is("ellipse") {
        (SHAPE_ELLIPSE, &[][..])
    } else if !o.get("polygon").arr().is_empty() {
        (SHAPE_POLYGON, o.get("polygon").arr())
    } else if !o.get("polyline").arr().is_empty() {
        (SHAPE_POLYLINE, o.get("polyline").arr())
    } else {
        (SHAPE_RECT, &[][..])
    };
    let points = list.iter().map(|p| (x + real(p.get("x")), y + real(p.get("y")))).collect();
    // Tiled places a tile object by its bottom-left corner, others by the top-left
    if gid != 0 {
        y -= h;
    }
    let kind = o.get("type").str().filter(|s| !s.is_empty()).or_else(|| o.get("class").str()).unwrap_or("").to_string();
    ObjectDef { name: text(o.get("name")), kind, layer: layer.to_string(), x, y, w, h, rotation: real(o.get("rotation")), shape, gid, points, props: props(o) }
}

fn parse_layers(list: &[Json], offset: (f32, f32), visible: bool, map: &mut MapDef) -> Result<(), String> {
    for l in list {
        let off = (offset.0 + real(l.get("offsetx")), offset.1 + real(l.get("offsety")));
        let shown = visible && !matches!(l.get("visible"), Json::Bool(false));
        let name = text(l.get("name"));
        match l.get("type").str().unwrap_or("") {
            "tilelayer" => {
                if l.get("data").str().is_some() || l.get("encoding").str().is_some_and(|e| e != "csv") {
                    return Err(UNSUPPORTED_DATA.into());
                }
                let width = if l.get("width").is_null() { map.width } else { uint(l.get("width")) as i32 };
                let height = if l.get("height").is_null() { map.height } else { uint(l.get("height")) as i32 };
                let gids: Vec<u32> = l.get("data").arr().iter().map(uint).collect();
                if gids.len() != width as usize * height as usize {
                    return Err(format!("layer '{name}' has {} tiles, expected {width}x{height}", gids.len()));
                }
                map.layers.push(LayerDef { name, width, height, gids, visible: shown, offset: off });
            }
            "objectgroup" => map.objects.extend(l.get("objects").arr().iter().map(|o| parse_object(o, &name, off))),
            "group" => parse_layers(l.get("layers").arr(), off, shown, map)?,
            _ => {}
        }
    }
    Ok(())
}

/// Reads a `.tmx` or `.tmj` file and the tilesets it names.
pub fn parse_map(path: &str, read: ReadFile) -> Result<MapDef, String> {
    let file = Path::new(path);
    let body = read(file)?;
    let doc = if body.trim_start().starts_with('<') { tmx_to_json(&parse_xml(&body)?)? } else { json::parse(&body)? };
    if doc.get("tilewidth").is_null() {
        return Err("not a Tiled map".into());
    }
    if let Some(o) = doc.get("orientation").str().filter(|o| *o != "orthogonal") {
        return Err(format!("{o} maps are not supported; use an orthogonal map"));
    }
    if matches!(doc.get("infinite"), Json::Bool(true)) || doc.get("infinite").num().is_some_and(|n| n != 0.0) {
        return Err("infinite maps are not supported; turn off Infinite in Tiled's map properties".into());
    }
    let mut map = MapDef {
        width: uint(doc.get("width")) as i32,
        height: uint(doc.get("height")) as i32,
        tile_w: uint(doc.get("tilewidth")),
        tile_h: uint(doc.get("tileheight")),
        props: props(&doc),
        ..MapDef::default()
    };
    map.tilesets = doc.get("tilesets").arr().iter().map(|t| parse_tileset(t, file, read)).collect::<Result<_, _>>()?;
    map.tilesets.sort_by_key(|t| t.first_gid);
    parse_layers(doc.get("layers").arr(), (0.0, 0.0), true, &mut map)?;
    Ok(map)
}

/// The files a map needs beside itself, for `neptune build`.
pub fn dependencies(path: &str, read: ReadFile) -> Result<Vec<PathBuf>, String> {
    let body = read(Path::new(path))?;
    let doc = if body.trim_start().starts_with('<') { tmx_to_json(&parse_xml(&body)?)? } else { json::parse(&body)? };
    let mut out = Vec::new();
    for entry in doc.get("tilesets").arr() {
        if let Some(source) = entry.get("source").str() {
            out.push(beside(Path::new(path), source));
        }
    }
    out.extend(parse_map(path, read)?.tilesets.into_iter().map(|t| t.image));
    Ok(out)
}

// ---- tiles -----------------------------------------------------------------

/// Tiles cut from the tilesets, plus flipped copies made as they are needed.
pub struct Atlas {
    pub tiles: Vec<RgbaImage>,
    /// (first gid, frame of its first tile, tile count) of each tileset.
    ranges: Vec<(u32, i32, u32)>,
    variants: HashMap<u32, i32>,
}

impl Atlas {
    /// Cuts every tileset, given each one's image.
    pub fn new(map: &MapDef, images: &[RgbaImage]) -> Result<Atlas, String> {
        let mut tiles = Vec::new();
        let mut ranges = Vec::new();
        for (set, img) in map.tilesets.iter().zip(images) {
            if set.tile_w != map.tile_w || set.tile_h != map.tile_h {
                return Err(format!("tileset '{}' has {}x{} tiles but the map uses {}x{}", set.name, set.tile_w, set.tile_h, map.tile_w, map.tile_h));
            }
            let mut img = img.clone();
            if let Some(c) = set.transparent {
                for p in img.pixels_mut().filter(|p| p[0] == c[0] && p[1] == c[1] && p[2] == c[2]) {
                    p[3] = 0;
                }
            }
            ranges.push((set.first_gid, tiles.len() as i32, set.count));
            let columns = set.columns.max(1);
            for i in 0..set.count {
                let x = set.margin + (i % columns) * (set.tile_w + set.spacing);
                let y = set.margin + (i / columns) * (set.tile_h + set.spacing);
                if x + set.tile_w > img.width() || y + set.tile_h > img.height() {
                    return Err(format!("tileset '{}' is larger than its image", set.name));
                }
                tiles.push(imageops::crop_imm(&img, x, y, set.tile_w, set.tile_h).to_image());
            }
        }
        Ok(Atlas { tiles, ranges, variants: HashMap::new() })
    }

    /// The frame for a raw tile number (`EMPTY` for 0 or an unknown tile).
    pub fn frame(&mut self, raw: u32) -> i32 {
        let gid = raw & GID_MASK;
        let found = self.ranges.iter().find(|(first, _, count)| gid >= *first && gid < first + count);
        let Some(&(first, base, _)) = found.filter(|_| gid != 0) else { return EMPTY };
        let frame = base + (gid - first) as i32;
        let flags = raw & (FLIP_H | FLIP_V | FLIP_D);
        if flags == 0 {
            return frame;
        }
        let key = gid | flags;
        if let Some(&v) = self.variants.get(&key) {
            return v;
        }
        let mut t = self.tiles[frame as usize].clone();
        if flags & FLIP_D != 0 && t.width() == t.height() {
            // the diagonal flip comes first
            t = imageops::flip_horizontal(&imageops::rotate90(&t));
        }
        if flags & FLIP_H != 0 {
            imageops::flip_horizontal_in_place(&mut t);
        }
        if flags & FLIP_V != 0 {
            imageops::flip_vertical_in_place(&mut t);
        }
        self.tiles.push(t);
        let v = self.tiles.len() as i32 - 1;
        self.variants.insert(key, v);
        v
    }

    /// All tiles in one image, and its frame size.
    pub fn pack(&self, tile_w: u32, tile_h: u32) -> RgbaImage {
        let n = self.tiles.len().max(1) as u32;
        let columns = (n as f32).sqrt().ceil() as u32;
        let mut out = RgbaImage::new(columns * tile_w, n.div_ceil(columns) * tile_h);
        for (i, t) in self.tiles.iter().enumerate() {
            let i = i as u32;
            imageops::replace(&mut out, t, ((i % columns) * tile_w) as i64, ((i / columns) * tile_h) as i64);
        }
        out
    }
}

// ---- commands --------------------------------------------------------------

fn read_text(p: &Path) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))
}

fn build(h: &mut Host3D, path: &str) -> Result<i32, String> {
    let def = parse_map(path, &read_text)?;
    let images = def
        .tilesets
        .iter()
        .map(|t| image::open(&t.image).map(|i| i.to_rgba8()).map_err(|e| format!("cannot load {}: {e}", t.image.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let mut atlas = Atlas::new(&def, &images)?;
    let grids: Vec<Vec<i32>> = def.layers.iter().map(|l| l.gids.iter().map(|&g| atlas.frame(g)).collect()).collect();
    let tiles: Vec<i32> = def.objects.iter().map(|o| if o.gid != 0 { atlas.frame(o.gid) } else { -1 }).collect();
    let frames = atlas.tiles.len().max(1) as u32;
    let mut img = crate::image2d::Image2D::new(atlas.pack(def.tile_w, def.tile_h), def.tile_w, def.tile_h, 0, frames)?;
    img.path = path.replace('\\', "/");
    img.use_alpha_only();
    h.world.images.push(Some(img));
    let image = h.world.images.len() as i32;
    let mut layers = Vec::new();
    for (l, grid) in def.layers.iter().zip(grids) {
        let mut map = TileMap::new(image, l.width, l.height)?;
        for (i, &t) in grid.iter().enumerate() {
            map.set(i as i32 % l.width, i as i32 / l.width, t);
        }
        h.world.tilemaps.push(Some(map));
        layers.push(TiledLayer { name: l.name.clone(), map: h.world.tilemaps.len() as i32, visible: l.visible, offset: l.offset });
    }
    let mut objects = Vec::new();
    for (o, tile) in def.objects.iter().zip(tiles) {
        h.world.tiled_objects.push(Some(TiledObject { def: o.clone(), tile }));
        objects.push(h.world.tiled_objects.len() as i32);
    }
    h.world.tiled_maps.push(Some(TiledMap { image, layers, objects, props: def.props }));
    Ok(h.world.tiled_maps.len() as i32)
}

/// `LoadTiledMap`: 0 if the map cannot be loaded (like `LoadImage`).
pub fn load(h: &mut Host3D, path: &str) -> Result<i32, String> {
    match build(h, path) {
        Ok(id) => Ok(id),
        Err(e) => {
            eprintln!("LoadTiledMap: {path}: {e}");
            Ok(0)
        }
    }
}

pub fn map<'a>(h: &'a Host3D, m: i32) -> Result<&'a TiledMap, String> {
    usize::try_from(m - 1).ok().and_then(|i| h.world.tiled_maps.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Tiled map {m} does not exist"))
}

pub fn object<'a>(h: &'a Host3D, o: i32) -> Result<&'a TiledObject, String> {
    usize::try_from(o - 1).ok().and_then(|i| h.world.tiled_objects.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Tiled object {o} does not exist"))
}

/// `FreeTiledMap`: frees its layers, objects and image.
pub fn free(h: &mut Host3D, m: i32) -> Result<(), String> {
    let t = map(h, m)?.clone();
    for l in t.layers {
        h.world.tilemaps[l.map as usize - 1] = None;
    }
    for o in t.objects {
        h.world.tiled_objects[o as usize - 1] = None;
    }
    h.world.images[t.image as usize - 1] = None;
    h.world.tiled_maps[m as usize - 1] = None;
    Ok(())
}

/// The map's objects, optionally only those of one type.
pub fn objects(h: &Host3D, m: i32, kind: &str) -> Result<Vec<i32>, String> {
    let list = &map(h, m)?.objects;
    Ok(list.iter().copied().filter(|&o| kind.is_empty() || object(h, o).is_ok_and(|x| x.def.kind.eq_ignore_ascii_case(kind))).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TSX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<tileset version="1.10" name="ground" tilewidth="2" tileheight="2" spacing="1" margin="1" tilecount="4" columns="2">
 <image source="../art/ground.png" trans="ff00ff" width="7" height="7"/>
</tileset>"#;

    const TMX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" orientation="orthogonal" renderorder="right-down" width="3" height="2" tilewidth="2" tileheight="2" infinite="0">
 <properties><property name="music" value="cave &amp; wind"/><property name="dark" type="bool" value="true"/></properties>
 <tileset firstgid="1" source="ground.tsx"/>
 <layer id="1" name="Back" width="3" height="2" offsetx="4">
  <data encoding="csv">
1,2,0,
0,3,2147483652
</data>
 </layer>
 <group id="2" name="G" offsety="10" visible="0">
  <layer id="3" name="Inner" width="3" height="2"><data encoding="csv">0,0,0,0,0,1</data></layer>
  <objectgroup id="4" name="Things">
   <object id="1" name="start" type="spawn" x="8" y="16"><point/></object>
   <object id="2" name="coin" type="coin" gid="3" x="4" y="20" width="2" height="2">
    <properties><property name="value" type="int" value="5"/><property name="text">long
text</property></properties>
   </object>
   <object id="3" class="zone" x="1" y="1" width="9" height="9" rotation="45"><ellipse/></object>
   <object id="4" x="2" y="3"><polyline points="0,0 4,-2"/></object>
  </objectgroup>
 </group>
</map>"#;

    const TMJ: &str = r#"{"orientation":"orthogonal","width":3,"height":2,"tilewidth":2,"tileheight":2,"infinite":false,
     "properties":[{"name":"music","type":"string","value":"cave & wind"},{"name":"dark","type":"bool","value":true}],
     "tilesets":[{"firstgid":1,"source":"ground.tsx"}],
     "layers":[{"type":"tilelayer","name":"Back","width":3,"height":2,"offsetx":4,"visible":true,"data":[1,2,0,0,3,2147483652]},
      {"type":"group","name":"G","offsety":10,"visible":false,"layers":[
       {"type":"tilelayer","name":"Inner","width":3,"height":2,"data":[0,0,0,0,0,1]},
       {"type":"objectgroup","name":"Things","objects":[
        {"id":1,"name":"start","type":"spawn","x":8,"y":16,"point":true},
        {"id":2,"name":"coin","type":"coin","gid":3,"x":4,"y":20,"width":2,"height":2,"properties":[{"name":"value","type":"int","value":5},{"name":"text","type":"string","value":"long\ntext"}]},
        {"id":3,"class":"zone","x":1,"y":1,"width":9,"height":9,"rotation":45,"ellipse":true},
        {"id":4,"x":2,"y":3,"polyline":[{"x":0,"y":0},{"x":4,"y":-2}]}]}]}]}"#;

    fn files(name: &'static str, body: &'static str) -> impl Fn(&Path) -> Result<String, String> {
        move |p: &Path| {
            let p = p.to_string_lossy().replace('\\', "/");
            if p == "maps/ground.tsx" {
                Ok(TSX.to_string())
            } else if p == name {
                Ok(body.to_string())
            } else {
                Err(format!("no file {p}"))
            }
        }
    }

    #[test]
    fn tmx_and_tmj_read_the_same() {
        let a = parse_map("maps/a.tmx", &files("maps/a.tmx", TMX)).unwrap();
        let b = parse_map("maps/a.tmj", &files("maps/a.tmj", TMJ)).unwrap();
        assert_eq!(a, b);
        assert_eq!((a.width, a.height, a.tile_w), (3, 2, 2));
        let set = &a.tilesets[0];
        assert_eq!(set.image, PathBuf::from("art/ground.png"));
        assert_eq!((set.margin, set.spacing, set.transparent), (1, 1, Some([255, 0, 255])));
        assert_eq!(a.layers[0].offset, (4.0, 0.0));
        assert!(a.layers[0].visible);
        assert_eq!(a.layers[1].offset, (0.0, 10.0));
        assert!(!a.layers[1].visible, "a hidden group hides its layers");
        assert_eq!(crate::level::prop(&a.props, "music"), Some("cave & wind"));
        assert_eq!(crate::level::prop(&a.props, "dark"), Some("1"));
    }

    #[test]
    fn objects() {
        let m = parse_map("maps/a.tmx", &files("maps/a.tmx", TMX)).unwrap();
        let o = &m.objects;
        assert_eq!((o[0].name.as_str(), o[0].kind.as_str(), o[0].shape), ("start", "spawn", SHAPE_POINT));
        assert_eq!((o[0].x, o[0].y), (8.0, 26.0), "group offset applies");
        // tile objects are given by their top-left corner like the others
        assert_eq!((o[1].shape, o[1].x, o[1].y), (SHAPE_TILE, 4.0, 28.0));
        assert_eq!(crate::level::prop(&o[1].props, "value"), Some("5"));
        assert_eq!(crate::level::prop(&o[1].props, "text"), Some("long\ntext"));
        assert_eq!((o[2].kind.as_str(), o[2].shape, o[2].rotation), ("zone", SHAPE_ELLIPSE, 45.0));
        assert_eq!(o[3].points, [(2.0, 13.0), (6.0, 11.0)]);
    }

    #[test]
    fn dependencies_of_a_map() {
        let list = dependencies("maps/a.tmx", &files("maps/a.tmx", TMX)).unwrap();
        assert_eq!(list, [PathBuf::from("maps/ground.tsx"), PathBuf::from("art/ground.png")]);
    }

    #[test]
    fn unsupported_maps_say_what_to_change() {
        let f = |body: &'static str| parse_map("a.tmj", &files("a.tmj", body)).unwrap_err();
        assert!(f(r#"{"tilewidth":2,"orientation":"isometric"}"#).contains("isometric"));
        assert!(f(r#"{"tilewidth":2,"infinite":true}"#).contains("Infinite"));
        assert!(f(r#"{"tilewidth":2,"width":1,"height":1,"layers":[{"type":"tilelayer","data":"AAAA","encoding":"base64"}]}"#).contains("CSV"));
        assert!(f(r#"{"tilewidth":2,"tilesets":[{"firstgid":1,"name":"x"}]}"#).contains("collection"));
        assert!(f(r#"{"hello":1}"#).contains("not a Tiled map"));
        let t = r#"<map width="1" height="1" tilewidth="2" tileheight="2"><layer name="a"><data encoding="base64" compression="zlib">eJw=</data></layer></map>"#;
        assert!(parse_map("a.tmx", &files("a.tmx", t)).unwrap_err().contains("CSV"));
    }

    fn tileset() -> (MapDef, RgbaImage) {
        // 7x7 with margin 1, spacing 1: four 2x2 tiles, each its own colour
        let mut img = RgbaImage::new(7, 7);
        for (i, (x, y)) in [(1, 1), (4, 1), (1, 4), (4, 4)].into_iter().enumerate() {
            for dy in 0..2 {
                for dx in 0..2 {
                    img.put_pixel(x + dx, y + dy, image::Rgba([i as u8 * 10, dx as u8, dy as u8, 255]));
                }
            }
        }
        let map = parse_map("maps/a.tmx", &files("maps/a.tmx", TMX)).unwrap();
        (map, img)
    }

    #[test]
    fn tiles_are_cut_and_numbered() {
        let (map, img) = tileset();
        let mut a = Atlas::new(&map, &[img]).unwrap();
        assert_eq!(a.tiles.len(), 4);
        assert_eq!(a.frame(0), EMPTY);
        assert_eq!(a.frame(1), 0);
        assert_eq!(a.frame(4), 3);
        assert_eq!(a.frame(5), EMPTY);
        assert_eq!(a.tiles[2].get_pixel(1, 0).0, [20, 1, 0, 255]);
    }

    #[test]
    fn flipped_tiles_get_their_own_frames() {
        let (map, img) = tileset();
        let mut a = Atlas::new(&map, &[img]).unwrap();
        let f = a.frame(1 | FLIP_H);
        assert_eq!(f, 4);
        assert_eq!(a.frame(1 | FLIP_H), 4, "made once");
        assert_eq!(a.tiles[4].get_pixel(0, 0).0, [0, 1, 0, 255]);
        let v = a.frame(1 | FLIP_V);
        assert_eq!(a.tiles[v as usize].get_pixel(0, 0).0, [0, 0, 1, 255]);
        // the diagonal flip swaps x and y
        let d = a.frame(1 | FLIP_D);
        assert_eq!(a.tiles[d as usize].get_pixel(1, 0).0, [0, 0, 1, 255]);
        assert_eq!(a.tiles[d as usize].get_pixel(0, 1).0, [0, 1, 0, 255]);
    }

    #[test]
    fn tile_size_must_match_the_map() {
        let (mut map, img) = tileset();
        map.tile_w = 16;
        assert!(Atlas::new(&map, &[img]).err().unwrap().contains("the map uses"));
    }

    #[test]
    fn pack_lays_out_frames_in_a_grid() {
        let (map, img) = tileset();
        let a = Atlas::new(&map, &[img]).unwrap();
        let p = a.pack(2, 2);
        assert_eq!(p.dimensions(), (4, 4));
        assert_eq!(p.get_pixel(2, 2).0[0], 30);
    }
}
