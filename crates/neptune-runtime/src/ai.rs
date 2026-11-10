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
//! * **Links** (`CreateNavLink`): ways across gaps the mesh does not cover,
//!   such as a jump or a ladder. Routes use them like any other ground.
//! * **Crowds** (`AgentCrowd`): an agent that looks ahead and picks a velocity
//!   that misses the others, instead of only being pushed apart on contact.
//!
//! Everything is in Blitz space (positions as `PositionEntity` takes them).

use std::collections::{BinaryHeap, HashMap};

use fyrox::{
    core::{
        algebra::{Vector2, Vector3},
        math::TriangleDefinition,
    },
    utils::navmesh::Navmesh,
};

use crate::world::Host3D;

type V2 = Vector2<f32>;
type V3 = Vector3<f32>;

/// A way between two places on a navigation mesh (`CreateNavLink`).
#[derive(Clone, Debug, PartialEq)]
pub struct NavLink {
    pub a: V3,
    pub b: V3,
    pub two_way: bool,
    /// Multiplies the link's length when routes are compared.
    pub cost: f32,
    /// Height of the arc an agent follows across it (0 = a straight line).
    pub jump: f32,
    pub enabled: bool,
}

pub struct NavMesh {
    pub mesh: Navmesh,
    /// Fast closest-point and path queries (see navquery.rs).
    pub index: crate::navquery::NavIndex,
    pub links: Vec<Option<NavLink>>,
}

/// Points to walk through. `links[i]` is the link used to reach `points[i]`
/// (0 = walking on the mesh).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Route {
    pub points: Vec<V3>,
    pub links: Vec<i32>,
}

impl Route {
    fn walking(points: Vec<V3>) -> Route {
        let links = vec![0; points.len()];
        Route { points, links }
    }
}

/// How near a walked path must end to its goal to count as reaching it.
const REACHED: f32 = 0.05;

/// The cheapest route from `from` to `to` over `walk` (the mesh) and `links`
/// (id, link), or `None` if there is no complete way. Dijkstra over the two
/// ends of the journey and of every link, asking for walks only when needed.
pub fn route_over(from: V3, to: V3, links: &[(i32, NavLink)], walk: &mut impl FnMut(V3, V3) -> Option<Vec<V3>>) -> Option<Route> {
    #[derive(Clone)]
    enum Edge {
        Walk(Vec<V3>),
        Link(i32),
    }
    // 0 is the start, 1 the goal, then each link's two ends
    let mut places = vec![from, to];
    for (_, l) in links {
        places.push(l.a);
        places.push(l.b);
    }
    let n = places.len();
    let mut best = vec![f32::INFINITY; n];
    let mut came: Vec<Option<(usize, Edge)>> = vec![None; n];
    let mut done = vec![false; n];
    best[0] = 0.0;
    while let Some(u) = (0..n).filter(|&i| !done[i] && best[i].is_finite()).min_by(|&a, &b| best[a].total_cmp(&best[b])) {
        if u == 1 {
            break;
        }
        done[u] = true;
        // across a link that starts here
        if u >= 2 {
            let (id, l) = &links[(u - 2) / 2];
            let at_a = u % 2 == 0;
            if at_a || l.two_way {
                let v = if at_a { u + 1 } else { u - 1 };
                let d = best[u] + (l.b - l.a).norm() * l.cost;
                if d < best[v] {
                    best[v] = d;
                    came[v] = Some((u, Edge::Link(*id)));
                }
            }
        }
        // walking to anywhere else
        for v in 1..n {
            // the straight line is the least a walk can cost
            if done[v] || v == u || best[u] + (places[v] - places[u]).norm() >= best[v] {
                continue;
            }
            let Some(points) = walk(places[u], places[v]) else { continue };
            if points.last().is_none_or(|p| (p - places[v]).norm() > REACHED) {
                continue;
            }
            let d = best[u] + path_length(&points);
            if d < best[v] {
                best[v] = d;
                came[v] = Some((u, Edge::Walk(points)));
            }
        }
    }
    if !best[1].is_finite() {
        return None;
    }
    let mut legs = Vec::new();
    let mut at = 1;
    while let Some((prev, edge)) = came[at].clone() {
        legs.push((at, edge));
        at = prev;
    }
    let mut route = Route { points: vec![from], links: vec![0] };
    for (to, edge) in legs.into_iter().rev() {
        match edge {
            Edge::Walk(points) => {
                route.links.extend(std::iter::repeat_n(0, points.len().saturating_sub(1)));
                route.points.extend(points.into_iter().skip(1));
            }
            Edge::Link(id) => {
                route.points.push(places[to]);
                route.links.push(id);
            }
        }
    }
    Some(route)
}

