//! Animation layers, blend spaces, state machines and root motion (ext),
//! on top of the `Animate` playback in `anim.rs`.
//!
//! * **Layers** (`AnimLayer`): extra sequences played at the same time as the
//!   one `Animate` started (the base), blended in by weight, on the whole
//!   body or only from a named bone down (an upper body shooting while the
//!   legs run). A layer on the whole body with weight 0.3 mixes 70/30.
//! * **Blend spaces** (`CreateBlendSpace`, `BlendSpacePoint`, `AnimBlend`):
//!   sequences placed at points on a line or a plane (speed, or speed and
//!   direction); the model plays the mix that matches its parameters. All
//!   sequences play at the same point of their cycle, so steps stay in step.
//! * **State machines** (`AddAnimState`, `AddBlendState`,
//!   `AddAnimTransition`, `SetAnimParam`, `StartAnimStates`): named states
//!   that play a sequence or a blend space, and transitions that switch
//!   between them with a crossfade when a parameter condition holds (or
//!   when a one-shot animation ends).
//! * **Root motion** (`AnimRootMotion`): the movement of a root bone in the
//!   animation moves the entity instead, so feet do not slide.

use std::collections::{HashMap, HashSet};

use fyrox::{
    core::{algebra::Vector3, pool::Handle},
    generic_animation::{
        value::{TrackValue, ValueBinding},
        AnimationPose,
    },
    scene::node::Node,
};

use fyrox::graph::SceneGraph;

use crate::world::Host3D;

pub type Pose = AnimationPose<Handle<Node>>;

/// Number of layers above the base.
pub const LAYERS: usize = 8;

#[derive(Debug, Clone)]
pub struct Layer {
    pub seq: usize,
    pub time: f32,
    pub speed: f32,
    pub mode: i32,
    pub dir: f32,
    pub weight: f32,
    /// Nodes the layer affects (`None` = all).
    pub mask: Option<HashSet<Handle<Node>>>,
}

/// A blend space playing on the base layer.
#[derive(Debug, Clone)]
pub struct BlendPlay {
    pub space: i32,
    pub x: f32,
    pub y: f32,
    /// Position in the cycle, 0 to 1.
    pub phase: f32,
    /// State machine parameters that set x and y (blend states).
    pub params: Option<(String, Option<String>)>,
}

#[derive(Debug, Clone, Default)]
pub struct BlendSpace {
    /// (x, y, sequence)
    pub points: Vec<(f32, f32, usize)>,
}

#[derive(Debug, Clone)]
pub enum StateKind {
    Seq { seq: usize, mode: i32, speed: f32 },
    Blend { space: i32, px: String, py: Option<String>, speed: f32 },
}

#[derive(Debug, Clone)]
pub struct State {
    pub name: String,
    pub kind: StateKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    Always,
    /// The state's animation has finished (one-shot).
    End,
    Compare { param: String, op: String, value: f32 },
}

