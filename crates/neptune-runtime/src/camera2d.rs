//! The 2D camera (ext): `Camera2D`, `Camera2DEnable`, `ScreenToWorld2DX/Y`,
//! `WorldToScreen2DX/Y`.
//!
//! With the camera on, everything drawn on the screen in 2D (images, tile
//! maps, shapes, text) is in *world* coordinates: the camera's position is
//! the world point shown at the centre of the screen, `zoom` scales the view
//! (2 = everything twice as big) and `rotation` turns the camera clockwise
//! in degrees. `Camera2DEnable 0` goes back to screen coordinates for a HUD
//! drawn on top; UI widgets always use screen coordinates.

use fyrox::core::algebra::{Matrix3, Vector2};

#[derive(Debug, Clone, Copy)]
pub struct Camera2D {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub rotation: f32,
    /// Screen size (logical pixels) when the camera was set.
    pub screen: (f32, f32),
}

impl Default for Camera2D {
    fn default() -> Self {
        Camera2D { x: 0.0, y: 0.0, zoom: 1.0, rotation: 0.0, screen: (0.0, 0.0) }
    }
}

impl Camera2D {
    /// World to screen: centre * rotate(-rotation) * scale(zoom) * translate(-position).
    pub fn matrix(&self) -> Matrix3<f32> {
        let center = Matrix3::new_translation(&Vector2::new(self.screen.0 / 2.0, self.screen.1 / 2.0));
        let rot = Matrix3::new_rotation(-self.rotation.to_radians());
        let zoom = Matrix3::new_nonuniform_scaling(&Vector2::new(self.zoom, self.zoom));
        let pos = Matrix3::new_translation(&Vector2::new(-self.x, -self.y));
        center * rot * zoom * pos
    }

    pub fn to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        let p = self.matrix().transform_point(&fyrox::core::algebra::Point2::new(x, y));
        (p.x, p.y)
    }

    pub fn to_world(&self, x: f32, y: f32) -> (f32, f32) {
        let inv = self.matrix().try_inverse().unwrap_or_else(Matrix3::identity);
        let p = inv.transform_point(&fyrox::core::algebra::Point2::new(x, y));
        (p.x, p.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_round_trip() {
        let c = Camera2D { x: 100.0, y: 50.0, zoom: 2.0, rotation: 0.0, screen: (800.0, 600.0) };
        // the camera position is at the centre of the screen
        assert_eq!(c.to_screen(100.0, 50.0), (400.0, 300.0));
        // zoom doubles distances
        assert_eq!(c.to_screen(110.0, 50.0), (420.0, 300.0));
        let r = Camera2D { rotation: 90.0, ..c };
        // turning the camera clockwise turns the world anticlockwise on screen
        let (sx, sy) = r.to_screen(110.0, 50.0);
        assert!((sx - 400.0).abs() < 1e-3 && (sy - 280.0).abs() < 1e-3, "{sx},{sy}");
        let (wx, wy) = r.to_world(sx, sy);
        assert!((wx - 110.0).abs() < 1e-3 && (wy - 50.0).abs() < 1e-3);
    }
}
