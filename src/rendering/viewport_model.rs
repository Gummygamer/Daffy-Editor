//! Pure viewport math (zoom/pan/hit-testing), kept free of egui types so it
//! is unit-testable and deterministic.

use serde::{Deserialize, Serialize};

pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportModel {
    pub zoom: f32,
    /// World coordinate (in pixels of level space) at the viewport origin.
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for ViewportModel {
    fn default() -> Self {
        Self { zoom: 2.0, pan_x: -16.0, pan_y: -16.0 }
    }
}

impl ViewportModel {
    pub fn world_to_screen(&self, world: [f32; 2]) -> [f32; 2] {
        [(world[0] - self.pan_x) * self.zoom, (world[1] - self.pan_y) * self.zoom]
    }

    pub fn screen_to_world(&self, screen: [f32; 2]) -> [f32; 2] {
        [screen[0] / self.zoom + self.pan_x, screen[1] / self.zoom + self.pan_y]
    }

    /// Zoom by `factor` keeping the world point under `screen_anchor` fixed.
    pub fn zoom_at(&mut self, screen_anchor: [f32; 2], factor: f32) {
        let anchor_world = self.screen_to_world(screen_anchor);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        // Solve pan so anchor_world maps back to screen_anchor.
        self.pan_x = anchor_world[0] - screen_anchor[0] / self.zoom;
        self.pan_y = anchor_world[1] - screen_anchor[1] / self.zoom;
    }

    /// Pan by a screen-space delta (e.g. a mouse drag).
    pub fn pan_screen(&mut self, delta: [f32; 2]) {
        self.pan_x -= delta[0] / self.zoom;
        self.pan_y -= delta[1] / self.zoom;
    }

    /// Tile coordinate under a screen point, given the tile edge length in
    /// world pixels. Negative coordinates mean the point is outside the level.
    pub fn tile_at_screen(&self, screen: [f32; 2], tile_px: f32) -> Option<(u32, u32)> {
        let [wx, wy] = self.screen_to_world(screen);
        if wx < 0.0 || wy < 0.0 {
            return None;
        }
        Some(((wx / tile_px) as u32, (wy / tile_px) as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: [f32; 2], b: [f32; 2]) {
        assert!((a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3, "{a:?} != {b:?}");
    }

    #[test]
    fn world_screen_round_trip_at_various_zooms() {
        for zoom in [0.5f32, 1.0, 2.0, 7.5] {
            let v = ViewportModel { zoom, pan_x: -33.0, pan_y: 12.0 };
            let w = [123.0, 45.0];
            approx(v.screen_to_world(v.world_to_screen(w)), w);
        }
    }

    #[test]
    fn zoom_at_keeps_anchor_world_point_fixed() {
        let mut v = ViewportModel { zoom: 1.0, pan_x: 0.0, pan_y: 0.0 };
        let anchor = [100.0, 80.0];
        let before = v.screen_to_world(anchor);
        v.zoom_at(anchor, 2.0);
        approx(v.screen_to_world(anchor), before);
        assert_eq!(v.zoom, 2.0);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = ViewportModel::default();
        v.zoom_at([0.0, 0.0], 1000.0);
        assert_eq!(v.zoom, MAX_ZOOM);
        v.zoom_at([0.0, 0.0], 1e-6);
        assert_eq!(v.zoom, MIN_ZOOM);
    }

    #[test]
    fn tile_hit_test() {
        let v = ViewportModel { zoom: 2.0, pan_x: 0.0, pan_y: 0.0 };
        // screen (64, 32) -> world (32, 16) -> tile (2, 1) at 16px tiles
        assert_eq!(v.tile_at_screen([64.0, 32.0], 16.0), Some((2, 1)));
        // outside (negative world coords)
        let v2 = ViewportModel { zoom: 1.0, pan_x: -10.0, pan_y: -10.0 };
        assert_eq!(v2.tile_at_screen([5.0, 5.0], 16.0), None);
        let v3 = ViewportModel { zoom: 1.0, pan_x: 10.0, pan_y: 10.0 };
        assert_eq!(v3.tile_at_screen([0.0, 0.0], 16.0), Some((0, 0)));
    }

    #[test]
    fn pan_screen_moves_view() {
        let mut v = ViewportModel { zoom: 2.0, pan_x: 0.0, pan_y: 0.0 };
        v.pan_screen([20.0, -10.0]);
        approx([v.pan_x, v.pan_y], [-10.0, 5.0]);
    }
}
