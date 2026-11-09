//! Animation playback for loaded models (`Animate`, `SetAnimTime`, ...) and
//! access to a model's inner nodes (`FindChild`, `GetChild`).
//!
//! Each animation in a model file is one Blitz *sequence* (sequence 0 is the
//! first). `ExtractAnimSeq` adds sequences that play a frame range of an
//! existing one. Time is counted in frames of 1/60 s, so `Animate` speed 1
//! plays in real time when `UpdateWorld` runs at 60 Hz; like Blitz, the
//! animation only advances in `UpdateWorld`.
//!
//! Fyrox's animation player runs with speed 0 and without applying poses by
//! itself: NeptuneBASIC sets the time position, asks Fyrox for the pose and
//! applies it. That is what makes `Animate`'s `transition` work: for that
//! many frames the new sequence's pose is blended from the last pose shown.

use fyrox::{
    core::{
        algebra::{Matrix3, Vector3},
        pool::Handle,
    },
    generic_animation::{Animation, AnimationPose},
    scene::{
        animation::{AnimationPlayer, AnimationPoseExt},
        graph::Graph,
        node::Node,
    },
};

use fyrox::graph::SceneGraph;

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
    /// Transition frames still to blend, out of `blend_total`.
    pub transition: f32,
    pub blend_total: f32,
    /// Pose shown when the transition started.
    pub from_pose: Option<AnimationPose<Handle<Node>>>,
    /// Pose shown last (the start of the next transition).
    pub last_pose: Option<AnimationPose<Handle<Node>>>,
    /// Keyframe sequences made with `AddAnimSeq`/`SetAnimKey` (no Fyrox player).
    pub keys: Option<crate::keyanim::KeyAnim>,
    /// (ext) Layers, blend space, state machine and root motion.
    pub extra: crate::animblend::Extra,
}

impl AnimState {
    pub fn new(player: Handle<Node>, seqs: Vec<AnimHandle>) -> Self {
        AnimState {
            player,
            seqs,
            mode: 0,
            speed: 1.0,
            seq: 0,
            time: 0.0,
            dir: 1.0,
            transition: 0.0,
            blend_total: 0.0,
            from_pose: None,
            last_pose: None,
            keys: None,
            extra: Default::default(),
        }
    }

