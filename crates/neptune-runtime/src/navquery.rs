//! Fast queries on a navigation mesh: the closest point, and the shortest
//! path (A* over triangles, then the funnel algorithm to pull it straight).
//!
//! Fyrox 1.0's `Navmesh::query_closest` checks every triangle (its octree
//! query is disabled as buggy), and its path finder calls it twice per path,
//! which made agents and `FindPath` slow on large meshes. Here triangles are
//! bucketed in a grid over X/Z, so a query only looks at nearby ones.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use fyrox::core::algebra::Vector3;

type V3 = Vector3<f32>;

pub struct NavIndex {
    verts: Vec<V3>,
    tris: Vec<[u32; 3]>,
    /// Neighbouring triangle across each edge (a-b, b-c, c-a), if any.
    neighbours: Vec<[Option<u32>; 3]>,
    centres: Vec<V3>,
    x0: f32,
    z0: f32,
    cell: f32,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
}

/// The point of triangle a, b, c closest to p (Ericson, Real-Time Collision
/// Detection, 5.1.5).
pub fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(&ap), ac.dot(&ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(&bp), ac.dot(&bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(&cp), ac.dot(&cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Twice the signed area of triangle a, b, c seen from above (X/Z).
fn area2(a: V3, b: V3, c: V3) -> f32 {
    (c.x - a.x) * (b.z - a.z) - (b.x - a.x) * (c.z - a.z)
}

#[derive(PartialEq)]
struct Open(f32, u32);
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Open {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0)
    }
}

impl NavIndex {
    /// The triangles' corners (for drawing the mesh).
    pub fn triangles(&self) -> impl Iterator<Item = [V3; 3]> + '_ {
        self.tris.iter().map(|t| t.map(|i| self.verts[i as usize]))
    }

    pub fn new(verts: &[V3], tris: &[[u32; 3]]) -> NavIndex {
        let verts = verts.to_vec();
        let tris = tris.to_vec();
        // neighbours through shared edges
        let mut edges: HashMap<(u32, u32), Vec<(u32, usize)>> = HashMap::new();
        for (t, tri) in tris.iter().enumerate() {
            for e in 0..3 {
                let (a, b) = (tri[e], tri[(e + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_default().push((t as u32, e));
            }
        }
        let mut neighbours = vec![[None; 3]; tris.len()];
        for list in edges.values() {
            for &(t, e) in list {
                neighbours[t as usize][e] = list.iter().find(|(o, _)| *o != t).map(|(o, _)| *o);
            }
        }
        let centres: Vec<V3> = tris.iter().map(|t| (verts[t[0] as usize] + verts[t[1] as usize] + verts[t[2] as usize]) / 3.0).collect();
        // the grid: about four triangles per cell
        let (mut lo, mut hi) = (V3::repeat(f32::MAX), V3::repeat(f32::MIN));
        for v in &verts {
            lo = lo.inf(v);
            hi = hi.sup(v);
        }
        let area = ((hi.x - lo.x) * (hi.z - lo.z)).max(1e-6);
        let cell = (area / tris.len().max(1) as f32 * 4.0).sqrt().max(0.05);
        let cols = (((hi.x - lo.x) / cell).floor() as usize + 1).min(2048);
        let rows = (((hi.z - lo.z) / cell).floor() as usize + 1).min(2048);
        let cell = cell.max((hi.x - lo.x) / cols as f32).max((hi.z - lo.z) / rows as f32);
        let mut cells = vec![Vec::new(); cols * rows];
        let at = |x: f32, z: f32| (((x - lo.x) / cell).floor().clamp(0.0, cols as f32 - 1.0) as usize, ((z - lo.z) / cell).floor().clamp(0.0, rows as f32 - 1.0) as usize);
        for (t, tri) in tris.iter().enumerate() {
            let p = tri.map(|i| verts[i as usize]);
            let (x0, z0) = at(p[0].x.min(p[1].x).min(p[2].x), p[0].z.min(p[1].z).min(p[2].z));
            let (x1, z1) = at(p[0].x.max(p[1].x).max(p[2].x), p[0].z.max(p[1].z).max(p[2].z));
            for z in z0..=z1 {
                for x in x0..=x1 {
                    cells[z * cols + x].push(t as u32);
                }
            }
        }
        NavIndex { verts, tris, neighbours, centres, x0: lo.x, z0: lo.z, cell, cols, rows, cells }
    }

    fn corners(&self, t: u32) -> [V3; 3] {
        self.tris[t as usize].map(|i| self.verts[i as usize])
    }

    /// The point of the mesh closest to p, and its triangle.
    pub fn closest(&self, p: V3) -> Option<(V3, u32)> {
        if self.tris.is_empty() {
            return None;
        }
        let cx = ((p.x - self.x0) / self.cell).floor() as i64;
        let cz = ((p.z - self.z0) / self.cell).floor() as i64;
        // start inside the grid; rings grow outwards from there
        let (sx, sz) = (cx.clamp(0, self.cols as i64 - 1), cz.clamp(0, self.rows as i64 - 1));
        // how far outside the grid the point is, in cells (rings nearer than this hold nothing closer)
        let outside = (cx - sx).abs().max((cz - sz).abs());
        let mut best: Option<(V3, u32)> = None;
        let mut best_d = f32::MAX;
        let max_ring = self.cols.max(self.rows) as i64;
        for r in 0..=max_ring {
            // every triangle in ring r is at least (r - 1 + outside) cells away sideways
            let reach = (r - 1 + outside).max(0) as f32 * self.cell;
            if best.is_some() && reach * reach > best_d {
                break;
            }
            for z in (sz - r)..=(sz + r) {
                if z < 0 || z >= self.rows as i64 {
                    continue;
                }
                let edge_row = z == sz - r || z == sz + r;
                let mut x = sx - r;
                while x <= sx + r {
                    if x >= 0 && x < self.cols as i64 {
                        for &t in &self.cells[z as usize * self.cols + x as usize] {
                            let [a, b, c] = self.corners(t);
                            let q = closest_on_triangle(p, a, b, c);
                            let d = (q - p).norm_squared();
                            if d < best_d {
                                best_d = d;
                                best = Some((q, t));
                            }
                        }
                    }
                    // inner rows only need the ring's two end cells
                    x += if edge_row || r == 0 { 1 } else { 2 * r };
                }
            }
        }
        best
    }

    /// Triangles from `from` to `to` (A* over neighbouring triangles). If
    /// `to` cannot be reached, the way to the reachable triangle nearest it.
    fn corridor(&self, from: u32, to: u32, goal: V3) -> Vec<u32> {
        let n = self.tris.len();
        let mut g = vec![f32::INFINITY; n];
        let mut came = vec![u32::MAX; n];
        let mut open = BinaryHeap::new();
        g[from as usize] = 0.0;
        open.push(Open((self.centres[from as usize] - goal).norm(), from));
        let (mut nearest, mut nearest_h) = (from, f32::MAX);
        while let Some(Open(_, cur)) = open.pop() {
            let h = (self.centres[cur as usize] - goal).norm();
            if h < nearest_h {
                (nearest, nearest_h) = (cur, h);
            }
            if cur == to {
                nearest = to;
                break;
            }
            for nb in self.neighbours[cur as usize].into_iter().flatten() {
                let ng = g[cur as usize] + (self.centres[nb as usize] - self.centres[cur as usize]).norm();
                if ng < g[nb as usize] {
                    g[nb as usize] = ng;
                    came[nb as usize] = cur;
                    open.push(Open(ng + (self.centres[nb as usize] - goal).norm(), nb));
                }
            }
        }
        let mut out = vec![nearest];
        let mut c = nearest;
        while c != from {
            c = came[c as usize];
            out.push(c);
        }
        out.reverse();
        out
    }

    /// The shared edge of neighbouring triangles a and b as (left, right)
    /// seen walking from a into b.
    fn portal(&self, a: u32, b: u32) -> Option<(V3, V3)> {
        let e = self.neighbours[a as usize].iter().position(|n| *n == Some(b))?;
        let tri = self.tris[a as usize];
        let (p, q) = (self.verts[tri[e] as usize], self.verts[tri[(e + 1) % 3] as usize]);
        // the third corner of `a` lies behind the portal
        let behind = self.verts[tri[(e + 2) % 3] as usize];
        Some(if area2(behind, p, q) > 0.0 { (p, q) } else { (q, p) })
    }

    /// The shortest way from `start` to `end` over the mesh: a list of
    /// points starting at the mesh point nearest `start`. `None` if the mesh
    /// is empty.
    pub fn path(&self, start: V3, end: V3) -> Option<Vec<V3>> {
        let (s, st) = self.closest(start)?;
        let (e, et) = self.closest(end)?;
        let tris = self.corridor(st, et, e);
        let end_pt = if *tris.last()? == et { e } else { self.centres[*tris.last()? as usize] };
        if tris.len() == 1 {
            return Some(vec![s, end_pt]);
        }
        let mut portals: Vec<(V3, V3)> = vec![(s, s)];
        for w in tris.windows(2) {
            portals.push(self.portal(w[0], w[1])?);
        }
        portals.push((end_pt, end_pt));
        Some(funnel(&portals))
    }
}

/// The simple stupid funnel algorithm (Mikko Mononen) on portals given as
/// (left, right), in X/Z; heights come from the portal points.
fn funnel(portals: &[(V3, V3)]) -> Vec<V3> {
    let mut path = vec![portals[0].0];
    let (mut apex, mut left, mut right) = (portals[0].0, portals[0].0, portals[0].1);
    let (mut left_i, mut right_i) = (0usize, 0usize);
    let mut apex_i: usize;
    let mut i = 1;
    while i < portals.len() {
        let (pl, pr) = portals[i];
        // tighten the right side
        if area2(apex, right, pr) <= 0.0 {
            if apex == right || area2(apex, left, pr) > 0.0 {
                right = pr;
                right_i = i;
            } else {
                // right crossed left: left becomes a corner
                if path.last() != Some(&left) {
                    path.push(left);
                }
                apex = left;
                apex_i = left_i;
                (left, right) = (apex, apex);
                (left_i, right_i) = (apex_i, apex_i);
                i = apex_i + 1;
                continue;
            }
        }
        // tighten the left side
        if area2(apex, left, pl) >= 0.0 {
            if apex == left || area2(apex, right, pl) < 0.0 {
                left = pl;
                left_i = i;
            } else {
                if path.last() != Some(&right) {
                    path.push(right);
                }
                apex = right;
                apex_i = right_i;
                (left, right) = (apex, apex);
                (left_i, right_i) = (apex_i, apex_i);
                i = apex_i + 1;
                continue;
            }
        }
        i += 1;
    }
    let last = portals[portals.len() - 1].0;
    if path.last() != Some(&last) {
        path.push(last);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An L-shaped corridor of unit squares: x 0..5 at z 0..1, then z 1..5 at x 4..5.
    fn l_shape() -> NavIndex {
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        let mut square = |x: f32, z: f32| {
            let b = verts.len() as u32;
            verts.extend([V3::new(x, 0.0, z), V3::new(x + 1.0, 0.0, z), V3::new(x + 1.0, 0.0, z + 1.0), V3::new(x, 0.0, z + 1.0)]);
            tris.push([b, b + 1, b + 2]);
            tris.push([b, b + 2, b + 3]);
        };
        for x in 0..5 {
            square(x as f32, 0.0);
        }
        for z in 1..5 {
            square(4.0, z as f32);
        }
        // weld the corners shared between squares
        let mut map: HashMap<(i32, i32), u32> = HashMap::new();
        let mut welded = Vec::new();
        let remap: Vec<u32> = verts
            .iter()
            .map(|v| {
                *map.entry((v.x as i32, v.z as i32)).or_insert_with(|| {
                    welded.push(*v);
                    welded.len() as u32 - 1
                })
            })
            .collect();
        let tris: Vec<[u32; 3]> = tris.iter().map(|t| t.map(|i| remap[i as usize])).collect();
        NavIndex::new(&welded, &tris)
    }

    #[test]
    fn closest_points() {
        let m = l_shape();
        let (q, _) = m.closest(V3::new(2.5, 3.0, 0.5)).unwrap();
        assert!((q - V3::new(2.5, 0.0, 0.5)).norm() < 1e-4, "{q:?}");
        // off the mesh: onto the nearest edge
        let (q, _) = m.closest(V3::new(2.0, 0.0, 3.0)).unwrap();
        assert!((q - V3::new(2.0, 0.0, 1.0)).norm() < 1e-4, "{q:?}");
        let (q, _) = m.closest(V3::new(-10.0, 0.0, -10.0)).unwrap();
        assert!((q - V3::new(0.0, 0.0, 0.0)).norm() < 1e-4, "{q:?}");
    }

    #[test]
    fn paths_go_round_the_inside_corner() {
        let m = l_shape();
        let p = m.path(V3::new(0.5, 0.0, 0.5), V3::new(4.5, 0.0, 4.5)).unwrap();
        assert_eq!(p.first(), Some(&V3::new(0.5, 0.0, 0.5)));
        assert_eq!(p.last(), Some(&V3::new(4.5, 0.0, 4.5)));
        // straight to the inner corner (4, 1), then straight on
        assert_eq!(p.len(), 3, "{p:?}");
        assert!((p[1] - V3::new(4.0, 0.0, 1.0)).norm() < 1e-4, "{p:?}");
        // and the reverse
        let p = m.path(V3::new(4.5, 0.0, 4.5), V3::new(0.5, 0.0, 0.5)).unwrap();
        assert_eq!(p.len(), 3, "{p:?}");
        assert!((p[1] - V3::new(4.0, 0.0, 1.0)).norm() < 1e-4, "{p:?}");
        // a straight run needs no corners
        let p = m.path(V3::new(0.5, 0.0, 0.5), V3::new(4.5, 0.0, 0.5)).unwrap();
        assert_eq!(p.len(), 2, "{p:?}");
    }
}