#[derive(Debug, Clone)]
pub struct Transition {
    /// `None` = from any state.
    pub from: Option<usize>,
    pub to: usize,
    pub cond: Cond,
    pub duration: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Machine {
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub params: HashMap<String, f32>,
    pub current: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct RootMotion {
    pub bone: Handle<Node>,
    /// The bone's position at the start of the base sequence.
    pub rest: Option<Vector3<f32>>,
    /// Last (sequence, time, position) of the bone in the animation.
    pub prev: Option<(usize, f32, Vector3<f32>)>,
    /// Keep the up and down movement in the animation.
    pub keep_y: bool,
}

/// Everything beyond plain `Animate` for one entity.
#[derive(Debug, Clone, Default)]
pub struct Extra {
    pub layers: Vec<Option<Layer>>,
    pub blend: Option<BlendPlay>,
    pub machine: Option<Machine>,
    pub root: Option<RootMotion>,
}

impl Extra {
    pub fn active(&self) -> bool {
        self.layers.iter().any(Option::is_some) || self.blend.is_some() || self.machine.as_ref().is_some_and(|m| m.current.is_some())
    }
}

/// Weights of a blend space's points at (x, y). Points on a line (all the
/// same y) blend between the two neighbours; otherwise by inverse squared
/// distance. Weights add up to 1.
pub fn weights(points: &[(f32, f32, usize)], x: f32, y: f32) -> Vec<f32> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }
    let mut w = vec![0.0; n];
    let one_d = points.iter().all(|p| (p.1 - points[0].1).abs() < 1e-6);
    if one_d {
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| points[a].0.total_cmp(&points[b].0));
        if x <= points[idx[0]].0 {
            w[idx[0]] = 1.0;
        } else if x >= points[idx[n - 1]].0 {
            w[idx[n - 1]] = 1.0;
        } else {
            for p in idx.windows(2) {
                let (a, b) = (points[p[0]].0, points[p[1]].0);
                if x >= a && x <= b {
                    let t = if b > a { (x - a) / (b - a) } else { 0.0 };
                    w[p[0]] = 1.0 - t;
                    w[p[1]] += t;
                    break;
                }
            }
        }
        return w;
    }
    for (i, p) in points.iter().enumerate() {
        let d2 = (p.0 - x).powi(2) + (p.1 - y).powi(2);
        if d2 < 1e-10 {
            let mut exact = vec![0.0; n];
            exact[i] = 1.0;
            return exact;
        }
        w[i] = 1.0 / d2;
    }
    let sum: f32 = w.iter().sum();
    w.iter().map(|v| v / sum).collect()
}

/// Parses a transition condition: "" (always), "end", or `param op value`
/// with op one of `>`, `<`, `>=`, `<=`, `=`, `<>`.
pub fn parse_cond(s: &str) -> Result<Cond, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Cond::Always);
    }
    if s.eq_ignore_ascii_case("end") {
        return Ok(Cond::End);
    }
    for op in [">=", "<=", "<>", ">", "<", "="] {
        if let Some(i) = s.find(op) {
            let param = s[..i].trim().to_lowercase();
            let value = s[i + op.len()..].trim().parse::<f32>().map_err(|_| format!("'{s}': the value after {op} must be a number"))?;
            if param.is_empty() {
                break;
            }
            return Ok(Cond::Compare { param, op: op.to_string(), value });
        }
    }
    Err(format!("'{s}' is not a condition (use \"\", \"end\" or a comparison such as \"speed > 0.5\")"))
}

impl Cond {
    fn holds(&self, params: &HashMap<String, f32>, ended: bool) -> bool {
        match self {
            Cond::Always => true,
            Cond::End => ended,
            Cond::Compare { param, op, value } => {
                let v = params.get(param).copied().unwrap_or(0.0);
                match op.as_str() {
                    ">" => v > *value,
                    "<" => v < *value,
                    ">=" => v >= *value,
                    "<=" => v <= *value,
                    "=" => (v - value).abs() < 1e-6,
                    _ => (v - value).abs() >= 1e-6,
                }
            }
        }
    }
}

impl Machine {
    /// The transition to take now, if any.
    pub fn next(&self, ended: bool) -> Option<&Transition> {
        let cur = self.current?;
        self.transitions.iter().find(|t| t.from.is_none_or(|f| f == cur) && t.to != cur && t.cond.holds(&self.params, ended))
    }

    pub fn state(&self, name: &str) -> Option<usize> {
        self.states.iter().position(|s| s.name.eq_ignore_ascii_case(name))
    }
}

/// Position of a node in a pose, if the pose moves it.
pub fn pose_position(pose: &Pose, node: Handle<Node>) -> Option<Vector3<f32>> {
    pose.poses().get(&node)?.values.values.iter().find_map(|v| match (&v.binding, &v.value) {
        (ValueBinding::Position, TrackValue::Vector3(p)) => Some(*p),
        _ => None,
    })
}

pub fn set_pose_position(pose: &mut Pose, node: Handle<Node>, p: Vector3<f32>) {
    if let Some(np) = pose.poses_mut().get_mut(&node) {
        for v in np.values.values.iter_mut() {
            if let (ValueBinding::Position, TrackValue::Vector3(q)) = (&v.binding, &mut v.value) {
                *q = p;
            }
        }
    }
}

