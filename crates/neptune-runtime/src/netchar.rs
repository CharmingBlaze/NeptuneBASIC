//! (ext) Host-checked character movement for multiplayer games.
//!
//! The host decides where characters really are. A player's computer does
//! not wait to hear: it moves its character at once with the same rules and
//! sends the buttons it pressed. The host runs those buttons and says where
//! that really led. If the two disagree, the player's computer goes back to
//! what the host said and replays the buttons the host has not seen yet.
//!
//! The rules are here, independent of the engine; `netsync.rs` connects them
//! to the game. A character's position can only change through its inputs,
//! each of which has a length the host limits to the time that really
//! passed, so a modified game cannot run faster than anyone else.

use std::collections::VecDeque;

use neptune_vm::netgame::CharInput;

use crate::charcontrol::{CharWorld, Character, Hit, Settings, V3};
use fyrox::core::algebra::Matrix4;

/// Longest single input the host accepts, in seconds.
pub const MAX_INPUT_DT: f32 = 0.1;
/// Unconfirmed inputs repeated in each message, so a lost one is no loss.
const RESEND: usize = 16;
/// Inputs kept for replaying: four seconds at 60 frames a second.
const HISTORY: usize = 240;
/// Disagreements smaller than this are not worth correcting.
const POS_TOLERANCE: f32 = 0.02;
const VEL_TOLERANCE: f32 = 0.1;
/// A correction longer than this is a teleport, and is not smoothed.
const SNAP_DISTANCE: f32 = 5.0;
/// How quickly a correction is blended away, per second.
const SMOOTHING: f32 = 12.0;
/// Most time the host lets a player catch up on after a stall, in seconds.
const BUDGET_LIMIT: f32 = 0.25;
const QUEUE_LIMIT: usize = 128;

const JUMP_PRESSED: u32 = 1;
const JUMP_HELD: u32 = 2;
const SPRINT: u32 = 4;
const CROUCH: u32 = 8;
const DROP: u32 = 16;

/// What the player asked for during one `UpdateChar`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input {
    pub seq: u32,
    pub dt: f32,
    wish: V3,
    flags: u32,
}

impl Input {
    fn capture(c: &Character, seq: u32, dt: f32) -> Input {
        let mut flags = 0;
        for (on, bit) in [(c.pressed_jump, JUMP_PRESSED), (c.jump_held, JUMP_HELD), (c.sprint, SPRINT), (c.crouch_wanted, CROUCH), (c.pressed_drop, DROP)] {
            if on {
                flags |= bit;
            }
        }
        Input { seq, dt, wish: c.wish, flags }
    }

    fn apply(&self, c: &mut Character) {
        c.wish = self.wish;
        c.jump_held = self.flags & JUMP_HELD != 0;
        c.sprint = self.flags & SPRINT != 0;
        c.crouch_wanted = self.flags & CROUCH != 0;
        if self.flags & JUMP_PRESSED != 0 {
            c.press_jump();
        }
        if self.flags & DROP != 0 {
            c.press_drop();
        }
    }

    pub fn pack(&self) -> CharInput {
        (self.seq, [self.dt, self.wish.x, self.wish.y, self.wish.z, self.flags as f32])
    }

    /// `None` for values no honest game sends.
    pub fn unpack(raw: &CharInput) -> Option<Input> {
        let [dt, x, y, z, flags] = raw.1;
        let valid = raw.1.iter().all(|v| v.is_finite()) && dt > 0.0 && dt <= MAX_INPUT_DT && (0.0..32.0).contains(&flags);
        valid.then(|| Input { seq: raw.0, dt, wish: V3::new(x, y, z), flags: flags as u32 })
    }
}

/// Replays must not shove the same crate twice.
struct Quiet<'a>(&'a mut dyn CharWorld);

impl CharWorld for Quiet<'_> {
    fn cast(&mut self, feet: V3, motion: V3, height: f32, ignore: &[i32]) -> Option<Hit> {
        self.0.cast(feet, motion, height, ignore)
    }
    fn probe(&mut self, from: V3, motion: V3) -> Option<Hit> {
        self.0.probe(from, motion)
    }
    fn frame_of(&mut self, id: i32) -> Option<Matrix4<f32>> {
        self.0.frame_of(id)
    }
}

#[derive(Debug, Clone, Copy)]
struct Step {
    input: Input,
    pos: V3,
    vel: V3,
}

/// The player's side of a character: what it did that the host has not confirmed.
#[derive(Debug, Clone)]
pub struct Predictor {
    pub net_id: i32,
    next_seq: u32,
    last_ack: u32,
    history: VecDeque<Step>,
    /// Added to where the entity is drawn, so a correction doesn't jump.
    pub visual: V3,
    pub corrections: i32,
}

