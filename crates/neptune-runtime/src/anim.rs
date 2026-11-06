//! Animation playback for loaded models (`Animate`, `SetAnimTime`, ...) and
//! access to a model's inner nodes (`FindChild`, `GetChild`).
//!
//! Each animation in a model file is one Blitz *sequence* (sequence 0 is the
//! first). `ExtractAnimSeq` adds sequences that play a frame range of an
//! existing one. Time is counted in frames of 1/60 s, so `Animate` speed 1
//! plays in real time when `UpdateWorld` runs at 60 Hz; like Blitz, the
//! animation only advances in `UpdateWorld`.
//!
//! Fyrox's animation player runs with speed 0: NeptuneBASIC sets the time
//! position itself and Fyrox only computes and applies the pose.

use fyrox::{
    core::{
        algebra::{Matrix3, Vector3},
        pool::Handle,
    },
    generic_animation::Animation,
    scene::{animation::AnimationPlayer, graph::Graph, node::Node},
};

use crate::transform::Xform;
use crate::world::{Host3D, Kind};

/// Animation frames per second.
pub const FPS: f32 = 60.0;

type AnimHandle = Handle<Animation<Handle<Node>>>;

/// Playback state of an animated entity.
#[derive(Clone, Debug)]
pub struct AnimState {
    pub player: Handle<Node>,
    /// Sequence index → animation in the player.
    pub seqs: Vec<AnimHandle>,
    /// 0 stopped, 1 loop, 2 ping-pong, 3 one-shot.
    pub mode: i32,
    pub speed: f32,
    pub seq: usize,
    /// Current time in frames, relative to the start of the sequence.
    pub time: f32,
    /// Ping-pong direction.
    pub dir: f32,
    /// Blend from the previous pose (`transition` frames), remaining frames.
    pub transition: f32,
}

/// Finds the first animation player in a subtree.
pub fn find_player(graph: &Graph, root: Handle<Node>) -> Option<Handle<Node>> {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if graph[n].cast::<AnimationPlayer>().is_some() {
            return Some(n);
        }
        stack.extend(graph[n].children().iter().copied());
    }
    None
}

/// Prepares a player for manual control and lists its animations.
pub fn take_control(graph: &mut Graph, player: Handle<Node>) -> Vec<AnimHandle> {
    let Some(p) = graph[player].cast_mut::<AnimationPlayer>() else { return Vec::new() };
    let anims = p.animations_mut().get_value_mut_silent();
    let handles: Vec<AnimHandle> = anims.pair_iter().map(|(h, _)| h).collect();
    for a in anims.iter_mut() {
        a.set_speed(0.0);
        a.set_enabled(false);
        a.set_loop(true);
    }
    handles
}

/// Blitz-space local transform of a node.
pub fn xform_of_node(graph: &Graph, node: Handle<Node>) -> Xform {
    let t = graph[node].local_transform();
    let p = **t.position();
    let s = Matrix3::from_diagonal(&Vector3::new(-1.0, 1.0, 1.0));
    let rf = t.rotation().to_rotation_matrix().into_inner();
    Xform { pos: Vector3::new(-p.x, p.y, p.z), rot: s * rf * s, scale: **t.scale() }
}

