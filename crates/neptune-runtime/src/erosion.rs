//! Terrain erosion (ext): `ErodeTerrain` and `ThermalErodeTerrain`.
//!
//! Hydraulic erosion drops simulated raindrops on the terrain. Each runs
//! downhill, picking up soil where it speeds up and dropping it where it
//! slows or pools, so slopes get gullies and ridges and valleys fill with
//! sediment. Thermal erosion lets slopes steeper than an angle crumble until
//! they are no steeper. Heights are worked on in grid squares (`vertical` is
//! the terrain's Y scale over its X/Z scale), so the result does not depend
//! on how the terrain is scaled later as long as its proportions stay.

/// A small seeded random number generator (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Height and gradient at a point between grid vertices (`w` vertices a row).
fn height_gradient(h: &[f32], w: usize, x: f32, z: f32) -> (f32, f32, f32) {
    let (xi, zi) = (x as usize, z as usize);
    let (u, v) = (x - xi as f32, z - zi as f32);
    let i = zi * w + xi;
    let (nw, ne, sw, se) = (h[i], h[i + 1], h[i + w], h[i + w + 1]);
    let gx = (ne - nw) * (1.0 - v) + (se - sw) * v;
    let gz = (sw - nw) * (1.0 - u) + (se - ne) * u;
    let height = nw * (1.0 - u) * (1.0 - v) + ne * u * (1.0 - v) + sw * (1.0 - u) * v + se * u * v;
    (height, gx, gz)
}

/// Hydraulic erosion of a square grid of `w` x `w` heights (in grid units).
pub fn hydraulic(h: &mut [f32], w: usize, drops: usize, seed: u64) {
    if w < 4 {
        return;
    }
    const INERTIA: f32 = 0.05;
    const CAPACITY: f32 = 4.0;
    const MIN_SLOPE: f32 = 0.01;
    const DEPOSIT: f32 = 0.3;
    const ERODE: f32 = 0.3;
    const EVAPORATE: f32 = 0.01;
    const GRAVITY: f32 = 4.0;
    const STEPS: usize = 64;
    const RADIUS: i32 = 3;
    // the soil a drop takes comes from a small round area, weighted to the middle
    let mut brush: Vec<(i32, i32, f32)> = Vec::new();
    for dz in -RADIUS..=RADIUS {
        for dx in -RADIUS..=RADIUS {
            let d = ((dx * dx + dz * dz) as f32).sqrt();
            if d < RADIUS as f32 {
                brush.push((dx, dz, 1.0 - d / RADIUS as f32));
            }
        }
    }
    let total: f32 = brush.iter().map(|b| b.2).sum();
    for b in &mut brush {
        b.2 /= total;
    }
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let max = (w - 2) as f32;
    for _ in 0..drops {
        let (mut x, mut z) = (rng.next() * max, rng.next() * max);
        let (mut dx, mut dz) = (0.0f32, 0.0f32);
        let (mut speed, mut water, mut soil) = (1.0f32, 1.0f32, 0.0f32);
        for _ in 0..STEPS {
            let (xi, zi) = (x as usize, z as usize);
            let (u, v) = (x - xi as f32, z - zi as f32);
            let (height, gx, gz) = height_gradient(h, w, x, z);
            // turn downhill, keeping some of the old direction
            dx = dx * INERTIA - gx * (1.0 - INERTIA);
            dz = dz * INERTIA - gz * (1.0 - INERTIA);
            let len = (dx * dx + dz * dz).sqrt();
            if len < 1e-6 {
                // a flat spot: pick a random direction
                let a = rng.next() * std::f32::consts::TAU;
                (dx, dz) = (a.cos(), a.sin());
            } else {
                dx /= len;
                dz /= len;
            }
            let (nx, nz) = (x + dx, z + dz);
            if !(0.0..max).contains(&nx) || !(0.0..max).contains(&nz) {
                break;
            }
            let fall = height - height_gradient(h, w, nx, nz).0;
            let capacity = (-fall).max(MIN_SLOPE) * speed * water * CAPACITY;
            let i = zi * w + xi;
            if soil > capacity || fall < 0.0 {
                // uphill or overloaded: drop soil at the four corners around the drop
                let drop = if fall < 0.0 { (-fall).min(soil) } else { (soil - capacity) * DEPOSIT };
                soil -= drop;
                h[i] += drop * (1.0 - u) * (1.0 - v);
                h[i + 1] += drop * u * (1.0 - v);
                h[i + w] += drop * (1.0 - u) * v;
                h[i + w + 1] += drop * u * v;
            } else {
                // downhill with room for more: take soil from around the drop
                let take = ((capacity - soil) * ERODE).min(fall.max(0.0));
                for &(bx, bz, weight) in &brush {
                    let (cx, cz) = (xi as i32 + bx, zi as i32 + bz);
                    if cx < 0 || cz < 0 || cx >= w as i32 || cz >= w as i32 {
                        continue;
                    }
                    let j = cz as usize * w + cx as usize;
                    let amount = (take * weight).min(h[j].max(0.0));
                    h[j] -= amount;
                    soil += amount;
                }
            }
            speed = (speed * speed + fall * GRAVITY).max(0.0).sqrt();
            water *= 1.0 - EVAPORATE;
            (x, z) = (nx, nz);
        }
    }
}

