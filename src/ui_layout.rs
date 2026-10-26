//! Shared campaign geometry. Drawing and pointer blocking use these rectangles.
use macroquad::prelude::{Rect, Vec2};

#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub compact: bool,
    pub top: f32,
    pub bottom: f32,
}

impl Layout {
    pub fn new(width: f32, height: f32) -> Self {
        let compact = width < 760.0;
        let scale = (height / 1080.0).min(width / 1920.0).clamp(0.8, 1.25);
        Self {
            width,
            height,
            scale,
            compact,
            top: if compact { 104.0 } else { 78.0 * scale },
            bottom: 48.0,
        }
    }
    pub fn current() -> Self {
        Self::new(
            macroquad::prelude::screen_width(),
            macroquad::prelude::screen_height(),
        )
    }
    pub fn primary(self) -> Rect {
        let w = if self.compact {
            (self.width - 10.0).min(470.0)
        } else {
            600.0 * self.scale
        };
        Rect::new(
            0.0,
            self.top,
            w,
            (self.height - self.top - self.bottom).max(150.0),
        )
    }
    pub fn wide(self) -> Rect {
        Rect::new(
            0.0,
            self.top,
            self.width,
            (self.height - self.top - self.bottom).max(150.0),
        )
    }
    pub fn time(self) -> Rect {
        if self.compact {
            Rect::new(self.width - 230.0, 0.0, 230.0, 38.0)
        } else {
            Rect::new(
                self.width - 255.0 * self.scale,
                0.0,
                255.0 * self.scale,
                38.0 * self.scale,
            )
        }
    }
    pub fn outliner(self) -> Rect {
        let w = if self.compact {
            145.0
        } else {
            213.0 * self.scale
        };
        Rect::new(
            self.width - w - 5.0,
            self.top + 44.0,
            w,
            (self.height * 0.44).min(385.0),
        )
    }
    pub fn map_modes(self) -> Rect {
        let w = if self.compact { 64.0 } else { 306.0 };
        Rect::new(self.width - w - 5.0, self.height - 44.0, w, 39.0)
    }
    pub fn army_dock(self) -> Rect {
        Rect::new(
            5.0,
            self.height - 44.0,
            (self.width - self.map_modes().w - 17.0).max(36.0),
            39.0,
        )
    }
    pub fn minimap(self) -> Rect {
        Rect::new(self.width - 205.0, self.height - 158.0, 200.0, 108.0)
    }
    pub fn hud_contains(self, p: Vec2) -> bool {
        p.y < self.top || self.map_modes().contains(p) || self.army_dock().contains(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn campaign_panels_and_dock_fit_desktop_and_phone() {
        for (w, h) in [
            (1920.0, 1080.0),
            (1280.0, 720.0),
            (960.0, 540.0),
            (600.0, 800.0),
            (360.0, 740.0),
        ] {
            let l = Layout::new(w, h);
            for r in [
                l.primary(),
                l.wide(),
                l.time(),
                l.map_modes(),
                l.army_dock(),
            ] {
                assert!(
                    r.x >= 0.0 && r.y >= 0.0 && r.right() <= w + 0.1 && r.bottom() <= h + 0.1,
                    "{w}x{h}: {r:?}"
                );
            }
            assert!(l.primary().bottom() <= l.army_dock().y);
            assert!(l.army_dock().right() < l.map_modes().x);
        }
    }
}
