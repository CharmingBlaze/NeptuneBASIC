//! Easing curves (ext): `Ease`, `EaseBetween` and the `EASE_*` constants,
//! also used by the entity tweens in the runtime.
//!
//! An easing curve turns time (0 at the start, 1 at the end) into how far
//! along the movement is, so things speed up and slow down instead of
//! moving at one speed: "out" curves start fast and settle, "in" curves
//! start slow, "in out" do both; "back" overshoots a little, "elastic"
//! wobbles, "bounce" bounces.

use neptune_lang::commands::ConstVal;

const NAMES: [&str; 17] = [
    "LINEAR",
    "IN",
    "OUT",
    "IN_OUT",
    "IN_CUBIC",
    "OUT_CUBIC",
    "IN_OUT_CUBIC",
    "IN_SINE",
    "OUT_SINE",
    "IN_OUT_SINE",
    "IN_BACK",
    "OUT_BACK",
    "IN_OUT_BACK",
    "IN_ELASTIC",
    "OUT_ELASTIC",
    "IN_BOUNCE",
    "OUT_BOUNCE",
];

/// `EASE_LINEAR` = 0, `EASE_IN` = 1, ...
pub fn constants() -> Vec<(String, ConstVal)> {
    NAMES.iter().enumerate().map(|(i, n)| (format!("EASE_{n}"), ConstVal::Int(i as i32))).collect()
}

fn out_bounce(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984375
    }
}

/// How far along (0 at t = 0, 1 at t = 1; "back" and "elastic" go a little
/// beyond) curve `kind` is at time `t`, which is clamped to 0-1.
pub fn ease(t: f32, kind: i32) -> f32 {
    use std::f32::consts::PI;
    let t = t.clamp(0.0, 1.0);
    const C1: f32 = 1.70158;
    const C2: f32 = C1 * 1.525;
    const C3: f32 = C1 + 1.0;
    match kind {
        1 => t * t,
        2 => 1.0 - (1.0 - t) * (1.0 - t),
        3 => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
        4 => t * t * t,
        5 => 1.0 - (1.0 - t).powi(3),
        6 => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        7 => 1.0 - (t * PI / 2.0).cos(),
        8 => (t * PI / 2.0).sin(),
        9 => -((PI * t).cos() - 1.0) / 2.0,
        10 => C3 * t * t * t - C1 * t * t,
        11 => 1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2),
        12 => {
            if t < 0.5 {
                (2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2) / 2.0
            } else {
                ((2.0 * t - 2.0).powi(2) * ((C2 + 1.0) * (t * 2.0 - 2.0) + C2) + 2.0) / 2.0
            }
        }
        13 | 14 => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let c4 = 2.0 * PI / 3.0;
            if kind == 13 {
                -(2.0f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin()
            } else {
                2.0f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
            }
        }
        15 => 1.0 - out_bounce(1.0 - t),
        16 => out_bounce(t),
        _ => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_starts_at_0_and_ends_at_1() {
        for k in 0..NAMES.len() as i32 {
            assert!(ease(0.0, k).abs() < 1e-4, "kind {k} at 0");
            assert!((ease(1.0, k) - 1.0).abs() < 1e-4, "kind {k} at 1");
        }
        assert_eq!(ease(0.5, 0), 0.5);
        assert!(ease(0.5, 1) < 0.5 && ease(0.5, 2) > 0.5);
        assert!((ease(0.5, 3) - 0.5).abs() < 1e-6);
        // back overshoots, and time outside 0-1 is clamped
        assert!((0..100).any(|i| ease(i as f32 / 100.0, 11) > 1.0));
        assert_eq!(ease(2.0, 1), 1.0);
        assert!(constants().iter().any(|(n, v)| n == "EASE_OUT_BOUNCE" && *v == ConstVal::Int(16)));
    }
}
