//! Level editors other than Tiled (ext): LDtk, Ogmo Editor 3, Sprite Fusion,
//! Pyxel Edit and Aseprite tilemaps, plus Tiled's `.world` files.
//!
//! Each one is turned into a [`MapDef`], so a map behaves the same whichever
//! editor made it. `LoadTiledMap` looks at the file and picks the reader.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::RgbaImage;

use crate::inflate;
use crate::json::{self, Json};
use crate::tiled::{beside, color, parse_map, parse_xml_json, real, text, uint, LayerDef, LevelDef, MapDef, ObjectDef, ReadFile, TileDef, TilesetDef, FLIP_D, FLIP_H, FLIP_V, SHAPE_POINT, SHAPE_RECT, SHAPE_TILE};

/// Reads a map from any supported editor. `level` picks a level of a file
/// that holds several; empty means the first.
pub fn parse_any(path: &str, level: &str, files: ReadFile) -> Result<MapDef, String> {
    let file = Path::new(path);
    let ext = file.extension().map_or(String::new(), |e| e.to_string_lossy().to_ascii_lowercase());
    if ext == "aseprite" || ext == "ase" {
        return parse_aseprite(&files.bytes(file)?);
    }
    let body = files.text(file)?;
    if body.trim_start().starts_with('<') {
        let doc = parse_xml_json(&body)?;
        if doc.get("_name").str() == Some("tilemap") {
            return parse_pyxel(file, &pyxel_xml(&doc), files);
        }
        return parse_map(path, files);
    }
    let doc = json::parse(&body)?;
    let has = |k: &str| !doc.get(k).is_null();
    if has("ogmoVersion") {
        parse_ogmo(file, &doc, files)
    } else if has("tileswide") {
        parse_pyxel(file, &doc, files)
    } else if has("tileSize") && has("mapWidth") {
        Ok(parse_sprite_fusion(file, &doc))
    } else if has("defs") && (has("levels") || has("worlds")) {
        parse_ldtk(file, level, &doc, files)
    } else if has("maps") && (ext == "world" || doc.get("type").str() == Some("world")) {
        parse_world(file, level, &doc, files)
    } else {
        parse_map(path, files)
    }
}

fn int(v: &Json) -> i32 {
    v.num().map_or(0, |n| n as i32)
}

fn truthy(v: &Json) -> bool {
    matches!(v, Json::Bool(true)) || v.str() == Some("true")
}

fn pick_level(levels: &[LevelDef], want: &str) -> Result<usize, String> {
    if want.is_empty() {
        return if levels.is_empty() { Err("the file has no levels".into()) } else { Ok(0) };
    }
    levels.iter().position(|l| l.name.eq_ignore_ascii_case(want)).ok_or_else(|| format!("no level called '{want}'"))
}

/// A picture beside `file`, trying these names without regard to case.
fn find_beside(file: &Path, names: &[String], files: ReadFile) -> Option<PathBuf> {
    let dir = file.parent().unwrap_or(Path::new(""));
    let list = files.list(dir);
    names.iter().find_map(|n| list.iter().find(|f| f.eq_ignore_ascii_case(n)).map(|f| beside(file, f)))
}

// ---- Tiled worlds -----------------------------------------------------------

fn parse_world(file: &Path, level: &str, doc: &Json, files: ReadFile) -> Result<MapDef, String> {
    let maps = doc.get("maps").arr();
    if maps.is_empty() {
        return Err("this world only has patterns; list its maps instead".into());
    }
    let name = |m: &Json| Path::new(m.get("fileName").str().unwrap_or("")).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let levels: Vec<LevelDef> = maps.iter().map(|m| LevelDef { name: name(m), x: real(m.get("x")), y: real(m.get("y")) }).collect();
    let at = pick_level(&levels, level)?;
    let map_file = beside(file, maps[at].get("fileName").str().unwrap_or(""));
    let mut map = parse_any(&map_file.to_string_lossy(), "", files)?;
    map.level = levels[at].name.clone();
    map.levels = levels;
    Ok(map)
}

// ---- LDtk --------------------------------------------------------------------

struct LdtkSet {
    first: u32,
    columns: u32,
    grid: u32,
    padding: u32,
    spacing: u32,
}

impl LdtkSet {
    /// A tile's number from its pixel position in the tileset.
    fn gid_at(&self, x: u32, y: u32) -> u32 {
        let step = self.grid + self.spacing;
        self.first + x.saturating_sub(self.padding) / step.max(1) + y.saturating_sub(self.padding) / step.max(1) * self.columns
    }
}

fn ldtk_tilesets(doc: &Json, file: &Path) -> (Vec<TilesetDef>, HashMap<u32, LdtkSet>) {
    let (mut defs, mut sets) = (Vec::new(), HashMap::new());
    let mut first = 1;
    for t in doc.get("defs").get("tilesets").arr() {
        // LDtk's built-in icons have no file
        let Some(rel) = t.get("relPath").str() else { continue };
        let (grid, padding, spacing) = (uint(t.get("tileGridSize")).max(1), uint(t.get("padding")), uint(t.get("spacing")));
        let columns = crate::tiled::fit_count(uint(t.get("pxWid")), padding, grid, spacing).max(1);
        let rows = crate::tiled::fit_count(uint(t.get("pxHei")), padding, grid, spacing);
        let count = columns * rows;
        let mut tiles: HashMap<u32, TileDef> = HashMap::new();
        for c in t.get("customData").arr() {
            let id = uint(c.get("tileId"));
            tiles.entry(id).or_insert_with(|| TileDef { id, ..TileDef::default() }).props.push(("data".into(), text(c.get("data"))));
        }
        for e in t.get("enumTags").arr() {
            for id in e.get("tileIds").arr().iter().map(uint) {
                let tile = tiles.entry(id).or_insert_with(|| TileDef { id, ..TileDef::default() });
                tile.kind = text(e.get("enumValueId"));
                tile.props.push(("enum".into(), text(e.get("enumValueId"))));
            }
        }
        let mut tiles: Vec<TileDef> = tiles.into_values().collect();
        tiles.sort_by_key(|d| d.id);
        defs.push(TilesetDef { first_gid: first, name: text(t.get("identifier")), tile_w: grid, tile_h: grid, count, columns, margin: padding, spacing, image: beside(file, rel), source: file.to_path_buf(), tiles, ..TilesetDef::default() });
        sets.insert(uint(t.get("uid")), LdtkSet { first, columns, grid, padding, spacing });
        first += count;
    }
    (defs, sets)
}

/// A Point field in pixels: the middle of its cell.
fn ldtk_point(p: &Json, grid: f32, off: (f32, f32)) -> (f32, f32) {
    ((real(p.get("cx")) + 0.5) * grid + off.0, (real(p.get("cy")) + 0.5) * grid + off.1)
}

/// Fields as custom properties, and the first list of points.
type LdtkFields = (Vec<(String, String)>, Vec<(f32, f32)>);

