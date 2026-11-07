//! Blitz3D-compatible transform math.
//!
//! NeptuneBASIC programs use Blitz3D's left-handed coordinate system
//! (+X right, +Y up, +Z forward, angles in degrees, positive pitch = nose
//! down, positive yaw = turn left). Fyrox renders with +X to the left of
//! the screen, so every transform is mirrored across X on the way into the
//! engine: `p_f = S p_b`, `R_f = S R_b S` with `S = diag(-1, 1, 1)`. This
//! conjugation is a group homomorphism, so parent/child composition in
//! Fyrox's scene graph matches composition in Blitz space exactly.

use fyrox::core::algebra::{Matrix3, Matrix4, UnitQuaternion, Vector3};

/// Rotation about X (Blitz `pitchMatrix`).
fn rx(a: f32) -> Matrix3<f32> {
    let (s, c) = a.sin_cos();
    Matrix3::new(1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c)
}
/// Standard rotation about Y; Blitz yaw is `ry(-yaw)`.
fn ry(a: f32) -> Matrix3<f32> {
    let (s, c) = a.sin_cos();
    Matrix3::new(c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c)
}
/// Rotation about Z (Blitz `rollMatrix`).
fn rz(a: f32) -> Matrix3<f32> {
    let (s, c) = a.sin_cos();
    Matrix3::new(c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0)
}

/// Blitz `rotationMatrix(pitch, yaw, roll)` (degrees): `yaw * pitch * roll`.
/// The quaternion of a rotation matrix. Closed form (Shepperd's method),
/// renormalised so a matrix that drifted a little from many turns still
/// gives a clean rotation; nalgebra's `from_matrix` is an iterative solver
/// and was a large part of the cost of moving thousands of entities.
pub fn quat(m: &Matrix3<f32>) -> UnitQuaternion<f32> {
    let q = UnitQuaternion::from_rotation_matrix(&fyrox::core::algebra::Rotation3::from_matrix_unchecked(*m));
    UnitQuaternion::new_normalize(q.into_inner())
}

pub fn rotation(pitch: f32, yaw: f32, roll: f32) -> Matrix3<f32> {
    ry(-yaw.to_radians()) * rx(pitch.to_radians()) * rz(roll.to_radians())
}

/// Blitz `Vector::yaw()` in degrees.
pub fn vector_yaw(v: &Vector3<f32>) -> f32 {
    (-v.x.atan2(v.z)).to_degrees()
}

/// Blitz `Vector::pitch()` in degrees.
pub fn vector_pitch(v: &Vector3<f32>) -> f32 {
    (-v.y.atan2((v.x * v.x + v.z * v.z).sqrt())).to_degrees()
}

/// (pitch, yaw, roll) in degrees of a rotation matrix.
pub fn angles(m: &Matrix3<f32>) -> (f32, f32, f32) {
    let k: Vector3<f32> = m.column(2).into();
    let i: Vector3<f32> = m.column(0).into();
    let j: Vector3<f32> = m.column(1).into();
    (vector_pitch(&k), vector_yaw(&k), i.y.atan2(j.y).to_degrees())
}

/// Re-orthonormalizes a rotation matrix (prevents drift from repeated turns).
pub fn orthonormalize(m: &Matrix3<f32>) -> Matrix3<f32> {
    let k = m.column(2).normalize();
    let i = m.column(1).cross(&k).normalize();
    let j = k.cross(&i);
    Matrix3::from_columns(&[i, j, k])
}

/// A local transform in Blitz space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xform {
    pub pos: Vector3<f32>,
    pub rot: Matrix3<f32>,
    pub scale: Vector3<f32>,
}

impl Default for Xform {
    fn default() -> Self {
        Xform { pos: Vector3::zeros(), rot: Matrix3::identity(), scale: Vector3::new(1.0, 1.0, 1.0) }
    }
}

impl Xform {
    /// Full affine matrix `T * R * S`.
    pub fn matrix(&self) -> Matrix4<f32> {
        let mut m = Matrix4::identity();
        let rs = self.rot * Matrix3::from_diagonal(&self.scale);
        m.fixed_view_mut::<3, 3>(0, 0).copy_from(&rs);
        m.fixed_view_mut::<3, 1>(0, 3).copy_from(&self.pos);
        m
    }

    /// Position in Fyrox space.
    pub fn fyrox_position(&self) -> Vector3<f32> {
        Vector3::new(-self.pos.x, self.pos.y, self.pos.z)
    }

