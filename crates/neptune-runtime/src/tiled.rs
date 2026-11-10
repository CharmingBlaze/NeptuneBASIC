//! Maps made in a level editor (ext): `LoadTiledMap`.
//!
//! This file reads Tiled's two file formats, `.tmx` (XML) and `.tmj` (JSON),
//! and holds what every editor's map turns into: [`MapDef`]. The other
//! editors are read in `mapformats.rs`.
//!
//! Every layer becomes an ordinary tile map, so `DrawTileMap`, `TileAt`,
//! `TileMapBody2D` and the rest work on it. All tilesets are cut into one
//! animation image; a tile drawn flipped or rotated gets a frame of its own
//! there, so tile maps need no flip support. Frames are all one size, so
//! tiles of different sizes are padded to the largest (see [`FrameBox`]).
//! Objects keep their names, types, shapes and custom properties.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use image::{imageops, RgbaImage};

use crate::json::{self, Json};
use crate::tilegrid::{Layout, Shape};
use crate::tilemap::{TileAnims, TileMap, EMPTY};
use crate::world::Host3D;

pub const FLIP_H: u32 = 0x8000_0000;
pub const FLIP_V: u32 = 0x4000_0000;
pub const FLIP_D: u32 = 0x2000_0000;
/// The four high bits of a tile number are flags (the fourth is for hexagons).
pub const GID_MASK: u32 = 0x0FFF_FFFF;

pub const SHAPE_RECT: i32 = 0;
pub const SHAPE_ELLIPSE: i32 = 1;
pub const SHAPE_POINT: i32 = 2;
pub const SHAPE_POLYGON: i32 = 3;
pub const SHAPE_POLYLINE: i32 = 4;
pub const SHAPE_TILE: i32 = 5;
pub const SHAPE_TEXT: i32 = 6;

/// Where a map's files come from: the disk, or a test's strings.
pub trait MapFiles {
    fn text(&self, p: &Path) -> Result<String, String>;
    fn bytes(&self, p: &Path) -> Result<Vec<u8>, String> {
        Err(format!("cannot read {}", p.display()))
    }
    /// Names of the files in a folder.
    fn list(&self, _dir: &Path) -> Vec<String> {
        Vec::new()
    }
}

impl<F: Fn(&Path) -> Result<String, String>> MapFiles for F {
    fn text(&self, p: &Path) -> Result<String, String> {
        self(p)
    }
}

/// Files on disk, below `root` (empty for the game's own folder).
pub struct DiskFiles {
    pub root: PathBuf,
}

impl MapFiles for DiskFiles {
    fn text(&self, p: &Path) -> Result<String, String> {
        std::fs::read_to_string(self.root.join(p)).map_err(|e| format!("cannot read {}: {e}", p.display()))
    }

    fn bytes(&self, p: &Path) -> Result<Vec<u8>, String> {
        std::fs::read(self.root.join(p)).map_err(|e| format!("cannot read {}: {e}", p.display()))
    }

    fn list(&self, dir: &Path) -> Vec<String> {
        let dir = if dir.as_os_str().is_empty() { self.root.clone() } else { self.root.join(dir) };
        let dir = if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir };
        std::fs::read_dir(dir).map_or_else(|_| Vec::new(), |d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
    }
}

pub type ReadFile<'a> = &'a dyn MapFiles;

/// A collision shape drawn on a tile, in pixels from the tile's top-left.
#[derive(Debug, Clone, PartialEq)]
pub struct TileShape {
    /// A closed outline, or else a line.
    pub closed: bool,
    pub points: Vec<(f32, f32)>,
}

/// One tile's extras in a tileset.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TileDef {
    pub id: u32,
    /// Its own picture, in a tileset made of separate images.
    pub image: Option<PathBuf>,
    /// The part of its own picture to use (x, y, width, height).
    pub rect: Option<(u32, u32, u32, u32)>,
    pub kind: String,
    pub props: Vec<(String, String)>,
    /// (tile id, milliseconds)
    pub anim: Vec<(u32, u32)>,
    pub shapes: Vec<TileShape>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TilesetDef {
    pub first_gid: u32,
    pub name: String,
    pub tile_w: u32,
    pub tile_h: u32,
    /// 0: as many as fit in the image.
    pub count: u32,
    /// 0: as many as fit in the image.
    pub columns: u32,
    pub margin: u32,
    pub spacing: u32,
    /// Image file, from the game folder; empty for separate images.
    pub image: PathBuf,
    pub transparent: Option<[u8; 3]>,
    /// The file the tileset is in (or the map's, if embedded).
    pub source: PathBuf,
    /// Added to where every tile is drawn.
    pub offset: (f32, f32),
    /// Tiled's object alignment: where a tile object's position is on it.
    pub align: String,
    /// Tiles are scaled to the map's grid. With `fit`, keeping their shape.
    pub grid_render: bool,
    pub fit: bool,
    pub tiles: Vec<TileDef>,
    /// The picture itself, when the map file holds it (Aseprite).
    pub pixels: Option<RgbaImage>,
}

impl TilesetDef {
    /// The picture files this tileset needs, in the order [`Atlas::new`] wants them.
    pub fn image_files(&self) -> Vec<PathBuf> {
        if self.pixels.is_some() {
            Vec::new()
        } else if !self.image.as_os_str().is_empty() {
            vec![self.image.clone()]
        } else {
            self.tiles.iter().filter_map(|t| t.image.clone()).collect()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayerDef {
    pub name: String,
    /// `ShowTiledLayer` with this name also reaches the layer: one layer in
    /// an editor can become several here.
    pub group: String,
    pub width: i32,
    pub height: i32,
    /// Raw tile numbers, flags included; 0 is empty.
    pub gids: Vec<u32>,
    pub visible: bool,
    pub offset: (f32, f32),
    pub opacity: f32,
    pub tint: [u8; 3],
    pub parallax: (f32, f32),
    pub props: Vec<(String, String)>,
    /// Grid size if different from the map's.
    pub cell: Option<(u32, u32)>,
    /// Added to the position of the tiles in their cells.
    pub shift: (f32, f32),
    /// `gids` are values for the program, not tiles.
    pub data_only: bool,
}

impl LayerDef {
    pub fn new(name: &str, width: i32, height: i32, gids: Vec<u32>) -> LayerDef {
        LayerDef { name: name.to_string(), group: name.to_string(), width, height, gids, visible: true, opacity: 1.0, tint: [255; 3], parallax: (1.0, 1.0), ..LayerDef::default() }
    }
}

/// A layer that is one picture.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageLayerDef {
    pub name: String,
    pub image: PathBuf,
    pub visible: bool,
    pub offset: (f32, f32),
    pub opacity: f32,
    pub tint: [u8; 3],
    pub parallax: (f32, f32),
    pub repeat: (bool, bool),
    pub transparent: Option<[u8; 3]>,
    pub props: Vec<(String, String)>,
    /// How many tile layers are under it.
    pub z: usize,
}

/// A level of a file holding several, and where the editor puts it.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelDef {
    pub name: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
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
    /// A text object's words.
    pub text: String,
    /// Which part of a tile object its rotation turns about, 0 to 1.
    pub anchor: (f32, f32),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapDef {
    pub width: i32,
    pub height: i32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub tilesets: Vec<TilesetDef>,
    pub layers: Vec<LayerDef>,
    pub image_layers: Vec<ImageLayerDef>,
    pub objects: Vec<ObjectDef>,
    pub props: Vec<(String, String)>,
    pub shape: Shape,
    /// Tiled's render order: right to left, bottom to top.
    pub reverse: (bool, bool),
    pub background: Option<[u8; 3]>,
    pub parallax_origin: (f32, f32),
    /// Every level of the file; empty if it holds just one map.
    pub levels: Vec<LevelDef>,
    /// The name of the level this is.
    pub level: String,
}

/// What a frame of the map's image is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameInfo {
    pub kind: String,
    pub props: Vec<(String, String)>,
    pub shapes: Vec<TileShape>,
    /// Where the tile's top-left is inside its frame.
    pub place: (f32, f32),
    /// The tile's size.
    pub size: (f32, f32),
}

/// A loaded map.
#[derive(Debug, Clone)]
pub struct TiledMap {
    pub image: i32,
    pub layers: Vec<TiledLayer>,
    pub image_layers: Vec<TiledImageLayer>,
    pub objects: Vec<i32>,
    pub props: Vec<(String, String)>,
    pub width: i32,
    pub height: i32,
    pub tile_size: (u32, u32),
    pub pixel_size: (f32, f32),
    pub orientation: &'static str,
    pub background: Option<[u8; 3]>,
    pub parallax_origin: (f32, f32),
    pub frames: Arc<Vec<FrameInfo>>,
    pub levels: Vec<LevelDef>,
    pub level: String,
}

/// How a layer is drawn: shared by tile and image layers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerLook {
    pub offset: (f32, f32),
    pub opacity: f32,
    pub tint: [u8; 3],
    pub parallax: (f32, f32),
}