fn ldtk_fields(item: &Json, grid: f32, off: (f32, f32)) -> LdtkFields {
    let (mut props, mut path) = (Vec::new(), Vec::new());
    for f in item.get("fieldInstances").arr() {
        let value = f.get("__value");
        let shown = match f.get("__type").str().unwrap_or("") {
            "Point" if !value.is_null() => {
                let p = ldtk_point(value, grid, off);
                format!("{},{}", p.0, p.1)
            }
            "Array<Point>" => {
                let list: Vec<(f32, f32)> = value.arr().iter().map(|p| ldtk_point(p, grid, off)).collect();
                if path.is_empty() {
                    path = list.clone();
                }
                list.iter().map(|p| format!("{},{}", p.0, p.1)).collect::<Vec<_>>().join(" ")
            }
            _ => crate::level::prop_text(value),
        };
        props.push((text(f.get("__identifier")), shown));
    }
    (props, path)
}

fn ldtk_entity(e: &Json, layer: &str, grid: f32, off: (f32, f32), sets: &HashMap<u32, LdtkSet>) -> ObjectDef {
    let (w, h) = (real(e.get("width")), real(e.get("height")));
    let (px, pivot) = (e.get("px").arr(), e.get("__pivot").arr());
    let pos = |i: usize| px.get(i).map_or(0.0, real) - pivot.get(i).map_or(0.0, real) * [w, h][i];
    let (mut props, points) = ldtk_fields(e, grid, off);
    props.push(("iid".into(), text(e.get("iid"))));
    let tags: Vec<String> = e.get("__tags").arr().iter().map(text).collect();
    props.push(("tags".into(), tags.join(",")));
    let rect = e.get("__tile");
    let gid = sets.get(&uint(rect.get("tilesetUid"))).filter(|_| !rect.is_null()).map_or(0, |s| s.gid_at(uint(rect.get("x")), uint(rect.get("y"))));
    let kind = text(e.get("__identifier"));
    // an entity's name is a field called "name" if it has one
    let name = crate::level::prop(&props, "name").map_or_else(|| kind.clone(), str::to_string);
    ObjectDef { name, kind, layer: layer.to_string(), x: pos(0) + off.0, y: pos(1) + off.1, w, h, shape: if gid != 0 { SHAPE_TILE } else { SHAPE_RECT }, gid, points, props, ..ObjectDef::default() }
}

/// Tiles to grids. A cell can hold several tiles, stacked; each level of a
/// stack gets a grid of its own, so that it draws in the same order.
fn ldtk_stacks(tiles: &[Json], width: i32, height: i32, cell: u32, set: &LdtkSet) -> Vec<Vec<u32>> {
    let size = width as usize * height as usize;
    let mut grids: Vec<Vec<u32>> = Vec::new();
    let mut depth = vec![0usize; size];
    for t in tiles {
        let (px, f) = (t.get("px").arr(), uint(t.get("f")));
        let (cx, cy) = (px.first().map_or(0, int) / cell.max(1) as i32, px.get(1).map_or(0, int) / cell.max(1) as i32);
        if cx < 0 || cy < 0 || cx >= width || cy >= height {
            continue;
        }
        let at = (cy * width + cx) as usize;
        if grids.len() <= depth[at] {
            grids.push(vec![0; size]);
        }
        let flags = if f & 1 != 0 { FLIP_H } else { 0 } | if f & 2 != 0 { FLIP_V } else { 0 };
        let id = t.get("t").num().map_or(0, |n| n as u32);
        grids[depth[at]][at] = (set.first + id) | flags;
        depth[at] += 1;
    }
    grids
}

fn parse_ldtk(file: &Path, level: &str, doc: &Json, files: ReadFile) -> Result<MapDef, String> {
    let mut all: Vec<&Json> = doc.get("levels").arr().iter().collect();
    for world in doc.get("worlds").arr() {
        all.extend(world.get("levels").arr());
    }
    let levels: Vec<LevelDef> = all.iter().map(|l| LevelDef { name: text(l.get("identifier")), x: real(l.get("worldX")), y: real(l.get("worldY")) }).collect();
    let at = pick_level(&levels, level)?;
    // with "Save levels separately" a level is in a file of its own
    let loaded;
    let lv = match all[at].get("externalRelPath").str().filter(|_| all[at].get("layerInstances").arr().is_empty()) {
        Some(rel) => {
            loaded = json::parse(&files.text(&beside(file, rel))?)?;
            &loaded
        }
        None => all[at],
    };
    let (tilesets, sets) = ldtk_tilesets(doc, file);
    let grid = uint(doc.get("defaultGridSize")).max(1);
    let (level_props, _) = ldtk_fields(lv, grid as f32, (0.0, 0.0));
    let (wide, high) = (uint(lv.get("pxWid")), uint(lv.get("pxHei")));
    let mut map = MapDef {
        width: wide.div_ceil(grid) as i32,
        height: high.div_ceil(grid) as i32,
        tile_w: grid,
        tile_h: grid,
        tilesets,
        props: level_props,
        background: lv.get("__bgColor").str().and_then(color),
        level: levels[at].name.clone(),
        levels,
        ..MapDef::default()
    };
    // the first layer in the file is the one on top
    for l in lv.get("layerInstances").arr().iter().rev() {
        let name = text(l.get("__identifier"));
        let (width, height, cell) = (int(l.get("__cWid")), int(l.get("__cHei")), uint(l.get("__gridSize")).max(1));
        let off = (real(l.get("__pxTotalOffsetX")), real(l.get("__pxTotalOffsetY")));
        let (visible, opacity) = (!matches!(l.get("visible"), Json::Bool(false)), l.get("__opacity").num().unwrap_or(1.0) as f32);
        let base = LayerDef { offset: off, visible, opacity, cell: Some((cell, cell)), group: name.clone(), ..LayerDef::new(&name, width, height, Vec::new()) };
        let values = l.get("intGridCsv").arr();
        let has_values = values.iter().any(|v| uint(v) != 0);
        if has_values {
            map.layers.push(LayerDef { gids: values.iter().map(uint).collect(), data_only: true, ..base.clone() });
        }
        for e in l.get("entityInstances").arr() {
            map.objects.push(ldtk_entity(e, &name, cell as f32, off, &sets));
        }
        let Some(set) = sets.get(&uint(l.get("__tilesetDefUid"))).filter(|_| width > 0 && height > 0) else { continue };
        for key in ["autoLayerTiles", "gridTiles"] {
            for (i, gids) in ldtk_stacks(l.get(key).arr(), width, height, cell, set).into_iter().enumerate() {
                let stem = if has_values { format!("{name} tiles") } else { name.clone() };
                let shown = if i == 0 { stem } else { format!("{stem} {}", i + 1) };
                // LDtk puts a tile's top-left at the top-left of its cell
                map.layers.push(LayerDef { name: shown, gids, shift: (0.0, set.grid as f32 - cell as f32), ..base.clone() });
            }
        }
    }
    Ok(map)
}

// ---- Ogmo Editor 3 -----------------------------------------------------------

/// Tilesets get numbers this far apart, because how many tiles each has is
/// only known once its picture is loaded.
const OGMO_GAP: u32 = 1 << 20;

fn ogmo_project(level: &Path, files: ReadFile) -> Result<(Json, PathBuf), String> {
    let mut dir = level.parent().unwrap_or(Path::new("")).to_path_buf();
    for _ in 0..16 {
        if let Some(name) = files.list(&dir).iter().find(|f| f.to_ascii_lowercase().ends_with(".ogmo")) {
            let path = dir.join(name);
            return Ok((json::parse(&files.text(&path)?)?, path));
        }
        if !dir.pop() {
            break;
        }
    }
    Err("no .ogmo project found in this folder or the ones above it".into())
}

