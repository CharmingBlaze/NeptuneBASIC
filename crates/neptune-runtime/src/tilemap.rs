//! Tile maps with autotiling (ext).
//!
//! A tile map is a grid of frame numbers drawn from an animation image
//! (`LoadAnimImage`), so every tile is one frame. `DrawTileMap` draws only
//! the tiles that are on screen.
//!
//! Autotiling: `TileMapTerrain` says which frames draw a terrain, and
//! `AutoTile` paints cells with it. Each painted cell then picks the frame
//! that matches its neighbours, and so do the cells around it. Two layouts
//! are supported:
//!
//! * 16 frames (`mode` 16): neighbours north = 1, east = 2, south = 4,
//!   west = 8; the frame is `first + sum`.
//! * 47 frames (`mode` 47, "blob" tiles): all eight neighbours; corners
//!   count only when both sides next to them are the same terrain. The frame
//!   is `first +` the position of that value (north = 1, north-east = 2,
//!   east = 4, south-east = 8, south = 16, south-west = 32, west = 64,
//!   north-west = 128) among the 47 possible values in increasing order.
//!
//! Cells outside the map count as the same terrain, so terrain reaching the
//! edge of the map has no border there.

use std::collections::HashMap;

/// Empty cell.
pub const EMPTY: i32 = -1;

#[derive(Debug, Clone)]
pub struct TileMap {
    pub image: i32,
    pub width: i32,
    pub height: i32,
    tiles: Vec<i32>,
    terrain: Vec<i32>,
    /// terrain -> (first frame, mode)
    rules: HashMap<i32, (i32, i32)>,
}

/// The 47 distinct blob values, in increasing order.
fn blob_values() -> &'static [u8] {
    static V: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        let mut v: Vec<u8> = (0..=255u8).map(reduce_blob).collect();
        v.sort_unstable();
        v.dedup();
        v
    })
}

/// Clears corner bits whose two neighbouring sides are not both set.
fn reduce_blob(m: u8) -> u8 {
    const N: u8 = 1;
    const NE: u8 = 2;
    const E: u8 = 4;
    const SE: u8 = 8;
    const S: u8 = 16;
    const SW: u8 = 32;
    const W: u8 = 64;
    const NW: u8 = 128;
    let mut r = m & (N | E | S | W);
    for (corner, a, b) in [(NE, N, E), (SE, S, E), (SW, S, W), (NW, N, W)] {
        if m & corner != 0 && m & a != 0 && m & b != 0 {
            r |= corner;
        }
    }
    r
}