impl Predictor {
    pub fn new(net_id: i32) -> Predictor {
        Predictor { net_id, next_seq: 1, last_ack: 0, history: VecDeque::new(), visual: V3::zeros(), corrections: 0 }
    }

    /// Reads the buttons, just before the character moves.
    pub fn capture(&mut self, c: &Character, dt: f32) -> Input {
        let input = Input::capture(c, self.next_seq, dt);
        self.next_seq += 1;
        input
    }

    /// Remembers where those buttons led.
    pub fn record(&mut self, input: Input, c: &Character) {
        if self.history.len() >= HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(Step { input, pos: c.pos, vel: c.vel });
    }

    /// The newest inputs, to send; older ones are repeated in case one was lost.
    pub fn unconfirmed(&self) -> Vec<CharInput> {
        self.history.iter().rev().take(RESEND).rev().map(|s| s.input.pack()).collect()
    }

    /// Blends a correction away. Runs once per frame.
    pub fn settle(&mut self, dt: f32) {
        self.visual *= (-SMOOTHING * dt).exp();
        if self.visual.norm() < 1e-4 {
            self.visual = V3::zeros();
        }
    }

    /// The host says where the character was after input `ack`. Returns
    /// true if that was not where this computer thought it was.
    pub fn reconcile(&mut self, c: &mut Character, w: &mut dyn CharWorld, ack: u32, state: &[f32]) -> bool {
        if ack <= self.last_ack {
            return false;
        }
        self.last_ack = ack;
        let Some(step) = self.history.iter().find(|s| s.input.seq == ack).copied() else { return false };
        while self.history.front().is_some_and(|s| s.input.seq <= ack) {
            self.history.pop_front();
        }
        let Some((pos, vel)) = position_in(state) else { return false };
        if (step.pos - pos).norm() < POS_TOLERANCE && (step.vel - vel).norm() < VEL_TOLERANCE {
            return false;
        }

        // back to what the host said, then redo everything since
        let (seen, was_at) = (c.clone(), c.pos);
        if !c.restore(state) {
            return false;
        }
        let mut world = Quiet(w);
        for s in self.history.iter_mut() {
            s.input.apply(c);
            c.update(&mut world, s.input.dt);
            s.pos = c.pos;
            s.vel = c.vel;
        }
        // the wish and buttons of this frame still stand
        c.wish = seen.wish;
        c.jump_held = seen.jump_held;
        c.sprint = seen.sprint;
        c.crouch_wanted = seen.crouch_wanted;
        // what the game was told about the last frame hasn't changed
        c.copy_events(&seen);
        self.visual += was_at - c.pos;
        if self.visual.norm() > SNAP_DISTANCE {
            self.visual = V3::zeros();
        }
        self.corrections += 1;
        true
    }
}

fn position_in(state: &[f32]) -> Option<(V3, V3)> {
    let s = state.get(..6)?;
    Some((V3::new(s[0], s[1], s[2]), V3::new(s[3], s[4], s[5])))
}

/// The host's side of a player's character.
#[derive(Debug, Clone)]
pub struct HostSim {
    queue: VecDeque<Input>,
    /// The last input run.
    pub ack: u32,
    /// Seconds of movement this player is still allowed.
    budget: f32,
    /// Settings the player was last told about.
    pub told: Settings,
    /// Which way the player's model faces. Only for show, so it is believed.
    pub look: Option<[f32; 4]>,
    /// The player's own clock: how much time its inputs add up to.
    pub clock: f64,
}

impl HostSim {
    pub fn new(settings: Settings) -> HostSim {
        HostSim { queue: VecDeque::new(), ack: 0, budget: BUDGET_LIMIT, told: settings, look: None, clock: 0.0 }
    }

    /// Takes in a message of inputs; repeats and bad values are ignored.
    pub fn receive(&mut self, inputs: &[CharInput]) {
        for raw in inputs {
            let newest = self.queue.back().map_or(self.ack, |i| i.seq);
            if raw.0 > newest && self.queue.len() < QUEUE_LIMIT {
                if let Some(input) = Input::unpack(raw) {
                    self.queue.push_back(input);
                }
            }
        }
    }

    /// Real time has passed: that much more movement is allowed.
    pub fn refill(&mut self, seconds: f32) {
        self.budget = (self.budget + seconds.clamp(0.0, BUDGET_LIMIT)).min(BUDGET_LIMIT);
    }