fn flat<'a>(l: &'a Json, one: &str, two: &str) -> Vec<&'a Json> {
    if l.get(one).is_null() {
        l.get(two).arr().iter().flat_map(Json::arr).collect()
    } else {
        l.get(one).arr().iter().collect()
    }
}

fn image_width(path: &Path, files: ReadFile) -> Result<u32, String> {
    let bytes = files.bytes(path)?;
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    Ok(reader.into_dimensions().map_err(|e| format!("cannot load {}: {e}", path.display()))?.0)
}

fn ogmo_tiles(l: &Json, set: &TilesetDef, files: ReadFile) -> Result<Vec<u32>, String> {
    let mut gids: Vec<u32> = if !l.get("dataCoords").is_null() || !l.get("dataCoords2D").is_null() {
        let columns = crate::tiled::fit_count(image_width(&set.image, files)?, set.margin, set.tile_w, set.spacing).max(1);
        let tile = |c: &Json| {
            let p = c.arr();
            if p.len() < 2 || int(&p[0]) < 0 {
                0
            } else {
                set.first_gid + uint(&p[0]) + uint(&p[1]) * columns
            }
        };
        flat(l, "dataCoords", "dataCoords2D").into_iter().map(tile).collect()
    } else {
        flat(l, "data", "data2D").into_iter().map(|v| if int(v) < 0 || v.is_null() { 0 } else { set.first_gid + uint(v) }).collect()
    };
    for (g, f) in gids.iter_mut().zip(flat(l, "tileFlags", "tileFlags2D")) {
        let f = uint(f);
        if *g != 0 {
            *g |= if f & 4 != 0 { FLIP_H } else { 0 } | if f & 2 != 0 { FLIP_V } else { 0 } | if f & 1 != 0 { FLIP_D } else { 0 };
        }
    }
    Ok(gids)
}

fn ogmo_values(item: &Json) -> Vec<(String, String)> {
    match item.get("values") {
        Json::Obj(v) => v.iter().map(|(k, v)| (k.clone(), crate::level::prop_text(v))).collect(),
        _ => Vec::new(),
    }
}

fn ogmo_entity(e: &Json, layer: &str, off: (f32, f32), project: &Json) -> ObjectDef {
    let kind = text(e.get("name"));
    let template = project.get("entities").arr().iter().find(|t| t.get("name").str() == Some(kind.as_str()));
    let default = |group: &str, axis: &str| template.map_or(0.0, |t| real(t.get(group).get(axis)));
    let value = |k: &str, group: &str, axis: &str| e.get(k).num().map_or_else(|| default(group, axis), |n| n as f32);
    let (w, h) = (value("width", "size", "x"), value("height", "size", "y"));
    let (ox, oy) = (value("originX", "origin", "x"), value("originY", "origin", "y"));
    let radians = truthy(project.get("anglesRadians"));
    let mut props = ogmo_values(e);
    props.push(("id".into(), crate::level::prop_text(e.get("id"))));
    for k in ["flippedX", "flippedY", "color"] {
        if !e.get(k).is_null() {
            props.push((k.into(), crate::level::prop_text(e.get(k))));
        }
    }
    let name = crate::level::prop(&props, "name").map_or_else(|| kind.clone(), str::to_string);
    let rotation = real(e.get("rotation"));
    ObjectDef {
        name,
        kind,
        layer: layer.to_string(),
        x: real(e.get("x")) - ox + off.0,
        y: real(e.get("y")) - oy + off.1,
        w,
        h,
        rotation: if radians { rotation.to_degrees() } else { rotation },
        shape: if w == 0.0 && h == 0.0 { SHAPE_POINT } else { SHAPE_RECT },
        points: e.get("nodes").arr().iter().map(|n| (real(n.get("x")) + off.0, real(n.get("y")) + off.1)).collect(),
        props,
        ..ObjectDef::default()
    }
}

fn ogmo_decal(d: &Json, layer: &str, off: (f32, f32), project: &Json) -> ObjectDef {
    let texture = text(d.get("texture"));
    let name = Path::new(&texture).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let mut props = ogmo_values(d);
    props.push(("texture".into(), texture));
    for (k, fallback) in [("originX", 0.5), ("originY", 0.5), ("scaleX", 1.0), ("scaleY", 1.0)] {
        props.push((k.into(), d.get(k).num().unwrap_or(fallback).to_string()));
    }
    let rotation = real(d.get("rotation"));
    let rotation = if truthy(project.get("anglesRadians")) { rotation.to_degrees() } else { rotation };
    ObjectDef { name, kind: "decal".into(), layer: layer.to_string(), x: real(d.get("x")) + off.0, y: real(d.get("y")) + off.1, rotation, shape: SHAPE_POINT, props, ..ObjectDef::default() }
}

fn parse_ogmo(file: &Path, doc: &Json, files: ReadFile) -> Result<MapDef, String> {
    let (project, project_file) = ogmo_project(file, files)?;
    let tilesets: Vec<TilesetDef> = project
        .get("tilesets")
        .arr()
        .iter()
        .enumerate()
        .map(|(i, t)| TilesetDef {
            first_gid: 1 + i as u32 * OGMO_GAP,
            name: text(t.get("label")),
            tile_w: uint(t.get("tileWidth")),
            tile_h: uint(t.get("tileHeight")),
            margin: uint(t.get("tileMarginX")),
            spacing: uint(t.get("tileSeparationX")),
            image: beside(&project_file, t.get("path").str().unwrap_or("")),
            source: project_file.clone(),
            ..TilesetDef::default()
        })
        .collect();
    let list = doc.get("layers").arr();
    let first_grid = list.iter().find(|l| !l.get("gridCellWidth").is_null());
    let (tw, th) = first_grid.map_or((16, 16), |l| (uint(l.get("gridCellWidth")).max(1), uint(l.get("gridCellHeight")).max(1)));
    let mut map = MapDef {
        width: uint(doc.get("width")).div_ceil(tw) as i32,
        height: uint(doc.get("height")).div_ceil(th) as i32,
        tile_w: tw,
        tile_h: th,
        props: ogmo_values(doc),
        background: project.get("backgroundColor").str().and_then(color_rgba),
        ..MapDef::default()
    };
    // Ogmo draws from the last layer to the first, so the first is on top
    for l in list.iter().rev() {
        let name = text(l.get("name"));
        let off = (real(l.get("offsetX")), real(l.get("offsetY")));
        let (width, height) = (int(l.get("gridCellsX")), int(l.get("gridCellsY")));
        let base = LayerDef { offset: off, cell: Some((uint(l.get("gridCellWidth")).max(1), uint(l.get("gridCellHeight")).max(1))), ..LayerDef::new(&name, width, height, Vec::new()) };
        for e in l.get("entities").arr() {
            map.objects.push(ogmo_entity(e, &name, off, &project));
        }
        for d in l.get("decals").arr() {
            map.objects.push(ogmo_decal(d, &name, off, &project));
        }
        if !l.get("grid").is_null() || !l.get("grid2D").is_null() {
            let legend = project.get("layers").arr().iter().find(|p| p.get("name").str() == Some(name.as_str())).map(|p| p.get("legend"));
            let empty = match legend {
                Some(Json::Obj(keys)) if !keys.contains_key("0") => keys.keys().next().cloned().unwrap_or_default(),
                _ => "0".to_string(),
            };
            let gids: Vec<u32> = flat(l, "grid", "grid2D").into_iter().map(|v| grid_value(v, &empty)).collect();
            if gids.len() != width as usize * height as usize {
                return Err(format!("layer '{name}' has {} cells, expected {width}x{height}", gids.len()));
            }
            map.layers.push(LayerDef { gids, data_only: true, ..base });
            continue;
        }
        let label = l.get("tileset").str().filter(|s| !s.is_empty());
        let Some(label) = label else { continue };
        let set = map.tilesets_lookup(&tilesets, label).ok_or_else(|| format!("the project has no tileset called '{label}'"))?;
        let gids = ogmo_tiles(l, set, files)?;
        if gids.len() != width as usize * height as usize {
            return Err(format!("layer '{name}' has {} tiles, expected {width}x{height}", gids.len()));
        }
        map.layers.push(LayerDef { gids, ..base });
    }
    map.tilesets = tilesets;
    Ok(map)
}

