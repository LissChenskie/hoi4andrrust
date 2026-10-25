//! Pure camera math and touch-gesture tracking for large strategy maps.
//! Kept separate from Macroquad input so transformations are unit-testable.
use macroquad::prelude::{Rect, Vec2, vec2};

pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 24.0;

/// Fill a viewport without stretching province geometry. Crop the longer
/// world dimension and retain the same pixels-per-screen-pixel on both axes.
pub fn source_for_view(width: f32, height: f32, zoom: f32, pan: Vec2, view: Rect) -> Rect {
    let aspect = (view.w / view.h.max(1.0)).max(0.01);
    let base_w = width.min(height * aspect);
    let base_h = height.min(width / aspect);
    let z = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let sw = base_w / z;
    let sh = base_h / z;
    let mx = (width - sw).max(0.0);
    let my = (height - sh).max(0.0);
    Rect::new(
        (mx * 0.5 + pan.x).clamp(0.0, mx),
        (my * 0.5 + pan.y).clamp(0.0, my),
        sw,
        sh,
    )
}
pub fn clamp_for_view(width: f32, height: f32, zoom: f32, pan: Vec2, view: Rect) -> Vec2 {
    let s = source_for_view(width, height, zoom, Vec2::ZERO, view);
    let mx = (width - s.w).max(0.0) * 0.5;
    let my = (height - s.h).max(0.0) * 0.5;
    vec2(pan.x.clamp(-mx, mx), pan.y.clamp(-my, my))
}
pub fn anchor_for_view(
    width: f32,
    height: f32,
    zoom: f32,
    world: Vec2,
    screen: Vec2,
    view: Rect,
) -> Vec2 {
    let s = source_for_view(width, height, zoom, Vec2::ZERO, view);
    let u = ((screen.x - view.x) / view.w.max(1.0)).clamp(0.0, 1.0);
    let v = ((screen.y - view.y) / view.h.max(1.0)).clamp(0.0, 1.0);
    clamp_for_view(
        width,
        height,
        zoom,
        vec2(world.x - u * s.w - s.x, world.y - v * s.h - s.y),
        view,
    )
}

pub fn source_rect(width: f32, height: f32, zoom: f32, pan: Vec2) -> Rect {
    let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let source_w = width / zoom;
    let source_h = height / zoom;
    let max_x = (width - source_w).max(0.0);
    let max_y = (height - source_h).max(0.0);
    Rect::new(
        (max_x * 0.5 + pan.x).clamp(0.0, max_x),
        (max_y * 0.5 + pan.y).clamp(0.0, max_y),
        source_w,
        source_h,
    )
}

pub fn clamp_pan(width: f32, height: f32, zoom: f32, pan: Vec2) -> Vec2 {
    let z = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let max_x = (width - width / z).max(0.0) * 0.5;
    let max_y = (height - height / z).max(0.0) * 0.5;
    vec2(pan.x.clamp(-max_x, max_x), pan.y.clamp(-max_y, max_y))
}

pub fn screen_to_world(source: Rect, view: Rect, position: Vec2) -> Vec2 {
    if view.w <= 0.0 || view.h <= 0.0 {
        return vec2(source.x, source.y);
    }
    vec2(
        source.x + (position.x - view.x) / view.w * source.w,
        source.y + (position.y - view.y) / view.h * source.h,
    )
}

