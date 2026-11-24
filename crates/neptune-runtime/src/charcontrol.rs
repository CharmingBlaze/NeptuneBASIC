//! The movement rules of character controllers (ext), shared by 3D and 2D.
//!
//! A character is not pushed around by the physics engine: it asks the world
//! "what would I hit going this way?" ([`CharWorld::cast`]), moves as far as
//! it can and slides along what it hit. That gives exact control over how
//! games feel: slopes, stairs, moving platforms, forgiving jumps. The world
//! behind the trait is Rapier in the game and a few boxes in the tests.
//!
//! Everything is in the program's own space, with `up` saying which way is
//! up: `(0, 1, 0)` in 3D and `(0, -1, 0)` in 2D (pixels, Y down). 2D is the
//! same problem with `z` always zero. `pos` is where the feet are.

use fyrox::core::algebra::{Matrix4, Point3, Vector3};

pub type V3 = Vector3<f32>;

pub const MODE_WALK: i32 = 0;
pub const MODE_FLY: i32 = 1;
pub const MODE_NOCLIP: i32 = 2;

/// What a cast ran into.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// How far along the motion, 0 to 1.
    pub t: f32,
    /// The surface's outward normal.
    pub normal: V3,
    /// The entity or 2D body that was hit.
    pub id: i32,
    pub one_way: bool,
    pub dynamic: bool,
    /// The character was already inside it.
    pub penetrating: bool,
}

pub trait CharWorld {
    /// The first thing the character's shape meets moving by `motion`.
    fn cast(&mut self, feet: V3, motion: V3, height: f32, ignore: &[i32]) -> Option<Hit>;
    /// The first thing a thin ray meets. Rounded feet resting on the edge of
    /// a step report a slanted surface; a ray finds the floor really there.
    fn probe(&mut self, _from: V3, _motion: V3) -> Option<Hit> {
        None
    }
    /// Where something is now, so whoever stands on it can move with it.
    fn frame_of(&mut self, _id: i32) -> Option<Matrix4<f32>> {
        None
    }
    /// A dynamic body was walked into.
    fn push(&mut self, _id: i32, _dir: V3, _speed: f32, _strength: f32) {}
}

macro_rules! settings {
    ($($name:ident),* $(,)?) => {
        /// Tunable numbers, read and set by name with `CharSet` and `CharGet`.
        #[derive(Debug, Clone, PartialEq)]
        pub struct Settings { $(pub $name: f32),* }
        impl Settings {
            pub fn field(&mut self, name: &str) -> Option<&mut f32> {
                match name.to_ascii_lowercase().as_str() {
                    $(stringify!($name) => Some(&mut self.$name),)*
                    _ => None,
                }
            }
            /// Every setting, in a fixed order, to send over the network.
            pub fn values(&self) -> Vec<f32> {
                vec![$(self.$name),*]
            }
            pub fn set_values(&mut self, values: &[f32]) {
                let mut v = values.iter().copied();
                $(if let Some(x) = v.next() { self.$name = x; })*
            }
        }
    };
}

settings!(
    speed, sprint, crouch, acceleration, friction, air_acceleration, air_friction, gravity, max_fall, jump_speed, jump_cut, coyote_time,
    jump_buffer, air_jumps, max_slope, step_height, snap, skin, wall_slide, wall_jump, wall_jump_up, wall_jump_out, wall_jump_lock, push,
    knock_decay, crouch_height, face_rate,
);

impl Settings {
    /// Metres and seconds.
    pub fn defaults_3d() -> Settings {
        Settings {
            speed: 5.0,
            sprint: 1.6,
            crouch: 0.5,
            acceleration: 50.0,
            friction: 60.0,
            air_acceleration: 18.0,
            air_friction: 4.0,
            gravity: 20.0,
            max_fall: 40.0,
            jump_speed: 8.0,
            jump_cut: 0.5,
            coyote_time: 0.1,
            jump_buffer: 0.12,
            air_jumps: 0.0,
            max_slope: 46.0,
            step_height: 0.35,
            snap: 0.3,
            skin: 0.02,
            wall_slide: 0.0,
            wall_jump: 0.0,
            wall_jump_up: 8.0,
            wall_jump_out: 6.0,
            wall_jump_lock: 0.2,
            push: 1.0,
            knock_decay: 20.0,
            crouch_height: 1.0,
            face_rate: 0.0,
        }
    }

    /// Pixels and seconds, at about 45 pixels to the metre.
    pub fn defaults_2d() -> Settings {
        Settings {
            speed: 220.0,
            sprint: 1.5,
            crouch: 0.5,
            acceleration: 2400.0,
            friction: 3000.0,
            air_acceleration: 1500.0,
            air_friction: 300.0,
            gravity: 1500.0,
            max_fall: 900.0,
            jump_speed: 580.0,
            jump_cut: 0.5,
            coyote_time: 0.1,
            jump_buffer: 0.12,
            air_jumps: 0.0,
            max_slope: 46.0,
            step_height: 0.0,
            snap: 8.0,
            skin: 0.5,
            wall_slide: 0.0,
            wall_jump: 0.0,
            wall_jump_up: 520.0,
            wall_jump_out: 300.0,
            wall_jump_lock: 0.15,
            push: 1.0,
            knock_decay: 900.0,
            crouch_height: 0.0,
            face_rate: 0.0,
        }
    }
}

/// One character. The program sets the `wish` and buttons, then [`update`]
/// moves it and fills in what happened.
///
/// [`update`]: Character::update
#[derive(Debug, Clone)]
pub struct Character {
    pub up: V3,
    pub s: Settings,
    pub pos: V3,
    pub vel: V3,
    pub stand_height: f32,
    pub height: f32,
    /// How round the feet are: the capsule's radius, 0 for a box.
    pub foot_radius: f32,
    pub mode: i32,

    // ---- what the program asks for
    /// Wanted direction and amount (length up to 1).
    pub wish: V3,
    pub jump_held: bool,
    pub sprint: bool,
    pub crouch_wanted: bool,
    pub buffer: f32,
    pub drop_timer: f32,
    pub dash_vel: V3,
    pub dash_time: f32,
    pub knock: V3,
    /// `CharJump` and `CharDrop` were called since the last update.
    pub pressed_jump: bool,
    pub pressed_drop: bool,