/// Ogmo's colours are `#rrggbbaa`.
fn color_rgba(s: &str) -> Option<[u8; 3]> {
    let h = s.trim_start_matches('#');
    color(h.get(..6)?)
}

/// A grid cell's value for the program: a number, or the character's code.
fn grid_value(v: &Json, empty: &str) -> u32 {
    let s = crate::level::prop_text(v);
    if s == empty || s.is_empty() {
        return 0;
    }
    s.parse().unwrap_or_else(|_| s.chars().next().map_or(0, |c| c as u32))
}

impl MapDef {
    fn tilesets_lookup<'a>(&self, sets: &'a [TilesetDef], label: &str) -> Option<&'a TilesetDef> {
        sets.iter().find(|t| t.name == label)
    }
}

// ---- Sprite Fusion -----------------------------------------------------------

fn parse_sprite_fusion(file: &Path, doc: &Json) -> MapDef {
    let size = uint(doc.get("tileSize")).max(1);
    let (width, height) = (int(doc.get("mapWidth")), int(doc.get("mapHeight")));
    let set = TilesetDef { first_gid: 1, name: "spritesheet".into(), tile_w: size, tile_h: size, image: beside(file, "spritesheet.png"), source: file.to_path_buf(), ..TilesetDef::default() };
    let mut map = MapDef { width, height, tile_w: size, tile_h: size, tilesets: vec![set], ..MapDef::default() };
    // layers are listed from the top down
    for l in doc.get("layers").arr().iter().rev() {
        let mut gids = vec![0; width.max(0) as usize * height.max(0) as usize];
        for t in l.get("tiles").arr() {
            let (x, y) = (int(t.get("x")), int(t.get("y")));
            // the id is written as text
            let id = t.get("id").str().and_then(|s| s.parse().ok()).or_else(|| t.get("id").num().map(|n| n as u32));
            if let (Some(id), true) = (id, x >= 0 && y >= 0 && x < width && y < height) {
                gids[(y * width + x) as usize] = 1 + id;
            }
        }
        let collider = truthy(l.get("collider"));
        map.layers.push(LayerDef { props: vec![("collider".into(), (collider as i32).to_string())], ..LayerDef::new(l.get("name").str().unwrap_or(""), width, height, gids) });
    }
    map
}

// ---- Pyxel Edit --------------------------------------------------------------

/// Pyxel Edit's XML export, as its JSON export would say it.
fn pyxel_xml(doc: &Json) -> Json {
    let mut out = doc.clone();
    let layers: Vec<Json> = doc
        .get("children")
        .arr()
        .iter()
        .filter(|c| c.get("_name").str() == Some("layer"))
        .map(|l| {
            let mut one = l.clone();
            if let Json::Obj(m) = &mut one {
                m.insert("tiles".into(), l.get("children").clone());
            }
            one
        })
        .collect();
    if let Json::Obj(m) = &mut out {
        m.insert("layers".into(), Json::Arr(layers));
    }
    out
}

/// Flags for a tile mirrored, then turned clockwise in quarter turns.
fn pyxel_flags(flip_x: bool, rot: u32) -> u32 {
    match (flip_x, rot % 4) {
        (false, 0) => 0,
        (false, 1) => FLIP_D | FLIP_H,
        (false, 2) => FLIP_H | FLIP_V,
        (false, _) => FLIP_D | FLIP_V,
        (true, 0) => FLIP_H,
        (true, 1) => FLIP_D | FLIP_H | FLIP_V,
        (true, 2) => FLIP_V,
        (true, _) => FLIP_D,
    }
}

fn parse_pyxel(file: &Path, doc: &Json, files: ReadFile) -> Result<MapDef, String> {
    let (width, height) = (int(doc.get("tileswide")), int(doc.get("tileshigh")));
    let (tw, th) = (uint(doc.get("tilewidth")), uint(doc.get("tileheight")));
    let stem = file.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let names = [format!("{stem}.png"), format!("{stem}_tileset.png"), format!("{stem}-tileset.png"), "tileset.png".to_string()];
    let image = find_beside(file, &names, files).ok_or_else(|| format!("export the tileset from Pyxel Edit as {stem}.png, next to the map"))?;
    let set = TilesetDef { first_gid: 1, name: stem, tile_w: tw, tile_h: th, image, source: file.to_path_buf(), ..TilesetDef::default() };
    let mut map = MapDef { width, height, tile_w: tw, tile_h: th, tilesets: vec![set], ..MapDef::default() };
    let mut layers: Vec<&Json> = doc.get("layers").arr().iter().collect();
    // layer 0 is the one on top
    layers.sort_by_key(|l| std::cmp::Reverse(int(l.get("number"))));
    for l in layers {
        let mut gids = vec![0; width.max(0) as usize * height.max(0) as usize];
        for t in l.get("tiles").arr() {
            let index = if t.get("index").is_null() { int(t.get("y")) * width + int(t.get("x")) } else { int(t.get("index")) };
            let tile = t.get("tile").num().map_or(-1, |n| n as i32);
            if tile >= 0 && index >= 0 && (index as usize) < gids.len() {
                gids[index as usize] = (1 + tile as u32) | pyxel_flags(truthy(t.get("flipX")), uint(t.get("rot")));
            }
        }
        map.layers.push(LayerDef::new(l.get("name").str().unwrap_or(""), width, height, gids));
    }
    Ok(map)
}

// ---- Aseprite ----------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.at.checked_add(n).and_then(|end| self.data.get(self.at..end)).ok_or("the Aseprite file ends early")?;
        self.at += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    fn rest(&self) -> &'a [u8] {
        &self.data[self.at.min(self.data.len())..]
    }
}

struct AseLayer {
    name: String,
    visible: bool,
    opacity: f32,
    /// Only tilemap layers have one.
    tileset: Option<u32>,
}

struct AseCel {
    layer: usize,
    x: f32,
    y: f32,
    opacity: f32,
    width: i32,
    height: i32,
    masks: [u32; 4],
    tiles: Vec<u8>,
}

struct AseTileset {
    id: u32,
    name: String,
    count: u32,
    w: u32,
    h: u32,
    pixels: Vec<u8>,
}