    pub fn has_work(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Runs what has arrived. Inputs beyond the time that really passed are
    /// ignored, and the player is corrected.
    pub fn run(&mut self, c: &mut Character, w: &mut dyn CharWorld) {
        while let Some(input) = self.queue.pop_front() {
            self.ack = input.seq;
            self.clock += input.dt as f64;
            if input.dt <= self.budget {
                self.budget -= input.dt;
                input.apply(c);
                c.update(w, input.dt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charcontrol::MODE_WALK;

    const DT: f32 = 1.0 / 60.0;

    /// Flat ground at height 0, and a wall at `wall_x`.
    struct Flat {
        wall_x: f32,
        pushes: i32,
    }

    impl CharWorld for Flat {
        fn cast(&mut self, feet: V3, motion: V3, _height: f32, _ignore: &[i32]) -> Option<Hit> {
            let hit = |t: f32, normal: V3| Hit { t: t.max(0.0), normal, id: 1, one_way: false, dynamic: false, penetrating: false };
            let down = (motion.y < 0.0 && feet.y + motion.y <= 0.0).then(|| hit(feet.y / -motion.y, V3::y()));
            let side = (motion.x > 0.0 && feet.x + motion.x >= self.wall_x).then(|| hit((self.wall_x - feet.x) / motion.x, -V3::x()));
            match (down, side) {
                (Some(a), Some(b)) => Some(if a.t < b.t { a } else { b }),
                (a, b) => a.or(b),
            }
        }
        fn push(&mut self, _id: i32, _dir: V3, _speed: f32, _strength: f32) {
            self.pushes += 1;
        }
    }

    fn world() -> Flat {
        Flat { wall_x: 1000.0, pushes: 0 }
    }

    fn character() -> Character {
        let mut c = Character::new(V3::zeros(), V3::y(), 1.8, Settings::defaults_3d());
        c.mode = MODE_WALK;
        c
    }

    /// A player and the host, joined by a network that takes `lag` frames
    /// each way.
    struct Game {
        player: Character,
        predictor: Predictor,
        host: Character,
        sim: HostSim,
        to_host: VecDeque<(usize, Vec<CharInput>)>,
        to_player: VecDeque<(usize, u32, Vec<f32>)>,
        lag: usize,
        frame: usize,
        world: Flat,
    }

    impl Game {
        fn new(lag: usize) -> Game {
            let host = character();
            let told = host.s.clone();
            Game { player: character(), predictor: Predictor::new(1), host, sim: HostSim::new(told), to_host: VecDeque::new(), to_player: VecDeque::new(), lag, frame: 0, world: world() }
        }

        /// One frame on both computers.
        fn frame(&mut self) {
            self.frame += 1;
            // the player moves at once
            let input = self.predictor.capture(&self.player, DT);
            self.player.update(&mut self.world, DT);
            self.predictor.record(input, &self.player);
            self.predictor.settle(DT);
            self.to_host.push_back((self.frame + self.lag, self.predictor.unconfirmed()));
            // the host hears about it later
            while self.to_host.front().is_some_and(|m| m.0 <= self.frame) {
                let (_, inputs) = self.to_host.pop_front().unwrap();
                self.sim.receive(&inputs);
            }
            self.sim.refill(DT);
            self.sim.run(&mut self.host, &mut self.world);
            if self.sim.ack != 0 {
                self.to_player.push_back((self.frame + self.lag, self.sim.ack, self.host.snapshot()));
            }
            // and the answer comes back
            while self.to_player.front().is_some_and(|m| m.0 <= self.frame) {
                let (_, ack, state) = self.to_player.pop_front().unwrap();
                self.predictor.reconcile(&mut self.player, &mut self.world, ack, &state);
            }
        }

        fn frames(&mut self, n: usize) {
            for _ in 0..n {
                self.frame();
            }
        }
    }

    #[test]
    fn the_player_moves_before_the_host_has_heard() {
        let mut g = Game::new(6);
        g.frames(3);
        g.player.wish = V3::x();
        g.frames(3);
        assert!(g.player.pos.x > 0.0);
        assert_eq!(g.host.pos.x, 0.0);
    }

    #[test]
    fn an_honest_player_is_never_corrected() {
        let mut g = Game::new(8);
        g.frames(10);
        g.player.wish = V3::new(1.0, 0.0, 0.3);
        g.frames(40);
        g.player.press_jump();
        g.frames(40);
        g.player.wish = V3::zeros();
        g.player.sprint = true;
        g.frames(40);
        assert_eq!(g.predictor.corrections, 0);
        g.frames(20);
        assert!((g.player.pos - g.host.pos).norm() < 0.05);
    }

    #[test]
    fn a_shove_from_the_host_reaches_the_player() {
        let mut g = Game::new(5);
        g.frames(30);
        g.host.impulse(V3::new(0.0, 0.0, 20.0));
        g.frames(100);
        assert!(g.predictor.corrections > 0);
        assert!(g.player.pos.z > 1.0, "{}", g.player.pos.z);
        assert!((g.player.pos - g.host.pos).norm() < 0.05, "{:?} {:?}", g.player.pos, g.host.pos);
    }

    #[test]
    fn a_faster_copy_of_the_game_gains_nothing() {
        let mut g = Game::new(4);
        g.frames(10);
        // a modified game gives itself five times the speed
        g.player.s.speed *= 5.0;
        g.player.s.acceleration *= 5.0;
        g.player.wish = V3::x();
        g.frames(120);
        // what counts is where the host says it is; the cheat's own screen
        // only gets ahead by what it can do before an answer comes back
        assert!(g.host.pos.x < 5.0 * 2.1, "{}", g.host.pos.x);
        assert!(g.player.pos.x - g.host.pos.x < 25.0 * 0.25, "{} {}", g.player.pos.x, g.host.pos.x);
    }

    #[test]
    fn sending_more_time_than_has_passed_gains_nothing() {
        let mut c = character();
        let mut w = world();
        let mut sim = HostSim::new(c.s.clone());
        c.wish = V3::x();
        // two seconds of inputs, all at once, in no time at all
        let flood: Vec<CharInput> = (1..=120).map(|seq| Input { seq, dt: DT, wish: V3::x(), flags: 0 }.pack()).collect();
        sim.receive(&flood);
        sim.run(&mut c, &mut w);
        assert_eq!(sim.ack, 120);
        assert!(c.pos.x < 5.0 * BUDGET_LIMIT + 0.01, "{}", c.pos.x);
    }

    #[test]
    fn lost_messages_cost_nothing() {
        let mut g = Game::new(3);
        g.player.wish = V3::x();
        for i in 0..200 {
            g.frame();
            if i % 3 == 0 {
                g.to_host.pop_back();
            }
        }
        assert_eq!(g.predictor.corrections, 0);
    }

    #[test]
    fn a_replay_does_not_repeat_what_the_game_was_told() {
        let mut g = Game::new(5);
        g.frames(30);
        g.player.press_jump();
        g.frame();
        assert!(g.player.jumped);
        g.frames(2);
        assert!(!g.player.jumped);
        g.host.impulse(V3::new(0.0, 0.0, 8.0));
        let told_before = g.world.pushes;
        for _ in 0..20 {
            g.frame();
            assert!(!g.player.jumped);
        }
        assert_eq!(g.world.pushes, told_before);
    }

    #[test]
    fn corrections_are_blended_in_but_teleports_are_not() {
        let mut p = Predictor::new(1);
        let mut c = character();
        let mut w = world();
        let input = p.capture(&c, DT);
        c.update(&mut w, DT);
        p.record(input, &c);

        let mut host = c.clone();
        host.pos.x += 1.0;
        assert!(p.reconcile(&mut c, &mut w, input.seq, &host.snapshot()));
        assert!((p.visual.x + 1.0).abs() < 0.01, "{}", p.visual.x);
        p.settle(0.5);
        assert!(p.visual.norm() < 0.01);

        let input = p.capture(&c, DT);
        c.update(&mut w, DT);
        p.record(input, &c);
        host = c.clone();
        host.pos.x += 500.0;
        assert!(p.reconcile(&mut c, &mut w, input.seq, &host.snapshot()));
        assert_eq!(p.visual, V3::zeros());
    }

    #[test]
    fn stale_or_malformed_answers_are_ignored() {
        let mut p = Predictor::new(1);
        let mut c = character();
        let mut w = world();
        let input = p.capture(&c, DT);
        c.update(&mut w, DT);
        p.record(input, &c);
        assert!(!p.reconcile(&mut c, &mut w, input.seq, &[1.0, 2.0]));
        let mut far = c.clone();
        far.pos.x = 9.0;
        assert!(!p.reconcile(&mut c, &mut w, input.seq, &far.snapshot()), "already answered");
    }

    #[test]
    fn settings_survive_the_trip() {
        let mut a = Settings::defaults_3d();
        a.speed = 12.5;
        a.face_rate = 90.0;
        let mut b = Settings::defaults_2d();
        b.set_values(&a.values());
        assert_eq!(a, b);
    }

    #[test]
    fn nonsense_inputs_are_refused() {
        assert!(Input::unpack(&(1, [f32::NAN, 0.0, 0.0, 0.0, 0.0])).is_none());
        assert!(Input::unpack(&(1, [DT, f32::INFINITY, 0.0, 0.0, 0.0])).is_none());
        assert!(Input::unpack(&(1, [5.0, 0.0, 0.0, 0.0, 0.0])).is_none());
        assert!(Input::unpack(&(1, [-1.0, 0.0, 0.0, 0.0, 0.0])).is_none());
        assert!(Input::unpack(&(1, [DT, 0.0, 0.0, 0.0, 999.0])).is_none());
        assert!(Input::unpack(&(1, [DT, 1.0, 0.0, 0.0, 3.0])).is_some());
    }
}