/// Another agent, as a crowd agent sees it.
#[derive(Clone, Copy, Debug)]
pub struct Nearby {
    pub pos: V2,
    pub vel: V2,
    pub radius: f32,
    /// It dodges too, so each side only has to take half of the avoiding.
    pub shares: bool,
}

/// Seconds until two circles touch (`gap` from one to the other, `closing`
/// the first's velocity relative to the second, `radius` the two together).
fn time_to_hit(gap: V2, closing: V2, radius: f32) -> f32 {
    let toward = gap.dot(&closing);
    if toward <= 0.0 {
        return f32::INFINITY; // moving apart, even if touching
    }
    let c = gap.norm_squared() - radius * radius;
    if c <= 0.0 {
        return 0.0;
    }
    let a = closing.norm_squared();
    let disc = toward * toward - a * c;
    if a < 1e-8 || disc < 0.0 {
        return f32::INFINITY;
    }
    (toward - disc.sqrt()) / a
}

/// The velocity closest to `desired` that does not run into anyone within
/// `horizon` seconds: tries speeds and directions around the desired one
/// and scores them (reciprocal velocity obstacles, by sampling). Directions
/// are tried in the same order relative to each agent's heading, so two
/// agents meeting head on both step to the same side and pass.
pub fn avoid_velocity(pos: V2, radius: f32, vel: V2, desired: V2, max_speed: f32, horizon: f32, others: &[Nearby]) -> V2 {
    if others.is_empty() || max_speed <= 0.0 || horizon <= 0.0 {
        return desired;
    }
    let score = |c: V2| {
        let mut soonest = horizon;
        for o in others {
            // with a partner, assume it takes its half of the change
            let closing = if o.shares { c * 2.0 - vel - o.vel } else { c - o.vel };
            soonest = soonest.min(time_to_hit(o.pos - pos, closing, radius + o.radius));
        }
        3.0 * (1.0 - soonest / horizon) + (c - desired).norm() / max_speed + 0.1 * (c - vel).norm() / max_speed
    };
    let heading = if desired.norm() > 1e-4 { desired.y.atan2(desired.x) } else { vel.y.atan2(vel.x) };
    let (mut best, mut best_score) = (desired, score(desired));
    let mut consider = |c: V2| {
        let s = score(c);
        if s < best_score {
            best = c;
            best_score = s;
        }
    };
    consider(V2::zeros());
    const STEPS: i32 = 16;
    for ring in [1.0, 0.75, 0.5, 0.25] {
        for k in 0..STEPS {
            // 0, +1, -1, +2, -2... steps around the heading
            let turn = if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) };
            let angle = heading + turn as f32 * std::f32::consts::TAU / STEPS as f32;
            consider(V2::new(angle.cos(), angle.sin()) * max_speed * ring);
        }
    }
    best
}

/// Most other agents a crowd agent pays attention to.
const MAX_NEIGHBOURS: usize = 10;

/// Where an agent was at the start of its turn.
#[derive(Clone, Copy)]
struct Seen {
    pos: V3,
    radius: f32,
    vel: V3,
    crowd: bool,
}