fn ase_pixels(raw: &[u8], depth: u16, w: u32, h: u32, palette: &[[u8; 4]], transparent: u8) -> Result<RgbaImage, String> {
    let n = (depth / 8).max(1) as usize;
    if raw.len() != w as usize * h as usize * n {
        return Err("a tileset's picture has the wrong size".into());
    }
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for p in raw.chunks_exact(n) {
        let rgba = match depth {
            32 => [p[0], p[1], p[2], p[3]],
            16 => [p[0], p[0], p[0], p[1]],
            _ if p[0] == transparent => [0; 4],
            _ => palette.get(p[0] as usize).copied().unwrap_or([0; 4]),
        };
        out.extend_from_slice(&rgba);
    }
    RgbaImage::from_raw(w, h, out).ok_or_else(|| "bad tileset picture".to_string())
}

fn parse_aseprite(data: &[u8]) -> Result<MapDef, String> {
    let mut r = Reader { data, at: 0 };
    r.u32()?;
    if r.u16()? != 0xA5E0 {
        return Err("not an Aseprite file".into());
    }
    let frames = r.u16()?;
    let (sprite_w, sprite_h, depth) = (r.u16()? as u32, r.u16()? as u32, r.u16()?);
    let flags = r.u32()?;
    r.take(10)?;
    let transparent = r.u8()?;
    r.take(115 - 24 + 8)?; // to the end of the 128-byte header
    r.at = 128;
    if frames == 0 {
        return Err("the Aseprite file has no frames".into());
    }
    let layers_have_opacity = flags & 1 != 0;
    // tilemaps are read from the first frame
    let start = r.at;
    let frame_bytes = r.u32()? as usize;
    if r.u16()? != 0xF1FA {
        return Err("bad Aseprite frame".into());
    }
    let old_chunks = r.u16()? as u32;
    r.take(4)?;
    let new_chunks = r.u32()?;
    let chunks = if new_chunks == 0 { old_chunks } else { new_chunks };
    let frame_end = start + frame_bytes;
    let (mut layers, mut cels, mut sets) = (Vec::new(), Vec::new(), Vec::new());
    let mut palette: Vec<[u8; 4]> = Vec::new();
    for _ in 0..chunks {
        if r.at >= frame_end {
            break;
        }
        let at = r.at;
        let size = r.u32()? as usize;
        let kind = r.u16()?;
        let mut c = Reader { data: r.data.get(..at + size).ok_or("the Aseprite file ends early")?, at: r.at };
        match kind {
            0x2004 => {
                let lf = c.u16()?;
                let layer_type = c.u16()?;
                c.take(8)?;
                let opacity = c.u8()?;
                c.take(3)?;
                let name = c.string()?;
                let tileset = if layer_type == 2 { Some(c.u32()?) } else { None };
                layers.push(AseLayer { name, visible: lf & 1 != 0, opacity: if layers_have_opacity { opacity as f32 / 255.0 } else { 1.0 }, tileset });
            }
            0x2005 => {
                let layer = c.u16()? as usize;
                let (x, y) = (c.i16()? as f32, c.i16()? as f32);
                let opacity = c.u8()? as f32 / 255.0;
                let cel_type = c.u16()?;
                c.take(7)?;
                if cel_type == 3 {
                    let (width, height, bits) = (c.u16()? as i32, c.u16()? as i32, c.u16()?);
                    if bits != 32 {
                        return Err(format!("{bits}-bit tiles are not supported"));
                    }
                    let masks = [c.u32()?, c.u32()?, c.u32()?, c.u32()?];
                    c.take(10)?;
                    cels.push(AseCel { layer, x, y, opacity, width, height, masks, tiles: inflate::zlib(c.rest())? });
                }
            }
            0x2019 => {
                let (new_size, first, last) = (c.u32()? as usize, c.u32()? as usize, c.u32()? as usize);
                c.take(8)?;
                palette.resize(new_size.max(palette.len()), [0; 4]);
                for i in first..=last {
                    let entry_flags = c.u16()?;
                    let rgba = [c.u8()?, c.u8()?, c.u8()?, c.u8()?];
                    if entry_flags & 1 != 0 {
                        c.string()?;
                    }
                    if let Some(p) = palette.get_mut(i) {
                        *p = rgba;
                    }
                }
            }
            0x2023 => {
                let (id, set_flags, count) = (c.u32()?, c.u32()?, c.u32()?);
                let (w, h) = (c.u16()? as u32, c.u16()? as u32);
                c.take(16)?;
                let name = c.string()?;
                if set_flags & 1 != 0 {
                    c.take(8)?;
                }
                if set_flags & 2 == 0 {
                    return Err(format!("tileset '{name}' is in another file"));
                }
                let n = c.u32()? as usize;
                sets.push(AseTileset { id, name, count, w, h, pixels: inflate::zlib(c.take(n)?)? });
            }
            _ => {}
        }
        r.at = at + size;
    }
    let mut map = MapDef::default();
    let mut firsts: HashMap<u32, (u32, u32, u32)> = HashMap::new();
    let mut first = 1;
    for s in sets {
        let pixels = ase_pixels(&s.pixels, depth, s.w, s.h * s.count, &palette, transparent)?;
        firsts.insert(s.id, (first, s.w, s.h));
        map.tilesets.push(TilesetDef { first_gid: first, name: s.name, tile_w: s.w, tile_h: s.h, count: s.count, columns: 1, pixels: Some(pixels), ..TilesetDef::default() });
        first += s.count;
    }
    for c in cels {
        let Some(layer) = layers.get(c.layer) else { continue };
        let Some(&(base, w, h)) = layer.tileset.and_then(|t| firsts.get(&t)) else { continue };
        if map.tile_w == 0 {
            (map.tile_w, map.tile_h) = (w, h);
        }
        if c.tiles.len() != c.width as usize * c.height as usize * 4 {
            return Err(format!("layer '{}' has the wrong number of tiles", layer.name));
        }
        let [id_mask, x_flip, y_flip, d_flip] = c.masks;
        let gids = c
            .tiles
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| {
                let v = u32::from_le_bytes(*b);
                let id = v & id_mask;
                if id == 0 {
                    return 0;
                }
                let flag = |mask: u32, bit: u32| if mask != 0 && v & mask != 0 { bit } else { 0 };
                (base + id) | flag(x_flip, FLIP_H) | flag(y_flip, FLIP_V) | flag(d_flip, FLIP_D)
            })
            .collect();
        map.layers.push(LayerDef { visible: layer.visible, opacity: layer.opacity * c.opacity, offset: (c.x, c.y), cell: Some((w, h)), ..LayerDef::new(&layer.name, c.width, c.height, gids) });
    }
    if map.layers.is_empty() {
        return Err("the first frame has no tilemap layers".into());
    }
    map.width = sprite_w.div_ceil(map.tile_w) as i32;
    map.height = sprite_h.div_ceil(map.tile_h) as i32;
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::prop;
    use crate::tiled::MapFiles;

    struct Fake(Vec<(&'static str, String)>, Vec<u8>);

    impl MapFiles for Fake {
        fn text(&self, p: &Path) -> Result<String, String> {
            let p = p.to_string_lossy().replace('\\', "/");
            self.0.iter().find(|(n, _)| *n == p).map(|(_, b)| b.clone()).ok_or(format!("no file {p}"))
        }

        fn bytes(&self, _: &Path) -> Result<Vec<u8>, String> {
            Ok(self.1.clone())
        }

        fn list(&self, dir: &Path) -> Vec<String> {
            let dir = dir.to_string_lossy().replace('\\', "/");
            self.0.iter().filter_map(|(n, _)| Path::new(n).parent().filter(|p| p.to_string_lossy().replace('\\', "/") == dir).map(|_| n.rsplit('/').next().unwrap().to_string())).collect()
        }
    }

    fn fake(files: &[(&'static str, &str)]) -> Fake {
        Fake(files.iter().map(|(n, b)| (*n, b.to_string())).collect(), Vec::new())
    }

    const LDTK: &str = r##"{"defaultGridSize":8,"jsonVersion":"1.5.3","externalLevels":false,
      "defs":{"tilesets":[{"uid":1,"identifier":"Ground","relPath":"art/g.png","pxWid":32,"pxHei":16,"tileGridSize":8,"spacing":0,"padding":0,"customData":[{"tileId":3,"data":"hi"}],"enumTags":[{"enumValueId":"Lava","tileIds":[2]}]},
        {"uid":2,"identifier":"Icons","relPath":null,"pxWid":8,"pxHei":8,"tileGridSize":8}]},
      "levels":[{"identifier":"One","worldX":100,"worldY":-50,"pxWid":16,"pxHei":16,"__bgColor":"#102030",
        "fieldInstances":[{"__identifier":"title","__type":"String","__value":"Start"}],
        "layerInstances":[
         {"__identifier":"Things","__type":"Entities","__cWid":2,"__cHei":2,"__gridSize":8,"__opacity":1,"__pxTotalOffsetX":0,"__pxTotalOffsetY":0,"visible":true,"intGridCsv":[],"autoLayerTiles":[],"gridTiles":[],
          "entityInstances":[{"__identifier":"Coin","__pivot":[0.5,1],"__tags":["pickup"],"iid":"abc","width":8,"height":8,"px":[8,16],"__tile":{"tilesetUid":1,"x":8,"y":8,"w":8,"h":8},
           "fieldInstances":[{"__identifier":"name","__type":"String","__value":"Gold"},{"__identifier":"value","__type":"Int","__value":5},{"__identifier":"path","__type":"Array<Point>","__value":[{"cx":0,"cy":1},{"cx":1,"cy":1}]},{"__identifier":"to","__type":"Point","__value":{"cx":1,"cy":0}}]}]},
         {"__identifier":"Art","__type":"Tiles","__cWid":2,"__cHei":1,"__gridSize":8,"__opacity":0.5,"__pxTotalOffsetX":3,"__pxTotalOffsetY":4,"__tilesetDefUid":1,"visible":false,"intGridCsv":[],"autoLayerTiles":[],
          "gridTiles":[{"px":[0,0],"src":[8,0],"f":1,"t":1},{"px":[8,0],"src":[16,0],"f":2,"t":2},{"px":[0,0],"src":[0,8],"f":0,"t":4}],"entityInstances":[]},
         {"__identifier":"Solid","__type":"IntGrid","__cWid":2,"__cHei":1,"__gridSize":8,"__opacity":1,"__pxTotalOffsetX":0,"__pxTotalOffsetY":0,"__tilesetDefUid":1,"visible":true,"intGridCsv":[0,3],"autoLayerTiles":[{"px":[8,0],"src":[0,0],"f":0,"t":0}],"gridTiles":[],"entityInstances":[]}]},
       {"identifier":"Two","worldX":0,"worldY":0,"externalRelPath":"w/two.ldtkl","layerInstances":null}]}"##;

    const LDTK_TWO: &str = r#"{"identifier":"Two","pxWid":8,"pxHei":8,"fieldInstances":[],"layerInstances":[]}"#;

    fn ldtk(level: &str) -> MapDef {
        parse_any("m/a.ldtk", level, &fake(&[("m/a.ldtk", LDTK), ("m/w/two.ldtkl", LDTK_TWO)])).unwrap()
    }

    #[test]
    fn ldtk_levels() {
        let m = ldtk("");
        assert_eq!((m.level.as_str(), m.width, m.height, m.tile_w), ("One", 2, 2, 8));
        assert_eq!(m.levels, [LevelDef { name: "One".into(), x: 100.0, y: -50.0 }, LevelDef { name: "Two".into(), x: 0.0, y: 0.0 }]);
        assert_eq!(m.background, Some([16, 32, 48]));
        assert_eq!(prop(&m.props, "title"), Some("Start"));
        assert_eq!(ldtk("two").level, "Two", "saved in a file of its own");
        assert!(parse_any("m/a.ldtk", "Nope", &fake(&[("m/a.ldtk", LDTK)])).unwrap_err().contains("Nope"));
    }

    #[test]
    fn ldtk_layers_go_bottom_first_and_split_stacks() {
        let m = ldtk("");
        let names: Vec<&str> = m.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Solid", "Solid tiles", "Art", "Art 2"]);
        assert!(m.layers[0].data_only);
        assert_eq!(m.layers[0].gids, [0, 3]);
        assert_eq!(m.layers[1].gids, [0, 1]);
        let (art, art2) = (&m.layers[2], &m.layers[3]);
        assert_eq!(art.gids, [2 | FLIP_H, 3 | FLIP_V]);
        assert_eq!(art2.gids, [5, 0], "drawn over the first");
        assert_eq!((art.offset, art.opacity, art.visible, art2.group.as_str()), ((3.0, 4.0), 0.5, false, "Art"));
    }

    #[test]
    fn ldtk_tilesets_keep_their_extras() {
        let m = ldtk("");
        assert_eq!(m.tilesets.len(), 1, "built-in icons have no file");
        let s = &m.tilesets[0];
        assert_eq!((s.count, s.columns, s.image.clone()), (8, 4, PathBuf::from("m/art/g.png")));
        assert_eq!(s.tiles[0].kind, "Lava");
        assert_eq!(prop(&s.tiles[1].props, "data"), Some("hi"));
    }

    #[test]
    fn ldtk_entities() {
        let o = &ldtk("").objects[0];
        assert_eq!((o.name.as_str(), o.kind.as_str(), o.layer.as_str()), ("Gold", "Coin", "Things"));
        assert_eq!((o.x, o.y, o.w, o.shape), (4.0, 8.0, 8.0, SHAPE_TILE));
        assert_eq!(o.gid, 6);
        assert_eq!(o.points, [(4.0, 12.0), (12.0, 12.0)]);
        assert_eq!(prop(&o.props, "to"), Some("12,4"));
        assert_eq!(prop(&o.props, "tags"), Some("pickup"));
        assert_eq!(prop(&o.props, "value"), Some("5"));
    }

    const OGMO_PROJECT: &str = r##"{"name":"p","ogmoVersion":"3.4.0","anglesRadians":true,"backgroundColor":"#102030ff",
      "layers":[{"definition":"grid","name":"Solid","legend":{"0":"#00000000","1":"#000000ff"}}],
      "entities":[{"name":"Door","size":{"x":16,"y":32},"origin":{"x":8,"y":32}}],
      "tilesets":[{"label":"ground","path":"art/g.png","tileWidth":16,"tileHeight":16,"tileMarginX":1,"tileSeparationX":2},{"label":"two","path":"art/t.png","tileWidth":16,"tileHeight":16}]}"##;

    const OGMO_LEVEL: &str = r#"{"ogmoVersion":"3.4.0","width":48,"height":16,"offsetX":0,"offsetY":0,"values":{"hard":true},"layers":[
      {"name":"Top","_eid":"1","offsetX":0,"offsetY":0,"gridCellWidth":16,"gridCellHeight":16,"gridCellsX":3,"gridCellsY":1,"entities":[
        {"name":"Door","id":7,"_eid":"9","x":24,"y":32,"rotation":3.14159274,"flippedX":true,"values":{"locked":false,"name":"Exit"},"nodes":[{"x":1,"y":2},{"x":3,"y":4}]},
        {"name":"Mark","id":8,"_eid":"9","x":5,"y":6}]},
      {"name":"Decor","_eid":"2","offsetX":0,"offsetY":0,"folder":"pics","decals":[{"x":10,"y":20,"texture":"pics/tree.png","rotation":0,"scaleX":2}]},
      {"name":"Solid","_eid":"3","offsetX":0,"offsetY":0,"gridCellWidth":16,"gridCellHeight":16,"gridCellsX":3,"gridCellsY":1,"arrayMode":0,"grid":["0","1","a"]},
      {"name":"Art","_eid":"4","offsetX":2,"offsetY":3,"gridCellWidth":16,"gridCellHeight":16,"gridCellsX":3,"gridCellsY":1,"tileset":"two","exportMode":0,"arrayMode":0,"data":[-1,0,5],"tileFlags":[0,4,3]},
      {"name":"Art2","_eid":"5","offsetX":0,"offsetY":0,"gridCellWidth":16,"gridCellHeight":16,"gridCellsX":3,"gridCellsY":1,"tileset":"ground","exportMode":0,"arrayMode":1,"data2D":[[1,-1,2]]}]}"#;

    #[test]
    fn ogmo_level() {
        let files = fake(&[("g/levels/one.json", OGMO_LEVEL), ("g/p.ogmo", OGMO_PROJECT)]);
        let m = parse_any("g/levels/one.json", "", &files).unwrap();
        let names: Vec<&str> = m.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Art2", "Art", "Solid"], "the first layer in the file is on top");
        assert_eq!((m.width, m.height, m.background), (3, 1, Some([16, 32, 48])));
        assert_eq!(m.tilesets[0].image, PathBuf::from("g/art/g.png"));
        assert_eq!((m.tilesets[0].margin, m.tilesets[0].spacing), (1, 2));
        assert_eq!(m.layers[2].gids, [0, 1, 97], "the solid grid is data");
        assert!(m.layers[2].data_only);
        let second = m.tilesets[1].first_gid;
        assert_eq!(m.layers[1].gids, [0, second | FLIP_H, (second + 5) | FLIP_V | FLIP_D]);
        assert_eq!(m.layers[1].offset, (2.0, 3.0));
        assert_eq!(m.layers[0].gids, [2, 0, 3]);
        assert_eq!(prop(&m.props, "hard"), Some("1"));
    }

    #[test]
    fn ogmo_objects() {
        let files = fake(&[("g/levels/one.json", OGMO_LEVEL), ("g/p.ogmo", OGMO_PROJECT)]);
        let m = parse_any("g/levels/one.json", "", &files).unwrap();
        let by = |n: &str| m.objects.iter().find(|o| o.kind == n).unwrap();
        let d = by("Door");
        assert_eq!((d.name.as_str(), d.x, d.y, d.w, d.h, d.shape), ("Exit", 16.0, 0.0, 16.0, 32.0, SHAPE_RECT));
        assert!((d.rotation - 180.0).abs() < 0.01);
        assert_eq!(d.points, [(1.0, 2.0), (3.0, 4.0)]);
        assert_eq!(prop(&d.props, "flippedX"), Some("1"));
        assert_eq!(by("Mark").shape, SHAPE_POINT);
        let t = by("decal");
        assert_eq!((t.name.as_str(), t.x, t.y), ("tree", 10.0, 20.0));
        assert_eq!(prop(&t.props, "texture"), Some("pics/tree.png"));
        assert_eq!(prop(&t.props, "scaleX"), Some("2"));
        assert_eq!(prop(&t.props, "scaleY"), Some("1"));
    }

    #[test]
    fn ogmo_tile_coordinates_use_the_picture_width() {
        let level = r#"{"ogmoVersion":"3","width":16,"height":16,"layers":[{"name":"A","gridCellWidth":16,"gridCellHeight":16,"gridCellsX":2,"gridCellsY":1,"tileset":"t","dataCoords":[[1,2],[-1]]}]}"#;
        let project = r#"{"tilesets":[{"label":"t","path":"t.png","tileWidth":16,"tileHeight":16}]}"#;
        let mut png = Vec::new();
        RgbaImage::new(64, 16).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut files = fake(&[("l.json", level), ("p.ogmo", project)]);
        files.1 = png;
        assert_eq!(parse_any("l.json", "", &files).unwrap().layers[0].gids, [1 + 1 + 2 * 4, 0]);
    }

    #[test]
    fn ogmo_needs_its_project() {
        assert!(parse_any("l.json", "", &fake(&[("l.json", OGMO_LEVEL)])).unwrap_err().contains(".ogmo"));
    }

    #[test]
    fn sprite_fusion_map() {
        let doc = r#"{"tileSize":32,"mapWidth":2,"mapHeight":2,"layers":[{"name":"Top","tiles":[{"id":"7","x":1,"y":0}],"collider":true},{"name":"Base","tiles":[{"id":"0","x":0,"y":1}],"collider":false}]}"#;
        let m = parse_any("sf/map.json", "", &fake(&[("sf/map.json", doc)])).unwrap();
        assert_eq!((m.width, m.tile_w, m.tilesets[0].image.clone()), (2, 32, PathBuf::from("sf/spritesheet.png")));
        assert_eq!((m.layers[0].name.as_str(), m.layers[1].name.as_str()), ("Base", "Top"));
        assert_eq!(m.layers[1].gids, [0, 8, 0, 0]);
        assert_eq!(m.layers[0].gids, [0, 0, 1, 0]);
        assert_eq!(prop(&m.layers[1].props, "collider"), Some("1"));
    }

    const PYXEL_JSON: &str = r#"{"tileswide":2,"tileshigh":1,"tilewidth":8,"tileheight":8,"layers":[
      {"name":"Top","number":0,"tiles":[{"x":0,"y":0,"index":0,"tile":3,"rot":1,"flipX":false},{"x":1,"y":0,"index":1,"tile":-1,"rot":0,"flipX":false}]},
      {"name":"Base","number":1,"tiles":[{"x":0,"y":0,"index":0,"tile":0,"rot":0,"flipX":true},{"x":1,"y":0,"index":1,"tile":1,"rot":2,"flipX":true}]}]}"#;

    #[test]
    fn pyxel_edit_json_and_xml() {
        let xml = r#"<tilemap tileswide="2" tileshigh="1" tilewidth="8" tileheight="8"><layer number="1" name="Base"><tile x="0" y="0" index="0" tile="0" rot="0" flipX="true"/><tile x="1" y="0" index="1" tile="1" rot="2" flipX="true"/></layer>
          <layer number="0" name="Top"><tile x="0" y="0" index="0" tile="3" rot="1" flipX="false"/><tile x="1" y="0" index="1" tile="-1" rot="0" flipX="false"/></layer></tilemap>"#;
        let a = parse_any("p/a.json", "", &fake(&[("p/a.json", PYXEL_JSON), ("p/a.png", "")])).unwrap();
        let b = parse_any("p/a.xml", "", &fake(&[("p/a.xml", xml), ("p/A.PNG", "")])).unwrap();
        assert_eq!(a.tilesets[0].image, PathBuf::from("p/a.png"));
        for m in [&a, &b] {
            assert_eq!(m.layers[0].name, "Base", "layer 0 is on top");
            assert_eq!(m.layers[0].gids, [1 | FLIP_H, 2 | FLIP_V]);
            assert_eq!(m.layers[1].gids, [4 | FLIP_D | FLIP_H, 0]);
        }
    }

    #[test]
    fn pyxel_edit_says_which_picture_it_needs() {
        assert!(parse_any("p/a.json", "", &fake(&[("p/a.json", PYXEL_JSON)])).unwrap_err().contains("a.png"));
    }

    #[test]
    fn tiled_worlds() {
        let world = r#"{"type":"world","maps":[{"fileName":"maps/a.tmx","x":0,"y":0},{"fileName":"maps/b.tmx","x":320,"y":0}]}"#;
        let tmx = |w: u32| format!(r#"<map width="{w}" height="1" tilewidth="2" tileheight="2"/>"#);
        let (a, b) = (tmx(1), tmx(2));
        let files = fake(&[("g/w.world", world), ("g/maps/a.tmx", &a), ("g/maps/b.tmx", &b)]);
        let first = parse_any("g/w.world", "", &files).unwrap();
        assert_eq!((first.width, first.level.as_str(), first.levels.len()), (1, "a", 2));
        let second = parse_any("g/w.world", "B", &files).unwrap();
        assert_eq!((second.width, second.levels[1].x), (2, 320.0));
    }

    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let n = data.len() as u16;
        let mut out = vec![0x78, 0x01, 1];
        out.extend(n.to_le_bytes());
        out.extend((!n).to_le_bytes());
        out.extend(data);
        out.extend([0; 4]);
        out
    }

    fn chunk(kind: u16, body: Vec<u8>) -> Vec<u8> {
        let mut out = (body.len() as u32 + 6).to_le_bytes().to_vec();
        out.extend(kind.to_le_bytes());
        out.extend(body);
        out
    }

    fn name(s: &str) -> Vec<u8> {
        let mut out = (s.len() as u16).to_le_bytes().to_vec();
        out.extend(s.as_bytes());
        out
    }

    fn le(v: &[u32], widths: &[usize]) -> Vec<u8> {
        v.iter().zip(widths).flat_map(|(v, w)| v.to_le_bytes()[..*w].to_vec()).collect()
    }

    fn aseprite(depth: u16) -> Vec<u8> {
        let n = (depth / 8) as usize;
        // a palette, a normal layer, a tilemap layer, a tileset of three 1x2 tiles, a cel
        let mut chunks = chunk(0x2019, {
            let mut p = le(&[3, 0, 2], &[4, 4, 4]);
            p.extend([0; 8]);
            for c in [[0u8, 0, 0, 0], [10, 20, 30, 255], [40, 50, 60, 255]] {
                p.extend([0, 0]);
                p.extend(c);
            }
            p
        });
        chunks.extend(chunk(0x2004, {
            let mut l = le(&[1, 0, 0, 0, 0, 0], &[2; 6]);
            l.extend([255, 0, 0, 0]);
            l.extend(name("Sketch"));
            l
        }));
        chunks.extend(chunk(0x2004, {
            let mut l = le(&[1, 2, 0, 0, 0, 0], &[2; 6]);
            l.extend([128, 0, 0, 0]);
            l.extend(name("Ground"));
            l.extend(4u32.to_le_bytes());
            l
        }));
        chunks.extend(chunk(0x2023, {
            let mut t = le(&[4, 6, 3], &[4, 4, 4]);
            t.extend(le(&[1, 2, 0], &[2, 2, 2]));
            t.extend([0; 14]);
            t.extend(name("Set"));
            let mut px = vec![0u8; n * 6];
            px[n * 2] = 1; // first pixel of tile 1
            if depth == 32 {
                px[n * 2..n * 3].copy_from_slice(&[9, 8, 7, 255]);
            }
            let z = zlib_stored(&px);
            t.extend((z.len() as u32).to_le_bytes());
            t.extend(z);
            t
        }));
        chunks.extend(chunk(0x2005, {
            let mut c = le(&[1, 3, 4], &[2, 2, 2]);
            c.push(255);
            c.extend(le(&[3, 0], &[2, 2]));
            c.extend([0; 5]);
            c.extend(le(&[2, 1, 32], &[2, 2, 2]));
            c.extend(le(&[0x1fff_ffff, 0x8000_0000, 0x4000_0000, 0x2000_0000], &[4; 4]));
            c.extend([0; 10]);
            let tiles = le(&[1 | 0x8000_0000, 0], &[4, 4]);
            c.extend(zlib_stored(&tiles));
            c
        }));
        let mut file = vec![0u8; 128];
        file[4..6].copy_from_slice(&0xA5E0u16.to_le_bytes());
        file[6..8].copy_from_slice(&1u16.to_le_bytes());
        file[8..10].copy_from_slice(&4u16.to_le_bytes());
        file[10..12].copy_from_slice(&4u16.to_le_bytes());
        file[12..14].copy_from_slice(&depth.to_le_bytes());
        file[14..18].copy_from_slice(&1u32.to_le_bytes());
        file.extend(((chunks.len() + 16) as u32).to_le_bytes());
        file.extend(0xF1FAu16.to_le_bytes());
        file.extend([5, 0, 100, 0, 0, 0]);
        file.extend(5u32.to_le_bytes());
        file.extend(chunks);
        file
    }

    #[test]
    fn aseprite_tilemap() {
        let m = parse_aseprite(&aseprite(32)).unwrap();
        assert_eq!((m.width, m.height, m.tile_w, m.tile_h), (4, 2, 1, 2));
        assert_eq!(m.layers.len(), 1, "only tilemap layers");
        let l = &m.layers[0];
        assert_eq!((l.name.as_str(), l.width, l.height, l.offset), ("Ground", 2, 1, (3.0, 4.0)));
        assert_eq!(l.gids, [2 | FLIP_H, 0]);
        assert!((l.opacity - 128.0 / 255.0).abs() < 0.001);
        let s = &m.tilesets[0];
        assert_eq!((s.count, s.columns), (3, 1));
        assert_eq!(s.pixels.as_ref().unwrap().get_pixel(0, 2).0, [9, 8, 7, 255]);
    }

    #[test]
    fn aseprite_palette_colours() {
        let m = parse_aseprite(&aseprite(8)).unwrap();
        let p = m.tilesets[0].pixels.as_ref().unwrap();
        assert_eq!(p.get_pixel(0, 2).0, [10, 20, 30, 255]);
        assert_eq!(p.get_pixel(0, 0).0[3], 0);
    }

    #[test]
    fn not_an_aseprite_file() {
        assert!(parse_aseprite(&[0; 200]).unwrap_err().contains("not an Aseprite"));
        assert!(parse_aseprite(&[1, 2, 3]).is_err());
    }
}