    // ---- how it is now
    pub grounded: bool,
    pub ground_normal: V3,
    pub ground_id: i32,
    ground_frame: Option<Matrix4<f32>>,
    pub platform_vel: V3,
    pub on_wall: bool,
    pub wall_normal: V3,
    wall_timer: f32,
    pub wall_sliding: bool,
    pub on_ceiling: bool,
    pub sliding: bool,
    pub crouching: bool,
    jumping: bool,
    coyote: f32,
    pub air_left: i32,
    wall_lock: f32,

    // ---- what happened during the last update
    pub landed: bool,
    pub left_ground: bool,
    pub jumped: bool,
    pub wall_jumped: bool,
    pub stepped: bool,
    /// How fast it was falling when it landed.
    pub impact: f32,
}

fn approach(from: V3, to: V3, amount: f32) -> V3 {
    let d = to - from;
    let len = d.norm();
    if len <= amount || len < 1e-9 {
        to
    } else {
        from + d / len * amount
    }
}

impl Character {
    pub fn new(pos: V3, up: V3, height: f32, s: Settings) -> Character {
        Character {
            up,
            s,
            pos,
            vel: V3::zeros(),
            stand_height: height,
            height,
            foot_radius: 0.0,
            mode: MODE_WALK,
            wish: V3::zeros(),
            jump_held: true,
            sprint: false,
            crouch_wanted: false,
            buffer: 0.0,
            drop_timer: 0.0,
            dash_vel: V3::zeros(),
            dash_time: 0.0,
            knock: V3::zeros(),
            pressed_jump: false,
            pressed_drop: false,
            grounded: false,
            ground_normal: up,
            ground_id: 0,
            ground_frame: None,
            platform_vel: V3::zeros(),
            on_wall: false,
            wall_normal: V3::zeros(),
            wall_timer: 0.0,
            wall_sliding: false,
            on_ceiling: false,
            sliding: false,
            crouching: false,
            jumping: false,
            coyote: 0.0,
            air_left: 0,
            wall_lock: 0.0,
            landed: false,
            left_ground: false,
            jumped: false,
            wall_jumped: false,
            stepped: false,
            impact: 0.0,
        }
    }

    /// `CharJump`: remembered briefly, so a press just before landing counts.
    pub fn press_jump(&mut self) {
        self.buffer = self.s.jump_buffer.max(1e-3);
        self.pressed_jump = true;
    }

    /// `CharDrop`: falls through one-way platforms for a moment.
    pub fn press_drop(&mut self) {
        self.drop_timer = 0.25;
        self.pressed_drop = true;
        if self.grounded {
            self.grounded = false;
            self.ground_id = 0;
            self.ground_frame = None;
        }
    }

    pub fn dash(&mut self, dir: V3, speed: f32, seconds: f32) {
        if let Some(d) = dir.try_normalize(1e-6) {
            self.dash_vel = d * speed;
            self.dash_time = seconds;
        }
    }

    /// `CharImpulse`: a shove. Upward parts lift the character off the ground.
    pub fn impulse(&mut self, v: V3) {
        let vertical = v.dot(&self.up);
        self.knock += v - self.up * vertical;
        if vertical != 0.0 {
            self.vel += self.up * vertical;
            if vertical > 0.0 {
                self.grounded = false;
                self.ground_id = 0;
                self.jumping = false;
            }
        }
    }

    /// Moves the character instantly, without sweeping.
    pub fn teleport(&mut self, pos: V3) {
        self.pos = pos;
        self.grounded = false;
        self.ground_id = 0;
        self.ground_frame = None;
        self.platform_vel = V3::zeros();
        self.on_wall = false;
        self.wall_timer = 0.0;
    }

    fn walkable(&self, n: &V3) -> bool {
        n.dot(&self.up) >= self.s.max_slope.to_radians().cos()
    }

    fn one_way_blocks(&self, h: &Hit, motion: &V3) -> bool {
        self.drop_timer <= 0.0 && !h.penetrating && motion.dot(&self.up) < 0.0 && h.normal.dot(&self.up) > 0.7
    }

    fn cast(&self, w: &mut dyn CharWorld, from: V3, motion: V3) -> Option<Hit> {
        let mut ignore = Vec::new();
        loop {
            let h = w.cast(from, motion, self.height, &ignore)?;
            if h.one_way && !self.one_way_blocks(&h, &motion) {
                ignore.push(h.id);
                if ignore.len() > 32 {
                    return None;
                }
                continue;
            }
            return Some(h);
        }
    }

    /// Everything that decides how it moves next, for the host to send a
    /// player's computer. The ground underfoot is left out: an entity's
    /// number means something else on each computer.
    pub fn snapshot(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(40);
        for a in [self.pos, self.vel, self.dash_vel, self.knock, self.ground_normal, self.platform_vel, self.wall_normal] {
            v.extend_from_slice(&[a.x, a.y, a.z]);
        }
        v.extend_from_slice(&[self.height, self.mode as f32, self.buffer, self.drop_timer, self.dash_time, self.wall_timer, self.coyote, self.air_left as f32, self.wall_lock]);
        v.extend([self.grounded, self.on_wall, self.wall_sliding, self.on_ceiling, self.sliding, self.crouching, self.jumping].map(|b| b as i32 as f32));
        v
    }

    /// Goes back to a [`Character::snapshot`]. False if it is not one.
    pub fn restore(&mut self, s: &[f32]) -> bool {
        if s.len() != 37 || s.iter().any(|v| !v.is_finite()) {
            return false;
        }
        let v3 = |i: usize| V3::new(s[i], s[i + 1], s[i + 2]);
        (self.pos, self.vel, self.dash_vel, self.knock) = (v3(0), v3(3), v3(6), v3(9));
        (self.ground_normal, self.platform_vel, self.wall_normal) = (v3(12), v3(15), v3(18));
        self.height = s[21];
        self.mode = s[22] as i32;
        (self.buffer, self.drop_timer, self.dash_time, self.wall_timer, self.coyote) = (s[23], s[24], s[25], s[26], s[27]);
        self.air_left = s[28] as i32;
        self.wall_lock = s[29];
        let on = |i: usize| s[i] != 0.0;
        (self.grounded, self.on_wall, self.wall_sliding, self.on_ceiling, self.sliding, self.crouching, self.jumping) = (on(30), on(31), on(32), on(33), on(34), on(35), on(36));
        if !self.grounded {
            self.ground_id = 0;
            self.ground_frame = None;
        }
        true
    }

