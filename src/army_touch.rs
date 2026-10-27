//! Pointer policy shared by the mobile army screen and its tests.
use macroquad::prelude::{Rect, Vec2};
#[derive(Debug, Default)]
pub struct Gesture {
    start: Option<Vec2>,
    last: Option<Vec2>,
    dragged: bool,
    cancelled: bool,
}
#[derive(Debug, Default, PartialEq)]
pub struct GestureResult {
    pub tap: Option<Vec2>,
    pub scroll: f32,
}
impl Gesture {
    pub fn update(
        &mut self,
        p: Vec2,
        pressed: bool,
        down: bool,
        released: bool,
        contacts: usize,
        bounds: Rect,
    ) -> GestureResult {
        let mut out = GestureResult::default();
        if contacts >= 2 {
            self.cancelled = true;
        }
        if pressed {
            self.start = bounds.contains(p).then_some(p);
            self.last = self.start;
            self.dragged = false;
            // A second finger must not restart a cancelled gesture.
            if contacts < 2 && !down {
                self.cancelled = false;
            }
        }
        if let Some(start) = self.start {
            if start.distance(p) > 10.0 {
                self.dragged = true;
            }
            if down && self.dragged && !self.cancelled {
                out.scroll = self.last.map_or(0.0, |last| last.y - p.y);
            }
            self.last = Some(p);
            if released && !self.dragged && !self.cancelled && bounds.contains(p) {
                out.tap = Some(p);
            }
        }
        if released || (!down && contacts == 0) {
            self.start = None;
            self.last = None;
            self.dragged = false;
            if contacts == 0 {
                self.cancelled = false;
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolling_and_pinch_never_activate_a_row_on_release() {
        let r = Rect::new(0., 0., 200., 200.);
        let mut g = Gesture::default();
        assert_eq!(
            g.update(Vec2::new(10., 100.), true, true, false, 1, r).tap,
            None
        );
        assert!(
            g.update(Vec2::new(10., 60.), false, true, false, 1, r)
                .scroll
                > 0.
        );
        assert_eq!(
            g.update(Vec2::new(10., 60.), false, false, true, 0, r).tap,
            None
        );
        g.update(Vec2::new(10., 60.), true, true, false, 1, r);
        g.update(Vec2::new(10., 60.), false, true, false, 2, r);
        assert_eq!(
            g.update(Vec2::new(10., 60.), false, false, true, 0, r).tap,
            None
        );
        g.update(Vec2::new(10., 60.), true, true, false, 1, r);
        assert_eq!(
            g.update(Vec2::new(10., 60.), false, false, true, 0, r).tap,
            Some(Vec2::new(10., 60.))
        );
    }
    #[test]
    fn press_outside_drag_back_and_exit_are_not_taps() {
        let mut g = Gesture::default();
        let r = Rect::new(0., 0., 200., 200.);
        g.update(Vec2::new(220., 50.), true, true, false, 1, r);
        assert_eq!(
            g.update(Vec2::new(50., 50.), false, false, true, 0, r).tap,
            None
        );
        g.update(Vec2::new(50., 50.), true, true, false, 1, r);
        g.update(Vec2::new(50., 100.), false, true, false, 1, r);
        assert_eq!(
            g.update(Vec2::new(50., 50.), false, false, true, 0, r).tap,
            None
        );
    }
}