impl TileMap {
    pub fn new(image: i32, width: i32, height: i32) -> Result<TileMap, String> {
        if width <= 0 || height <= 0 || width as i64 * height as i64 > 16_000_000 {
            return Err(format!("tile map size {width}x{height} is not allowed"));
        }
        let n = (width * height) as usize;
        Ok(TileMap { image, width, height, tiles: vec![EMPTY; n], terrain: vec![0; n], rules: HashMap::new() })
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height).then(|| (y * self.width + x) as usize)
    }

    /// Frame at a cell; `EMPTY` for empty cells and cells outside the map.
    pub fn get(&self, x: i32, y: i32) -> i32 {
        self.index(x, y).map_or(EMPTY, |i| self.tiles[i])
    }

    /// Sets a cell's frame directly (negative = empty). The cell stops being
    /// autotiled.
    pub fn set(&mut self, x: i32, y: i32, tile: i32) {
        if let Some(i) = self.index(x, y) {
            self.tiles[i] = tile.max(EMPTY);
            self.terrain[i] = 0;
        }
    }

    pub fn terrain_at(&self, x: i32, y: i32) -> i32 {
        self.index(x, y).map_or(0, |i| self.terrain[i])
    }

    pub fn set_rule(&mut self, terrain: i32, first: i32, mode: i32) -> Result<(), String> {
        if terrain <= 0 {
            return Err("terrain numbers start at 1".into());
        }
        if mode != 16 && mode != 47 {
            return Err(format!("autotile mode must be 16 or 47, not {mode}"));
        }
        self.rules.insert(terrain, (first, mode));
        // cells already painted with it follow the new frames
        for y in 0..self.height {
            for x in 0..self.width {
                if self.terrain_at(x, y) == terrain {
                    self.refresh(x, y);
                }
            }
        }
        Ok(())
    }

    /// Paints a cell with a terrain (0 erases it) and updates the frames of
    /// the cell and its neighbours.
    pub fn paint(&mut self, x: i32, y: i32, terrain: i32) {
        let Some(i) = self.index(x, y) else { return };
        self.terrain[i] = terrain.max(0);
        if terrain <= 0 {
            self.tiles[i] = EMPTY;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                self.refresh(x + dx, y + dy);
            }
        }
    }

    fn same(&self, x: i32, y: i32, terrain: i32) -> bool {
        match self.index(x, y) {
            Some(i) => self.terrain[i] == terrain,
            None => true,
        }
    }

    /// Recomputes an autotiled cell's frame from its neighbours.
    fn refresh(&mut self, x: i32, y: i32) {
        let Some(i) = self.index(x, y) else { return };
        let t = self.terrain[i];
        if t <= 0 {
            return;
        }
        let Some(&(first, mode)) = self.rules.get(&t) else { return };
        let s = |dx: i32, dy: i32| self.same(x + dx, y + dy, t);
        self.tiles[i] = if mode == 16 {
            first + s(0, -1) as i32 + 2 * s(1, 0) as i32 + 4 * s(0, 1) as i32 + 8 * s(-1, 0) as i32
        } else {
            let bits = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];
            let m = bits.iter().enumerate().fold(0u8, |m, (b, &(dx, dy))| if s(dx, dy) { m | 1 << b } else { m });
            let r = reduce_blob(m);
            first + blob_values().iter().position(|&v| v == r).unwrap_or(0) as i32
        };
    }

    /// Text form: a header line, then the frames and the terrain numbers, one
    /// row per line.
    pub fn save(&self) -> String {
        let mut out = format!("NEPTUNE_TILEMAP 1\n{} {}\n", self.width, self.height);
        let mut rules: Vec<_> = self.rules.iter().collect();
        rules.sort();
        out += &format!("rules {}\n", rules.len());
        for (t, (first, mode)) in rules {
            out += &format!("{t} {first} {mode}\n");
        }
        for grid in [&self.tiles, &self.terrain] {
            for row in grid.chunks(self.width as usize) {
                out += &row.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
                out.push('\n');
            }
        }
        out
    }

    /// Reads [`TileMap::save`] output, or a plain CSV grid as exported by the
    /// Tiled editor (tile numbers start at 1 there; 0 is empty).
    pub fn load(text: &str, image: i32) -> Result<TileMap, String> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        let nums = |l: &str| -> Result<Vec<i32>, String> {
            l.split([',', ' ', '\t']).filter(|s| !s.is_empty()).map(|s| s.trim().parse::<i32>().map_err(|_| format!("'{s}' is not a tile number"))).collect()
        };
        if text.trim_start().starts_with("NEPTUNE_TILEMAP") {
            lines.next();
            let size = nums(lines.next().ok_or("missing map size")?)?;
            let [w, h] = size[..] else { return Err("bad map size".into()) };
            let mut map = TileMap::new(image, w, h)?;
            let rules_line = lines.next().ok_or("missing rules")?;
            let n: usize = rules_line.strip_prefix("rules").ok_or("missing rules")?.trim().parse().map_err(|_| "bad rule count")?;
            for _ in 0..n {
                let r = nums(lines.next().ok_or("missing rule")?)?;
                let [t, first, mode] = r[..] else { return Err("bad rule".into()) };
                map.rules.insert(t, (first, mode));
            }
            for grid in [&mut map.tiles, &mut map.terrain] {
                for row in grid.chunks_mut(w as usize) {
                    let v = nums(lines.next().ok_or("the map has too few rows")?)?;
                    if v.len() != row.len() {
                        return Err("a map row has the wrong length".into());
                    }
                    row.copy_from_slice(&v);
                }
            }
            Ok(map)
        } else {
            let rows: Vec<Vec<i32>> = lines.map(nums).collect::<Result<_, _>>()?;
            let w = rows.iter().map(Vec::len).max().unwrap_or(0) as i32;
            let mut map = TileMap::new(image, w, rows.len() as i32)?;
            for (y, row) in rows.iter().enumerate() {
                for (x, &v) in row.iter().enumerate() {
                    map.tiles[y * w as usize + x] = if v <= 0 { EMPTY } else { v - 1 };
                }
            }
            Ok(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_has_47_values() {
        assert_eq!(blob_values().len(), 47);
        assert_eq!(blob_values()[0], 0);
        assert_eq!(*blob_values().last().unwrap(), 255);
    }

    #[test]
    fn sixteen_frame_autotiling() {
        let mut m = TileMap::new(1, 5, 5).unwrap();
        m.set_rule(1, 100, 16).unwrap();
        m.paint(2, 2, 1);
        // a lone cell: neighbours inside the map are different
        assert_eq!(m.get(2, 2), 100);
        m.paint(3, 2, 1);
        assert_eq!(m.get(2, 2), 102); // east
        assert_eq!(m.get(3, 2), 108); // west
        m.paint(3, 2, 0);
        assert_eq!((m.get(2, 2), m.get(3, 2)), (100, EMPTY));
        // the map edge counts as the same terrain
        m.paint(0, 0, 1);
        assert_eq!(m.get(0, 0), 100 + 1 + 8);
    }

    #[test]
    fn blob_autotiling() {
        let mut m = TileMap::new(1, 5, 5).unwrap();
        m.set_rule(2, 0, 47).unwrap();
        for y in 1..4 {
            for x in 1..4 {
                m.paint(x, y, 2);
            }
        }
        // centre surrounded on all sides: the last blob frame
        assert_eq!(m.get(2, 2), 46);
        // top-left corner of the block: east, south-east and south
        let expected = blob_values().iter().position(|&v| v == 4 | 8 | 16).unwrap() as i32;
        assert_eq!(m.get(1, 1), expected);
        assert_eq!(m.terrain_at(1, 1), 2);
        // setting a tile directly stops autotiling there
        m.set(1, 1, 7);
        assert_eq!((m.get(1, 1), m.terrain_at(1, 1)), (7, 0));
    }

    #[test]
    fn save_and_load() {
        let mut m = TileMap::new(3, 4, 2).unwrap();
        m.set_rule(1, 10, 16).unwrap();
        m.paint(1, 1, 1);
        m.set(0, 0, 5);
        let back = TileMap::load(&m.save(), 3).unwrap();
        assert_eq!(back.save(), m.save());
        // Tiled CSV: numbers start at 1, 0 is empty
        let t = TileMap::load("1,2,0\n0,3,4\n", 1).unwrap();
        assert_eq!((t.width, t.height), (3, 2));
        assert_eq!((t.get(0, 0), t.get(2, 0), t.get(2, 1)), (0, EMPTY, 3));
        assert!(TileMap::load("1,x\n", 1).is_err());
    }
}