    /// Takes on what `other` reported about its last update.
    pub fn copy_events(&mut self, other: &Character) {
        self.landed = other.landed;
        self.left_ground = other.left_ground;
        self.jumped = other.jumped;
        self.wall_jumped = other.wall_jumped;
        self.stepped = other.stepped;
        self.impact = other.impact;
    }

    /// Advances by `dt` seconds, in short steps if that is long. Reports what
    /// happened over the whole time.
    pub fn update(&mut self, w: &mut dyn CharWorld, dt: f32) {
        self.pressed_jump = false;
        self.pressed_drop = false;
        self.landed = false;
        self.left_ground = false;
        self.jumped = false;
        self.wall_jumped = false;
        self.stepped = false;
        self.impact = 0.0;
        let steps = (dt / (1.0 / 30.0)).ceil().clamp(1.0, 4.0);
        for _ in 0..steps as i32 {
            self.step(w, dt / steps);
        }
    }

    fn step(&mut self, w: &mut dyn CharWorld, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        match self.mode {
            MODE_NOCLIP => {
                let wish = self.limited_wish();
                self.vel = wish * self.target_speed();
                self.pos += self.vel * dt;
                self.clear_contacts();
            }
            MODE_FLY => {
                self.update_crouch(w);
                self.carry(w, dt);
                self.fly(w, dt);
            }
            _ => {
                self.update_crouch(w);
                self.carry(w, dt);
                self.walk(w, dt);
            }
        }
        self.buffer = (self.buffer - dt).max(0.0);
        self.drop_timer = (self.drop_timer - dt).max(0.0);
        self.wall_lock = (self.wall_lock - dt).max(0.0);
        self.dash_time = (self.dash_time - dt).max(0.0);
    }

    fn clear_contacts(&mut self) {
        self.grounded = false;
        self.ground_id = 0;
        self.ground_frame = None;
        self.on_wall = false;
        self.wall_sliding = false;
        self.on_ceiling = false;
        self.sliding = false;
    }

    fn limited_wish(&self) -> V3 {
        let n = self.wish.norm();
        if n > 1.0 {
            self.wish / n
        } else {
            self.wish
        }
    }

    fn target_speed(&self) -> f32 {
        let mut v = self.s.speed;
        if self.crouching {
            v *= self.s.crouch;
        } else if self.sprint {
            v *= self.s.sprint;
        }
        v
    }

    fn update_crouch(&mut self, w: &mut dyn CharWorld) {
        let low = if self.s.crouch_height > 0.0 { self.s.crouch_height } else { self.stand_height * 0.6 };
        if self.crouch_wanted {
            self.height = low.min(self.stand_height);
            self.crouching = true;
        } else if self.height < self.stand_height {
            // stay down while there is no room to stand
            let rise = self.stand_height - self.height;
            let blocked = self.cast(w, self.pos, self.up * (rise + self.s.skin)).is_some();
            if !blocked {
                self.height = self.stand_height;
                self.crouching = false;
            }
        } else {
            self.crouching = false;
        }
    }

    /// Moves with what the character stands on, turning included.
    fn carry(&mut self, w: &mut dyn CharWorld, dt: f32) {
        self.platform_vel = V3::zeros();
        if !self.grounded || self.ground_id == 0 {
            self.ground_frame = None;
            return;
        }
        let Some(now) = w.frame_of(self.ground_id) else {
            self.ground_frame = None;
            return;
        };
        if let Some(inverse) = self.ground_frame.and_then(|last| last.try_inverse()) {
            let moved = (now * inverse).transform_point(&Point3::from(self.pos)).coords - self.pos;
            if moved.norm() > 1e-6 {
                self.platform_vel = moved / dt;
                let len = moved.norm();
                let room = match self.cast(w, self.pos, moved / len * (len + self.s.skin)) {
                    // a floor rising under the feet carries the character, not stops it
                    Some(h) if h.normal.dot(&moved) < 0.0 && h.id != self.ground_id => (h.t * (len + self.s.skin) - self.s.skin).max(0.0),
                    _ => len,
                };
                self.pos += moved / len * room;
            }
        }
        self.ground_frame = Some(now);
    }

    fn fly(&mut self, w: &mut dyn CharWorld, dt: f32) {
        let target = self.limited_wish() * self.target_speed();
        let rate = if target.norm() > 1e-4 { self.s.acceleration } else { self.s.friction };
        self.vel = approach(self.vel, target, rate * dt);
        self.clear_contacts();
        let motion = (self.vel + self.knock) * dt;
        self.knock = approach(self.knock, V3::zeros(), self.s.knock_decay * dt);
        self.move_and_slide(w, motion, false);
    }

    fn start_jump(&mut self, speed: f32) {
        let vertical = self.vel.dot(&self.up);
        // momentum of a moving platform is kept
        let keep = self.platform_vel;
        self.vel += self.up * (speed - vertical) + (keep - self.up * keep.dot(&self.up));
        self.grounded = false;
        self.ground_id = 0;
        self.ground_frame = None;
        self.coyote = 0.0;
        self.buffer = 0.0;
        self.jumping = true;
        self.jumped = true;
    }