    /// Rotation in Fyrox space.
    pub fn fyrox_rotation(&self) -> UnitQuaternion<f32> {
        let s = Matrix3::from_diagonal(&Vector3::new(-1.0, 1.0, 1.0));
        quat(&(s * self.rot * s))
    }
}

/// World-space information of an entity, computed from its ancestors.
#[derive(Clone, Copy, Debug)]
pub struct WorldXform {
    pub matrix: Matrix4<f32>,
    /// Accumulated rotation, scale ignored (Blitz semantics for world rotations).
    pub rot: Matrix3<f32>,
    pub scale: Vector3<f32>,
}

impl WorldXform {
    pub fn identity() -> Self {
        WorldXform { matrix: Matrix4::identity(), rot: Matrix3::identity(), scale: Vector3::new(1.0, 1.0, 1.0) }
    }
    pub fn child(&self, local: &Xform) -> WorldXform {
        WorldXform {
            matrix: self.matrix * local.matrix(),
            rot: self.rot * local.rot,
            scale: self.scale.component_mul(&local.scale),
        }
    }
    pub fn position(&self) -> Vector3<f32> {
        self.matrix.fixed_view::<3, 1>(0, 3).into()
    }
    /// Converts a world-space point into this frame's local space.
    pub fn to_local_point(&self, p: &Vector3<f32>) -> Vector3<f32> {
        let inv = self.matrix.try_inverse().unwrap_or_else(Matrix4::identity);
        inv.transform_point(&(*p).into()).coords
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quat_matches_the_iterative_solver() {
        for (p, y, r) in [(0.0, 0.0, 0.0), (30.0, 45.0, 10.0), (-80.0, 170.0, -45.0), (89.0, -120.0, 179.0), (0.0, 180.0, 0.0)] {
            let m = rotation(p, y, r);
            let a = quat(&m);
            let b = UnitQuaternion::from_matrix(&m);
            assert!(a.angle_to(&b) < 1e-3, "{p} {y} {r}");
        }
        // a matrix that drifted from many small turns still gives a unit quaternion
        let mut m = Matrix3::identity();
        for _ in 0..10_000 {
            m *= rotation(0.7, 1.3, 0.2);
        }
        assert!((quat(&m).into_inner().norm() - 1.0).abs() < 1e-5);
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn blitz_rotation_conventions() {
        let fwd = Vector3::z();
        // positive yaw turns left (towards -X)
        let v = rotation(0.0, 90.0, 0.0) * fwd;
        assert!(close(v.x, -1.0) && close(v.z, 0.0), "{v:?}");
        // positive pitch tilts the nose down
        let v = rotation(90.0, 0.0, 0.0) * fwd;
        assert!(close(v.y, -1.0), "{v:?}");
        // angles round-trip
        let (p, y, r) = angles(&rotation(30.0, -45.0, 10.0));
        assert!(close(p, 30.0) && close(y, -45.0) && close(r, 10.0), "{p} {y} {r}");
    }

    #[test]
    fn point_at_matches_vector_angles() {
        let target = Vector3::new(3.0, -2.0, 5.0);
        let m = rotation(vector_pitch(&target), vector_yaw(&target), 0.0);
        let k = m * Vector3::z();
        let t = target.normalize();
        assert!(close(k.x, t.x) && close(k.y, t.y) && close(k.z, t.z), "{k:?} vs {t:?}");
    }

    #[test]
    fn hierarchy_and_mirroring() {
        let parent = Xform { pos: Vector3::new(1.0, 0.0, 0.0), rot: rotation(0.0, 90.0, 0.0), scale: Vector3::new(2.0, 2.0, 2.0) };
        let child = Xform { pos: Vector3::new(0.0, 0.0, 1.0), ..Default::default() };
        let w = WorldXform::identity().child(&parent).child(&child);
        // child is 1 unit "forward" of a parent yawed left and scaled 2x
        let p = w.position();
        assert!(close(p.x, -1.0) && close(p.z, 0.0), "{p:?}");
        // mirrored rotation is a proper rotation that maps mirrored vectors consistently
        let q = parent.fyrox_rotation();
        let vb = parent.rot * Vector3::new(0.3, 0.2, 0.9);
        let vf = q * Vector3::new(-0.3, 0.2, 0.9);
        assert!(close(vf.x, -vb.x) && close(vf.y, vb.y) && close(vf.z, vb.z));
    }
}
