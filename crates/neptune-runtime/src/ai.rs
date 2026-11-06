//! Navigation and pathfinding (ext): navigation meshes, paths, grid A* and
//! agents that walk paths.
//!
//! * **Navigation meshes** (`CreateNavMesh`, `GenerateNavMesh`): the
//!   surfaces characters can walk on. `CreateNavMesh` uses a mesh made for
//!   it (or a level's floor); `GenerateNavMesh` samples the pickable
//!   entities (`EntityPickMode`) from above on a grid, keeps flat enough
//!   ground and joins neighbouring cells whose heights differ by at most a
//!   step, so walls and boxes become holes.
//! * **Paths** (`FindPath`): the shortest way between two points over a
//!   navigation mesh (Fyrox's A* with string pulling), read point by point.
//! * **Grids** (`CreatePathGrid`, `FindGridPath`): A* over a grid of costs,
//!   for 2D and strategy games; `TileMapPathGrid` makes one from a tile map.
//! * **Agents** (`CreateAgent`, `AgentMoveTo`, `AgentFollow`): an entity that
//!   walks along paths by itself in `UpdateWorld`, turns to face where it
//!   goes and keeps clear of other agents.
//!
//! Everything is in Blitz space (positions as `PositionEntity` takes them).

use std::collections::{BinaryHeap, HashMap};

use fyrox::{
    core::{algebra::Vector3, math::TriangleDefinition},
    utils::navmesh::{Navmesh, NavmeshAgent},
};

use crate::world::Host3D;

type V3 = Vector3<f32>;

pub struct NavMesh {
    pub mesh: Navmesh,
}

impl std::fmt::Debug for NavMesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NavMesh({} triangles)", self.mesh.triangles().len())
    }
}

/// Builds a navigation mesh from triangles, joining corners at the same place.
pub fn navmesh_from(tris: &[[V3; 3]]) -> Navmesh {
    let mut index: HashMap<[i64; 3], u32> = HashMap::new();
    let mut verts: Vec<V3> = Vec::new();
    let mut defs = Vec::new();
    let mut id = |p: V3, verts: &mut Vec<V3>| {
        let k = [(p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64, (p.z * 1000.0).round() as i64];
        *index.entry(k).or_insert_with(|| {
            verts.push(p);
            verts.len() as u32 - 1
        })
    };
    for t in tris {
        let (a, b, c) = (id(t[0], &mut verts), id(t[1], &mut verts), id(t[2], &mut verts));
        if a != b && b != c && a != c {
            defs.push(TriangleDefinition([a, b, c]));
        }
    }
    Navmesh::new(defs, verts)
}

/// Walkable triangles of a grid of heights (`None` = no ground): two per
/// cell whose corners all exist and differ by at most `step`.
pub fn grid_triangles(heights: &[Option<f32>], cols: usize, rows: usize, x0: f32, z0: f32, cell: f32, step: f32) -> Vec<[V3; 3]> {
    let mut out = Vec::new();
    let at = |i: usize, j: usize| heights[j * cols + i].map(|y| Vector3::new(x0 + i as f32 * cell, y, z0 + j as f32 * cell));
    for j in 0..rows.saturating_sub(1) {
        for i in 0..cols.saturating_sub(1) {
            let (Some(a), Some(b), Some(c), Some(d)) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)) else { continue };
            let ys = [a.y, b.y, c.y, d.y];
            let (lo, hi) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
            if hi - lo <= step {
                out.push([a, b, c]);
                out.push([a, c, d]);
            }
        }
    }
    out
}