    fn walk(&mut self, w: &mut dyn CharWorld, dt: f32) {
        let was_grounded = self.grounded;
        if self.grounded {
            self.coyote = self.s.coyote_time;
            self.air_left = self.s.air_jumps as i32;
        } else {
            self.coyote = (self.coyote - dt).max(0.0);
        }

        // ---- running
        let raw = self.limited_wish();
        let wish = raw - self.up * raw.dot(&self.up);
        let dashing = self.dash_time > 0.0;
        if dashing {
            self.vel = self.dash_vel;
        } else if self.wall_lock <= 0.0 {
            let vertical = self.up * self.vel.dot(&self.up);
            let flat = self.vel - vertical;
            let target = wish * self.target_speed();
            let moving = target.norm() > 1e-4;
            let rate = match (self.grounded, moving) {
                (true, true) => self.s.acceleration,
                (true, false) => self.s.friction,
                (false, true) => self.s.air_acceleration,
                (false, false) => self.s.air_friction,
            };
            self.vel = approach(flat, target, rate * dt) + vertical;
        }

        // ---- jumping
        if self.buffer > 0.0 && !dashing {
            if self.grounded || self.coyote > 0.0 {
                self.start_jump(self.s.jump_speed);
            } else if self.s.wall_jump != 0.0 && self.wall_timer > 0.0 {
                self.vel = self.wall_normal * self.s.wall_jump_out + self.up * self.s.wall_jump_up;
                self.wall_lock = self.s.wall_jump_lock;
                self.wall_timer = 0.0;
                self.buffer = 0.0;
                self.jumping = true;
                self.jumped = true;
                self.wall_jumped = true;
            } else if self.air_left > 0 {
                self.air_left -= 1;
                self.start_jump(self.s.jump_speed);
            }
        }

        // ---- gravity
        let flat = self.vel - self.up * self.vel.dot(&self.up);
        let mut rise = self.vel.dot(&self.up);
        if self.jumping && !self.jump_held && rise > 0.0 {
            rise *= self.s.jump_cut;
            self.jumping = false;
        }
        if rise <= 0.0 {
            self.jumping = false;
        }
        self.wall_sliding = false;
        if dashing {
            rise = self.dash_vel.dot(&self.up);
        } else if self.grounded {
            rise = 0.0;
        } else {
            rise = (rise - self.s.gravity * dt).max(-self.s.max_fall);
            let into_wall = self.on_wall && wish.dot(&self.wall_normal) < -0.1;
            if self.s.wall_slide > 0.0 && into_wall && rise < -self.s.wall_slide {
                rise = -self.s.wall_slide;
                self.wall_sliding = true;
            }
        }
        self.vel = flat + self.up * rise;

        // ---- moving
        let fall_before = -self.vel.dot(&self.up);
        let mut motion = (self.vel + self.knock) * dt;
        self.knock = approach(self.knock, V3::zeros(), self.s.knock_decay * dt);
        if self.grounded {
            // along the slope, at the same speed across the ground
            let n = self.ground_normal;
            motion -= self.up * (motion.dot(&n) / self.up.dot(&n).max(0.1));
        }
        self.on_ceiling = false;
        self.sliding = false;
        self.move_and_slide(w, motion, was_grounded);

        // ---- what is underfoot?
        let rising = self.vel.dot(&self.up) > 1e-3;
        let mut found = None;
        if !rising {
            let reach = if was_grounded { self.s.snap } else { 0.0 } + self.s.skin * 2.0;
            if let Some(h) = self.cast(w, self.pos, -self.up * (reach + self.s.skin)) {
                let drop = (h.t * (reach + self.s.skin) - self.s.skin).max(0.0);
                if self.walkable(&h.normal) {
                    if !h.penetrating {
                        self.pos -= self.up * drop;
                    }
                    found = Some(h);
                } else if let Some(floor) = self.floor_under(w, reach) {
                    // on the edge of a step: it is safe to stand here
                    found = Some(floor);
                } else {
                    self.sliding = true;
                }
            }
        }
        match found {
            Some(h) => {
                if !was_grounded {
                    self.landed = true;
                    self.impact = self.impact.max(fall_before);
                }
                if h.id != self.ground_id {
                    self.ground_frame = None;
                }
                self.grounded = true;
                self.ground_normal = h.normal;
                self.ground_id = h.id;
                self.vel -= self.up * self.vel.dot(&self.up);
                if self.ground_frame.is_none() {
                    self.ground_frame = w.frame_of(h.id);
                }
            }
            None => {
                if was_grounded && !self.jumped {
                    self.left_ground = true;
                }
                self.grounded = false;
                self.ground_id = 0;
                self.ground_frame = None;
            }
        }
        self.probe_wall(w, wish, dt);
    }

    /// A walkable floor straight under the middle of the character.
    fn floor_under(&self, w: &mut dyn CharWorld, reach: f32) -> Option<Hit> {
        if self.foot_radius <= 0.0 {
            return None;
        }
        let from = self.pos + self.up * self.foot_radius;
        let hit = w.probe(from, -self.up * (self.foot_radius + reach + self.s.step_height))?;
        self.walkable(&hit.normal).then_some(hit)
    }

    fn probe_wall(&mut self, w: &mut dyn CharWorld, wish: V3, dt: f32) {
        self.on_wall = false;
        let towards = wish.try_normalize(1e-4).or_else(|| (self.wall_timer > 0.0).then_some(-self.wall_normal));
        if let Some(d) = towards {
            let reach = self.s.skin * 4.0;
            if let Some(h) = self.cast(w, self.pos, d * reach) {
                if h.normal.dot(&self.up).abs() < 0.35 {
                    self.on_wall = true;
                    self.wall_normal = h.normal;
                    self.wall_timer = self.s.coyote_time.max(0.05);
                    return;
                }
            }
        }
        self.wall_timer = (self.wall_timer - dt).max(0.0);
    }

    /// Moves as far as possible, sliding along whatever is in the way.
    fn move_and_slide(&mut self, w: &mut dyn CharWorld, motion: V3, grounded: bool) {
        let skin = self.s.skin;
        let mut rest = motion;
        let mut planes: Vec<V3> = Vec::with_capacity(4);
        let mut unstick = 0;
        for _ in 0..6 {
            let dist = rest.norm();
            if dist < 1e-6 {
                break;
            }
            let dir = rest / dist;
            let Some(hit) = self.cast(w, self.pos, dir * (dist + skin)) else {
                self.pos += rest;
                break;
            };
            if hit.penetrating {
                // started inside something: step out and try again
                unstick += 1;
                if unstick > 3 || hit.normal.norm() < 0.5 {
                    break;
                }
                self.pos += hit.normal * skin * 2.0;
                continue;
            }
            let travel = (hit.t * (dist + skin) - skin).clamp(0.0, dist);
            self.pos += dir * travel;
            let left = rest - dir * travel;

            if hit.dynamic && self.s.push > 0.0 {
                w.push(hit.id, -hit.normal, self.vel.norm(), self.s.push);
            }
            let up_part = hit.normal.dot(&self.up);
            let walkable = self.walkable(&hit.normal);
            if up_part < -0.3 {
                self.on_ceiling = true;
                self.jumping = false;
            }
            if !walkable && grounded && self.try_step(w, left) {
                self.stepped = true;
                break;
            }
            // Standing on the ground, a slope too steep to walk is a wall: its
            // upward lean must not lift the character.
            let mut n = hit.normal;
            if !walkable && grounded && up_part > 0.0 {
                n = (n - self.up * up_part).try_normalize(1e-4).unwrap_or(n);
            }
            let mut next = left - n * left.dot(&n);
            // into a corner between two surfaces: only along the crease
            for p in &planes {
                if next.dot(p) < -1e-5 {
                    let crease = n.cross(p);
                    next = match crease.try_normalize(1e-5) {
                        Some(c) => c * next.dot(&c),
                        None => V3::zeros(),
                    };
                    break;
                }
            }
            planes.push(n);
            // running onto a walkable slope changes the path, not the speed
            let into = self.vel.dot(&n);
            if into < 0.0 && !(walkable && grounded) {
                self.vel -= n * into;
            }
            rest = next;
        }
    }