    /// Weight of the old pose during a transition (1 at the start, 0 at the end).
    pub fn blend_weight(&self) -> f32 {
        if self.from_pose.is_some() && self.blend_total > 0.0 {
            (self.transition / self.blend_total).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Moves an animation time on by `frames`: mode 1 loops, 2 ping-pongs, 3
/// stops at the end. Returns the new time and direction, and whether it stopped.
pub fn advance(time: f32, dir: f32, speed: f32, frames: f32, len: f32, mode: i32) -> (f32, f32, bool) {
    let (mut time, mut dir) = (time + speed * dir * frames, dir);
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
    (time, dir, stop)
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
    p.set_auto_apply(false);
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
    /// Number of sequences an entity can play.
    pub fn seq_count(&mut self, e: i32) -> Result<usize, String> {
        let st = self.anim_mut(e)?;
        Ok(st.keys.as_ref().map_or(st.seqs.len(), |k| k.seqs.len()))
    }

    pub fn seq_length(&mut self, e: i32, seq: usize) -> Result<f32, String> {
        let st = self.anim_mut(e)?;
        if let Some(k) = &st.keys {
            return k.seqs.get(seq).map(|s| s.length).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"));
        }
        let (player, a) = (st.player, *st.seqs.get(seq).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?);
        self.with_animation(player, a, |anim| anim.length() * FPS)
    }

    /// Shows sequence `seq` at `time` frames, blended with the transition's
    /// starting pose while a transition is running.
    fn pose(&mut self, e: i32, seq: usize, time: f32) -> Result<(), String> {
        if let Some(seq_data) = self.anim_mut(e)?.keys.as_ref().and_then(|k| k.seqs.get(seq)).cloned() {
            let mut xf = self.live_xf(self.ent(e)?);
            seq_data.sample(time, &mut xf);
            return self.set_xf(e, |x| *x = xf);
        }
        let (weight, from, blend, layers) = {
            let st = self.anim_mut(e)?;
            let w = st.blend_weight();
            (w, if w > 0.0 { st.from_pose.clone() } else { None }, st.extra.blend.clone(), st.extra.layers.clone())
        };
        // the base: one sequence, or the mix of a blend space
        let (mut pose, cycle) = match &blend {
            Some(bp) => (self.blend_pose(e, bp)?, (usize::MAX, bp.phase)),
            None => (self.sample(e, seq, time)?, (seq, time)),
        };
        if let Some(from) = from {
            // lerp each bone from the new pose towards the old one by `weight`
            pose.blend_with(&from, weight);
        }
        self.apply_root_motion(e, &mut pose, cycle)?;
        // layers on top, each by its weight and on its part of the body
        for layer in layers.iter().flatten().filter(|l| l.weight > 0.0) {
            let lp = self.sample(e, layer.seq, layer.time)?;
            crate::animblend::blend_masked(&mut pose, &lp, layer.weight.min(1.0), layer.mask.as_ref());
        }
        pose.apply(self.graph()?);
        self.anim_mut(e)?.last_pose = Some(pose);
        Ok(())
    }

    /// The pose of sequence `seq` at `time` frames.
    pub fn sample(&mut self, e: i32, seq: usize, time: f32) -> Result<crate::animblend::Pose, String> {
        let st = self.anim_mut(e)?;
        let (player, a) = (st.player, *st.seqs.get(seq).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?);
        self.with_animation(player, a, |anim| {
            let start = anim.time_slice().start;
            anim.set_time_position(start + time / FPS);
            // speed is 0, so this only computes the pose for the current time
            anim.tick(0.0);
            anim.pose().clone()
        })
    }

    /// The mix of a blend space's sequences, all at the same point of their cycle.
    fn blend_pose(&mut self, e: i32, bp: &crate::animblend::BlendPlay) -> Result<crate::animblend::Pose, String> {
        let points = self.blend_space(bp.space)?.points.clone();
        let w = crate::animblend::weights(&points, bp.x, bp.y);
        let mut poses = Vec::new();
        for (p, weight) in points.iter().zip(w) {
            if weight > 1e-4 {
                let len = self.seq_length(e, p.2)?;
                poses.push((self.sample(e, p.2, bp.phase * len)?, weight));
            }
        }
        crate::animblend::mix(poses).ok_or_else(|| format!("Blend space {} has no sequences", bp.space))
    }

    /// Length of one cycle of a blend space at its parameters (weighted).
    pub fn blend_length(&mut self, e: i32, bp: &crate::animblend::BlendPlay) -> Result<f32, String> {
        let points = self.blend_space(bp.space)?.points.clone();
        let w = crate::animblend::weights(&points, bp.x, bp.y);
        let mut len = 0.0;
        for (p, weight) in points.iter().zip(w) {
            if weight > 0.0 {
                len += weight * self.seq_length(e, p.2)?;
            }
        }
        Ok(len.max(1e-3))
    }

    /// Root motion: the root bone's movement since the last pose moves the
    /// entity, and the bone stays where it started.
    fn apply_root_motion(&mut self, e: i32, pose: &mut crate::animblend::Pose, cycle: (usize, f32)) -> Result<(), String> {
        let Some(mut rm) = self.anim_mut(e)?.extra.root.clone() else { return Ok(()) };
        if rm.bone.is_none() {
            // the topmost moving bone of the model
            let root = self.ent(e)?.node;
            let graph = self.graph()?;
            let moving: Vec<Handle<Node>> = pose.poses().keys().copied().filter(|n| crate::animblend::pose_position(pose, *n).is_some()).collect();
            let depth = |mut n: Handle<Node>| {
                let mut d = 0;
                while n.is_some() && n != root {
                    n = graph[n].parent();
                    d += 1;
                }
                d
            };
            match moving.into_iter().min_by_key(|n| depth(*n)) {
                Some(b) => rm.bone = b,
                None => return Ok(()),
            }
        }
        let Some(p) = crate::animblend::pose_position(pose, rm.bone) else { return Ok(()) };
        let rest = *rm.rest.get_or_insert(p);
        let mut delta = Vector3::zeros();
        if let Some((pseq, ptime, pp)) = rm.prev {
            if pseq == cycle.0 && cycle.1 >= ptime {
                delta = p - pp;
            } else if pseq == cycle.0 && cycle.0 != usize::MAX {
                // the cycle wrapped: to the end, then from the start
                let len = self.seq_length(e, cycle.0)?;
                let end = crate::animblend::pose_position(&self.sample(e, cycle.0, len)?, rm.bone).unwrap_or(pp);
                let start = crate::animblend::pose_position(&self.sample(e, cycle.0, 0.0)?, rm.bone).unwrap_or(p);
                delta = (end - pp) + (p - start);
            }
        }
        rm.prev = Some((cycle.0, cycle.1, p));
        let keep_y = rm.keep_y;
        let bone = rm.bone;
        self.anim_mut(e)?.extra.root = Some(rm);
        crate::animblend::set_pose_position(pose, bone, Vector3::new(rest.x, if keep_y { p.y } else { rest.y }, rest.z));
        if keep_y {
            delta.y = 0.0;
        }
        if delta.norm_squared() > 0.0 {
            // bone space -> world (Fyrox) -> Blitz
            let graph = self.graph()?;
            let parent = graph[bone].parent();
            let m = if parent.is_some() { graph[parent].global_transform() } else { fyrox::core::algebra::Matrix4::identity() };
            let w = m.transform_vector(&delta);
            self.set_xf(e, |x| x.pos += Vector3::new(-w.x, w.y, w.z))?;
        }
        Ok(())
    }

    /// `Animate`.
    pub fn animate(&mut self, e: i32, mode: i32, speed: f32, seq: i32, transition: f32) -> Result<(), String> {
        let n = self.seq_count(e)?;
        let seq = usize::try_from(seq).ok().filter(|&s| s < n).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?;
        let st = self.anim_mut(e)?;
        let restart = st.seq != seq || st.mode == 0 || st.extra.blend.is_some();
        st.extra.blend = None;
        st.mode = mode.clamp(0, 3);
        st.speed = speed;
        if restart && transition > 0.0 && st.last_pose.is_some() {
            st.from_pose = st.last_pose.clone();
            st.transition = transition;
            st.blend_total = transition;
        } else if restart {
            st.from_pose = None;
            st.transition = 0.0;
        }
        st.seq = seq;
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
        let n = self.seq_count(e)?;
        let seq = usize::try_from(seq).ok().filter(|&s| s < n).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?;
        let st = self.anim_mut(e)?;
        st.mode = 0;
        st.seq = seq;
        st.time = time.max(0.0);
        st.from_pose = None;
        st.transition = 0.0;
        self.pose(e, seq, time.max(0.0))
    }

    /// Advances every playing animation by `frames` (called by `UpdateWorld`).
    pub fn update_animations(&mut self, frames: f32) -> Result<(), String> {
        let animated: Vec<i32> = self.entities().filter(|(_, e)| e.anim.as_ref().is_some_and(|a| a.mode != 0 || a.extra.active())).map(|(i, _)| i).collect();
        for e in animated {
            if self.anim_mut(e)?.keys.is_none() {
                self.step_machine(e)?;
            }
            let (seq, mut time, speed, mode, mut dir, blend) = {
                let a = self.anim_mut(e)?;
                (a.seq, a.time, a.speed, a.mode, a.dir, a.extra.blend.clone())
            };
            let mut stop = false;
            if let Some(mut bp) = blend {
                // a blend state follows its state machine parameters
                if let Some((px, py)) = bp.params.clone() {
                    if let Some(m) = &self.anim_mut(e)?.extra.machine {
                        bp.x = m.params.get(&px).copied().unwrap_or(0.0);
                        bp.y = py.and_then(|py| m.params.get(&py).copied()).unwrap_or(0.0);
                    }
                }
                if mode != 0 {
                    let len = self.blend_length(e, &bp)?;
                    bp.phase = (bp.phase + speed * frames / len).rem_euclid(1.0);
                }
                self.anim_mut(e)?.extra.blend = Some(bp);
            } else if mode != 0 {
                let len = self.seq_length(e, seq)?.max(1e-3);
                (time, dir, stop) = advance(time, dir, speed, frames, len, mode);
            }
            {
                let a = self.anim_mut(e)?;
                a.time = time;
                a.dir = dir;
                if a.transition > 0.0 {
                    a.transition = (a.transition - frames.abs()).max(0.0);
                    if a.transition == 0.0 {
                        a.from_pose = None;
                    }
                }
                if stop {
                    a.mode = 0;
                }
            }
            // layers keep their own time
            let mut layers = self.anim_mut(e)?.extra.layers.clone();
            for l in layers.iter_mut().flatten().filter(|l| l.mode != 0) {
                let len = self.seq_length(e, l.seq)?.max(1e-3);
                let (t, d, s) = advance(l.time, l.dir, l.speed, frames, len, l.mode);
                (l.time, l.dir) = (t, d);
                if s {
                    l.mode = 0;
                }
            }
            self.anim_mut(e)?.extra.layers = layers;
            self.pose(e, seq, time)?;
        }
        Ok(())
    }

    /// Takes the state machine's first transition whose condition holds.
    fn step_machine(&mut self, e: i32) -> Result<(), String> {
        let next = {
            let a = self.anim_mut(e)?;
            let ended = a.mode == 0 && a.extra.blend.is_none();
            a.extra.machine.as_ref().and_then(|m| m.next(ended).cloned())
        };
        if let Some(t) = next {
            self.enter_state(e, t.to, t.duration)?;
        }
        Ok(())
    }

    /// Switches the state machine to state `idx`, crossfading over `transition` frames.
    pub fn enter_state(&mut self, e: i32, idx: usize, transition: f32) -> Result<(), String> {
        let state = {
            let m = self.anim_mut(e)?.extra.machine.as_mut().ok_or_else(|| format!("Entity {e} has no animation states"))?;
            m.current = Some(idx);
            m.states.get(idx).cloned().ok_or("animation state missing")?
        };
        match state.kind {
            crate::animblend::StateKind::Seq { seq, mode, speed } => {
                // a new state always starts its animation from the beginning
                self.anim_mut(e)?.mode = 0;
                self.animate(e, mode, speed, seq as i32, transition)
            }
            crate::animblend::StateKind::Blend { space, px, py, speed } => self.start_blend(e, space, 0.0, 0.0, speed, transition, Some((px, py))),
        }
    }

    /// Plays a blend space on the base layer (`AnimBlend`, blend states).
    #[allow(clippy::too_many_arguments)]
    pub fn start_blend(&mut self, e: i32, space: i32, x: f32, y: f32, speed: f32, transition: f32, params: Option<(String, Option<String>)>) -> Result<(), String> {
        self.blend_space(space)?;
        let st = self.anim_mut(e)?;
        if st.keys.is_some() {
            return Err(format!("Entity {e} is not an animated model"));
        }
        let same = st.extra.blend.as_ref().is_some_and(|b| b.space == space) && st.mode != 0;
        if same {
            // already playing: only the parameters change
            let b = st.extra.blend.as_mut().unwrap();
            b.x = x;
            b.y = y;
            st.speed = speed;
            return Ok(());
        }
        if transition > 0.0 && st.last_pose.is_some() {
            st.from_pose = st.last_pose.clone();
            st.transition = transition;
            st.blend_total = transition;
        } else {
            st.from_pose = None;
            st.transition = 0.0;
        }
        st.mode = 1;
        st.speed = speed;
        st.extra.blend = Some(crate::animblend::BlendPlay { space, x, y, phase: 0.0, params });
        let (seq, time) = (st.seq, st.time);
        self.pose(e, seq, time)
    }

    /// `AnimLayer`: plays `seq` on `layer` (1-8) on top of the base, on the
    /// bones from `bone` down ("" = the whole model).
    #[allow(clippy::too_many_arguments)]
    pub fn anim_layer(&mut self, e: i32, layer: i32, seq: i32, mode: i32, speed: f32, weight: f32, bone: &str) -> Result<(), String> {
        let n = self.seq_count(e)?;
        if self.anim_mut(e)?.keys.is_some() {
            return Err(format!("Entity {e} is not an animated model"));
        }
        let seq = usize::try_from(seq).ok().filter(|&s| s < n).ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?;
        let li = usize::try_from(layer - 1).ok().filter(|&l| l < crate::animblend::LAYERS).ok_or_else(|| format!("layer must be 1 to {}, not {layer}", crate::animblend::LAYERS))?;
        let mask = if bone.is_empty() { None } else { Some(self.bone_set(e, bone)?) };
        let time = if speed < 0.0 { self.seq_length(e, seq)? } else { 0.0 };
        let layers = &mut self.anim_mut(e)?.extra.layers;
        if layers.len() <= li {
            layers.resize(li + 1, None);
        }
        layers[li] = Some(crate::animblend::Layer { seq, time, speed, mode: mode.clamp(0, 3), dir: 1.0, weight: weight.clamp(0.0, 1.0), mask });
        Ok(())
    }

    fn layer_mut(&mut self, e: i32, layer: i32) -> Result<&mut crate::animblend::Layer, String> {
        let li = usize::try_from(layer - 1).unwrap_or(usize::MAX);
        self.anim_mut(e)?.extra.layers.get_mut(li).and_then(Option::as_mut).ok_or_else(|| format!("Entity {e} has no animation layer {layer}"))
    }

    pub fn anim_layer_weight(&mut self, e: i32, layer: i32, weight: f32) -> Result<(), String> {
        self.layer_mut(e, layer)?.weight = weight.clamp(0.0, 1.0);
        Ok(())
    }

    pub fn anim_layer_stop(&mut self, e: i32, layer: i32) -> Result<(), String> {
        self.layer_mut(e, layer)?;
        self.anim_mut(e)?.extra.layers[layer as usize - 1] = None;
        Ok(())
    }

    fn machine_mut(&mut self, e: i32) -> Result<&mut crate::animblend::Machine, String> {
        let st = self.anim_mut(e)?;
        if st.keys.is_some() {
            return Err(format!("Entity {e} is not an animated model"));
        }
        Ok(st.extra.machine.get_or_insert_with(Default::default))
    }

    /// `AddAnimState` / `AddBlendState`: returns the state's number.
    pub fn add_anim_state(&mut self, e: i32, name: &str, kind: crate::animblend::StateKind) -> Result<i32, String> {
        if let crate::animblend::StateKind::Seq { seq, .. } = &kind {
            if *seq >= self.seq_count(e)? {
                return Err(format!("entity {e} has no animation sequence {seq}"));
            }
        }
        let m = self.machine_mut(e)?;
        if m.state(name).is_some() {
            return Err(format!("animation state '{name}' already exists"));
        }
        m.states.push(crate::animblend::State { name: name.to_string(), kind });
        Ok(m.states.len() as i32 - 1)
    }

    /// `AddAnimTransition`: from `from` ("" or "*" = any state) to `to` when
    /// `condition` holds, crossfading over `duration` frames.
    pub fn add_anim_transition(&mut self, e: i32, from: &str, to: &str, condition: &str, duration: f32) -> Result<(), String> {
        let cond = crate::animblend::parse_cond(condition)?;
        let m = self.machine_mut(e)?;
        let from = if from.is_empty() || from == "*" { None } else { Some(m.state(from).ok_or_else(|| format!("no animation state '{from}'"))?) };
        let to = m.state(to).ok_or_else(|| format!("no animation state '{to}'"))?;
        m.transitions.push(crate::animblend::Transition { from, to, cond, duration: duration.max(0.0) });
        Ok(())
    }

    pub fn set_anim_param(&mut self, e: i32, name: &str, v: f32) -> Result<(), String> {
        self.machine_mut(e)?.params.insert(name.to_lowercase(), v);
        Ok(())
    }

    pub fn anim_param(&mut self, e: i32, name: &str) -> Result<f32, String> {
        Ok(self.machine_mut(e)?.params.get(&name.to_lowercase()).copied().unwrap_or(0.0))
    }

    /// `StartAnimStates`: runs the state machine from state `name`.
    pub fn start_anim_states(&mut self, e: i32, name: &str) -> Result<(), String> {
        let idx = self.machine_mut(e)?.state(name).ok_or_else(|| format!("no animation state '{name}'"))?;
        self.enter_state(e, idx, 0.0)
    }

    pub fn anim_state_name(&mut self, e: i32) -> Result<String, String> {
        let m = self.machine_mut(e)?;
        Ok(m.current.and_then(|i| m.states.get(i)).map(|s| s.name.clone()).unwrap_or_default())
    }

    /// `AnimRootMotion`: the root bone's movement moves the entity.
    pub fn anim_root_motion(&mut self, e: i32, bone: &str, enable: bool, keep_y: bool) -> Result<(), String> {
        let bone = if bone.is_empty() || !enable {
            Handle::NONE
        } else {
            let root = self.ent(e)?.node;
            let graph = self.graph()?;
            graph.traverse_handle_iter(root).find(|&n| graph[n].name().eq_ignore_ascii_case(bone)).ok_or_else(|| format!("Entity {e} has no bone named '{bone}'"))?
        };
        let st = self.anim_mut(e)?;
        st.extra.root = enable.then_some(crate::animblend::RootMotion { bone, rest: None, prev: None, keep_y });
        Ok(())
    }

    /// `AddAnimSeq`: starts a keyframe sequence of `length` frames.
    pub fn add_anim_seq(&mut self, e: i32, length: f32) -> Result<i32, String> {
        let ent = self.ent_mut(e)?;
        let st = ent.anim.get_or_insert_with(|| AnimState::new(Handle::NONE, Vec::new()));
        if st.keys.is_none() && !st.seqs.is_empty() {
            return Err(format!("Entity {e} is an animated model; use ExtractAnimSeq for its sequences"));
        }
        let keys = st.keys.get_or_insert_with(Default::default);
        keys.seqs.push(crate::keyanim::Seq { length: length.max(0.0), keys: Default::default() });
        Ok(keys.seqs.len() as i32 - 1)
    }

    /// `SetAnimKey`: records the entity's transform at `frame` of its last sequence.
    pub fn set_anim_key(&mut self, e: i32, frame: i32, pos: bool, rot: bool, scale: bool) -> Result<(), String> {
        let xf = self.live_xf(self.ent(e)?);
        let st = self.anim_mut(e)?;
        let seq = st.keys.as_mut().and_then(|k| k.seqs.last_mut()).ok_or("SetAnimKey needs AddAnimSeq first")?;
        seq.keys.insert(frame.max(0) as u32, crate::keyanim::key_from(&xf, pos, rot, scale));
        seq.length = seq.length.max(frame as f32);
        Ok(())
    }

    /// `ExtractAnimSeq`: a new sequence playing frames `first..=last` of `seq`.
    pub fn extract_seq(&mut self, e: i32, first: f32, last: f32, seq: i32) -> Result<i32, String> {
        if let Some(k) = self.anim_mut(e)?.keys.as_mut() {
            // keyframe sequences: copy the keys in range, shifted to start at 0
            let src = k.seqs.get(usize::try_from(seq).unwrap_or(usize::MAX)).cloned().ok_or_else(|| format!("entity {e} has no animation sequence {seq}"))?;
            let (a, b) = (first.min(last).max(0.0) as u32, first.max(last).max(0.0) as u32);
            let keys = src.keys.range(a..=b).map(|(f, key)| (f - a, *key)).collect();
            k.seqs.push(crate::keyanim::Seq { length: (b - a) as f32, keys });
            return Ok(k.seqs.len() as i32 - 1);
        }
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
        // a model inside a loaded scene file can be animated like a LoadMesh model
        // (the model itself, or the pivot holding it as LoadMesh makes)
        let graph = self.graph()?;
        let model = std::iter::once(node).chain(graph[node].children().iter().copied()).find(|&n| graph[n].is_resource_instance_root());
        if let Some(model) = model {
            let res = graph[model].resource();
            // only a player of this model, not of a model inside it (a scene file)
            let own = |p: Handle<Node>| {
                let mut n = graph[p].parent();
                while n.is_some() && n != model && !graph[n].is_resource_instance_root() {
                    n = graph[n].parent();
                }
                n == model
            };
            if let Some(player) = find_player(graph, model).filter(|&p| own(p)) {
                if !self.entities().any(|(_, e)| e.anim.as_ref().is_some_and(|a| a.player == player)) {
                    let seqs = take_control(self.graph()?, player);
                    if !seqs.is_empty() {
                        let e = self.ent_mut(id)?;
                        e.anim = Some(AnimState::new(player, seqs));
                        e.model = true;
                        e.model_res = res;
                    }
                }
            }
        }
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
            // a level's physics bodies have left its node tree
            None if self.ent(e)?.level_objects.is_some() => {
                Ok(self.entities().find(|(_, x)| x.level_owner == e as u32 && x.name.eq_ignore_ascii_case(name)).map_or(0, |(id, _)| id))
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_weight_fades_out() {
        let mut st = AnimState::new(Handle::NONE, Vec::new());
        assert_eq!(st.blend_weight(), 0.0, "no transition without a start pose");
        st.from_pose = Some(AnimationPose::default());
        st.blend_total = 20.0;
        st.transition = 20.0;
        assert_eq!(st.blend_weight(), 1.0);
        st.transition = 5.0;
        assert_eq!(st.blend_weight(), 0.25);
        st.transition = 0.0;
        assert_eq!(st.blend_weight(), 0.0);
    }
}