/// The pan needed to keep a given map pixel beneath a screen-space anchor.
pub fn pan_for_anchor(
    width: f32,
    height: f32,
    zoom: f32,
    world: Vec2,
    screen: Vec2,
    view: Rect,
) -> Vec2 {
    if view.w <= 0.0 || view.h <= 0.0 {
        return Vec2::ZERO;
    }
    let z = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let source_w = width / z;
    let source_h = height / z;
    let u = ((screen.x - view.x) / view.w).clamp(0.0, 1.0);
    let v = ((screen.y - view.y) / view.h).clamp(0.0, 1.0);
    clamp_pan(
        width,
        height,
        z,
        vec2(
            world.x - u * source_w - (width - source_w) * 0.5,
            world.y - v * source_h - (height - source_h) * 0.5,
        ),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchPoint {
    pub id: u64,
    pub position: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinchMotion {
    pub previous_center: Vec2,
    pub center: Vec2,
    pub zoom_factor: f32,
}

/// Uses stable finger IDs so a lifted finger or second-hand tap cannot
/// trigger a massive unwanted zoom jump.
#[derive(Debug, Default)]
pub struct PinchTracker {
    previous: Option<(u64, u64, Vec2, f32)>,
}

impl PinchTracker {
    pub fn reset(&mut self) {
        self.previous = None;
    }

    pub fn update(
        &mut self,
        contacts: &[TouchPoint],
        viewport: Rect,
        blocked: bool,
    ) -> Option<PinchMotion> {
        if blocked || contacts.len() != 2 {
            self.reset();
            return None;
        }
        let mut points = [contacts[0], contacts[1]];
        points.sort_unstable_by_key(|point| point.id);
        let p = points[0].position;
        let q = points[1].position;
        if !viewport.contains(p) || !viewport.contains(q) {
            self.reset();
            return None;
        }
        let center = (p + q) * 0.5;
        let distance = p.distance(q);
        if !distance.is_finite() || distance < 12.0 {
            self.reset();
            return None;
        }
        let new_record = (points[0].id, points[1].id, center, distance);
        let result = if let Some((a, b, prev_center, prev_distance)) = self.previous {
            if a == points[0].id && b == points[1].id {
                Some(PinchMotion {
                    previous_center: prev_center,
                    center,
                    zoom_factor: (distance / prev_distance).clamp(0.60, 1.60),
                })
            } else {
                None
            }
        } else {
            None
        };
        self.previous = Some(new_record);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn viewport() -> Rect {
        Rect::new(100.0, 50.0, 900.0, 600.0)
    }
    #[test]
    fn fullscreen_map_keeps_aspect_and_anchor_on_different_displays() {
        for view in [
            Rect::new(0.0, 0.0, 1920.0, 1080.0),
            Rect::new(0.0, 0.0, 600.0, 800.0),
        ] {
            let source = source_for_view(5632.0, 2048.0, 2.0, Vec2::ZERO, view);
            assert!((source.w / source.h - view.w / view.h).abs() < 0.0001);
            let screen = vec2(view.w * 0.6, view.h * 0.4);
            let world = screen_to_world(source, view, screen);
            let pan = anchor_for_view(5632.0, 2048.0, 3.0, world, screen, view);
            let zoomed = source_for_view(5632.0, 2048.0, 3.0, pan, view);
            assert!((screen_to_world(zoomed, view, screen) - world).length() < 0.01);
            assert!(
                zoomed.x >= 0.0
                    && zoomed.y >= 0.0
                    && zoomed.right() <= 5632.0
                    && zoomed.bottom() <= 2048.0
            );
        }
    }
    fn pair(a: f32, b: f32) -> [TouchPoint; 2] {
        [
            TouchPoint {
                id: 12,
                position: vec2(a, 250.0),
            },
            TouchPoint {
                id: 54,
                position: vec2(b, 250.0),
            },
        ]
    }

    #[test]
    fn anchored_zoom_preserves_world_point() {
        let view = viewport();
        let world = vec2(420.0, 120.0);
        let point = vec2(300.0, 200.0);
        let p = pan_for_anchor(2000.0, 1000.0, 4.0, world, point, view);
        let rect = source_rect(2000.0, 1000.0, 4.0, p);
        let sampled = screen_to_world(rect, view, point);
        assert!((sampled - world).length() < 0.01);
    }

    #[test]
    fn camera_clamps_overscroll_and_normalizes_zoom() {
        assert_eq!(
            clamp_pan(2000.0, 1000.0, 1.0, vec2(999.0, -999.0)),
            Vec2::ZERO
        );
        let p = clamp_pan(2000.0, 1000.0, 2.0, vec2(999.0, -999.0));
        assert_eq!(p, vec2(500.0, -250.0));
        let rect = source_rect(2000.0, 1000.0, 2.0, p);
        assert_eq!(
            (rect.x, rect.y, rect.w, rect.h),
            (1000.0, 0.0, 1000.0, 500.0)
        );
    }

    #[test]
    fn two_finger_scale_and_translation_are_tracked() {
        let mut touch = PinchTracker::default();
        let view = viewport();
        assert!(touch.update(&pair(280.0, 380.0), view, false).is_none());
        let event = touch.update(&pair(250.0, 410.0), view, false).unwrap();
        assert!((event.zoom_factor - 1.6).abs() < 0.001);
        assert_eq!(event.center, vec2(330.0, 250.0));
        let event = touch.update(&pair(260.0, 420.0), view, false).unwrap();
        assert_eq!(event.previous_center, vec2(330.0, 250.0));
        assert_eq!(event.center, vec2(340.0, 250.0));
    }

    #[test]
    fn invalid_fingers_and_ui_interruption_reset_pinch() {
        let mut touch = PinchTracker::default();
        let view = viewport();
        touch.update(&pair(280.0, 380.0), view, false);
        assert!(touch.update(&pair(260.0, 400.0), view, true).is_none());
        assert!(touch.update(&pair(260.0, 400.0), view, false).is_none());
        let replaced = [
            TouchPoint {
                id: 1,
                position: vec2(260.0, 250.0),
            },
            TouchPoint {
                id: 54,
                position: vec2(400.0, 250.0),
            },
        ];
        assert!(touch.update(&replaced, view, false).is_none());
    }

    #[test]
    fn zoom_to_anchor_clamps_at_world_edges() {
        let view = viewport();
        let world = vec2(0.0, 0.0);
        let p = pan_for_anchor(2000.0, 1000.0, 4.0, world, vec2(view.x, view.y), view);
        let source = source_rect(2000.0, 1000.0, 4.0, p);
        assert!((source.x, source.y) == (0.0, 0.0));
    }
}
