//! Memory-efficient authoritative state pixel lookup.
//! HOI4 state IDs normally fit into u16. Preserve larger modded IDs by
//! transparently switching to u32 rather than truncating them.
#[derive(Debug)]
pub enum StatePixelIds {
    Compact(Vec<u16>),
    Wide(Vec<u32>),
}

impl StatePixelIds {
    pub fn new(pixel_count: usize, max_state_id: u32) -> Self {
        if max_state_id <= u16::MAX as u32 {
            Self::Compact(vec![0; pixel_count])
        } else {
            Self::Wide(vec![0; pixel_count])
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Compact(data) => data.len(),
            Self::Wide(data) => data.len(),
        }
    }

    pub fn get(&self, index: usize) -> Option<u32> {
        match self {
            Self::Compact(data) => data.get(index).map(|id| u32::from(*id)),
            Self::Wide(data) => data.get(index).copied(),
        }
    }

    pub fn set(&mut self, index: usize, state_id: u32) {
        match self {
            Self::Compact(data) => {
                data[index] =
                    u16::try_from(state_id).expect("Compact state IDs require values <= 65535")
            }
            Self::Wide(data) => data[index] = state_id,
        }
    }

    pub fn bytes_used(&self) -> usize {
        match self {
            Self::Compact(data) => data.capacity() * std::mem::size_of::<u16>(),
            Self::Wide(data) => data.capacity() * std::mem::size_of::<u32>(),
        }
    }

    #[cfg(test)]
    pub fn from_slice(ids: &[u32]) -> Self {
        let mut result = Self::new(ids.len(), ids.iter().copied().max().unwrap_or(0));
        for (index, &id) in ids.iter().enumerate() {
            result.set(index, id);
        }
        result
    }
}

/// Inclusive rectangular footprint of one state in world-map pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateBounds {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl StateBounds {
    pub fn new(x: u32, y: u32) -> Self {
        Self {
            left: x,
            top: y,
            right: x,
            bottom: y,
        }
    }

    pub fn include(&mut self, x: u32, y: u32) {
        self.left = self.left.min(x);
        self.top = self.top.min(y);
        self.right = self.right.max(x);
        self.bottom = self.bottom.max(y);
    }

    pub fn width(self) -> u32 {
        self.right - self.left + 1
    }
    pub fn height(self) -> u32 {
        self.bottom - self.top + 1
    }

    pub fn rect(self) -> macroquad::prelude::Rect {
        macroquad::prelude::Rect::new(
            self.left as f32,
            self.top as f32,
            self.width() as f32,
            self.height() as f32,
        )
    }
}

/// Only create pixels covering the state, not a full-world RGBA texture every
/// time the user selects another state. Neighbours still use global coordinates.
pub fn outline_rgba(
    ids: &StatePixelIds,
    map_width: u32,
    map_height: u32,
    state_id: u32,
    bounds: StateBounds,
) -> Vec<u8> {
    let width = bounds.width() as usize;
    let height = bounds.height() as usize;
    let map_width = map_width as usize;
    let map_height = map_height as usize;
    let mut rgba = vec![0u8; width * height * 4];
    for local_y in 0..height {
        let y = bounds.top as usize + local_y;
        for local_x in 0..width {
            let x = bounds.left as usize + local_x;
            let index = y * map_width + x;
            if ids.get(index) != Some(state_id) {
                continue;
            }
            let differs = (x > 0 && ids.get(index - 1) != Some(state_id))
                || (x + 1 < map_width && ids.get(index + 1) != Some(state_id))
                || (y > 0 && ids.get(index - map_width) != Some(state_id))
                || (y + 1 < map_height && ids.get(index + map_width) != Some(state_id));
            if differs {
                let o = (local_y * width + local_x) * 4;
                rgba[o..o + 4].copy_from_slice(&[255, 218, 105, 235]);
            }
        }
    }
    rgba
}

/// Project a cropped state outline into the current view without drawing over
/// the HUD or outside the map. Returns (source UV pixel rect, destination rect).
pub fn visible_outline(
    bounds: StateBounds,
    visible_world: macroquad::prelude::Rect,
    screen_view: macroquad::prelude::Rect,
) -> Option<(macroquad::prelude::Rect, macroquad::prelude::Rect)> {
    use macroquad::prelude::Rect;
    if visible_world.w <= 0.0
        || visible_world.h <= 0.0
        || screen_view.w <= 0.0
        || screen_view.h <= 0.0
    {
        return None;
    }
    let state = bounds.rect();
    let left = state.x.max(visible_world.x);
    let top = state.y.max(visible_world.y);
    let right = (state.x + state.w).min(visible_world.x + visible_world.w);
    let bottom = (state.y + state.h).min(visible_world.y + visible_world.h);
    if right <= left || bottom <= top {
        return None;
    }
    let source = Rect::new(left - state.x, top - state.y, right - left, bottom - top);
    let dest = Rect::new(
        screen_view.x + (left - visible_world.x) / visible_world.w * screen_view.w,
        screen_view.y + (top - visible_world.y) / visible_world.h * screen_view.h,
        (right - left) / visible_world.w * screen_view.w,
        (bottom - top) / visible_world.h * screen_view.h,
    );
    Some((source, dest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_states_use_half_the_memory_without_losing_ids() {
        let ids = StatePixelIds::from_slice(&[0, 1, 1024, 65535]);
        assert!(matches!(ids, StatePixelIds::Compact(_)));
        assert_eq!(ids.bytes_used(), 8);
        assert_eq!(ids.len(), 4);
        assert_eq!(ids.get(3), Some(65535));
        assert_eq!(ids.get(4), None);
    }

    #[test]
    fn selected_outline_allocates_only_state_bounds() {
        let ids = StatePixelIds::from_slice(&[0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 0, 0, 0]);
        let mut bounds = StateBounds::new(1, 1);
        bounds.include(2, 1);
        assert_eq!((bounds.width(), bounds.height()), (2, 1));
        let rgba = outline_rgba(&ids, 5, 3, 2, bounds);
        assert_eq!(rgba, vec![255, 218, 105, 235, 255, 218, 105, 235]);
    }

    #[test]
    fn selected_outline_is_clipped_to_map_view() {
        use macroquad::prelude::Rect;
        let bounds = StateBounds {
            left: 100,
            top: 100,
            right: 199,
            bottom: 199,
        };
        let viewport = Rect::new(0.0, 0.0, 400.0, 400.0);
        let view = Rect::new(20.0, 50.0, 800.0, 800.0);
        let (src, dst) = visible_outline(bounds, viewport, view).unwrap();
        assert_eq!(src, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(dst, Rect::new(220.0, 250.0, 200.0, 200.0));
        assert!(visible_outline(bounds, Rect::new(300.0, 300.0, 50.0, 50.0), view).is_none());
    }

    #[test]
    fn large_modded_ids_use_lossless_wide_fallback() {
        let ids = StatePixelIds::from_slice(&[0, 55, 65536, 500000]);
        assert!(matches!(ids, StatePixelIds::Wide(_)));
        assert_eq!(ids.bytes_used(), 16);
        assert_eq!(ids.get(3), Some(500000));
    }
}