    /// Up, across and back down: climbs a step too small to stop for.
    fn try_step(&mut self, w: &mut dyn CharWorld, across: V3) -> bool {
        let step = self.s.step_height;
        if step <= 0.0 {
            return false;
        }
        let flat = across - self.up * across.dot(&self.up);
        let dist = flat.norm();
        if dist < 1e-6 {
            return false;
        }
        let skin = self.s.skin;
        let dir = flat / dist;
        let rise = match self.cast(w, self.pos, self.up * (step + skin)) {
            Some(h) => (h.t * (step + skin) - skin).max(0.0),
            None => step,
        };
        if rise < 1e-4 {
            return false;
        }
        let high = self.pos + self.up * rise;
        // Look far enough ahead to be over the step, even though this frame's
        // move may be tiny: rounded feet only touch its edge at first.
        let look = dist.max(self.foot_radius);
        let clear = match self.cast(w, high, dir * (look + skin)) {
            Some(h) if h.penetrating => return false,
            Some(h) => (h.t * (look + skin) - skin).clamp(0.0, look),
            None => look,
        };
        if clear < 1e-4 {
            return false;
        }
        let Some(floor) = self.cast(w, high + dir * clear, -self.up * (rise + skin)) else { return false };
        if floor.penetrating || !self.walkable(&floor.normal) {
            return false;
        }
        let over = high + dir * dist.min(clear);
        let Some(down) = self.cast(w, over, -self.up * (rise + skin)) else { return false };
        if down.penetrating {
            return false;
        }
        let landed = over - self.up * (down.t * (rise + skin) - skin).max(0.0);
        if (landed - self.pos).dot(&self.up) < 1e-4 {
            return false;
        }
        self.pos = landed;
        true
    }