/// Blends `other` into `base` by `weight`, only for nodes in `mask`.
pub fn blend_masked(base: &mut Pose, other: &Pose, weight: f32, mask: Option<&HashSet<Handle<Node>>>) {
    match mask {
        None => base.blend_with(other, weight),
        Some(m) => {
            for (node, np) in other.poses() {
                if m.contains(node) {
                    if let Some(b) = base.poses_mut().get_mut(node) {
                        b.blend_with(np, weight);
                    }
                }
            }
        }
    }
}

/// Weighted average of poses (weights sum to 1).
pub fn mix(poses: Vec<(Pose, f32)>) -> Option<Pose> {
    let mut it = poses.into_iter().filter(|(_, w)| *w > 1e-4);
    let (mut acc, mut total) = it.next()?;
    for (p, w) in it {
        total += w;
        acc.blend_with(&p, w / total);
    }
    Some(acc)
}

impl Host3D<'_> {
    pub fn blend_space(&self, s: i32) -> Result<&BlendSpace, String> {
        usize::try_from(s - 1)
            .ok()
            .and_then(|i| self.world.blend_spaces.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Blend space {s} does not exist"))
    }

    /// The node named `name` inside an entity's model, and everything under it.
    pub fn bone_set(&mut self, e: i32, name: &str) -> Result<HashSet<Handle<Node>>, String> {
        let root = self.ent(e)?.node;
        let graph = self.graph()?;
        let start = graph
            .traverse_handle_iter(root)
            .find(|&n| graph[n].name().eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("Entity {e} has no bone named '{name}'"))?;
        Ok(graph.traverse_handle_iter(start).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_d_weights() {
        let pts = [(0.0, 0.0, 0), (1.0, 0.0, 1), (3.0, 0.0, 2)];
        assert_eq!(weights(&pts, -1.0, 0.0), vec![1.0, 0.0, 0.0]);
        assert_eq!(weights(&pts, 0.5, 0.0), vec![0.5, 0.5, 0.0]);
        assert_eq!(weights(&pts, 2.0, 0.0), vec![0.0, 0.5, 0.5]);
        assert_eq!(weights(&pts, 9.0, 0.0), vec![0.0, 0.0, 1.0]);
    }

    #[test]
    fn two_d_weights() {
        let pts = [(0.0, 0.0, 0), (1.0, 0.0, 1), (0.0, 1.0, 2)];
        assert_eq!(weights(&pts, 1.0, 0.0), vec![0.0, 1.0, 0.0]);
        let w = weights(&pts, 0.6, 0.6);
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert!((w[1] - w[2]).abs() < 1e-5 && w[1] > w[0]);
    }

    #[test]
    fn conditions_and_transitions() {
        assert_eq!(parse_cond("").unwrap(), Cond::Always);
        assert_eq!(parse_cond("END").unwrap(), Cond::End);
        assert_eq!(parse_cond(" Speed >= 2.5 ").unwrap(), Cond::Compare { param: "speed".into(), op: ">=".into(), value: 2.5 });
        assert!(parse_cond("speed > fast").is_err());
        let mut m = Machine::default();
        for n in ["idle", "run", "jump"] {
            m.states.push(State { name: n.into(), kind: StateKind::Seq { seq: 0, mode: 1, speed: 1.0 } });
        }
        m.transitions.push(Transition { from: Some(0), to: 1, cond: parse_cond("speed > 0.5").unwrap(), duration: 5.0 });
        m.transitions.push(Transition { from: None, to: 2, cond: parse_cond("jump = 1").unwrap(), duration: 5.0 });
        m.transitions.push(Transition { from: Some(2), to: 0, cond: Cond::End, duration: 5.0 });
        m.current = Some(0);
        assert!(m.next(false).is_none());
        m.params.insert("speed".into(), 1.0);
        assert_eq!(m.next(false).unwrap().to, 1);
        m.current = Some(1);
        m.params.insert("jump".into(), 1.0);
        assert_eq!(m.next(false).unwrap().to, 2);
        m.current = Some(2);
        m.params.insert("jump".into(), 0.0);
        assert!(m.next(false).is_none());
        assert_eq!(m.next(true).unwrap().to, 0);
    }
}