#[derive(Debug, Clone)]
pub struct TiledLayer {
    pub name: String,
    pub group: String,
    pub map: i32,
    pub visible: bool,
    pub look: LayerLook,
    pub props: Arc<Vec<(String, String)>>,
}

#[derive(Debug, Clone)]
pub struct TiledImageLayer {
    pub name: String,
    pub image: i32,
    pub visible: bool,
    pub look: LayerLook,
    pub repeat: (bool, bool),
    pub z: usize,
    pub props: Arc<Vec<(String, String)>>,
}

/// An object of a loaded map; `tile` is its frame, or -1.
#[derive(Debug, Clone)]
pub struct TiledObject {
    pub def: ObjectDef,
    pub tile: i32,
    pub map: i32,
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

/// The small part of XML that level editors write.
pub(crate) fn parse_xml_json(text: &str) -> Result<Json, String> {
    fn convert(x: &Xml) -> Json {
        let mut m = attrs_json(x);
        m.insert("_name".into(), Json::Str(x.name.clone()));
        m.insert("children".into(), Json::Arr(x.children.iter().map(convert).collect()));
        Json::Obj(m)
    }
    Ok(convert(&parse_xml(text)?))
}

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

const NUMBER_ATTRS: &[&str] = &[
    "width", "height", "tilewidth", "tileheight", "firstgid", "tilecount", "columns", "margin", "spacing", "offsetx", "offsety", "x", "y", "rotation", "gid", "id", "infinite", "opacity", "parallaxx",
    "parallaxy", "parallaxoriginx", "parallaxoriginy", "hexsidelength", "tileid", "duration", "repeatx", "repeaty", "tileswide", "tileshigh", "number", "index", "tile", "rot",
];

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

fn tile_json(x: &Xml) -> Json {
    let mut m = attrs_json(x);
    m.insert("properties".into(), properties_json(x));
    if let Some(image) = x.child("image") {
        m.insert("image".into(), Json::Str(image.attr("source").unwrap_or("").to_string()));
    }
    if let Some(a) = x.child("animation") {
        m.insert("animation".into(), Json::Arr(a.children.iter().filter(|f| f.name == "frame").map(|f| Json::Obj(attrs_json(f))).collect()));
    }
    if let Some(g) = x.child("objectgroup") {
        let list = g.children.iter().filter(|o| o.name == "object").map(object_json).collect();
        m.insert("objectgroup".into(), Json::Obj(BTreeMap::from([("objects".to_string(), Json::Arr(list))])));
    }
    Json::Obj(m)
}

fn tileset_json(x: &Xml) -> Json {
    let mut m = attrs_json(x);
    if let Some(image) = x.child("image") {
        m.insert("image".into(), Json::Str(image.attr("source").unwrap_or("").to_string()));
        if let Some(t) = image.attr("trans") {
            m.insert("transparentcolor".into(), Json::Str(t.to_string()));
        }
    }
    if let Some(o) = x.child("tileoffset") {
        m.insert("tileoffset".into(), Json::Obj(attrs_json(o)));
    }
    let tiles: Vec<Json> = x.children.iter().filter(|c| c.name == "tile").map(tile_json).collect();
    if !tiles.is_empty() {
        m.insert("tiles".into(), Json::Arr(tiles));
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
    if let Some(t) = x.child("text") {
        let mut words = attrs_json(t);
        words.insert("text".into(), Json::Str(t.text.clone()));
        m.insert("text".into(), Json::Obj(words));
    }
    Json::Obj(m)
}

const NO_ZSTD: &str = "Zstandard layers are not supported; set Tile Layer Format to CSV or Base64 (zlib or gzip) in Tiled's map properties";

fn gids_from_base64(text: &str, compression: Option<&str>) -> Result<Vec<u32>, String> {
    let raw = crate::inflate::base64(text)?;
    let bytes = match compression.unwrap_or("") {
        "" => raw,
        "zlib" => crate::inflate::zlib(&raw)?,
        "gzip" => crate::inflate::gzip(&raw)?,
        "zstd" => return Err(NO_ZSTD.into()),
        other => return Err(format!("{other} compression is not supported")),
    };
    if !bytes.len().is_multiple_of(4) {
        return Err("a layer's tile data has the wrong length".into());
    }
    Ok(bytes.as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect())
}

/// The tile numbers in a `<data>` or `<chunk>` element, in any encoding.
fn xml_gids(node: &Xml, encoding: Option<&str>, compression: Option<&str>) -> Result<Json, String> {
    let gids: Vec<u32> = match encoding {
        Some("csv") => node.text.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| s.parse().map_err(|_| format!("'{s}' is not a tile number"))).collect::<Result<_, _>>()?,
        Some("base64") => gids_from_base64(node.text.trim(), compression)?,
        Some(other) => return Err(format!("{other} layers are not supported")),
        None => node.children.iter().filter(|c| c.name == "tile").map(|c| c.attr("gid").and_then(|g| g.parse().ok()).unwrap_or(0)).collect(),
    };
    Ok(Json::Arr(gids.into_iter().map(|g| Json::Num(g as f64)).collect()))
}

fn layers_json(x: &Xml) -> Result<Json, String> {
    let mut out = Vec::new();
    for c in &x.children {
        let mut m = attrs_json(c);
        m.insert("properties".into(), properties_json(c));
        match c.name.as_str() {
            "layer" => {
                m.insert("type".into(), Json::Str("tilelayer".into()));
                let data = c.child("data").ok_or("a layer has no data")?;
                let (encoding, compression) = (data.attr("encoding"), data.attr("compression"));
                let chunks = data.children.iter().filter(|n| n.name == "chunk").collect::<Vec<_>>();
                if chunks.is_empty() {
                    m.insert("data".into(), xml_gids(data, encoding, compression)?);
                } else {
                    let mut list = Vec::new();
                    for chunk in chunks {
                        let mut one = attrs_json(chunk);
                        one.insert("data".into(), xml_gids(chunk, encoding, compression)?);
                        list.push(Json::Obj(one));
                    }
                    m.insert("chunks".into(), Json::Arr(list));
                }
            }
            "objectgroup" => {
                m.insert("type".into(), Json::Str("objectgroup".into()));
                m.insert("objects".into(), Json::Arr(c.children.iter().filter(|o| o.name == "object").map(object_json).collect()));
            }
            "imagelayer" => {
                m.insert("type".into(), Json::Str("imagelayer".into()));
                if let Some(image) = c.child("image") {
                    m.insert("image".into(), Json::Str(image.attr("source").unwrap_or("").to_string()));
                    if let Some(t) = image.attr("trans") {
                        m.insert("transparentcolor".into(), Json::Str(t.to_string()));
                    }
                }
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

fn template_json(x: &Xml) -> Result<Json, String> {
    if x.name != "template" {
        return Err(format!("expected a Tiled template, found <{}>", x.name));
    }
    let object = x.child("object").ok_or("a template has no object")?;
    let mut m = BTreeMap::from([("object".to_string(), object_json(object))]);
    if let Some(t) = x.child("tileset") {
        m.insert("tileset".into(), tileset_json(t));
    }
    Ok(Json::Obj(m))
}

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

pub(crate) fn beside(file: &Path, name: &str) -> PathBuf {
    tidy(&file.parent().unwrap_or(Path::new("")).join(name))
}

pub(crate) fn uint(v: &Json) -> u32 {
    v.num().map_or(0, |n| n as u32)
}

pub(crate) fn real(v: &Json) -> f32 {
    v.num().unwrap_or(0.0) as f32
}

pub(crate) fn text(v: &Json) -> String {
    v.str().unwrap_or("").to_string()
}

/// A Tiled yes/no, which is `true` in JSON and `1` in XML.
fn yes(v: &Json) -> bool {
    matches!(v, Json::Bool(true)) || v.num().is_some_and(|n| n != 0.0)
}

fn props(v: &Json) -> Vec<(String, String)> {
    v.get("properties").arr().iter().map(|p| (text(p.get("name")), crate::level::prop_text(p.get("value")))).collect()
}

pub(crate) fn color(s: &str) -> Option<[u8; 3]> {
    let h = s.trim_start_matches('#');
    let h = if h.len() == 8 { &h[2..] } else { h };
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some([byte(0)?, byte(2)?, byte(4)?])
}

fn rotated(p: (f32, f32), about: (f32, f32), degrees: f32) -> (f32, f32) {
    if degrees == 0.0 {
        return p;
    }
    let (s, c) = degrees.to_radians().sin_cos();
    let (dx, dy) = (p.0 - about.0, p.1 - about.1);
    (about.0 + dx * c - dy * s, about.1 + dx * s + dy * c)
}

/// A shape from Tiled's collision editor, as an outline or a line.
fn tile_shape(o: &ObjectDef) -> Option<TileShape> {
    let about = (o.x, o.y);
    let (closed, points): (bool, Vec<(f32, f32)>) = match o.shape {
        SHAPE_RECT if o.w > 0.0 && o.h > 0.0 => (true, vec![(o.x, o.y), (o.x + o.w, o.y), (o.x + o.w, o.y + o.h), (o.x, o.y + o.h)]),
        SHAPE_ELLIPSE if o.w > 0.0 && o.h > 0.0 => {
            let (rx, ry) = (o.w / 2.0, o.h / 2.0);
            let round = (0..16).map(|i| {
                let a = i as f32 / 16.0 * std::f32::consts::TAU;
                (o.x + rx + rx * a.cos(), o.y + ry + ry * a.sin())
            });
            (true, round.collect())
        }
        SHAPE_POLYGON => (true, o.points.clone()),
        SHAPE_POLYLINE => (false, o.points.clone()),
        _ => return None,
    };
    Some(TileShape { closed, points: points.into_iter().map(|p| rotated(p, about, o.rotation)).collect() })
}

fn parse_tile(t: &Json, file: &Path) -> Result<TileDef, String> {
    let image = t.get("image").str().filter(|s| !s.is_empty()).map(|s| beside(file, s));
    let has_size = !t.get("width").is_null() && !t.get("height").is_null();
    let rect = (image.is_some() && has_size).then(|| (uint(t.get("x")), uint(t.get("y")), uint(t.get("width")), uint(t.get("height"))));
    let mut shapes = Vec::new();
    for o in t.get("objectgroup").get("objects").arr() {
        shapes.extend(tile_shape(&parse_object(o, "", (0.0, 0.0))?));
    }
    Ok(TileDef {
        id: uint(t.get("id")),
        image,
        rect,
        kind: t.get("type").str().filter(|s| !s.is_empty()).or_else(|| t.get("class").str()).unwrap_or("").to_string(),
        props: props(t),
        anim: t.get("animation").arr().iter().map(|f| (uint(f.get("tileid")), uint(f.get("duration")))).collect(),
        shapes,
    })
}

fn parse_tileset(entry: &Json, map_file: &Path, read: ReadFile) -> Result<TilesetDef, String> {
    let first_gid = uint(entry.get("firstgid"));
    let (doc, file) = match entry.get("source").str() {
        Some(source) => {
            let file = beside(map_file, source);
            let body = read.text(&file)?;
            let doc = if body.trim_start().starts_with('<') { tileset_json(&parse_xml(&body)?) } else { json::parse(&body)? };
            (doc, file)
        }
        None => (entry.clone(), map_file.to_path_buf()),
    };
    let name = text(doc.get("name"));
    let tiles = doc.get("tiles").arr().iter().map(|t| parse_tile(t, &file)).collect::<Result<Vec<_>, _>>()?;
    let image = doc.get("image").str().filter(|s| !s.is_empty());
    if image.is_none() && !tiles.iter().any(|t| t.image.is_some()) {
        return Err(format!("tileset '{name}' has no image"));
    }
    let mut count = uint(doc.get("tilecount"));
    if image.is_none() {
        // separate images can have gaps in their numbers
        count = count.max(tiles.iter().map(|t| t.id + 1).max().unwrap_or(0));
    }
    let offset = doc.get("tileoffset");
    Ok(TilesetDef {
        first_gid,
        tile_w: uint(doc.get("tilewidth")),
        tile_h: uint(doc.get("tileheight")),
        count,
        columns: uint(doc.get("columns")),
        margin: uint(doc.get("margin")),
        spacing: uint(doc.get("spacing")),
        image: image.map(|i| beside(&file, i)).unwrap_or_default(),
        transparent: doc.get("transparentcolor").str().and_then(color),
        source: file,
        offset: (real(offset.get("x")), real(offset.get("y"))),
        align: text(doc.get("objectalignment")),
        grid_render: doc.get("tilerendersize").str() == Some("grid"),
        fit: doc.get("fillmode").str() == Some("preserve-aspect-fit"),
        tiles,
        pixels: None,
        name,
    })
}

/// The object a template file describes, with this object's own values on top.
fn with_template(o: &Json, file: &Path, read: ReadFile, map: &mut MapDef) -> Result<Json, String> {
    let Some(name) = o.get("template").str() else { return Ok(o.clone()) };
    let tx = beside(file, name);
    let body = read.text(&tx)?;
    let doc = if body.trim_start().starts_with('<') { template_json(&parse_xml(&body)?)? } else { json::parse(&body)? };
    let Json::Obj(mut merged) = doc.get("object").clone() else { return Err(format!("{} has no object", tx.display())) };
    // the template numbers its tile by its own tileset
    let raw = uint(doc.get("object").get("gid"));
    if raw != 0 && !doc.get("tileset").is_null() {
        let set = doc.get("tileset");
        let source = beside(&tx, set.get("source").str().unwrap_or(""));
        let first = match map.tilesets.iter().find(|t| t.source == source) {
            Some(t) => t.first_gid,
            None => {
                let mut entry = set.clone();
                let next = map.tilesets.iter().map(|t| t.first_gid + t.count).max().unwrap_or(1);
                if let (Json::Obj(e), Some(source)) = (&mut entry, source.to_str()) {
                    e.insert("firstgid".into(), Json::Num(next as f64));
                    // parse_tileset looks beside the template for it
                    e.insert("source".into(), Json::Str(source.rsplit(['/', '\\']).next().unwrap_or(source).to_string()));
                }
                let t = parse_tileset(&entry, &tx, read)?;
                map.tilesets.push(t);
                next
            }
        };
        let local = (raw & GID_MASK).saturating_sub(uint(set.get("firstgid")));
        merged.insert("gid".into(), Json::Num(((first + local) | (raw & !GID_MASK)) as f64));
    }
    let mut properties: Vec<Json> = doc.get("object").get("properties").arr().to_vec();
    for p in o.get("properties").arr() {
        properties.retain(|q| q.get("name") != p.get("name"));
        properties.push(p.clone());
    }
    if let Json::Obj(own) = o {
        for (k, v) in own {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged.insert("properties".into(), Json::Arr(properties));
    Ok(Json::Obj(merged))
}

fn parse_object(o: &Json, layer: &str, offset: (f32, f32)) -> Result<ObjectDef, String> {
    let (x, y) = (real(o.get("x")) + offset.0, real(o.get("y")) + offset.1);
    let (w, h) = (real(o.get("width")), real(o.get("height")));
    let gid = uint(o.get("gid"));
    let is = |k: &str| matches!(o.get(k), Json::Bool(true));
    let words = o.get("text");
    let (shape, list) = if gid != 0 {
        (SHAPE_TILE, &[][..])
    } else if !words.is_null() {
        (SHAPE_TEXT, &[][..])
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
    let kind = o.get("type").str().filter(|s| !s.is_empty()).or_else(|| o.get("class").str()).unwrap_or("").to_string();
    let mut own = props(o);
    if let Json::Obj(style) = words {
        // how a text object looks is there for the program to read
        own.extend(style.iter().filter(|(k, _)| *k != "text").map(|(k, v)| (format!("text.{k}"), crate::level::prop_text(v))));
    }
    Ok(ObjectDef { name: text(o.get("name")), kind, layer: layer.to_string(), x, y, w, h, rotation: real(o.get("rotation")), shape, gid, points, props: own, text: text(words.get("text")), anchor: (0.0, 0.0) })
}

/// What a layer takes from the groups it is in.
#[derive(Clone, Copy)]
struct Inherited {
    offset: (f32, f32),
    visible: bool,
    opacity: f32,
    tint: [u8; 3],
    parallax: (f32, f32),
}

fn multiply(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [0, 1, 2].map(|i| (a[i] as u32 * b[i] as u32 / 255) as u8)
}

/// A layer's tile numbers, from a `data` value that is a list or Base64.
fn cells(data: &Json, layer: &Json) -> Result<Vec<u32>, String> {
    match data.str() {
        Some(s) => gids_from_base64(s, layer.get("compression").str()),
        None => Ok(data.arr().iter().map(uint).collect()),
    }
}

/// (x, y, width, height, tiles)
type JoinedChunks = (i32, i32, i32, i32, Vec<u32>);

/// A layer of an infinite map, as one rectangle.
fn join_chunks(l: &Json) -> Result<Option<JoinedChunks>, String> {
    let chunks = l.get("chunks").arr();
    let area = |c: &Json| (real(c.get("x")) as i32, real(c.get("y")) as i32, real(c.get("width")) as i32, real(c.get("height")) as i32);
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for (x, y, w, h) in chunks.iter().map(area) {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + w), y1.max(y + h));
    }
    if chunks.is_empty() || x1 <= x0 || y1 <= y0 {
        return Ok(None);
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut out = vec![0; w as usize * h as usize];
    for c in chunks {
        let (cx, cy, cw, ch) = area(c);
        let list = cells(c.get("data"), l)?;
        if list.len() != cw as usize * ch as usize {
            return Err("a chunk of an infinite map has the wrong size".into());
        }
        for (i, g) in list.into_iter().enumerate() {
            let (tx, ty) = (cx - x0 + i as i32 % cw, cy - y0 + i as i32 / cw);
            out[(ty * w + tx) as usize] = g;
        }
    }
    Ok(Some((x0, y0, w, h, out)))
}

fn parse_layers(list: &[Json], up: Inherited, infinite: bool, file: &Path, read: ReadFile, map: &mut MapDef) -> Result<(), String> {
    for l in list {
        let mut off = (up.offset.0 + real(l.get("offsetx")), up.offset.1 + real(l.get("offsety")));
        let shown = up.visible && !matches!(l.get("visible"), Json::Bool(false));
        let opacity = up.opacity * l.get("opacity").num().unwrap_or(1.0) as f32;
        let tint = l.get("tintcolor").str().and_then(color).map_or(up.tint, |c| multiply(c, up.tint));
        let factor = |k: &str| l.get(k).num().unwrap_or(1.0) as f32;
        let parallax = (up.parallax.0 * factor("parallaxx"), up.parallax.1 * factor("parallaxy"));
        let name = text(l.get("name"));
        match l.get("type").str().unwrap_or("") {
            "tilelayer" => {
                let mut width = if l.get("width").is_null() { map.width } else { uint(l.get("width")) as i32 };
                let mut height = if l.get("height").is_null() { map.height } else { uint(l.get("height")) as i32 };
                let gids = match join_chunks(l)? {
                    Some((x, y, w, h, list)) => {
                        let grid = Layout { shape: map.shape, cell: (map.tile_w as f32, map.tile_h as f32), rows: map.height, ..Layout::default() };
                        let (at, zero) = (grid.cell_pos(x, y), grid.cell_pos(0, 0));
                        off = (off.0 + at.0 - zero.0, off.1 + at.1 - zero.1);
                        (width, height) = (w, h);
                        list
                    }
                    None if infinite => {
                        (width, height) = (1, 1);
                        vec![0]
                    }
                    None => cells(l.get("data"), l)?,
                };
                if gids.len() != width as usize * height as usize {
                    return Err(format!("layer '{name}' has {} tiles, expected {width}x{height}", gids.len()));
                }
                map.layers.push(LayerDef { visible: shown, offset: off, opacity, tint, parallax, props: props(l), ..LayerDef::new(&name, width, height, gids) });
            }
            "objectgroup" => {
                for o in l.get("objects").arr() {
                    let o = with_template(o, file, read, map)?;
                    map.objects.push(parse_object(&o, &name, off)?);
                }
            }
            "imagelayer" => {
                if let Some(image) = l.get("image").str().filter(|s| !s.is_empty()) {
                    map.image_layers.push(ImageLayerDef {
                        name,
                        image: beside(file, image),
                        visible: shown,
                        offset: off,
                        opacity,
                        tint,
                        parallax,
                        repeat: (yes(l.get("repeatx")), yes(l.get("repeaty"))),
                        transparent: l.get("transparentcolor").str().and_then(color),
                        props: props(l),
                        z: map.layers.len(),
                    });
                }
            }
            "group" => parse_layers(l.get("layers").arr(), Inherited { offset: off, visible: shown, opacity, tint, parallax }, infinite, file, read, map)?,
            _ => {}
        }
    }
    Ok(())
}

fn map_shape(doc: &Json) -> Result<Shape, String> {
    let axis_x = doc.get("staggeraxis").str() == Some("x");
    let odd = doc.get("staggerindex").str() != Some("even");
    match doc.get("orientation").str().unwrap_or("orthogonal") {
        "orthogonal" => Ok(Shape::Ortho),
        "isometric" => Ok(Shape::Iso),
        "staggered" => Ok(Shape::Hex { axis_x, odd, side: 0.0 }),
        "hexagonal" => Ok(Shape::Hex { axis_x, odd, side: real(doc.get("hexsidelength")) }),
        other => Err(format!("{other} maps are not supported")),
    }
}

/// Turns object positions into map pixels and top-left corners.
fn place_objects(map: &mut MapDef) {
    let (tw, th, rows) = (map.tile_w as f32, map.tile_h as f32, map.height as f32);
    let iso = map.shape == Shape::Iso;
    // isometric maps keep objects in a square space, which is measured in tile heights
    let screen = |p: (f32, f32)| ((p.0 / th - p.1 / th) * tw / 2.0 + rows * tw / 2.0, (p.0 / th + p.1 / th) * th / 2.0);
    let tilesets = std::mem::take(&mut map.tilesets);
    for o in &mut map.objects {
        if iso {
            if o.shape == SHAPE_RECT {
                o.points = vec![(o.x, o.y), (o.x + o.w, o.y), (o.x + o.w, o.y + o.h), (o.x, o.y + o.h)];
            }
            o.points = o.points.iter().map(|&p| screen(p)).collect();
            (o.x, o.y) = screen((o.x, o.y));
        }
        if o.gid != 0 {
            let gid = o.gid & GID_MASK;
            let set = tilesets.iter().rev().find(|t| gid >= t.first_gid);
            let anchor = match set.map_or("", |t| t.align.as_str()) {
                "topleft" => (0.0, 0.0),
                "top" => (0.5, 0.0),
                "topright" => (1.0, 0.0),
                "left" => (0.0, 0.5),
                "center" => (0.5, 0.5),
                "right" => (1.0, 0.5),
                "bottomleft" => (0.0, 1.0),
                "bottom" => (0.5, 1.0),
                "bottomright" => (1.0, 1.0),
                _ if iso => (0.5, 1.0),
                _ => (0.0, 1.0),
            };
            o.anchor = anchor;
            o.x -= o.w * anchor.0;
            o.y -= o.h * anchor.1;
        }
    }
    map.tilesets = tilesets;
}

/// Reads a `.tmx` or `.tmj` file and the tilesets it names.
pub fn parse_map(path: &str, read: ReadFile) -> Result<MapDef, String> {
    let file = Path::new(path);
    let body = read.text(file)?;
    let doc = if body.trim_start().starts_with('<') { tmx_to_json(&parse_xml(&body)?)? } else { json::parse(&body)? };
    if doc.get("tilewidth").is_null() {
        return Err("not a Tiled map".into());
    }
    let order = doc.get("renderorder").str().unwrap_or("");
    let mut map = MapDef {
        width: uint(doc.get("width")) as i32,
        height: uint(doc.get("height")) as i32,
        tile_w: uint(doc.get("tilewidth")),
        tile_h: uint(doc.get("tileheight")),
        props: props(&doc),
        shape: map_shape(&doc)?,
        reverse: (order.starts_with("left"), order.ends_with("up")),
        background: doc.get("backgroundcolor").str().and_then(color),
        parallax_origin: (real(doc.get("parallaxoriginx")), real(doc.get("parallaxoriginy"))),
        ..MapDef::default()
    };
    map.tilesets = doc.get("tilesets").arr().iter().map(|t| parse_tileset(t, file, read)).collect::<Result<_, _>>()?;
    map.tilesets.sort_by_key(|t| t.first_gid);
    let infinite = yes(doc.get("infinite"));
    let top = Inherited { offset: (0.0, 0.0), visible: true, opacity: 1.0, tint: [255; 3], parallax: (1.0, 1.0) };
    parse_layers(doc.get("layers").arr(), top, infinite, file, read, &mut map)?;
    if infinite {
        map.width = map.layers.iter().map(|l| l.width).max().unwrap_or(1);
        map.height = map.layers.iter().map(|l| l.height).max().unwrap_or(1);
    }
    place_objects(&mut map);
    Ok(map)
}

/// Passes reads on and remembers what was read.
struct Recording<'a> {
    inner: ReadFile<'a>,
    seen: std::cell::RefCell<Vec<PathBuf>>,
}

impl Recording<'_> {
    fn note(&self, p: &Path) {
        self.seen.borrow_mut().push(p.to_path_buf());
    }
}

impl MapFiles for Recording<'_> {
    fn text(&self, p: &Path) -> Result<String, String> {
        self.note(p);
        self.inner.text(p)
    }

    fn bytes(&self, p: &Path) -> Result<Vec<u8>, String> {
        self.note(p);
        self.inner.bytes(p)
    }

    fn list(&self, dir: &Path) -> Vec<String> {
        self.inner.list(dir)
    }
}

/// The files a map needs beside itself, for `neptune build`: every level's
/// tilesets, templates, projects and pictures.
pub fn dependencies(path: &str, files: ReadFile) -> Result<Vec<PathBuf>, String> {
    let rec = Recording { inner: files, seen: Default::default() };
    let first = crate::mapformats::parse_any(path, "", &rec)?;
    let mut defs = Vec::new();
    for level in first.levels.iter().skip(1) {
        defs.push(crate::mapformats::parse_any(path, &level.name, &rec)?);
    }
    defs.push(first);
    let mut out = rec.seen.into_inner();
    for d in &defs {
        out.extend(d.tilesets.iter().flat_map(TilesetDef::image_files));
        out.extend(d.image_layers.iter().map(|l| l.image.clone()));
    }
    let own = tidy(Path::new(path));
    let mut unique: Vec<PathBuf> = Vec::new();
    for p in out {
        if p != own && !unique.contains(&p) {
            unique.push(p);
        }
    }
    Ok(unique)
}

// ---- tiles -----------------------------------------------------------------

/// The size of a frame, and where its edges are measured from the point a
/// tile hangs from: the bottom-left of its cell, or the bottom-middle on an
/// isometric map. Every tile fits inside, however big it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameBox {
    pub w: u32,
    pub h: u32,
    pub left: i32,
    pub top: i32,
}

/// Tiles cut from the tilesets, plus flipped copies made as they are needed.
pub struct Atlas {
    pub tiles: Vec<RgbaImage>,
    pub info: Vec<FrameInfo>,
    offsets: Vec<(f32, f32)>,
    /// (first gid, frame of its first tile, tile count) of each tileset.
    ranges: Vec<(u32, i32, u32)>,
    variants: HashMap<u32, i32>,
    /// Animation of a tile by its number: (tile number, milliseconds).
    gid_anims: HashMap<u32, Vec<(u32, u32)>>,
    pub anims: Vec<(i32, Vec<(i32, u32)>)>,
    map_tile: (u32, u32),
}

fn clear_color(img: &mut RgbaImage, c: Option<[u8; 3]>) {
    if let Some(c) = c {
        for p in img.pixels_mut().filter(|p| p[0] == c[0] && p[1] == c[1] && p[2] == c[2]) {
            p[3] = 0;
        }
    }
}

/// Scales a tile to the map's grid, for Tiled's "Tile render size: grid".
fn fit_to_grid(t: RgbaImage, set: &TilesetDef, grid: (u32, u32)) -> (RgbaImage, (f32, f32), (f32, f32)) {
    let (w, h) = t.dimensions();
    if !set.grid_render || (w, h) == grid || w == 0 || h == 0 {
        return (t, (1.0, 1.0), (0.0, 0.0));
    }
    let (mut sx, mut sy) = (grid.0 as f32 / w as f32, grid.1 as f32 / h as f32);
    if set.fit {
        sx = sx.min(sy);
        sy = sx;
    }
    let (nw, nh) = (((w as f32 * sx).round() as u32).max(1), ((h as f32 * sy).round() as u32).max(1));
    let scaled = imageops::resize(&t, nw, nh, imageops::FilterType::Nearest);
    if (nw, nh) == grid {
        return (scaled, (sx, sy), (0.0, 0.0));
    }
    // kept its shape: centred in the cell
    let mut out = RgbaImage::new(grid.0, grid.1);
    let at = ((grid.0.saturating_sub(nw)) / 2, (grid.1.saturating_sub(nh)) / 2);
    imageops::replace(&mut out, &scaled, at.0 as i64, at.1 as i64);
    (out, (sx, sy), (at.0 as f32, at.1 as f32))
}

/// How many tiles fit along a tileset picture of `size` pixels.
pub(crate) fn fit_count(size: u32, margin: u32, tile: u32, spacing: u32) -> u32 {
    (size + spacing).saturating_sub(2 * margin) / (tile + spacing).max(1)
}

impl Atlas {
    /// Cuts every tileset, given the pictures that [`TilesetDef::image_files`] names.
    pub fn new(map: &MapDef, images: &[RgbaImage]) -> Result<Atlas, String> {
        let grid = (map.tile_w, map.tile_h);
        let mut a = Atlas { tiles: Vec::new(), info: Vec::new(), offsets: Vec::new(), ranges: Vec::new(), variants: HashMap::new(), gid_anims: HashMap::new(), anims: Vec::new(), map_tile: grid };
        let mut next = images.iter();
        for set in &map.tilesets {
            let base = a.tiles.len() as i32;
            let mut cut: Vec<RgbaImage> = Vec::new();
            if set.pixels.is_none() && set.image.as_os_str().is_empty() {
                let mut own: HashMap<u32, RgbaImage> = HashMap::new();
                for t in set.tiles.iter().filter(|t| t.image.is_some()) {
                    let mut img = next.next().ok_or_else(|| format!("tileset '{}' is missing a picture", set.name))?.clone();
                    clear_color(&mut img, set.transparent);
                    if let Some((x, y, w, h)) = t.rect {
                        img = imageops::crop_imm(&img, x, y, w, h).to_image();
                    }
                    own.insert(t.id, img);
                }
                cut.extend((0..set.count).map(|i| own.remove(&i).unwrap_or_else(|| RgbaImage::new(1, 1))));
            } else {
                let mut img = match &set.pixels {
                    Some(p) => p.clone(),
                    None => next.next().ok_or_else(|| format!("tileset '{}' is missing a picture", set.name))?.clone(),
                };
                clear_color(&mut img, set.transparent);
                let columns = if set.columns > 0 { set.columns } else { fit_count(img.width(), set.margin, set.tile_w, set.spacing).max(1) };
                let count = if set.count > 0 { set.count } else { columns * fit_count(img.height(), set.margin, set.tile_h, set.spacing) };
                for i in 0..count {
                    let x = set.margin + (i % columns) * (set.tile_w + set.spacing);
                    let y = set.margin + (i / columns) * (set.tile_h + set.spacing);
                    if x + set.tile_w > img.width() || y + set.tile_h > img.height() {
                        return Err(format!("tileset '{}' is larger than its image", set.name));
                    }
                    cut.push(imageops::crop_imm(&img, x, y, set.tile_w, set.tile_h).to_image());
                }
            }
            a.ranges.push((set.first_gid, base, cut.len() as u32));
            for (i, t) in cut.into_iter().enumerate() {
                let extra = set.tiles.iter().find(|d| d.id == i as u32);
                let (t, scale, shift) = fit_to_grid(t, set, grid);
                let shapes = extra.map_or_else(Vec::new, |d| {
                    let moved = |s: &TileShape| TileShape { closed: s.closed, points: s.points.iter().map(|p| (p.0 * scale.0 + shift.0, p.1 * scale.1 + shift.1)).collect() };
                    d.shapes.iter().map(moved).collect()
                });
                a.info.push(FrameInfo {
                    kind: extra.map_or_else(String::new, |d| d.kind.clone()),
                    props: extra.map_or_else(Vec::new, |d| d.props.clone()),
                    shapes,
                    place: (0.0, 0.0),
                    size: (t.width() as f32, t.height() as f32),
                });
                a.tiles.push(t);
                a.offsets.push(set.offset);
            }
            for t in set.tiles.iter().filter(|t| !t.anim.is_empty()) {
                a.gid_anims.insert(set.first_gid + t.id, t.anim.iter().map(|&(id, ms)| (set.first_gid + id, ms)).collect());
            }
        }
        let animated: Vec<(u32, Vec<(u32, u32)>)> = a.gid_anims.iter().map(|(g, s)| (*g, s.clone())).collect();
        for (gid, seq) in animated {
            let f = a.frame(gid);
            if f >= 0 {
                let frames = seq.iter().map(|&(g, ms)| (a.frame(g), ms)).filter(|p| p.0 >= 0).collect();
                a.anims.push((f, frames));
            }
        }
        Ok(a)
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
        let mut info = self.info[frame as usize].clone();
        let flip = |p: (f32, f32), w: f32, h: f32| {
            let (mut x, mut y) = if flags & FLIP_D != 0 { (p.1, p.0) } else { p };
            if flags & FLIP_H != 0 {
                x = w - x;
            }
            if flags & FLIP_V != 0 {
                y = h - y;
            }
            (x, y)
        };
        if flags & FLIP_D != 0 {
            // the diagonal flip comes first
            t = imageops::flip_horizontal(&imageops::rotate90(&t));
        }
        if flags & FLIP_H != 0 {
            imageops::flip_horizontal_in_place(&mut t);
        }
        if flags & FLIP_V != 0 {
            imageops::flip_vertical_in_place(&mut t);
        }
        info.size = (t.width() as f32, t.height() as f32);
        let (w, h) = info.size;
        for s in &mut info.shapes {
            s.points = s.points.iter().map(|&p| flip(p, w, h)).collect();
        }
        self.tiles.push(t);
        self.info.push(info);
        self.offsets.push(self.offsets[frame as usize]);
        let v = self.tiles.len() as i32 - 1;
        self.variants.insert(key, v);
        // a flipped animated tile plays flipped frames
        if let Some(seq) = self.gid_anims.get(&gid).cloned() {
            let frames = seq.iter().map(|&(g, ms)| (self.frame(g | flags), ms)).filter(|p| p.0 >= 0).collect();
            self.anims.push((v, frames));
        }
        v
    }

    /// The frame size that holds every tile.
    pub fn frame_box(&self, centered: bool) -> FrameBox {
        if self.tiles.is_empty() {
            let (w, h) = (self.map_tile.0.max(1), self.map_tile.1.max(1));
            return FrameBox { w, h, left: 0, top: -(h as i32) };
        }
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for (t, off) in self.tiles.iter().zip(&self.offsets) {
            let (w, h) = (t.width() as i32, t.height() as i32);
            let left = off.0.round() as i32 - if centered { w / 2 } else { 0 };
            let bottom = off.1.round() as i32;
            (x0, y0, x1, y1) = (x0.min(left), y0.min(bottom - h), x1.max(left + w), y1.max(bottom));
        }
        FrameBox { w: (x1 - x0) as u32, h: (y1 - y0) as u32, left: x0, top: y0 }
    }

    /// All tiles in one image, in frames of the size given.
    pub fn pack(&mut self, b: &FrameBox) -> RgbaImage {
        let n = self.tiles.len().max(1) as u32;
        let columns = (n as f32).sqrt().ceil() as u32;
        let mut out = RgbaImage::new(columns * b.w, n.div_ceil(columns) * b.h);
        // tiles hang from the same point in every frame; `frame_box` says how they were measured
        let centered = b.left < 0 && self.tiles.iter().zip(&self.offsets).any(|(t, o)| o.0.round() as i32 - (t.width() as i32 / 2) == b.left);
        for i in 0..self.tiles.len() {
            let (t, off) = (&self.tiles[i], self.offsets[i]);
            let left = off.0.round() as i32 - if centered { t.width() as i32 / 2 } else { 0 };
            let (px, py) = ((left - b.left) as i64, (off.1.round() as i32 - t.height() as i32 - b.top) as i64);
            let at = i as u32;
            imageops::replace(&mut out, t, ((at % columns) * b.w) as i64 + px, ((at / columns) * b.h) as i64 + py);
            self.info[i].place = (px as f32, py as f32);
        }
        out
    }
}

// ---- commands --------------------------------------------------------------

/// The largest image the graphics cards we run on all accept.
const MAX_IMAGE: u32 = 16384;

fn orientation_name(shape: Shape) -> &'static str {
    match shape {
        Shape::Ortho => "orthogonal",
        Shape::Iso => "isometric",
        Shape::Hex { side: 0.0, .. } => "staggered",
        Shape::Hex { .. } => "hexagonal",
    }
}

fn open_image(path: &Path) -> Result<RgbaImage, String> {
    image::open(path).map(|i| i.to_rgba8()).map_err(|e| format!("cannot load {}: {e}", path.display()))
}

fn build(h: &mut Host3D, path: &str, level: &str) -> Result<i32, String> {
    let mut def = crate::mapformats::parse_any(path, level, &DiskFiles { root: PathBuf::new() })?;
    let images = def.tilesets.iter().flat_map(TilesetDef::image_files).map(|p| open_image(&p)).collect::<Result<Vec<_>, _>>()?;
    let mut atlas = Atlas::new(&def, &images)?;
    let grids: Vec<Vec<i32>> = def.layers.iter().map(|l| l.gids.iter().map(|&g| if l.data_only { if g == 0 { EMPTY } else { g as i32 } } else { atlas.frame(g) }).collect()).collect();
    let tiles: Vec<i32> = def.objects.iter().map(|o| if o.gid != 0 { atlas.frame(o.gid) } else { -1 }).collect();
    // an object on a tile takes the tile's type and properties unless it has its own
    for (o, &t) in def.objects.iter_mut().zip(&tiles) {
        if let Some(info) = usize::try_from(t).ok().and_then(|t| atlas.info.get(t)) {
            if o.kind.is_empty() {
                o.kind = info.kind.clone();
            }
            for (k, v) in &info.props {
                if crate::level::prop(&o.props, k).is_none() {
                    o.props.push((k.clone(), v.clone()));
                }
            }
        }
    }
    let centered = def.shape == Shape::Iso;
    let frame = atlas.frame_box(centered);
    let frames = atlas.tiles.len().max(1) as u32;
    let columns = (frames as f32).sqrt().ceil() as u32;
    if columns * frame.w > MAX_IMAGE || frames.div_ceil(columns) * frame.h > MAX_IMAGE {
        return Err(format!("{frames} tiles in frames of {}x{} do not fit in one image; use fewer or smaller tiles", frame.w, frame.h));
    }
    let mut img = crate::image2d::Image2D::new(atlas.pack(&frame), frame.w, frame.h, 0, frames)?;
    img.path = path.replace('\\', "/");
    img.use_alpha_only();
    h.world.images.push(Some(img));
    let image = h.world.images.len() as i32;
    let anims = (!atlas.anims.is_empty()).then(|| Arc::new(TileAnims::new(atlas.anims.drain(..))));
    let mut layers = Vec::new();
    for (l, grid) in def.layers.iter().zip(grids) {
        let mut map = TileMap::new(image, l.width, l.height)?;
        for (i, &t) in grid.iter().enumerate() {
            map.set(i as i32 % l.width, i as i32 / l.width, t);
        }
        let cell = l.cell.unwrap_or((def.tile_w, def.tile_h));
        let cell = (cell.0 as f32, cell.1 as f32);
        let hang = if centered { cell.0 / 2.0 } else { 0.0 };
        map.layout = Some(Layout { shape: def.shape, cell, rows: def.height, frame: (hang + frame.left as f32 + l.shift.0, cell.1 + frame.top as f32 + l.shift.1), reverse: def.reverse });
        map.data_only = l.data_only;
        map.anims = anims.clone();
        h.world.tilemaps.push(Some(map));
        layers.push(TiledLayer {
            name: l.name.clone(),
            group: l.group.clone(),
            map: h.world.tilemaps.len() as i32,
            visible: l.visible && !l.data_only,
            look: LayerLook { offset: l.offset, opacity: l.opacity, tint: l.tint, parallax: l.parallax },
            props: Arc::new(l.props.clone()),
        });
    }
    let mut image_layers = Vec::new();
    for l in &def.image_layers {
        let mut pixels = open_image(&l.image)?;
        clear_color(&mut pixels, l.transparent);
        let (w, ht) = pixels.dimensions();
        let mut layer_image = crate::image2d::Image2D::new(pixels, w, ht, 0, 1)?;
        layer_image.path = l.image.to_string_lossy().replace('\\', "/");
        layer_image.use_alpha_only();
        h.world.images.push(Some(layer_image));
        image_layers.push(TiledImageLayer {
            name: l.name.clone(),
            image: h.world.images.len() as i32,
            visible: l.visible,
            look: LayerLook { offset: l.offset, opacity: l.opacity, tint: l.tint, parallax: l.parallax },
            repeat: l.repeat,
            z: l.z,
            props: Arc::new(l.props.clone()),
        });
    }
    let id = h.world.tiled_maps.len() as i32 + 1;
    let mut objects = Vec::new();
    for (o, tile) in def.objects.iter().zip(tiles) {
        h.world.tiled_objects.push(Some(TiledObject { def: o.clone(), tile, map: id }));
        objects.push(h.world.tiled_objects.len() as i32);
    }
    let grid = Layout { shape: def.shape, cell: (def.tile_w as f32, def.tile_h as f32), rows: def.height, ..Layout::default() };
    h.world.tiled_maps.push(Some(TiledMap {
        image,
        layers,
        image_layers,
        objects,
        props: def.props,
        width: def.width,
        height: def.height,
        tile_size: (def.tile_w, def.tile_h),
        pixel_size: grid.pixel_size(def.width, def.height),
        orientation: orientation_name(def.shape),
        background: def.background,
        parallax_origin: def.parallax_origin,
        frames: Arc::new(atlas.info),
        levels: def.levels,
        level: def.level,
    }));
    Ok(id)
}

/// `LoadTiledMap`: 0 if the map cannot be loaded (like `LoadImage`).
pub fn load(h: &mut Host3D, path: &str, level: &str) -> Result<i32, String> {
    match build(h, path, level) {
        Ok(id) => Ok(id),
        Err(e) => {
            eprintln!("LoadTiledMap: {path}: {e}");
            Ok(0)
        }
    }
}

/// The static body of a map's collision shapes (`TiledMapBody2D`). Only
/// tiles given shapes in the editor take part.
pub fn collision_body(h: &mut Host3D, m: i32, layer: &str, x: f32, y: f32) -> Result<i32, String> {
    let t = map(h, m)?.clone();
    let mut outlines: Vec<TileShape> = Vec::new();
    for l in t.layers.iter().filter(|l| layer.is_empty() || l.name.eq_ignore_ascii_case(layer) || l.group.eq_ignore_ascii_case(layer)) {
        let Some(grid) = h.world.tilemaps.get(l.map as usize - 1).and_then(Option::as_ref) else { continue };
        let (Some(layout), false) = (&grid.layout, grid.data_only) else { continue };
        for cy in 0..grid.height {
            for cx in 0..grid.width {
                let Some(info) = usize::try_from(grid.get(cx, cy)).ok().and_then(|f| t.frames.get(f)) else { continue };
                let (px, py) = layout.cell_pos(cx, cy);
                let (ox, oy) = (px + layout.frame.0 + info.place.0 + l.look.offset.0, py + layout.frame.1 + info.place.1 + l.look.offset.1);
                outlines.extend(info.shapes.iter().map(|s| TileShape { closed: s.closed, points: s.points.iter().map(|p| (p.0 + ox, p.1 + oy)).collect() }));
            }
        }
    }
    let b = h.create_body2d(1, x, y)?;
    for s in outlines {
        if s.closed {
            h.body2d_polygon(b, &polygon_text(&s.points))?;
        } else {
            for pair in s.points.windows(2) {
                h.body2d_segment(b, pair[0].0, pair[0].1, pair[1].0, pair[1].1)?;
            }
        }
    }
    Ok(b)
}

fn polygon_text(points: &[(f32, f32)]) -> String {
    points.iter().map(|p| format!("{},{}", p.0, p.1)).collect::<Vec<_>>().join(",")
}

/// `TileMapBody2D` on a grid of diamonds or hexagons: one outline per cell.
pub fn grid_body(h: &mut Host3D, m: i32, layout: &Layout, x: f32, y: f32) -> Result<i32, String> {
    let outlines: Vec<String> = {
        let grid = h.world.tilemaps.get(m as usize - 1).and_then(Option::as_ref).ok_or_else(|| format!("Tile map {m} does not exist"))?;
        let mut list = Vec::new();
        for cy in 0..grid.height {
            for cx in 0..grid.width {
                if grid.get(cx, cy) >= 0 {
                    list.push(polygon_text(&layout.polygon(cx, cy)));
                }
            }
        }
        list
    };
    let b = h.create_body2d(1, x, y)?;
    for p in outlines {
        h.body2d_polygon(b, &p)?;
    }
    Ok(b)
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
    for l in t.image_layers {
        h.world.images[l.image as usize - 1] = None;
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
        assert!(f(r#"{"tilewidth":2,"orientation":"sideways"}"#).contains("sideways"));
        assert!(f(r#"{"tilewidth":2,"width":1,"height":1,"layers":[{"type":"tilelayer","data":"AAAA","encoding":"base64","compression":"zstd"}]}"#).contains("Zstandard"));
        assert!(f(r#"{"tilewidth":2,"tilesets":[{"firstgid":1,"name":"x"}]}"#).contains("no image"));
        assert!(f(r#"{"hello":1}"#).contains("not a Tiled map"));
        let t = r#"<map width="1" height="1" tilewidth="2" tileheight="2"><layer name="a"><data encoding="base64" compression="zlib">eJw=</data></layer></map>"#;
        assert!(parse_map("a.tmx", &files("a.tmx", t)).unwrap_err().contains("compressed data ends early"));
    }

    fn only_map(body: &str) -> MapDef {
        let body = body.to_string();
        parse_map("a.tmx", &move |_: &Path| Ok(body.clone())).unwrap()
    }

    #[test]
    fn every_layer_encoding_reads_the_same() {
        // gids 1, 2147483652 (tile 4, flipped) and 0
        let tmx = |data: &str| format!(r#"<map width="3" height="1" tilewidth="2" tileheight="2"><layer name="a" width="3" height="1">{data}</layer></map>"#);
        let want = vec![1, 2147483652, 0];
        assert_eq!(only_map(&tmx(r#"<data encoding="csv">1,2147483652,0</data>"#)).layers[0].gids, want);
        assert_eq!(only_map(&tmx(r#"<data encoding="base64"> AQAAAAQAAIAAAAAA </data>"#)).layers[0].gids, want);
        assert_eq!(only_map(&tmx(r#"<data><tile gid="1"/><tile gid="2147483652"/><tile/></data>"#)).layers[0].gids, want);
        assert_eq!(only_map(&tmx(&format!(r#"<data encoding="base64" compression="zlib">{ZLIB_LAYER}</data>"#))).layers[0].gids, want);
        assert_eq!(only_map(&tmx(&format!(r#"<data encoding="base64" compression="gzip">{GZIP_LAYER}</data>"#))).layers[0].gids, want);
        let tmj = format!(r#"{{"tilewidth":2,"tileheight":2,"width":3,"height":1,"layers":[{{"type":"tilelayer","name":"a","width":3,"height":1,"encoding":"base64","compression":"zlib","data":"{ZLIB_LAYER}"}}]}}"#);
        assert_eq!(parse_map("a.tmj", &move |_: &Path| Ok(tmj.clone())).unwrap().layers[0].gids, want);
    }

    const ZLIB_LAYER: &str = "eJxjZGBgYGFgaGBgYGAAAAK4AIY=";
    const GZIP_LAYER: &str = "H4sIAAAAAAAC/2NkYGBgYWBoAFIMAGg03NUMAAAA";

    #[test]
    fn infinite_maps_join_their_chunks() {
        let m = only_map(
            r#"<map infinite="1" width="30" height="20" tilewidth="4" tileheight="4"><layer name="a" width="30" height="20"><data encoding="csv">
              <chunk x="-2" y="0" width="2" height="1">1,2</chunk><chunk x="0" y="1" width="2" height="1">3,4</chunk></data></layer>
              <layer name="empty" width="30" height="20"><data encoding="csv"></data></layer></map>"#,
        );
        let l = &m.layers[0];
        assert_eq!((l.width, l.height, l.offset), (4, 2, (-8.0, 0.0)));
        assert_eq!(l.gids, [1, 2, 0, 0, 0, 0, 3, 4]);
        assert_eq!((m.layers[1].width, m.layers[1].gids.len()), (1, 1));
        assert_eq!((m.width, m.height), (4, 2));
    }

    #[test]
    fn map_kinds() {
        let map = |attrs: &str| only_map(&format!(r#"<map {attrs} width="1" height="1" tilewidth="2" tileheight="2"/>"#)).shape;
        assert_eq!(map(r#"orientation="isometric""#), Shape::Iso);
        assert_eq!(map(r#"orientation="staggered" staggeraxis="x" staggerindex="even""#), Shape::Hex { axis_x: true, odd: false, side: 0.0 });
        assert_eq!(map(r#"orientation="hexagonal" staggeraxis="y" staggerindex="odd" hexsidelength="6""#), Shape::Hex { axis_x: false, odd: true, side: 6.0 });
    }

    #[test]
    fn layers_look_and_image_layers() {
        let m = only_map(
            r##"<map width="1" height="1" tilewidth="2" tileheight="2" backgroundcolor="#102030" parallaxoriginx="5" renderorder="left-up">
             <imagelayer name="Sky" repeatx="1" parallaxx="0.5" offsetx="3"><image source="../sky.png" trans="ff00ff"/><properties><property name="far" value="yes"/></properties></imagelayer>
             <group name="G" opacity="0.5" tintcolor="#80ffffff" parallaxx="0.5"><layer name="L" opacity="0.5" tintcolor="#ff0000" parallaxy="2" width="1" height="1"><data encoding="csv">0</data><properties><property name="solid" type="bool" value="true"/></properties></layer></group>
             <imagelayer name="Front"><image source="f.png"/></imagelayer></map>"##,
        );
        assert_eq!(m.background, Some([16, 32, 48]));
        assert_eq!(m.parallax_origin, (5.0, 0.0));
        assert_eq!(m.reverse, (true, true));
        let sky = &m.image_layers[0];
        assert_eq!((sky.image.clone(), sky.repeat, sky.parallax.0, sky.z, sky.transparent), (PathBuf::from("../sky.png"), (true, false), 0.5, 0, Some([255, 0, 255])));
        assert_eq!(crate::level::prop(&sky.props, "far"), Some("yes"));
        assert_eq!(m.image_layers[1].z, 1, "above the tile layer");
        let l = &m.layers[0];
        assert_eq!((l.opacity, l.tint, l.parallax), (0.25, [255, 0, 0], (0.5, 2.0)));
        assert_eq!(crate::level::prop(&l.props, "solid"), Some("1"));
    }

    const RICH_TSX: &str = r#"<tileset name="rich" tilewidth="2" tileheight="4" tilecount="3" columns="0" objectalignment="center">
      <tileoffset x="1" y="-1"/>
      <tile id="0" type="spike"><properties><property name="hurts" type="int" value="3"/></properties><image width="2" height="4" source="a.png"/>
        <animation><frame tileid="0" duration="100"/><frame tileid="2" duration="200"/></animation>
        <objectgroup><object x="0" y="1" width="2" height="1"/><object x="1" y="1"><polyline points="0,0 1,1"/></object></objectgroup></tile>
      <tile id="2" x="1" y="0" width="1" height="2"><image width="4" height="4" source="b.png"/></tile>
    </tileset>"#;

    fn rich_map(body: &'static str) -> MapDef {
        parse_map("m/a.tmx", &move |p: &Path| match p.to_string_lossy().replace('\\', "/").as_str() {
            "m/a.tmx" => Ok(body.to_string()),
            "m/rich.tsx" => Ok(RICH_TSX.to_string()),
            "m/t.tx" => Ok(r#"<template><tileset firstgid="7" source="rich.tsx"/><object name="fromT" type="crate" gid="9" width="2" height="4"><properties><property name="a" value="1"/><property name="b" value="2"/></properties></object></template>"#.to_string()),
            other => Err(format!("no file {other}")),
        })
        .unwrap()
    }

    const RICH_TMX: &str = r##"<map width="2" height="1" tilewidth="2" tileheight="2"><tileset firstgid="1" source="rich.tsx"/>
      <layer name="L" width="2" height="1"><data encoding="csv">1,2147483651</data></layer>
      <objectgroup name="O"><object template="t.tx" x="10" y="20"><properties><property name="b" value="own"/></properties></object>
        <object x="1" y="2" width="9"><text pixelsize="20" color="#ff0000" wrap="1">Hello</text></object></objectgroup></map>"##;

    #[test]
    fn collection_tilesets_animations_and_shapes() {
        let m = rich_map(RICH_TMX);
        let set = &m.tilesets[0];
        assert_eq!((set.count, set.offset, set.align.as_str()), (3, (1.0, -1.0), "center"));
        assert_eq!(set.image_files(), [PathBuf::from("m/a.png"), PathBuf::from("m/b.png")]);
        let (a, b) = (RgbaImage::from_pixel(2, 4, image::Rgba([9, 0, 0, 255])), RgbaImage::from_fn(4, 4, |x, _| image::Rgba([x as u8, 1, 0, 255])));
        let mut atlas = Atlas::new(&m, &[a, b]).unwrap();
        assert_eq!(atlas.tiles.len(), 3);
        assert_eq!(atlas.tiles[2].dimensions(), (1, 2), "cut to its rectangle");
        assert_eq!(atlas.tiles[2].get_pixel(0, 0)[0], 1);
        assert_eq!(atlas.tiles[1].dimensions(), (1, 1), "a gap in the numbers");
        assert_eq!(atlas.info[0].kind, "spike");
        assert_eq!(atlas.info[0].shapes[0].points[2], (2.0, 2.0));
        assert!(!atlas.info[0].shapes[1].closed);
        assert_eq!(atlas.anims, [(0, vec![(0, 100), (2, 200)])]);
        // a flipped animated tile plays flipped frames and flips its shapes
        let f = atlas.frame(1 | FLIP_H);
        assert_eq!(atlas.info[f as usize].shapes[0].points[0], (2.0, 1.0));
        let (v0, v1) = (f, atlas.frame(3 | FLIP_H));
        assert!(atlas.anims.contains(&(v0, vec![(v0, 100), (v1, 200)])));
    }

    #[test]
    fn frames_hold_every_size_of_tile() {
        let m = rich_map(RICH_TMX);
        let (a, b) = (RgbaImage::new(2, 4), RgbaImage::new(4, 4));
        let mut atlas = Atlas::new(&m, &[a, b]).unwrap();
        // tiles are 2x4, 1x1 and 1x2, all moved by (1, -1)
        let frame = atlas.frame_box(false);
        assert_eq!(frame, FrameBox { w: 2, h: 4, left: 1, top: -5 });
        atlas.pack(&frame);
        assert_eq!(atlas.info[1].place, (0.0, 3.0), "small tiles sit at the bottom of the frame");
    }

    #[test]
    fn templates_fill_in_objects() {
        let m = rich_map(RICH_TMX);
        let o = &m.objects[0];
        // tile 2 of the map's copy of the tileset
        assert_eq!((o.name.as_str(), o.kind.as_str(), o.gid), ("fromT", "crate", 3));
        assert_eq!(crate::level::prop(&o.props, "a"), Some("1"));
        assert_eq!(crate::level::prop(&o.props, "b"), Some("own"));
        // objectalignment is center
        assert_eq!((o.x, o.y, o.anchor), (9.0, 18.0, (0.5, 0.5)));
    }

    #[test]
    fn a_template_can_bring_its_own_tileset() {
        let body = r#"<map width="1" height="1" tilewidth="2" tileheight="2"><objectgroup name="O"><object template="t.tx" x="0" y="0"/></objectgroup></map>"#;
        let m = rich_map(body);
        assert_eq!(m.tilesets.len(), 1);
        assert_eq!(m.objects[0].gid, 3);
    }

    #[test]
    fn text_objects() {
        let m = rich_map(RICH_TMX);
        let o = &m.objects[1];
        assert_eq!((o.shape, o.text.as_str()), (SHAPE_TEXT, "Hello"));
        assert_eq!(crate::level::prop(&o.props, "text.color"), Some("#ff0000"));
    }

    #[test]
    fn isometric_objects_are_moved_to_the_screen() {
        let m = only_map(
            r#"<map orientation="isometric" width="4" height="4" tilewidth="64" tileheight="32"><tileset firstgid="1" name="t" tilewidth="64" tileheight="32" tilecount="1" columns="1"><image source="a.png"/></tileset>
             <objectgroup name="O"><object x="32" y="0"><point/></object><object x="0" y="0" width="32" height="32"/><object gid="1" x="32" y="32" width="64" height="32"/></objectgroup></map>"#,
        );
        // one tile height along the first axis is one tile across the diamond
        assert_eq!((m.objects[0].x, m.objects[0].y), (128.0 + 32.0, 16.0));
        assert_eq!(m.objects[1].points[2], (128.0, 32.0));
        // tile objects hang from their bottom middle
        assert_eq!((m.objects[2].x, m.objects[2].y), (128.0 - 32.0, 0.0));
    }

    #[test]
    fn dependencies_include_everything_read() {
        let list = dependencies("m/a.tmx", &|p: &Path| match p.to_string_lossy().replace('\\', "/").as_str() {
            "m/a.tmx" => Ok(RICH_TMX.to_string()),
            "m/rich.tsx" => Ok(RICH_TSX.to_string()),
            "m/t.tx" => Ok(r#"<template><object name="x"/></template>"#.to_string()),
            other => Err(format!("no file {other}")),
        })
        .unwrap();
        let names: Vec<String> = list.iter().map(|p| p.to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(names, ["m/rich.tsx", "m/t.tx", "m/a.png", "m/b.png"]);
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
    fn a_tileset_can_differ_from_the_map() {
        let (mut map, img) = tileset();
        map.tile_w = 16;
        assert_eq!(Atlas::new(&map, &[img]).unwrap().tiles[0].dimensions(), (2, 2));
    }

    #[test]
    fn tiles_can_be_scaled_to_the_grid() {
        let (mut map, img) = tileset();
        map.tile_w = 4;
        map.tile_h = 2;
        map.tilesets[0].grid_render = true;
        let a = Atlas::new(&map, std::slice::from_ref(&img)).unwrap();
        assert_eq!(a.tiles[0].dimensions(), (4, 2));
        map.tilesets[0].fit = true;
        let a = Atlas::new(&map, &[img]).unwrap();
        assert_eq!(a.tiles[0].dimensions(), (4, 2), "kept square inside the cell");
        assert_eq!(a.tiles[0].get_pixel(0, 0)[3], 0);
        assert_eq!(a.tiles[0].get_pixel(1, 0)[3], 255);
    }

    #[test]
    fn pack_lays_out_frames_in_a_grid() {
        let (map, img) = tileset();
        let mut a = Atlas::new(&map, &[img]).unwrap();
        let frame = a.frame_box(false);
        assert_eq!(frame, FrameBox { w: 2, h: 2, left: 0, top: -2 });
        let p = a.pack(&frame);
        assert_eq!(p.dimensions(), (4, 4));
        assert_eq!(p.get_pixel(2, 2).0[0], 30);
    }
}