    /// A word for what the character is doing, to pick an animation by.
    pub fn state_name(&self) -> &'static str {
        if self.mode != MODE_WALK {
            return "fly";
        }
        let flat = (self.vel - self.up * self.vel.dot(&self.up)).norm();
        if self.dash_time > 0.0 {
            "dash"
        } else if self.wall_sliding {
            "wallslide"
        } else if self.sliding {
            "slide"
        } else if !self.grounded {
            if self.vel.dot(&self.up) > 0.0 {
                "jump"
            } else {
                "fall"
            }
        } else if self.crouching {
            if flat > self.s.speed * 0.05 {
                "crawl"
            } else {
                "crouch"
            }
        } else if flat < self.s.speed * 0.05 {
            "idle"
        } else if flat > self.s.speed * 1.1 {
            "run"
        } else {
            "walk"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UP: V3 = V3::new(0.0, 1.0, 0.0);
    const DT: f32 = 1.0 / 60.0;

    #[derive(Clone)]
    struct Block {
        lo: V3,
        hi: V3,
        id: i32,
        one_way: bool,
    }

    /// A slope rising along +X from `x0`, between `x0` and `x1`.
    #[derive(Clone)]
    struct Ramp {
        x0: f32,
        x1: f32,
        rise: f32,
        id: i32,
    }

    #[derive(Default)]
    struct Boxes {
        blocks: Vec<Block>,
        ramps: Vec<Ramp>,
        moved: Vec<(i32, V3)>,
        pushed: Vec<i32>,
        dynamic: Vec<i32>,
    }

    impl Boxes {
        fn add(&mut self, lo: (f32, f32, f32), hi: (f32, f32, f32)) -> i32 {
            let id = self.blocks.len() as i32 + 1;
            self.blocks.push(Block { lo: V3::new(lo.0, lo.1, lo.2), hi: V3::new(hi.0, hi.1, hi.2), id, one_way: false });
            id
        }

        fn floor(&mut self) -> i32 {
            self.add((-1000.0, -1.0, -1000.0), (1000.0, 0.0, 1000.0))
        }

        fn ray(&self, from: V3, motion: V3, ignore: &[i32]) -> Option<Hit> {
            let mut best: Option<Hit> = None;
            for b in self.blocks.iter().filter(|b| !ignore.contains(&b.id)) {
                let inside = (0..3).all(|a| from[a] > b.lo[a] + 1e-6 && from[a] < b.hi[a] - 1e-6);
                if inside {
                    return Some(Hit { t: 0.0, normal: UP, id: b.id, one_way: b.one_way, dynamic: self.dynamic.contains(&b.id), penetrating: true });
                }
                let (mut enter, mut leave, mut axis) = (0.0f32, 1.0f32, None);
                let mut miss = false;
                for a in 0..3 {
                    if motion[a].abs() < 1e-9 {
                        if from[a] < b.lo[a] || from[a] > b.hi[a] {
                            miss = true;
                        }
                        continue;
                    }
                    let (t1, t2) = ((b.lo[a] - from[a]) / motion[a], (b.hi[a] - from[a]) / motion[a]);
                    let (near, far) = (t1.min(t2), t1.max(t2));
                    if near > enter {
                        enter = near;
                        axis = Some(a);
                    }
                    leave = leave.min(far);
                }
                if miss || enter > leave || axis.is_none() || best.is_some_and(|h| h.t <= enter) {
                    continue;
                }
                let a = axis.unwrap();
                let mut normal = V3::zeros();
                normal[a] = -motion[a].signum();
                best = Some(Hit { t: enter, normal, id: b.id, one_way: b.one_way, dynamic: self.dynamic.contains(&b.id), penetrating: false });
            }
            for r in self.ramps.iter().filter(|r| !ignore.contains(&r.id)) {
                let slope = r.rise / (r.x1 - r.x0);
                let height = |p: &V3| p.y - slope * (p.x - r.x0);
                let (h0, h1) = (height(&from), height(&(from + motion)));
                if h0 >= 0.0 && h1 < 0.0 {
                    let t = h0 / (h0 - h1);
                    let x = from.x + motion.x * t;
                    if x >= r.x0 && x <= r.x1 && best.is_none_or(|h| t < h.t) {
                        let normal = V3::new(-slope, 1.0, 0.0).normalize();
                        best = Some(Hit { t, normal, id: r.id, one_way: false, dynamic: false, penetrating: false });
                    }
                }
            }
            best
        }
    }

    impl CharWorld for Boxes {
        fn cast(&mut self, feet: V3, motion: V3, height: f32, ignore: &[i32]) -> Option<Hit> {
            // the shape is two points: feet and head
            let a = self.ray(feet, motion, ignore);
            let b = self.ray(feet + UP * height, motion, ignore);
            match (a, b) {
                (Some(x), Some(y)) => Some(if x.t <= y.t { x } else { y }),
                (x, y) => x.or(y),
            }
        }

        fn frame_of(&mut self, id: i32) -> Option<Matrix4<f32>> {
            let m = self.moved.iter().find(|m| m.0 == id)?;
            Some(Matrix4::new_translation(&m.1))
        }

        fn push(&mut self, id: i32, _dir: V3, _speed: f32, _strength: f32) {
            self.pushed.push(id);
        }
    }

    fn hero(y: f32) -> Character {
        Character::new(V3::new(0.0, y, 0.0), UP, 1.8, Settings::defaults_3d())
    }

    fn run(c: &mut Character, w: &mut Boxes, frames: usize) {
        for _ in 0..frames {
            c.update(w, DT);
        }
    }

    fn close(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() < tolerance
    }

    fn standing() -> (Character, Boxes) {
        let mut w = Boxes::default();
        w.floor();
        let mut c = hero(0.1);
        run(&mut c, &mut w, 10);
        assert!(c.grounded);
        (c, w)
    }

    #[test]
    fn falls_lands_and_reports_it() {
        let mut w = Boxes::default();
        w.floor();
        let mut c = hero(2.0);
        let mut landed = 0;
        let mut impact = 0.0f32;
        for _ in 0..60 {
            c.update(&mut w, DT);
            if c.landed {
                landed += 1;
                impact = c.impact;
            }
        }
        assert_eq!(landed, 1);
        assert!(c.grounded);
        assert!(c.pos.y.abs() < 0.05, "{}", c.pos.y);
        // falling 2 m under gravity 20 is about 8.9 m/s
        assert!(close(impact, 8.9, 0.5), "{impact}");
    }

    #[test]
    fn runs_at_its_speed_and_stops() {
        let (mut c, mut w) = standing();
        c.wish = V3::new(0.0, 0.0, 1.0);
        run(&mut c, &mut w, 60);
        assert!(close(c.vel.z, 5.0, 0.01));
        assert!(close(c.pos.z, 4.75, 0.3), "{}", c.pos.z);
        assert_eq!(c.state_name(), "walk");
        c.wish = V3::zeros();
        run(&mut c, &mut w, 20);
        assert_eq!(c.vel.z, 0.0);
        assert_eq!(c.state_name(), "idle");
    }

    #[test]
    fn a_long_wish_is_limited_and_sprint_is_faster() {
        let (mut c, mut w) = standing();
        c.wish = V3::new(1.0, 0.0, 1.0);
        run(&mut c, &mut w, 40);
        assert!(close(c.vel.norm(), 5.0, 0.01));
        c.sprint = true;
        run(&mut c, &mut w, 40);
        assert!(close(c.vel.norm(), 8.0, 0.01));
        assert_eq!(c.state_name(), "run");
    }

    #[test]
    fn jumps_to_the_expected_height() {
        let (mut c, mut w) = standing();
        c.press_jump();
        c.update(&mut w, DT);
        assert!(c.jumped && !c.grounded);
        let mut top = 0.0f32;
        for _ in 0..90 {
            c.update(&mut w, DT);
            top = top.max(c.pos.y);
        }
        // v * v / (2 * g)
        assert!(close(top, 1.6, 0.12), "{top}");
        assert!(c.grounded);
    }

    #[test]
    fn letting_go_early_makes_a_shorter_jump() {
        let (mut c, mut w) = standing();
        c.jump_held = false;
        c.press_jump();
        let mut top = 0.0f32;
        for _ in 0..90 {
            c.update(&mut w, DT);
            top = top.max(c.pos.y);
        }
        assert!(top < 0.6, "{top}");
    }

    #[test]
    fn coyote_time_allows_a_late_jump_and_ends() {
        let mut w = Boxes::default();
        w.add((-10.0, -1.0, -10.0), (10.0, 0.0, 0.0));
        let start = |w: &mut Boxes| {
            let mut c = hero(0.1);
            c.pos.z = -0.5;
            run(&mut c, w, 10);
            assert!(c.grounded);
            c.wish = V3::new(0.0, 0.0, 1.0);
            let mut off = false;
            for _ in 0..200 {
                c.update(w, DT);
                if c.left_ground {
                    off = true;
                    break;
                }
            }
            assert!(off);
            c
        };
        let mut c = start(&mut w);
        run(&mut c, &mut w, 3);
        c.press_jump();
        c.update(&mut w, DT);
        assert!(c.jumped, "still allowed just after the edge");

        let mut c = start(&mut w);
        run(&mut c, &mut w, 12);
        c.press_jump();
        c.update(&mut w, DT);
        assert!(!c.jumped, "too late");
    }

    #[test]
    fn a_press_just_before_landing_still_jumps() {
        let mut w = Boxes::default();
        w.floor();
        let mut c = hero(0.6);
        // falling: press about 4 frames before reaching the floor
        while c.pos.y > 0.35 {
            c.update(&mut w, DT);
        }
        c.press_jump();
        let mut jumped = false;
        for _ in 0..30 {
            c.update(&mut w, DT);
            jumped |= c.jumped;
        }
        assert!(jumped);
    }

    #[test]
    fn air_jumps_are_counted_and_refilled() {
        let (mut c, mut w) = standing();
        c.s.air_jumps = 1.0;
        c.update(&mut w, DT);
        c.press_jump();
        run(&mut c, &mut w, 20);
        c.press_jump();
        c.update(&mut w, DT);
        assert!(c.jumped, "second jump");
        run(&mut c, &mut w, 10);
        c.press_jump();
        c.update(&mut w, DT);
        assert!(!c.jumped, "no third jump");
        c.buffer = 0.0;
        run(&mut c, &mut w, 120);
        assert_eq!(c.air_left, 1);
    }

    #[test]
    fn walls_stop_and_diagonals_slide() {
        let (mut c, mut w) = standing();
        w.add((-50.0, 0.0, 3.0), (50.0, 5.0, 4.0));
        c.wish = V3::new(1.0, 0.0, 1.0);
        run(&mut c, &mut w, 180);
        assert!(c.pos.z < 3.0 && c.pos.z > 2.9, "{}", c.pos.z);
        assert!(c.pos.x > 5.0, "kept sliding along the wall: {}", c.pos.x);
        assert!(c.on_wall);
        assert!(c.grounded);
    }

    #[test]
    fn climbs_steps_but_not_walls() {
        let (mut c, mut w) = standing();
        w.add((-5.0, 0.0, 2.0), (5.0, 0.3, 50.0));
        c.wish = V3::new(0.0, 0.0, 1.0);
        run(&mut c, &mut w, 60);
        assert!(c.pos.z > 2.5, "{}", c.pos.z);
        assert!(close(c.pos.y, 0.3, 0.05), "{}", c.pos.y);

        let (mut c, mut w) = standing();
        w.add((-5.0, 0.0, 2.0), (5.0, 0.6, 50.0));
        c.wish = V3::new(0.0, 0.0, 1.0);
        run(&mut c, &mut w, 60);
        assert!(c.pos.z < 2.0, "{}", c.pos.z);
        assert!(c.pos.y < 0.05);
    }

    #[test]
    fn rounded_feet_can_stand_on_the_edge_of_a_step() {
        // the shape reports a slanted edge; the ray finds flat floor below
        struct Edge;
        impl CharWorld for Edge {
            fn cast(&mut self, feet: V3, motion: V3, _height: f32, _ignore: &[i32]) -> Option<Hit> {
                (motion.y < 0.0 && feet.y + motion.y < 0.0).then(|| Hit {
                    t: (feet.y / -motion.y).clamp(0.0, 1.0),
                    normal: V3::new(0.6, 0.5, 0.0).normalize(),
                    id: 1,
                    one_way: false,
                    dynamic: false,
                    penetrating: false,
                })
            }
            fn probe(&mut self, from: V3, motion: V3) -> Option<Hit> {
                Some(Hit { t: (from.y + 0.3) / -motion.y, normal: UP, id: 2, one_way: false, dynamic: false, penetrating: false })
            }
        }
        let mut c = hero(0.1);
        c.foot_radius = 0.4;
        let mut w = Edge;
        for _ in 0..30 {
            c.update(&mut w, DT);
        }
        assert!(c.grounded && !c.sliding);
        assert_eq!(c.ground_id, 2);
    }

    #[test]
    fn walks_up_gentle_slopes_at_full_speed_and_sticks_going_down() {
        let (mut c, mut w) = standing();
        // 30 degrees
        w.ramps.push(Ramp { x0: 1.0, x1: 21.0, rise: 11.55, id: 90 });
        c.wish = V3::new(1.0, 0.0, 0.0);
        run(&mut c, &mut w, 90);
        assert!(c.grounded);
        assert!(c.pos.y > 3.0, "climbed: {}", c.pos.y);
        assert!(close(c.pos.y, (c.pos.x - 1.0) * 0.5775, 0.1), "on the slope: {:?}", c.pos);
        assert!(close(c.vel.x, 5.0, 0.3), "{}", c.vel.x);

        // back down: never leaves the ground
        c.wish = V3::new(-1.0, 0.0, 0.0);
        run(&mut c, &mut w, 5);
        for _ in 0..40 {
            c.update(&mut w, DT);
            assert!(c.grounded, "left the ramp at {:?}", c.pos);
        }
    }

    #[test]
    fn slopes_too_steep_cannot_be_climbed() {
        let (mut c, mut w) = standing();
        // 60 degrees
        w.ramps.push(Ramp { x0: 1.0, x1: 21.0, rise: 34.6, id: 90 });
        c.wish = V3::new(1.0, 0.0, 0.0);
        run(&mut c, &mut w, 120);
        assert!(c.pos.x < 1.1 && c.pos.y < 0.2, "{:?}", c.pos);
    }

    #[test]
    fn ceilings_stop_a_jump() {
        let (mut c, mut w) = standing();
        w.add((-5.0, 2.3, -5.0), (5.0, 3.0, 5.0));
        c.press_jump();
        let mut bumped = false;
        let mut top = 0.0f32;
        for _ in 0..60 {
            c.update(&mut w, DT);
            bumped |= c.on_ceiling;
            top = top.max(c.pos.y);
        }
        assert!(bumped);
        assert!(top < 0.51, "{top}");
    }

    #[test]
    fn moving_platforms_carry_and_add_momentum_to_a_jump() {
        let mut w = Boxes::default();
        let plat = w.add((-50.0, -1.0, -50.0), (50.0, 0.0, 50.0));
        w.moved.push((plat, V3::zeros()));
        let mut c = hero(0.1);
        run(&mut c, &mut w, 10);
        assert_eq!(c.ground_id, plat);
        let start = c.pos.x;
        for i in 1..=30 {
            w.moved[0].1.x = i as f32 * 0.1;
            c.update(&mut w, DT);
        }
        assert!(close(c.pos.x - start, 3.0, 0.11), "{}", c.pos.x - start);
        w.moved[0].1.x += 0.1;
        c.press_jump();
        c.update(&mut w, DT);
        assert!(close(c.vel.x, 6.0, 0.5), "{}", c.vel.x);
    }

    #[test]
    fn one_way_platforms() {
        let mut w = Boxes::default();
        w.floor();
        let id = w.add((-10.0, 2.0, -10.0), (10.0, 2.2, 10.0));
        w.blocks[1].one_way = true;
        let mut c = hero(0.1);
        c.height = 0.1;
        c.stand_height = 0.1;
        run(&mut c, &mut w, 10);
        c.s.jump_speed = 12.0;
        c.press_jump();
        let mut top = 0.0f32;
        for _ in 0..80 {
            c.update(&mut w, DT);
            top = top.max(c.pos.y);
        }
        assert!(top > 3.0, "passed up through it: {top}");
        assert_eq!(c.ground_id, id, "landed on top");
        assert!(close(c.pos.y, 2.2, 0.05), "{}", c.pos.y);
        c.press_drop();
        run(&mut c, &mut w, 60);
        assert!(c.pos.y < 0.1, "dropped through: {}", c.pos.y);
    }

    #[test]
    fn wall_slide_and_wall_jump() {
        let mut w = Boxes::default();
        w.floor();
        w.add((1.0, 0.0, -9.0), (2.0, 30.0, 9.0));
        let mut c = hero(10.0);
        c.s.wall_slide = 2.0;
        c.s.wall_jump = 1.0;
        c.wish = V3::new(1.0, 0.0, 0.0);
        run(&mut c, &mut w, 40);
        assert!(c.on_wall && c.wall_sliding);
        assert!(close(c.vel.y, -2.0, 0.01), "{}", c.vel.y);
        assert_eq!(c.state_name(), "wallslide");
        c.press_jump();
        c.update(&mut w, DT);
        assert!(c.wall_jumped);
        assert!(c.vel.x < -5.0, "{}", c.vel.x);
        assert!(c.vel.y > 7.0, "{}", c.vel.y);
    }

    #[test]
    fn dash_ignores_gravity_and_ends() {
        let mut w = Boxes::default();
        let mut c = hero(50.0);
        c.dash(V3::new(1.0, 0.0, 0.0), 20.0, 0.25);
        run(&mut c, &mut w, 10);
        assert_eq!(c.state_name(), "dash");
        assert!(close(c.pos.y, 50.0, 0.01), "{}", c.pos.y);
        assert!(close(c.pos.x, 10.0 / 60.0 * 20.0, 0.01));
        run(&mut c, &mut w, 10);
        assert_ne!(c.state_name(), "dash");
    }

    #[test]
    fn crouching_lowers_the_head_and_waits_for_room() {
        let (mut c, mut w) = standing();
        w.add((-5.0, 1.2, 2.0), (5.0, 2.0, 6.0));
        c.crouch_wanted = true;
        c.wish = V3::new(0.0, 0.0, 1.0);
        run(&mut c, &mut w, 240);
        assert!(close(c.height, 1.0, 0.001));
        assert!(c.pos.z > 2.5, "went under: {}", c.pos.z);
        c.pos.z = 4.0;
        c.wish = V3::zeros();
        c.crouch_wanted = false;
        run(&mut c, &mut w, 5);
        assert!(c.crouching, "no room to stand");
        c.pos.z = -3.0;
        run(&mut c, &mut w, 5);
        assert!(!c.crouching);
        assert!(close(c.height, 1.8, 0.001));
    }

    #[test]
    fn crouching_is_slower() {
        let (mut c, mut w) = standing();
        c.crouch_wanted = true;
        c.wish = V3::new(0.0, 0.0, 1.0);
        run(&mut c, &mut w, 60);
        assert!(close(c.vel.z, 2.5, 0.01));
        assert_eq!(c.state_name(), "crawl");
    }

    #[test]
    fn a_shove_moves_and_fades() {
        let (mut c, mut w) = standing();
        c.impulse(V3::new(6.0, 0.0, 0.0));
        run(&mut c, &mut w, 10);
        assert!(c.pos.x > 0.4, "{}", c.pos.x);
        run(&mut c, &mut w, 60);
        let x = c.pos.x;
        run(&mut c, &mut w, 10);
        assert_eq!(c.pos.x, x, "it died away");
        c.impulse(V3::new(0.0, 5.0, 0.0));
        c.update(&mut w, DT);
        assert!(!c.grounded);
    }

    #[test]
    fn flying_ignores_gravity_but_not_walls() {
        let (mut c, mut w) = standing();
        c.mode = MODE_FLY;
        c.wish = V3::new(0.0, 1.0, 0.0);
        run(&mut c, &mut w, 60);
        assert!(c.pos.y > 4.0, "{}", c.pos.y);
        assert_eq!(c.state_name(), "fly");
        w.add((-5.0, 0.0, 2.0), (5.0, 50.0, 3.0));
        c.wish = V3::new(0.0, 0.0, 1.0);
        run(&mut c, &mut w, 120);
        assert!(c.pos.z < 2.0, "{}", c.pos.z);
        c.wish = V3::zeros();
        run(&mut c, &mut w, 30);
        let y = c.pos.y;
        run(&mut c, &mut w, 30);
        assert_eq!(c.pos.y, y, "hovers");
    }

    #[test]
    fn noclip_goes_through_anything() {
        let (mut c, mut w) = standing();
        w.add((-5.0, 0.0, 2.0), (5.0, 50.0, 3.0));
        c.mode = MODE_NOCLIP;
        c.wish = V3::new(0.0, 0.0, 1.0);
        run(&mut c, &mut w, 60);
        assert!(c.pos.z > 4.0);
    }

    #[test]
    fn walking_into_a_dynamic_body_pushes_it() {
        let (mut c, mut w) = standing();
        let crate_id = w.add((-1.0, 0.0, 2.0), (1.0, 2.0, 4.0));
        w.dynamic.push(crate_id);
        c.wish = V3::new(0.0, 0.0, 1.0);
        run(&mut c, &mut w, 60);
        assert!(w.pushed.contains(&crate_id));
    }

    #[test]
    fn works_the_same_with_y_down() {
        // pixels, up is -Y: a floor whose top is y = 100
        struct Floor;
        impl CharWorld for Floor {
            fn cast(&mut self, feet: V3, motion: V3, _height: f32, _ignore: &[i32]) -> Option<Hit> {
                let to = feet + motion;
                (feet.y <= 100.0 && to.y > 100.0).then(|| Hit {
                    t: (100.0 - feet.y) / motion.y,
                    normal: V3::new(0.0, -1.0, 0.0),
                    id: 1,
                    one_way: false,
                    dynamic: false,
                    penetrating: false,
                })
            }
        }
        let mut c = Character::new(V3::new(0.0, 50.0, 0.0), V3::new(0.0, -1.0, 0.0), 32.0, Settings::defaults_2d());
        let mut w = Floor;
        for _ in 0..60 {
            c.update(&mut w, DT);
        }
        assert!(c.grounded);
        assert!(close(c.pos.y, 100.0, 1.0), "{}", c.pos.y);
        c.press_jump();
        c.update(&mut w, DT);
        assert!(c.vel.y < -500.0, "{}", c.vel.y);
        for _ in 0..12 {
            c.update(&mut w, DT);
        }
        assert!(c.pos.y < 40.0, "up is smaller y: {}", c.pos.y);
    }

    #[test]
    fn settings_are_found_by_name() {
        let mut s = Settings::defaults_3d();
        *s.field("JUMP_SPEED").unwrap() = 3.0;
        assert_eq!(s.jump_speed, 3.0);
        assert!(s.field("nonsense").is_none());
    }

    #[test]
    fn a_long_frame_is_broken_into_short_steps() {
        let mut w = Boxes::default();
        w.floor();
        let mut c = hero(0.5);
        c.update(&mut w, 0.13);
        assert!(c.pos.y > -0.01, "did not go through the floor: {}", c.pos.y);
    }
}