/// The agents close enough to matter to agent `me`, nearest first.
fn nearby(all: &[Option<Seen>], me: usize, pos: V2, radius: f32, speed: f32, horizon: f32) -> Vec<Nearby> {
    let mut found: Vec<(f32, Nearby)> = all
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != me)
        .filter_map(|(_, s)| {
            let s = (*s)?;
            let (at, vel) = (V2::new(s.pos.x, s.pos.z), V2::new(s.vel.x, s.vel.z));
            let d = (at - pos).norm();
            (d < radius + s.radius + (speed + vel.norm()) * horizon).then_some((d, Nearby { pos: at, vel, radius: s.radius, shares: s.crowd }))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.truncate(MAX_NEIGHBOURS);
    found.into_iter().map(|(_, n)| n).collect()
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
    /// Seconds since the path was made (a moving goal is re-planned at most
    /// four times a second).
    planned_ago: f32,
    /// For each point of `path`, the link that leads to it (0 = walking).
    links: Vec<i32>,
    /// `AgentCrowd`: seconds to look ahead, if it steers around others.
    crowd: Option<f32>,
    velocity: V3,
    /// Seconds a crowd agent has hardly moved.
    stalled: f32,
    /// How far above the ground it is, part way through a jump.
    arc: f32,
}

impl Agent {
    /// The link being crossed now (0 = none).
    pub fn link_here(&self) -> i32 {
        self.links.get(self.next).copied().unwrap_or(0)
    }

    /// Drops the route for a new destination; a link already started is
    /// finished first.
    fn forget_route(&mut self) {
        if self.link_here() == 0 {
            self.path.clear();
            self.links.clear();
        }
        self.planned_for = None;
    }
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
        let tris: Vec<[u32; 3]> = mesh.triangles().iter().map(|t| t.0).collect();
        let index = crate::navquery::NavIndex::new(mesh.vertices(), &tris);
        self.world.navmeshes.push(Some(NavMesh { mesh, index, links: Vec::new() }));
        Ok(self.world.navmeshes.len() as i32)
    }

    fn navmesh_mut(&mut self, n: i32) -> Result<&mut NavMesh, String> {
        usize::try_from(n - 1).ok().and_then(|i| self.world.navmeshes.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("Navigation mesh {n} does not exist"))
    }

    fn nav_link_mut(&mut self, n: i32, l: i32) -> Result<&mut NavLink, String> {
        usize::try_from(l - 1).ok().and_then(|i| self.navmesh_mut(n).ok()?.links.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("Navigation link {l} does not exist"))
    }

    /// Makes agents on a navigation mesh think again about their routes.
    fn replan_agents(&mut self, n: i32) {
        for a in self.world.agents.iter_mut().flatten().filter(|a| a.navmesh == n) {
            a.planned_for = None;
        }
    }

    /// `CreateNavLink`: its ends are moved to the closest points of the mesh.
    pub fn create_nav_link(&mut self, n: i32, a: V3, b: V3, two_way: bool, cost: f32, jump: f32) -> Result<i32, String> {
        let (Some(a), Some(b)) = (self.navmesh_closest(n, a)?, self.navmesh_closest(n, b)?) else { return Err("the navigation mesh has no walkable triangles".into()) };
        let nav = self.navmesh_mut(n)?;
        nav.links.push(Some(NavLink { a, b, two_way, cost: cost.max(0.0), jump: jump.max(0.0), enabled: true }));
        let id = nav.links.len() as i32;
        self.replan_agents(n);
        Ok(id)
    }

    /// `NavLinkEnable`: a closed link (a shut door) is left out of new routes.
    pub fn nav_link_enable(&mut self, n: i32, l: i32, enabled: bool) -> Result<(), String> {
        self.nav_link_mut(n, l)?.enabled = enabled;
        self.replan_agents(n);
        Ok(())
    }

    pub fn free_nav_link(&mut self, n: i32, l: i32) -> Result<(), String> {
        self.nav_link_mut(n, l)?;
        self.navmesh_mut(n)?.links[l as usize - 1] = None;
        self.replan_agents(n);
        Ok(())
    }

    /// The best route, using any open links. With no complete way, as far
    /// as the mesh goes towards the goal.
    fn plan(&self, n: i32, from: V3, to: V3) -> Result<Option<Route>, String> {
        let nav = self.navmesh(n)?;
        let open: Vec<(i32, NavLink)> = nav.links.iter().enumerate().filter_map(|(i, l)| Some((i as i32 + 1, l.clone().filter(|l| l.enabled)?))).collect();
        if let (false, Some((s, _)), Some((e, _))) = (open.is_empty(), nav.index.closest(from), nav.index.closest(to)) {
            if let Some(route) = route_over(s, e, &open, &mut |a, b| nav.index.path(a, b)) {
                return Ok(Some(route));
            }
        }
        Ok(nav.index.path(from, to).map(Route::walking))
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
                if let Some(hit) = crate::pick::pick(p, Vector3::new(0.0, -2.0 * top.abs().max(1000.0), 0.0), 0.0, targets.iter().map(|(e, s)| (*e, s))) {
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
        Ok(self.navmesh(n)?.index.closest(p).map(|(q, _)| q))
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
        let points = self.plan(n, from, to)?.unwrap_or_default().points;
        if points.is_empty() {
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
            planned_ago: 0.0,
            links: Vec::new(),
            crowd: None,
            velocity: V3::zeros(),
            stalled: 0.0,
            arc: 0.0,
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
        let mut positions: Vec<Option<Seen>> = Vec::with_capacity(n);
        for i in 0..n {
            let p = match &self.world.agents[i] {
                Some(a) if self.ent(a.entity).is_ok() && self.navmesh(a.navmesh).is_ok() => Some(Seen { pos: self.world_of(a.entity)?.position(), radius: a.radius, vel: a.velocity, crowd: a.crowd.is_some() }),
                _ => None,
            };
            positions.push(p);
        }
        for i in 0..n {
            if self.world.agents[i].as_ref().is_some_and(|a| self.elsewhere(a.entity)) {
                continue;
            }
            let Some(mut ag) = self.world.agents[i].take() else { continue };
            if self.ent(ag.entity).is_err() {
                // the entity was freed: so is its agent
                continue;
            }
            let origin = self.world_of(ag.entity)?.position();
            // the agent's feet, on the navigation mesh
            let lift = Vector3::new(0.0, ag.height, 0.0);
            // the ground under a jump, not the top of its arc
            let pos = origin - lift - Vector3::new(0.0, ag.arc, 0.0);
            ag.arc = 0.0;
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
                ag.planned_ago += dt;
                let moved = ag.planned_for.is_none_or(|p| (p - goal).norm() > ag.radius.max(0.25));
                // pushed off its line, a crowd agent needs a fresh route now and then
                let refresh = ag.crowd.is_some() && ag.planned_ago >= 1.0;
                let replan = ag.link_here() == 0 && (ag.path.is_empty() || ag.planned_for.is_none() || (moved && ag.planned_ago >= 0.25) || refresh);
                if replan {
                    if let Some(route) = self.plan(ag.navmesh, pos, goal)? {
                        ag.path = route.points;
                        ag.links = route.links;
                        ag.next = 1.min(ag.path.len().saturating_sub(1));
                    }
                    ag.planned_for = Some(goal);
                    ag.planned_ago = 0.0;
                }
                if let Some(horizon) = ag.crowd.filter(|_| ag.link_here() == 0 && ag.next < ag.path.len()) {
                    // steer around the others towards the next corner
                    let target = ag.path[ag.next];
                    let me = V2::new(new_pos.x, new_pos.z);
                    let to = V2::new(target.x, target.z) - me;
                    let d = to.norm();
                    let last = ag.next + 1 == ag.path.len();
                    let want = if last { ag.speed.min(d / dt.max(1e-4)) } else { ag.speed };
                    let desired = if d > 1e-5 { to / d * want } else { V2::zeros() };
                    let near = nearby(&positions, i, me, ag.radius, ag.speed, horizon);
                    let v = avoid_velocity(me, ag.radius, V2::new(ag.velocity.x, ag.velocity.z), desired, ag.speed, horizon, &near);
                    new_pos.x += v.x * dt;
                    new_pos.z += v.y * dt;
                    if d > 1e-5 {
                        new_pos.y += (target.y - new_pos.y) * (v.norm() * dt / d).min(1.0);
                    }
                    ag.stalled = if v.norm() < ag.speed * 0.2 { ag.stalled + dt } else { 0.0 };
                    let left = (V2::new(target.x, target.z) - V2::new(new_pos.x, new_pos.z)).norm();
                    // others standing on the goal: this is as close as it gets
                    let blocked = last && ag.stalled >= 1.0 && left <= ag.radius * 4.0;
                    if left <= (ag.speed * dt).max(ag.radius * 0.5) || blocked {
                        ag.next += 1;
                        ag.stalled = 0.0;
                    }
                } else {
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
                }
                // (sent somewhere else part way across a link: not there yet)
                if ag.next >= ag.path.len() && ag.planned_for.is_some() {
                    ag.arrived = ag.follow.is_none();
                    if ag.arrived {
                        ag.goal = None;
                    }
                }
            }
            let link = ag.link_here();
            // keep clear of the other agents (sideways only)
            for (j, other) in positions.iter().enumerate() {
                let Some(other) = other.filter(|_| link == 0) else { continue };
                if j == i {
                    continue;
                }
                let mut away = new_pos + lift - other.pos;
                away.y = 0.0;
                let d = away.norm();
                let min = ag.radius + other.radius;
                if d < min && d > 1e-4 {
                    new_pos += away / d * (min - d) * 0.5;
                }
            }
            if link != 0 {
                let (from, to) = (ag.path[ag.next - 1], ag.path[ag.next]);
                let whole = (to - from).norm();
                let t = if whole > 1e-5 { 1.0 - ((to - new_pos).norm() / whole).clamp(0.0, 1.0) } else { 1.0 };
                let jump = self.navmesh(ag.navmesh)?.links.get(link as usize - 1).and_then(Option::as_ref).map_or(0.0, |l| l.jump);
                ag.arc = 4.0 * jump * t * (1.0 - t);
            }
            let up = Vector3::new(0.0, ag.arc, 0.0);
            if (new_pos + lift + up - origin).norm_squared() > 1e-10 {
                // stay on the navigation mesh (a link is what leaves it)
                if link == 0 {
                    if let Ok(Some(q)) = self.navmesh_closest(ag.navmesh, new_pos) {
                        new_pos = q;
                    }
                }
                let moved = new_pos - pos;
                let e = ag.entity;
                self.set_xf(e, |x| x.pos = new_pos + lift + up)?;
                // turn towards where it is going, at most `turn` degrees a second
                if moved.x.abs() + moved.z.abs() > 1e-5 {
                    let want = crate::transform::vector_yaw(&Vector3::new(moved.x, 0.0, moved.z));
                    let (pitch, yaw, roll) = crate::transform::angles(&self.live_xf(self.ent(e)?).rot);
                    let mut diff = (want - yaw + 540.0).rem_euclid(360.0) - 180.0;
                    let max = ag.turn * dt;
                    diff = diff.clamp(-max, max);
                    self.set_xf(e, |x| x.rot = crate::transform::rotation(pitch, yaw + diff, roll))?;
                }
            }
            ag.velocity = if dt > 0.0 { (new_pos - pos) / dt } else { V3::zeros() };
            positions[i] = Some(Seen { pos: new_pos + lift + up, radius: ag.radius, vel: ag.velocity, crowd: ag.crowd.is_some() });
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
        ag.forget_route();
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
        ag.forget_route();
        Ok(())
    }

    /// `AgentCrowd`: turns looking ahead for other agents on or off.
    pub fn agent_crowd(&mut self, a: i32, on: bool, horizon: f32) -> Result<(), String> {
        self.agent_mut(a)?.crowd = on.then_some(horizon.max(0.1));
        Ok(())
    }

    pub fn agent_stop(&mut self, a: i32) -> Result<(), String> {
        let ag = self.agent_mut(a)?;
        ag.follow = None;
        ag.goal = None;
        ag.arrived = true;
        ag.path.clear();
        ag.links.clear();
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

    fn p(x: f32, z: f32) -> V3 {
        V3::new(x, 0.0, z)
    }

    fn link(a: V3, b: V3) -> NavLink {
        NavLink { a, b, two_way: true, cost: 1.0, jump: 0.0, enabled: true }
    }

    /// Two islands split at x = 5: walking is a straight line on one island.
    fn islands(a: V3, b: V3) -> Option<Vec<V3>> {
        ((a.x < 5.0) == (b.x < 5.0)).then(|| vec![a, b])
    }

    #[test]
    fn a_route_crosses_a_gap_by_a_link() {
        let links = [(7, link(p(4.0, 0.0), p(6.0, 0.0)))];
        let r = route_over(p(0.0, 0.0), p(10.0, 0.0), &links, &mut islands).unwrap();
        assert_eq!(r.points, vec![p(0.0, 0.0), p(4.0, 0.0), p(6.0, 0.0), p(10.0, 0.0)]);
        assert_eq!(r.links, vec![0, 0, 7, 0]);
    }

    #[test]
    fn no_link_means_no_way_across() {
        assert!(route_over(p(0.0, 0.0), p(10.0, 0.0), &[], &mut islands).is_none());
    }

    #[test]
    fn one_way_links_only_go_forwards() {
        let mut l = link(p(4.0, 0.0), p(6.0, 0.0));
        l.two_way = false;
        let links = [(1, l)];
        assert!(route_over(p(0.0, 0.0), p(10.0, 0.0), &links, &mut islands).is_some());
        assert!(route_over(p(10.0, 0.0), p(0.0, 0.0), &links, &mut islands).is_none());
    }

    #[test]
    fn two_way_links_also_go_backwards() {
        let links = [(1, link(p(4.0, 0.0), p(6.0, 0.0)))];
        let r = route_over(p(10.0, 0.0), p(0.0, 0.0), &links, &mut islands).unwrap();
        assert_eq!(r.points, vec![p(10.0, 0.0), p(6.0, 0.0), p(4.0, 0.0), p(0.0, 0.0)]);
    }

    #[test]
    fn a_costly_link_loses_to_a_longer_cheap_one() {
        let mut near = link(p(4.0, 0.0), p(6.0, 0.0));
        near.cost = 50.0;
        let far = link(p(4.0, 8.0), p(6.0, 8.0));
        let r = route_over(p(0.0, 0.0), p(10.0, 0.0), &[(1, near), (2, far)], &mut islands).unwrap();
        assert!(r.links.contains(&2) && !r.links.contains(&1));
    }

    #[test]
    fn a_link_is_only_taken_when_it_helps() {
        let links = [(1, link(p(1.0, 9.0), p(2.0, 9.0)))];
        let r = route_over(p(0.0, 0.0), p(3.0, 0.0), &links, &mut islands).unwrap();
        assert_eq!(r.links, vec![0, 0]);
    }

    #[test]
    fn links_chain_across_several_gaps() {
        // islands split at x = 5 and x = 15
        let mut three = |a: V3, b: V3| {
            let side = |v: V3| (v.x >= 5.0) as i32 + (v.x >= 15.0) as i32;
            (side(a) == side(b)).then(|| vec![a, b])
        };
        let links = [(1, link(p(14.0, 0.0), p(16.0, 0.0))), (2, link(p(4.0, 0.0), p(6.0, 0.0)))];
        let r = route_over(p(0.0, 0.0), p(20.0, 0.0), &links, &mut three).unwrap();
        assert_eq!(r.links, vec![0, 0, 2, 0, 1, 0]);
    }

    #[test]
    fn walks_that_stop_short_do_not_count() {
        let mut short = |a: V3, _b: V3| Some(vec![a, a + p(1.0, 0.0)]);
        assert!(route_over(p(0.0, 0.0), p(10.0, 0.0), &[], &mut short).is_none());
    }

    struct Walker {
        pos: V2,
        vel: V2,
        goal: V2,
    }

    /// Moves crowd agents to their goals; the closest any two ever came.
    fn simulate(w: &mut [Walker], radius: f32, speed: f32, seconds: f32) -> f32 {
        let dt = 1.0 / 60.0;
        let mut closest = f32::INFINITY;
        for _ in 0..(seconds / dt) as usize {
            let before: Vec<(V2, V2)> = w.iter().map(|a| (a.pos, a.vel)).collect();
            for (i, a) in w.iter_mut().enumerate() {
                let others: Vec<Nearby> = before.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, (pos, vel))| Nearby { pos: *pos, vel: *vel, radius, shares: true }).collect();
                let to = a.goal - a.pos;
                let want = speed.min(to.norm() / dt);
                let desired = if to.norm() > 1e-5 { to.normalize() * want } else { V2::zeros() };
                let v = avoid_velocity(a.pos, radius, a.vel, desired, speed, 2.0, &others);
                a.vel = v;
                a.pos += v * dt;
            }
            for i in 0..w.len() {
                for j in 0..i {
                    closest = closest.min((w[i].pos - w[j].pos).norm());
                }
            }
        }
        closest
    }

    fn walker(x: f32, z: f32, gx: f32, gz: f32) -> Walker {
        Walker { pos: V2::new(x, z), vel: V2::zeros(), goal: V2::new(gx, gz) }
    }

    fn all_arrived(w: &[Walker]) -> bool {
        w.iter().all(|a| (a.pos - a.goal).norm() < 0.3)
    }

    #[test]
    fn with_nobody_near_the_desired_velocity_stands() {
        let d = V2::new(2.0, 1.0);
        assert_eq!(avoid_velocity(V2::zeros(), 0.5, V2::zeros(), d, 3.0, 2.0, &[]), d);
    }

    #[test]
    fn two_agents_head_on_pass_without_touching() {
        let mut w = [walker(-8.0, 0.0, 8.0, 0.0), walker(8.0, 0.0, -8.0, 0.0)];
        let closest = simulate(&mut w, 0.5, 3.0, 12.0);
        assert!(closest >= 0.95, "came within {closest}");
        assert!(all_arrived(&w));
    }

    #[test]
    fn eight_agents_crossing_a_circle_stay_apart() {
        let mut w: Vec<Walker> = (0..8)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                walker(10.0 * a.cos(), 10.0 * a.sin(), -10.0 * a.cos(), -10.0 * a.sin())
            })
            .collect();
        let closest = simulate(&mut w, 0.5, 3.0, 25.0);
        assert!(closest >= 0.9, "came within {closest}");
        assert!(all_arrived(&w));
    }

    #[test]
    fn an_agent_walks_around_one_that_stands_still() {
        let still = Nearby { pos: V2::new(5.0, 0.0), vel: V2::zeros(), radius: 0.5, shares: false };
        let (mut pos, mut vel, goal, dt) = (V2::zeros(), V2::zeros(), V2::new(10.0, 0.0), 1.0 / 60.0);
        let mut closest = f32::INFINITY;
        for _ in 0..600 {
            let to: V2 = goal - pos;
            let desired = if to.norm() > 1e-4 { to.normalize() * 3.0f32.min(to.norm() / dt) } else { V2::zeros() };
            vel = avoid_velocity(pos, 0.5, vel, desired, 3.0, 2.0, &[still]);
            pos += vel * dt;
            closest = closest.min((pos - still.pos).norm());
        }
        assert!(closest >= 0.95, "came within {closest}");
        assert!((pos - goal).norm() < 0.3, "ended at {pos:?}");
    }

    #[test]
    fn touching_agents_may_still_move_apart() {
        let other = Nearby { pos: V2::new(0.5, 0.0), vel: V2::zeros(), radius: 0.5, shares: false };
        let v = avoid_velocity(V2::zeros(), 0.5, V2::zeros(), V2::new(-3.0, 0.0), 3.0, 2.0, &[other]);
        assert_eq!(v, V2::new(-3.0, 0.0));
    }

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