/// Thermal erosion: slopes steeper than `talus` (rise per grid square)
/// shed half their excess to the lower neighbour, `iterations` times.
pub fn thermal(h: &mut [f32], w: usize, iterations: usize, talus: f32) {
    let n = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)];
    for _ in 0..iterations {
        let mut delta = vec![0.0f32; h.len()];
        for z in 0..w as i32 {
            for x in 0..w as i32 {
                let i = (z as usize) * w + x as usize;
                for (dx, dz) in n {
                    let (cx, cz) = (x + dx, z + dz);
                    if cx < 0 || cz < 0 || cx >= w as i32 || cz >= w as i32 {
                        continue;
                    }
                    let j = cz as usize * w + cx as usize;
                    let diff = h[i] - h[j];
                    if diff > talus {
                        let mv = (diff - talus) * 0.25;
                        delta[i] -= mv;
                        delta[j] += mv;
                    }
                }
            }
        }
        for (v, d) in h.iter_mut().zip(delta) {
            *v += d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cone(w: usize) -> Vec<f32> {
        let c = (w - 1) as f32 / 2.0;
        (0..w * w).map(|i| (c - (((i % w) as f32 - c).powi(2) + ((i / w) as f32 - c).powi(2)).sqrt()).max(0.0)).collect()
    }

    #[test]
    fn rain_cuts_the_slopes_and_keeps_most_soil() {
        let w = 33;
        let mut h = cone(w);
        let before: f32 = h.iter().sum();
        let peak = h[16 * w + 16];
        hydraulic(&mut h, w, 3000, 7);
        let after: f32 = h.iter().sum();
        assert!(h.iter().all(|v| v.is_finite()));
        assert!(h != cone(w), "nothing changed");
        // soil moves rather than vanishing (drops leaving the map take some)
        assert!(after > before * 0.8 && after <= before * 1.01, "{before} {after}");
        assert!(h[16 * w + 16] <= peak + 1e-3);
        // the same seed gives the same terrain
        let mut again = cone(w);
        hydraulic(&mut again, w, 3000, 7);
        assert_eq!(h, again);
    }

    #[test]
    fn steep_slopes_crumble_to_the_angle() {
        let w = 9;
        let mut h = vec![0.0; w * w];
        h[4 * w + 4] = 10.0;
        let before: f32 = h.iter().sum();
        thermal(&mut h, w, 400, 1.0);
        let after: f32 = h.iter().sum();
        assert!((before - after).abs() < 1e-3);
        assert!(h[4 * w + 4] < 4.0);
        for z in 0..w {
            for x in 0..w - 1 {
                assert!((h[z * w + x] - h[z * w + x + 1]).abs() <= 1.0 + 0.05);
            }
        }
    }
}