impl Host3D<'_> {
    fn anim_mut(&mut self, e: i32) -> Result<&mut AnimState, String> {
        self.ent_mut(e)?.anim.as_mut().ok_or_else(|| format!("Entity {e} has no animation"))
    }

    fn with_animation<R>(&mut self, player: Handle<Node>, a: AnimHandle, f: impl FnOnce(&mut Animation<Handle<Node>>) -> R) -> Result<R, String> {
        let graph = self.graph()?;
        let p = graph[player].cast_mut::<AnimationPlayer>().ok_or("animation player missing")?;
        let anims = p.animations_mut().get_value_mut_silent();
        let anim = anims.try_get_mut(a).map_err(|_| "animation sequence missing".to_string())?;
        Ok(f(anim))
    }

    /// Length of a sequence in frames.
    pub fn seq_length(&mut self, e: i32, seq: usize) -> Result<f32, String> {
        let st = self.anim_mut(e)?.clone();
        let a = *st.seqs.get(seq).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?;
        self.with_animation(st.player, a, |anim| anim.length() * FPS)
    }

    /// Shows sequence `seq` at `time` frames.
    fn pose(&mut self, e: i32, seq: usize, time: f32) -> Result<(), String> {
        let st = self.anim_mut(e)?.clone();
        for (i, &a) in st.seqs.iter().enumerate() {
            let on = i == seq;
            self.with_animation(st.player, a, |anim| {
                anim.set_enabled(on);
                if on {
                    let start = anim.time_slice().start;
                    anim.set_time_position(start + time / FPS);
                }
            })?;
        }
        Ok(())
    }

    /// `Animate`.
    pub fn animate(&mut self, e: i32, mode: i32, speed: f32, seq: i32, transition: f32) -> Result<(), String> {
        let n = self.anim_mut(e)?.seqs.len();
        let seq = usize::try_from(seq).ok().filter(|&s| s < n).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?;
        let st = self.anim_mut(e)?;
        let restart = st.seq != seq || st.mode == 0;
        st.mode = mode.clamp(0, 3);
        st.speed = speed;
        st.seq = seq;
        st.transition = transition.max(0.0);
        if restart {
            st.time = if speed < 0.0 { f32::MAX } else { 0.0 };
            st.dir = 1.0;
        }
        let time = st.time;
        let len = self.seq_length(e, seq)?;
        let time = time.min(len);
        self.anim_mut(e)?.time = time;
        self.pose(e, seq, time)
    }

    /// `SetAnimTime`: stops playback and shows a frame.
    pub fn set_anim_time(&mut self, e: i32, time: f32, seq: i32) -> Result<(), String> {
        let n = self.anim_mut(e)?.seqs.len();
        let seq = usize::try_from(seq).ok().filter(|&s| s < n).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?;
        let st = self.anim_mut(e)?;
        st.mode = 0;
        st.seq = seq;
        st.time = time.max(0.0);
        self.pose(e, seq, time.max(0.0))
    }

    /// Advances every playing animation by `frames` (called by `UpdateWorld`).
    pub fn update_animations(&mut self, frames: f32) -> Result<(), String> {
        let animated: Vec<i32> = self.entities().filter(|(_, e)| e.anim.as_ref().is_some_and(|a| a.mode != 0)).map(|(i, _)| i).collect();
        for e in animated {
            let (seq, mut time, speed, mode, mut dir) = {
                let a = self.anim_mut(e)?;
                (a.seq, a.time, a.speed, a.mode, a.dir)
            };
            let len = self.seq_length(e, seq)?.max(1e-3);
            time += speed * dir * frames;
            let mut stop = false;
            match mode {
                1 => time = time.rem_euclid(len),
                2 => {
                    // ping-pong: bounce between the ends
                    while !(0.0..=len).contains(&time) {
                        time = if time > len { 2.0 * len - time } else { -time };
                        dir = -dir;
                    }
                }
                _ => {
                    if !(0.0..=len).contains(&time) {
                        time = time.clamp(0.0, len);
                        stop = true;
                    }
                }
            }
            {
                let a = self.anim_mut(e)?;
                a.time = time;
                a.dir = dir;
                if stop {
                    a.mode = 0;
                }
            }
            self.pose(e, seq, time)?;
        }
        Ok(())
    }

    /// `ExtractAnimSeq`: a new sequence playing frames `first..=last` of `seq`.
    pub fn extract_seq(&mut self, e: i32, first: f32, last: f32, seq: i32) -> Result<i32, String> {
        let st = self.anim_mut(e)?.clone();
        let src = *usize::try_from(seq).ok().and_then(|s| st.seqs.get(s)).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?;
        let graph = self.graph()?;
        let p = graph[st.player].cast_mut::<AnimationPlayer>().ok_or("animation player missing")?;
        let anims = p.animations_mut().get_value_mut_silent();
        let mut copy = anims.try_get(src).map_err(|_| "animation sequence missing".to_string())?.clone();
        let base = copy.time_slice().start;
        let (a, b) = (first.min(last), first.max(last));
        copy.set_time_slice(base + a / FPS..base + b / FPS);
        copy.set_enabled(false);
        let h = anims.add(copy);
        let st = self.anim_mut(e)?;
        st.seqs.push(h);
        Ok(st.seqs.len() as i32 - 1)
    }

    /// Entity for a node inside a model, registering it (and any unregistered
    /// ancestors) on first use so the entity tree mirrors the node tree.
    pub fn adopt_node(&mut self, node: Handle<Node>) -> Result<i32, String> {
        if let Some((id, _)) = self.entities().find(|(_, e)| e.node == node) {
            return Ok(id);
        }
        let parent_node = self.graph()?[node].parent();
        if parent_node.is_none() {
            return Err("node is not inside an entity".into());
        }
        let parent = self.adopt_node(parent_node)?;
        let id = self.register_child_node(node, parent)?;
        Ok(id)
    }

    /// Registers a node that is already linked under `parent`'s node.
    fn register_child_node(&mut self, node: Handle<Node>, parent: i32) -> Result<i32, String> {
        let xf = xform_of_node(self.graph()?, node);
        let name = self.graph()?[node].name().to_string();
        let id = self.attach_entity_linked(node, Kind::Pivot, parent)?;
        let e = self.ent_mut(id)?;
        e.xf = xf;
        e.name = name;
        e.adopted = true;
        e.own_materials = false;
        Ok(id)
    }

    /// `FindChild`: searches the entity's node tree by name (case-insensitive).
    pub fn find_child(&mut self, e: i32, name: &str) -> Result<i32, String> {
        let root = self.ent(e)?.node;
        let found = {
            let graph = self.graph()?;
            let mut stack: Vec<Handle<Node>> = graph[root].children().to_vec();
            let mut found = None;
            while let Some(n) = stack.pop() {
                if graph[n].name().eq_ignore_ascii_case(name) {
                    found = Some(n);
                    break;
                }
                stack.extend(graph[n].children().iter().rev().copied());
            }
            found
        };
        match found {
            Some(n) => self.adopt_node(n),
            None => Ok(0),
        }
    }

    /// Children of a model entity, including nodes inside the model.
    pub fn node_children(&mut self, e: i32) -> Result<Vec<i32>, String> {
        let ent = self.ent(e)?;
        if !(ent.model || ent.adopted) {
            return Ok(ent.children.iter().map(|&c| c as i32).collect());
        }
        let node = ent.node;
        let kids: Vec<Handle<Node>> = self
            .graph()?[node]
            .children()
            .iter()
            .copied()
            .filter(|_| true)
            .collect();
        let mut out = Vec::new();
        for k in kids {
            // skip Fyrox helper nodes such as the animation player
            if self.graph()?[k].cast::<AnimationPlayer>().is_some() {
                continue;
            }
            out.push(self.adopt_node(k)?);
        }
        Ok(out)
    }
}