/// A* over a grid of costs (0 = blocked). Returns cells from start to goal.
pub fn grid_path(costs: &[f32], w: usize, h: usize, from: (usize, usize), to: (usize, usize), diagonal: bool) -> Option<Vec<(usize, usize)>> {
    let idx = |x: usize, y: usize| y * w + x;
    if from.0 >= w || from.1 >= h || to.0 >= w || to.1 >= h || costs[idx(to.0, to.1)] <= 0.0 {
        return None;
    }
    #[derive(PartialEq)]
    struct Node(f32, usize);
    impl Eq for Node {}
    impl PartialOrd for Node {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Node {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            // smallest estimate first
            o.0.total_cmp(&self.0)
        }
    }
    let heur = |i: usize| {
        let (x, y) = ((i % w) as f32, (i / w) as f32);
        let (dx, dy) = ((x - to.0 as f32).abs(), (y - to.1 as f32).abs());
        if diagonal {
            dx.max(dy) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dy)
        } else {
            dx + dy
        }
    };
    let start = idx(from.0, from.1);
    let goal = idx(to.0, to.1);
    let mut g = vec![f32::INFINITY; w * h];
    let mut came = vec![usize::MAX; w * h];
    let mut open = BinaryHeap::new();
    g[start] = 0.0;
    open.push(Node(heur(start), start));
    let steps: &[(i32, i32)] = if diagonal { &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] } else { &[(1, 0), (-1, 0), (0, 1), (0, -1)] };
    while let Some(Node(_, cur)) = open.pop() {
        if cur == goal {
            let mut path = vec![(goal % w, goal / w)];
            let mut c = goal;
            while c != start {
                c = came[c];
                path.push((c % w, c / w));
            }
            path.reverse();
            return Some(path);
        }
        let (cx, cy) = ((cur % w) as i32, (cur / w) as i32);
        for &(dx, dy) in steps {
            let (nx, ny) = (cx + dx, cy + dy);
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let n = idx(nx as usize, ny as usize);
            let cost = costs[n];
            if cost <= 0.0 {
                continue;
            }
            // no cutting corners past blocked cells
            if dx != 0 && dy != 0 && (costs[idx((cx + dx) as usize, cy as usize)] <= 0.0 || costs[idx(cx as usize, (cy + dy) as usize)] <= 0.0) {
                continue;
            }
            let len = if dx != 0 && dy != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
            let ng = g[cur] + len * cost;
            if ng < g[n] {
                g[n] = ng;
                came[n] = cur;
                open.push(Node(ng + heur(n), n));
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct PathGrid {
    pub w: usize,
    pub h: usize,
    pub costs: Vec<f32>,
}

pub struct Agent {
    pub entity: i32,
    pub navmesh: i32,
    pub speed: f32,
    pub radius: f32,
    /// Degrees a second.
    pub turn: f32,
    pub goal: Option<V3>,
    pub follow: Option<(i32, f32)>,
    pub path: Vec<V3>,
    pub next: usize,
    pub arrived: bool,
    /// How far the entity's origin is above the navigation mesh.
    pub height: f32,
    /// Where the goal was when the path was made.
    planned_for: Option<V3>,
    finder: NavmeshAgent,
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Agent(entity {})", self.entity)
    }
}

fn path_length(points: &[V3]) -> f32 {
    points.windows(2).map(|w| (w[1] - w[0]).norm()).sum()
}

impl Host3D<'_> {
    pub fn navmesh(&self, n: i32) -> Result<&NavMesh, String> {
        usize::try_from(n - 1).ok().and_then(|i| self.world.navmeshes.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Navigation mesh {n} does not exist"))
    }

    fn add_navmesh(&mut self, mesh: Navmesh) -> Result<i32, String> {
        if mesh.triangles().is_empty() {
            return Err("the navigation mesh has no walkable triangles".into());
        }
        self.world.navmeshes.push(Some(NavMesh { mesh }));
        Ok(self.world.navmeshes.len() as i32)
    }

    /// `CreateNavMesh`: the triangles of an entity's mesh (in world space)
    /// that slope at most `max_slope` degrees.
    pub fn create_navmesh(&mut self, e: i32, max_slope: f32) -> Result<i32, String> {
        let w = self.world_of(e)?;
        let local = self.mesh_triangles(e)?;
        let min_up = max_slope.clamp(0.0, 90.0).to_radians().cos();
        let tris: Vec<[V3; 3]> = local
            .iter()
            .map(|t| t.map(|p| w.matrix.transform_point(&p.into()).coords))
            .filter(|t| (t[1] - t[0]).cross(&(t[2] - t[0])).try_normalize(1e-9).is_some_and(|n| n.y.abs() >= min_up - 1e-4))
            .collect();
        self.add_navmesh(navmesh_from(&tris))
    }

    /// `GenerateNavMesh`: samples the ground of pickable entities from above.
    #[allow(clippy::too_many_arguments)]
    pub fn generate_navmesh(&mut self, x: f32, z: f32, width: f32, depth: f32, cell: f32, max_slope: f32, step: f32, top: f32) -> Result<i32, String> {
        let cell = cell.max(0.05);
        let (cols, rows) = ((width / cell).floor() as usize + 1, (depth / cell).floor() as usize + 1);
        if cols * rows > 4_000_000 {
            return Err("GenerateNavMesh: too many cells (use a bigger cell size)".into());
        }
        let targets = crate::commands::pick_targets(self, &[], false)?;
        let min_up = max_slope.clamp(0.0, 90.0).to_radians().cos();
        let mut heights = vec![None; cols * rows];
        for j in 0..rows {
            for i in 0..cols {
                let p = Vector3::new(x + i as f32 * cell, top, z + j as f32 * cell);
                if let Some(hit) = crate::pick::pick(p, Vector3::new(0.0, -2.0 * top.abs().max(1000.0), 0.0), 0.0, &targets) {
                    if hit.normal.y >= min_up {
                        heights[j * cols + i] = Some(hit.point.y);
                    }
                }
            }
        }
        let tris = grid_triangles(&heights, cols, rows, x, z, cell, step);
        self.add_navmesh(navmesh_from(&tris))
    }

    /// `NavMeshEntity`: a mesh entity showing a navigation mesh (for checking it).
    pub fn navmesh_entity(&mut self, n: i32) -> Result<i32, String> {
        let nm = &self.navmesh(n)?.mesh;
        let verts: Vec<crate::mesh::Vert> = nm.vertices().iter().map(|p| crate::mesh::Vert { pos: Vector3::new(p.x, p.y + 0.02, p.z), normal: Vector3::y(), ..Default::default() }).collect();
        // front faces up: (b-a)x(c-a) points to +Y in Blitz space
        let vs = nm.vertices();
        let tris: Vec<[u32; 3]> = nm
            .triangles()
            .iter()
            .map(|t| {
                let [a, b, c] = t.0;
                let (pa, pb, pc) = (vs[a as usize], vs[b as usize], vs[c as usize]);
                if (pb - pa).cross(&(pc - pa)).y >= 0.0 {
                    [a, b, c]
                } else {
                    [a, c, b]
                }
            })
            .collect();
        let node = crate::world::mesh_node(crate::mesh::surface_data(&verts, &tris), "NavMesh");
        self.add_entity(node, crate::world::Kind::Mesh, 0)
    }

    /// The point of a navigation mesh closest to `p`.
    pub fn navmesh_closest(&self, n: i32, p: V3) -> Result<Option<V3>, String> {
        Ok(self.navmesh(n)?.mesh.query_closest(p).map(|(q, _)| q))
    }

    fn add_path(&mut self, points: Vec<V3>) -> i32 {
        match self.world.paths.iter().position(Option::is_none) {
            Some(i) => {
                self.world.paths[i] = Some(points);
                i as i32 + 1
            }
            None => {
                self.world.paths.push(Some(points));
                self.world.paths.len() as i32
            }
        }
    }

    pub fn path(&self, p: i32) -> Result<&Vec<V3>, String> {
        usize::try_from(p - 1).ok().and_then(|i| self.world.paths.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Path {p} does not exist"))
    }

    /// `FindPath`: the shortest way over a navigation mesh, or 0 if there is none.
    pub fn find_path(&mut self, n: i32, from: V3, to: V3) -> Result<i32, String> {
        let mut finder = NavmeshAgent::new();
        let ok = finder.calculate_path(&self.navmesh(n)?.mesh, from, to).is_ok();
        let points = finder.path().to_vec();
        if !ok || points.is_empty() {
            return Ok(0);
        }
        Ok(self.add_path(points))
    }

    pub fn path_grid(&mut self, g: i32) -> Result<&mut PathGrid, String> {
        usize::try_from(g - 1).ok().and_then(|i| self.world.path_grids.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("Path grid {g} does not exist"))
    }

    /// `FindGridPath`: cells (as x, y points) from start to goal, or 0.
    pub fn find_grid_path(&mut self, g: i32, from: (i32, i32), to: (i32, i32), diagonal: bool) -> Result<i32, String> {
        let grid = self.path_grid(g)?.clone();
        let cell = |p: (i32, i32)| (p.0.max(0) as usize, p.1.max(0) as usize);
        match grid_path(&grid.costs, grid.w, grid.h, cell(from), cell(to), diagonal) {
            Some(cells) => Ok(self.add_path(cells.into_iter().map(|(x, y)| Vector3::new(x as f32, y as f32, 0.0)).collect())),
            None => Ok(0),
        }
    }

    pub fn agent_mut(&mut self, a: i32) -> Result<&mut Agent, String> {
        usize::try_from(a - 1).ok().and_then(|i| self.world.agents.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("Agent {a} does not exist"))
    }

    /// `CreateAgent`.
    pub fn create_agent(&mut self, e: i32, navmesh: i32, speed: f32, radius: f32) -> Result<i32, String> {
        let pos = self.world_of(e)?.position();
        let height = self.navmesh_closest(navmesh, pos)?.map_or(0.0, |q| pos.y - q.y);
        let agent = Agent {
            entity: e,
            navmesh,
            speed: speed.max(0.0),
            radius: radius.max(0.0),
            turn: 360.0,
            goal: None,
            follow: None,
            path: Vec::new(),
            next: 0,
            arrived: true,
            height,
            planned_for: None,
            finder: NavmeshAgent::new(),
        };
        self.world.agents.push(Some(agent));
        Ok(self.world.agents.len() as i32)
    }

    /// Remaining distance along an agent's path.
    pub fn agent_distance(&mut self, a: i32) -> Result<f32, String> {
        let ag = self.agent_mut(a)?;
        if ag.arrived || ag.path.is_empty() {
            return Ok(0.0);
        }
        let (e, lift) = (ag.entity, Vector3::new(0.0, ag.height, 0.0));
        let rest: Vec<V3> = ag.path[ag.next.min(ag.path.len())..].to_vec();
        let pos = self.world_of(e)?.position() - lift;
        Ok(rest.first().map_or(0.0, |p| (p - pos).norm()) + path_length(&rest))
    }

    /// Moves every agent (called by `UpdateWorld`).
    pub fn update_agents(&mut self, frames: f32) -> Result<(), String> {
        let dt = frames / crate::anim::FPS;
        let n = self.world.agents.len();
        // where every agent is, for keeping them apart
        let mut positions: Vec<Option<(V3, f32)>> = Vec::with_capacity(n);
        for i in 0..n {
            let p = match &self.world.agents[i] {
                Some(a) if self.ent(a.entity).is_ok() => Some((self.world_of(a.entity)?.position(), a.radius)),
                _ => None,
            };
            positions.push(p);
        }
        for i in 0..n {
            let Some(mut ag) = self.world.agents[i].take() else { continue };
            if self.ent(ag.entity).is_err() {
                // the entity was freed: so is its agent
                continue;
            }
            let origin = self.world_of(ag.entity)?.position();
            // the agent's feet, on the navigation mesh
            let lift = Vector3::new(0.0, ag.height, 0.0);
            let pos = origin - lift;
            // following: the goal is the other entity, until close enough
            if let Some((target, keep)) = ag.follow {
                match self.world_of(target) {
                    Ok(t) => {
                        let tp = t.position();
                        if (tp - origin).norm() <= keep {
                            ag.arrived = true;
                            ag.goal = None;
                        } else {
                            ag.goal = Some(tp);
                            ag.arrived = false;
                        }
                    }
                    Err(_) => ag.follow = None,
                }
            }
            let mut new_pos = pos;
            if let (Some(goal), false) = (ag.goal, ag.arrived) {
                let replan = ag.path.is_empty() || ag.planned_for.is_none_or(|p| (p - goal).norm() > ag.radius.max(0.25));
                if replan {
                    let nav = &self.navmesh(ag.navmesh)?.mesh;
                    if ag.finder.calculate_path(nav, pos, goal).is_ok() {
                        ag.path = ag.finder.path().to_vec();
                        ag.next = 1.min(ag.path.len().saturating_sub(1));
                    }
                    ag.planned_for = Some(goal);
                }
                // walk towards the next corner, past corners already reached
                let mut budget = ag.speed * dt;
                while budget > 0.0 && ag.next < ag.path.len() {
                    let target = ag.path[ag.next];
                    let to = target - new_pos;
                    let d = to.norm();
                    if d <= budget {
                        new_pos = target;
                        budget -= d;
                        ag.next += 1;
                    } else {
                        new_pos += to / d * budget;
                        budget = 0.0;
                    }
                }
                if ag.next >= ag.path.len() {
                    ag.arrived = ag.follow.is_none();
                    if ag.arrived {
                        ag.goal = None;
                    }
                }
            }
            // keep clear of the other agents (sideways only)
            for (j, other) in positions.iter().enumerate() {
                let Some((op, orad)) = other else { continue };
                if j == i {
                    continue;
                }
                let mut away = new_pos + lift - op;
                away.y = 0.0;
                let d = away.norm();
                let min = ag.radius + orad;
                if d < min && d > 1e-4 {
                    new_pos += away / d * (min - d) * 0.5;
                }
            }
            if (new_pos - pos).norm_squared() > 1e-10 {
                // stay on the navigation mesh
                if let Ok(Some(q)) = self.navmesh_closest(ag.navmesh, new_pos) {
                    new_pos = q;
                }
                let moved = new_pos - pos;
                let e = ag.entity;
                self.set_xf(e, |x| x.pos = new_pos + lift)?;
                // turn towards where it is going, at most `turn` degrees a second
                if moved.x.abs() + moved.z.abs() > 1e-5 {
                    let want = crate::transform::vector_yaw(&Vector3::new(moved.x, 0.0, moved.z));
                    let (pitch, yaw, roll) = crate::transform::angles(&self.live_xf(self.ent(e)?).rot);
                    let mut diff = (want - yaw + 540.0).rem_euclid(360.0) - 180.0;
                    let max = ag.turn * dt;
                    diff = diff.clamp(-max, max);
                    self.set_xf(e, |x| x.rot = crate::transform::rotation(pitch, yaw + diff, roll))?;
                }
                positions[i] = Some((new_pos + lift, ag.radius));
            }
            self.world.agents[i] = Some(ag);
        }
        Ok(())
    }
}

impl Host3D<'_> {
    pub fn free_navmesh(&mut self, n: i32) -> Result<(), String> {
        self.navmesh(n)?;
        self.world.navmeshes[n as usize - 1] = None;
        for a in self.world.agents.iter_mut() {
            if a.as_ref().is_some_and(|a| a.navmesh == n) {
                *a = None;
            }
        }
        Ok(())
    }

    /// Point `i` (from 0) of a path.
    pub fn path_point(&self, p: i32, i: i32) -> Result<V3, String> {
        let pts = self.path(p)?;
        usize::try_from(i).ok().and_then(|i| pts.get(i)).copied().ok_or_else(|| format!("Path {p} has no point {i}"))
    }

    pub fn path_length(&self, p: i32) -> Result<f32, String> {
        Ok(path_length(self.path(p)?))
    }

    pub fn free_path(&mut self, p: i32) -> Result<(), String> {
        self.path(p)?;
        self.world.paths[p as usize - 1] = None;
        Ok(())
    }

    pub fn create_path_grid(&mut self, w: i32, h: i32) -> Result<i32, String> {
        if w < 1 || h < 1 || w as i64 * h as i64 > 16_000_000 {
            return Err(format!("CreatePathGrid: bad size {w} x {h}"));
        }
        self.world.path_grids.push(Some(PathGrid { w: w as usize, h: h as usize, costs: vec![1.0; (w * h) as usize] }));
        Ok(self.world.path_grids.len() as i32)
    }

    /// The cell index of (x, y), or an error outside the grid.
    fn grid_cell(&mut self, g: i32, x: i32, y: i32) -> Result<(&mut PathGrid, usize), String> {
        let grid = self.path_grid(g)?;
        if x < 0 || y < 0 || x as usize >= grid.w || y as usize >= grid.h {
            return Err(format!("({x}, {y}) is outside path grid {g}"));
        }
        let i = y as usize * grid.w + x as usize;
        Ok((grid, i))
    }

    pub fn set_path_cost(&mut self, g: i32, x: i32, y: i32, cost: f32) -> Result<(), String> {
        let (grid, i) = self.grid_cell(g, x, y)?;
        grid.costs[i] = cost.max(0.0);
        Ok(())
    }

    pub fn path_cost(&mut self, g: i32, x: i32, y: i32) -> Result<f32, String> {
        let (grid, i) = self.grid_cell(g, x, y)?;
        Ok(grid.costs[i])
    }

    /// `TileMapPathGrid`: empty tiles cost 1, other tiles are blocked (the
    /// same tiles `TileMapBody2D` makes solid).
    pub fn tilemap_path_grid(&mut self, map: i32) -> Result<i32, String> {
        let m = usize::try_from(map - 1).ok().and_then(|i| self.world.tilemaps.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Tile map {map} does not exist"))?;
        let (w, h) = (m.width, m.height);
        let costs = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| if m.get(x, y) == crate::tilemap::EMPTY { 1.0 } else { 0.0 }).collect();
        self.world.path_grids.push(Some(PathGrid { w: w as usize, h: h as usize, costs }));
        Ok(self.world.path_grids.len() as i32)
    }

    pub fn free_path_grid(&mut self, g: i32) -> Result<(), String> {
        self.path_grid(g)?;
        self.world.path_grids[g as usize - 1] = None;
        Ok(())
    }

    /// `AgentMoveTo`.
    pub fn agent_move_to(&mut self, a: i32, p: V3) -> Result<(), String> {
        let ag = self.agent_mut(a)?;
        ag.follow = None;
        ag.goal = Some(p);
        ag.arrived = false;
        ag.path.clear();
        Ok(())
    }

    /// `AgentFollow`: keeps walking towards an entity until within `distance`.
    pub fn agent_follow(&mut self, a: i32, e: i32, distance: f32) -> Result<(), String> {
        if e != 0 {
            self.ent(e)?;
        }
        let ag = self.agent_mut(a)?;
        ag.follow = (e != 0).then_some((e, distance.max(0.0)));
        ag.goal = None;
        ag.arrived = e == 0;
        ag.path.clear();
        Ok(())
    }

    pub fn agent_stop(&mut self, a: i32) -> Result<(), String> {
        let ag = self.agent_mut(a)?;
        ag.follow = None;
        ag.goal = None;
        ag.arrived = true;
        ag.path.clear();
        Ok(())
    }

    /// `AgentPath`: a copy of the path the agent is walking (0 if none).
    pub fn agent_path(&mut self, a: i32) -> Result<i32, String> {
        let ag = self.agent_mut(a)?;
        let pts = ag.path.clone();
        Ok(if pts.is_empty() { 0 } else { self.add_path(pts) })
    }

    pub fn free_agent(&mut self, a: i32) -> Result<(), String> {
        self.agent_mut(a)?;
        self.world.agents[a as usize - 1] = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_path_goes_around_walls() {
        // 5x5 with a wall down the middle except at the bottom
        let (w, h) = (5, 5);
        let mut costs = vec![1.0; w * h];
        for y in 0..4 {
            costs[y * w + 2] = 0.0;
        }
        let p = grid_path(&costs, w, h, (0, 0), (4, 0), false).unwrap();
        assert_eq!(p.first(), Some(&(0, 0)));
        assert_eq!(p.last(), Some(&(4, 0)));
        assert!(p.contains(&(2, 4)), "{p:?}");
        assert!(p.iter().all(|&(x, y)| costs[y * w + x] > 0.0));
        // fully blocked goal
        costs[4] = 0.0;
        assert!(grid_path(&costs, w, h, (0, 0), (4, 0), true).is_none());
    }

    #[test]
    fn cheaper_cells_are_preferred() {
        let (w, h) = (3, 3);
        let mut costs = vec![1.0; w * h];
        costs[w + 1] = 10.0; // a swamp in the middle
        let p = grid_path(&costs, w, h, (0, 1), (2, 1), false).unwrap();
        assert!(!p.contains(&(1, 1)), "{p:?}");
    }

    #[test]
    fn grid_triangles_skip_steps() {
        // 3x2 corners: the right column is a 2-unit wall top
        let heights = vec![Some(0.0), Some(0.0), Some(2.0), Some(0.0), Some(0.0), Some(2.0)];
        let t = grid_triangles(&heights, 3, 2, 0.0, 0.0, 1.0, 0.5);
        assert_eq!(t.len(), 2);
        let nm = navmesh_from(&t);
        assert_eq!(nm.vertices().len(), 4);
    }
}
