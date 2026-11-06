//! Joint motors and limits (ext), 3D and 2D: `JointMotor`, `JointTarget`,
//! `JointMotorOff`, `JointLimits`, `JointAngle`, `JointPosition` and the
//! same commands ending in `2D`.
//!
//! They work on hinge and slider joints. A hinge turns about its axis
//! (degrees), a slider moves along it (units, or pixels in 2D).
//! `JointMotor` drives the joint at a speed (per second); `JointTarget` pulls
//! it to an angle or position like a spring. Both push with at most
//! `max_force`. `JointAngle`/`JointPosition` read how far the joint has
//! turned or slid since it was made.
//!
//! In 3D, angles follow Blitz's left-hand rule about the axis given when the
//! joint was made, the direction of `EntityPitch` and `EntityRoll` (so a
//! wheel on an X axis rolls forward at positive speed; `EntityYaw` is the
//! other way: a door hinged on Y at +90 has yaw -90). In 2D, positive is
//! clockwise on screen, like `Body2DAngle`.

use fyrox::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        pool::Handle,
    },
    scene::node::Node,
};

use crate::world::Host3D;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Hinge,
    Slider,
    Other,
}

/// What a joint was made with, to measure it later (Fyrox space).
#[derive(Debug, Clone)]
pub struct JointInfo {
    pub kind: Kind,
    pub body1: Handle<Node>,
    pub body2: Handle<Node>,
    /// Axis in body 1's frame.
    pub axis: Vector3<f32>,
    /// Body 2's rotation relative to body 1 at the start.
    pub rel0: UnitQuaternion<f32>,
    /// Offset of body 2 along the axis at the start.
    pub off0: f32,
    /// -1 when the joint uses the opposite of the program's axis.
    pub dir: f32,
}

/// Direction of positive angles relative to Fyrox's. 3D angles follow
/// Blitz's left-hand rule, like `EntityPitch` and `EntityRoll` (checked by
/// rendering; `EntityYaw` turns the other way); 2D angles are clockwise.
fn sign(is_2d: bool) -> f32 {
    if is_2d {
        1.0
    } else {
        -1.0
    }
}

