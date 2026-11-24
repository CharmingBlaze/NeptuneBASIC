//! Where the cells of a tile map go (ext): orthogonal, isometric, staggered
//! and hexagonal grids, using the same geometry as the Tiled editor.
//!
//! A cell's *box* is the rectangle a tile of the map's tile size fills.
//! Boxes overlap on every grid but the orthogonal one. Staggered isometric
//! maps are hexagonal maps whose straight side has no length.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Shape {
    #[default]
    Ortho,
    Iso,
    Hex {
        /// Columns are staggered instead of rows.
        axis_x: bool,
        /// Odd rows or columns are the ones pushed along.
        odd: bool,
        /// Length of the straight side, in pixels.
        side: f32,
    },
}

/// How a tile map's cells are laid out and drawn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub shape: Shape,
    /// Size of a cell's box.
    pub cell: (f32, f32),
    /// Rows of the map (an isometric map's left edge depends on them).
    pub rows: i32,
    /// Top-left of a tile's frame, from the top-left of its cell's box.
    pub frame: (f32, f32),
    /// Draw right to left, bottom to top (Tiled's render order).
    pub reverse: (bool, bool),
}

impl Layout {
    pub fn ortho(cell: (f32, f32)) -> Layout {
        Layout { cell, ..Layout::default() }
    }

    fn staggered(&self, n: i32) -> bool {
        match self.shape {
            Shape::Hex { odd, .. } => (n.rem_euclid(2) == 1) == odd,
            _ => false,
        }
    }

    /// Is this column drawn after the others in its row? Only staggered
    /// columns overlap within a row, and the lower ones must come last.
    pub fn draws_late(&self, cx: i32) -> bool {
        matches!(self.shape, Shape::Hex { axis_x: true, .. }) && self.staggered(cx)
    }

    /// Top-left of a cell's box, from the top-left of the map.
    pub fn cell_pos(&self, cx: i32, cy: i32) -> (f32, f32) {
        let (w, h) = self.cell;
        let (x, y) = (cx as f32, cy as f32);
        match self.shape {
            Shape::Ortho => (x * w, y * h),
            Shape::Iso => ((x - y) * w / 2.0 + (self.rows - 1) as f32 * w / 2.0, (x + y) * h / 2.0),
            Shape::Hex { axis_x: false, side, .. } => (x * w + self.staggered(cy) as i32 as f32 * w / 2.0, y * (h + side) / 2.0),
            Shape::Hex { axis_x: true, side, .. } => (x * (w + side) / 2.0, y * h + self.staggered(cx) as i32 as f32 * h / 2.0),
        }
    }

    /// Middle of a cell's box.
    pub fn cell_center(&self, cx: i32, cy: i32) -> (f32, f32) {
        let (x, y) = self.cell_pos(cx, cy);
        (x + self.cell.0 / 2.0, y + self.cell.1 / 2.0)
    }

    /// The outline of a cell, in map pixels.
    pub fn polygon(&self, cx: i32, cy: i32) -> Vec<(f32, f32)> {
        let (w, h) = self.cell;
        let (x, y) = self.cell_pos(cx, cy);
        let corners: Vec<(f32, f32)> = match self.shape {
            Shape::Ortho => vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)],
            Shape::Iso => vec![(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)],
            Shape::Hex { axis_x: false, side, .. } => {
                let s = (h - side) / 2.0;
                vec![(w / 2.0, 0.0), (w, s), (w, h - s), (w / 2.0, h), (0.0, h - s), (0.0, s)]
            }
            Shape::Hex { axis_x: true, side, .. } => {
                let s = (w - side) / 2.0;
                vec![(s, 0.0), (w - s, 0.0), (w, h / 2.0), (w - s, h), (s, h), (0.0, h / 2.0)]
            }
        };
        let mut out: Vec<(f32, f32)> = corners.into_iter().map(|(px, py)| (x + px, y + py)).collect();
        // a side of no length repeats a corner
        out.dedup();
        out
    }

    /// The cell under a point in map pixels (it may be outside the map).
    pub fn pick(&self, x: f32, y: f32) -> (i32, i32) {
        let (w, h) = self.cell;
        let guess = match self.shape {
            Shape::Ortho => return ((x / w).floor() as i32, (y / h).floor() as i32),
            Shape::Iso => {
                let sx = x - self.rows as f32 * w / 2.0;
                return ((y / h + sx / w).floor() as i32, (y / h - sx / w).floor() as i32);
            }
            Shape::Hex { axis_x: false, side, .. } => {
                let cy = (y / ((h + side) / 2.0)).floor() as i32;
                (((x - self.staggered(cy) as i32 as f32 * w / 2.0) / w).floor() as i32, cy)
            }
            Shape::Hex { axis_x: true, side, .. } => {
                let cx = (x / ((w + side) / 2.0)).floor() as i32;
                (cx, ((y - self.staggered(cx) as i32 as f32 * h / 2.0) / h).floor() as i32)
            }
        };
        // cells interlock, so the true one is the guess or next to it
        for dy in [0, -1, 1, -2, 2] {
            for dx in [0, -1, 1, -2, 2] {
                if inside(&self.polygon(guess.0 + dx, guess.1 + dy), x, y) {
                    return (guess.0 + dx, guess.1 + dy);
                }
            }
        }
        guess
    }

    /// Cell range `(x0, y0, x1, y1)` to draw for a view of map pixels, on a
    /// `width` x `height` map whose frames are `frame_size`.
    pub fn visible(&self, view: (f32, f32, f32, f32), width: i32, height: i32, frame_size: (f32, f32)) -> (i32, i32, i32, i32) {
        // an image buffer has no edges to clip to, so its view is enormous
        let near = |v: f32| v.clamp(-1e7, 1e7);
        // a cell can be seen if its frame touches the view
        let left = near(view.0 - self.frame.0 - frame_size.0);
        let top = near(view.1 - self.frame.1 - frame_size.1);
        let right = near(view.2 - self.frame.0);
        let bottom = near(view.3 - self.frame.1);
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for (px, py) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
            let (cx, cy) = self.pick(px, py);
            (x0, y0, x1, y1) = (x0.min(cx), y0.min(cy), x1.max(cx), y1.max(cy));
        }
        let margin = if self.shape == Shape::Ortho { 0 } else { 2 };
        ((x0 - margin).max(0), (y0 - margin).max(0), (x1 + 1 + margin).min(width), (y1 + 1 + margin).min(height))
    }

    /// Size in pixels of the box around a whole map of this layout.
    pub fn pixel_size(&self, width: i32, height: i32) -> (f32, f32) {
        let (w, h) = self.cell;
        let (cols, rows) = (width as f32, height as f32);
        match self.shape {
            Shape::Ortho => (cols * w, rows * h),
            Shape::Iso => ((cols + rows) * w / 2.0, (cols + rows) * h / 2.0),
            Shape::Hex { axis_x: false, side, .. } => (cols * w + if height > 1 { w / 2.0 } else { 0.0 }, rows * (h + side) / 2.0 + (h - side) / 2.0),
            Shape::Hex { axis_x: true, side, .. } => (cols * (w + side) / 2.0 + (w - side) / 2.0, rows * h + if width > 1 { h / 2.0 } else { 0.0 }),
        }
    }
}

fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut hit = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > y) != (b.1 > y) && x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0 {
            hit = !hit;
        }
        j = i;
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(shape: Shape) -> Layout {
        Layout { shape, cell: (64.0, 32.0), rows: 10, ..Layout::default() }
    }

    fn round_trip(l: &Layout) {
        for cy in 0..8 {
            for cx in 0..8 {
                let (x, y) = l.cell_center(cx, cy);
                assert_eq!(l.pick(x, y), (cx, cy), "{:?}", l.shape);
            }
        }
    }

    #[test]
    fn every_grid_picks_the_cell_it_placed() {
        round_trip(&layout(Shape::Ortho));
        round_trip(&layout(Shape::Iso));
        round_trip(&layout(Shape::Hex { axis_x: false, odd: true, side: 0.0 }));
        round_trip(&layout(Shape::Hex { axis_x: true, odd: false, side: 0.0 }));
        round_trip(&layout(Shape::Hex { axis_x: false, odd: true, side: 16.0 }));
        round_trip(&layout(Shape::Hex { axis_x: true, odd: true, side: 32.0 }));
    }

    #[test]
    fn isometric_positions() {
        let l = layout(Shape::Iso);
        // the first cell's diamond touches the top in the middle of the left half
        assert_eq!(l.cell_pos(0, 0), (9.0 * 32.0, 0.0));
        assert_eq!(l.cell_pos(1, 0), (10.0 * 32.0, 16.0));
        assert_eq!(l.cell_pos(0, 1), (8.0 * 32.0, 16.0));
        assert_eq!(l.pixel_size(10, 10), (640.0, 320.0));
    }

    #[test]
    fn staggered_rows_and_columns() {
        let l = layout(Shape::Hex { axis_x: false, odd: true, side: 0.0 });
        assert_eq!(l.cell_pos(1, 0), (64.0, 0.0));
        assert_eq!(l.cell_pos(1, 1), (96.0, 16.0));
        let even = layout(Shape::Hex { axis_x: false, odd: false, side: 0.0 });
        assert_eq!(even.cell_pos(0, 0), (32.0, 0.0));
        assert_eq!(even.cell_pos(0, 1), (0.0, 16.0));
        let col = layout(Shape::Hex { axis_x: true, odd: true, side: 16.0 });
        assert_eq!(col.cell_pos(1, 0), (40.0, 16.0));
        assert_eq!(col.cell_pos(2, 1), (80.0, 32.0));
        assert!(col.draws_late(1) && !col.draws_late(2));
    }

    #[test]
    fn staggered_cells_are_diamonds() {
        assert_eq!(layout(Shape::Hex { axis_x: false, odd: true, side: 0.0 }).polygon(0, 0).len(), 4);
        assert_eq!(layout(Shape::Hex { axis_x: false, odd: true, side: 16.0 }).polygon(0, 0).len(), 6);
    }

    #[test]
    fn visible_range_covers_tall_tiles() {
        let mut l = Layout::ortho((32.0, 32.0));
        assert_eq!(l.visible((0.0, 0.0, 64.0, 64.0), 100, 100, (32.0, 32.0)), (0, 0, 3, 3));
        assert_eq!(l.visible((320.0, 320.0, 352.0, 352.0), 100, 100, (32.0, 32.0)), (9, 9, 12, 12));
        // a tile 64 tall hangs above its cell, so the row below still shows
        l.frame = (0.0, -32.0);
        assert_eq!(l.visible((0.0, 0.0, 32.0, 32.0), 100, 100, (32.0, 64.0)), (0, 0, 2, 3));
    }

    #[test]
    fn a_huge_view_is_the_whole_map() {
        // an image buffer has no size to clip to
        let huge = f32::MAX / 4.0;
        assert_eq!(Layout::ortho((32.0, 32.0)).visible((0.0, 0.0, huge, huge), 5, 4, (32.0, 32.0)), (0, 0, 5, 4));
        let l = layout(Shape::Hex { axis_x: false, odd: true, side: 0.0 });
        assert_eq!(l.visible((0.0, 0.0, huge, huge), 5, 4, (64.0, 32.0)), (0, 0, 5, 4));
    }
}