impl Host3D<'_> {
    fn joint_node(&self, j: i32, is_2d: bool) -> Result<Handle<Node>, String> {
        let i = usize::try_from(j - 1).ok();
        let node = if is_2d {
            i.and_then(|i| self.world.joints2d.get(i).copied().flatten())
        } else {
            i.and_then(|i| self.world.joints.get(i).copied()).filter(|h| h.is_some())
        };
        node.ok_or_else(|| format!("Joint {j} does not exist"))
    }

    /// Records a new joint's starting pose. `axis` is in Fyrox world space.
    #[allow(clippy::too_many_arguments)]
    pub fn record_joint(&mut self, j: i32, is_2d: bool, kind: Kind, body1: Handle<Node>, body2: Handle<Node>, axis: Vector3<f32>, dir: f32) -> Result<(), String> {
        let graph = self.graph()?;
        let (t1, t2) = (graph[body1].local_transform(), graph[body2].local_transform());
        let (r1, r2) = (**t1.rotation(), **t2.rotation());
        let (p1, p2) = (**t1.position(), **t2.position());
        let axis = r1.inverse() * axis.try_normalize(1e-9).unwrap_or_else(Vector3::x);
        let off0 = (r1.inverse() * (p2 - p1)).dot(&axis);
        self.world.joint_info.insert((is_2d, j), JointInfo { kind, body1, body2, axis, rel0: r1.inverse() * r2, off0, dir });
        Ok(())
    }

    fn joint_info(&self, j: i32, is_2d: bool) -> Result<JointInfo, String> {
        self.joint_node(j, is_2d)?;
        self.world.joint_info.get(&(is_2d, j)).cloned().ok_or_else(|| format!("Joint {j} is not a hinge or slider"))
    }

    /// Hinge angle (degrees) or slider offset (units; pixels in 2D).
    pub fn joint_value(&mut self, j: i32, is_2d: bool) -> Result<f32, String> {
        let info = self.joint_info(j, is_2d)?;
        let scale = self.world.physics2d_scale;
        let graph = self.graph()?;
        let (t1, t2) = (graph[info.body1].local_transform(), graph[info.body2].local_transform());
        let (r1, r2) = (**t1.rotation(), **t2.rotation());
        let (p1, p2) = (**t1.position(), **t2.position());
        Ok(info.dir * match info.kind {
            Kind::Slider => {
                let d = (r1.inverse() * (p2 - p1)).dot(&info.axis) - info.off0;
                if is_2d {
                    d * scale
                } else {
                    d
                }
            }
            _ => {
                // the turn since the start, in body 1's frame
                let delta = (r1.inverse() * r2) * info.rel0.inverse();
                sign(is_2d) * delta.scaled_axis().dot(&info.axis).to_degrees()
            }
        })
    }

    fn joint_units(&self, j: i32, is_2d: bool, v: f32) -> Result<f32, String> {
        let info = self.joint_info(j, is_2d)?;
        Ok(info.dir
            * match (info.kind, is_2d) {
                (Kind::Slider, true) => v / self.world.physics2d_scale,
                (Kind::Slider, false) => v,
                _ => sign(is_2d) * v.to_radians(),
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn set_motor(&mut self, j: i32, is_2d: bool, vel: f32, pos: f32, stiffness: f32, damping: f32, max_force: f32) -> Result<(), String> {
        let node = self.joint_node(j, is_2d)?;
        let graph = self.graph()?;
        if is_2d {
            use fyrox::scene::dim2::joint::{Joint, JointMotorParams};
            let jn = graph[node].cast_mut::<Joint>().ok_or("joint node missing")?;
            jn.set_motor_params(JointMotorParams { target_vel: vel, target_pos: pos, stiffness, damping, max_force });
        } else {
            use fyrox::scene::joint::{Joint, JointMotorParams};
            let jn = graph[node].cast_mut::<Joint>().ok_or("joint node missing")?;
            jn.set_motor_params(JointMotorParams { target_vel: vel, target_pos: pos, stiffness, damping, max_force });
        }
        Ok(())
    }

    /// `JointMotor`: turn or slide at `speed` per second.
    pub fn joint_motor(&mut self, j: i32, is_2d: bool, speed: f32, max_force: f32) -> Result<(), String> {
        let v = self.joint_units(j, is_2d, speed)?;
        let f = max_force.max(0.0);
        // force-based velocity motor: full force once 1 unit/s too slow
        self.set_motor(j, is_2d, v, 0.0, 0.0, f, f)
    }

    /// `JointTarget`: pull towards an angle or position like a spring.
    pub fn joint_target(&mut self, j: i32, is_2d: bool, target: f32, stiffness: f32, max_force: f32) -> Result<(), String> {
        let p = self.joint_units(j, is_2d, target)?;
        let k = stiffness.max(0.0);
        // damping for a spring that settles without much overshoot
        self.set_motor(j, is_2d, 0.0, p, k, 2.0 * k.sqrt(), max_force.max(0.0))
    }

    pub fn joint_motor_off(&mut self, j: i32, is_2d: bool) -> Result<(), String> {
        self.set_motor(j, is_2d, 0.0, 0.0, 0.0, 0.0, 0.0)
    }

    /// `JointLimits`: keep a hinge between two angles or a slider between
    /// two positions.
    pub fn joint_limits(&mut self, j: i32, is_2d: bool, min: f32, max: f32, enable: bool) -> Result<(), String> {
        let (a, b) = (self.joint_units(j, is_2d, min)?, self.joint_units(j, is_2d, max)?);
        let range = a.min(b)..a.max(b);
        let node = self.joint_node(j, is_2d)?;
        let graph = self.graph()?;
        if is_2d {
            use fyrox::scene::dim2::joint::{Joint, JointParams};
            let jn = graph[node].cast_mut::<Joint>().ok_or("joint node missing")?;
            let mut p = jn.params().clone();
            match &mut p {
                JointParams::BallJoint(h) => {
                    h.limits_enabled = enable;
                    h.limits_angles = range;
                }
                JointParams::PrismaticJoint(s) => {
                    s.limits_enabled = enable;
                    s.limits = range;
                }
                _ => return Err(format!("Joint {j} is not a hinge or slider")),
            }
            jn.set_params(p);
        } else {
            use fyrox::scene::joint::{Joint, JointParams};
            let jn = graph[node].cast_mut::<Joint>().ok_or("joint node missing")?;
            let mut p = jn.params().clone();
            match &mut p {
                JointParams::RevoluteJoint(h) => {
                    h.limits_enabled = enable;
                    h.limits = range;
                }
                JointParams::PrismaticJoint(s) => {
                    s.limits_enabled = enable;
                    s.limits = range;
                }
                _ => return Err(format!("Joint {j} is not a hinge or slider")),
            }
            jn.set_params(p);
        }
        Ok(())
    }
}
