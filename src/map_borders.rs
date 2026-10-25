//! Province/state outlines and country borders from current province control.
//! The same rasterizer builds the initial texture and cropped capture updates.
use std::collections::HashMap;

use crate::map_graph::ProvinceKind;
use crate::map_pixels::StateBounds;
use crate::world::World;

#[derive(Clone, Copy)]
struct Region<'a> {
    state: Option<u32>,
    controller: Option<&'a str>,
}

pub struct BorderRaster<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
    stride: usize,
    wrap_x: bool,
    regions: HashMap<[u8; 3], Region<'a>>,
}

impl<'a> BorderRaster<'a> {
    pub fn new(world: &'a World, pixels: &'a [u8], stride: usize) -> Self {
        let width = world.geography.width;
        let height = world.geography.height;
        assert!(matches!(stride, 3 | 4));
        assert_eq!(pixels.len(), width as usize * height as usize * stride);
        let regions = world
            .geography
            .provinces
            .iter()
            .map(|(&id, geography)| {
                let province = world.provinces.get(&id);
                let controller = if geography.definition.kind == ProvinceKind::Land {
                    province.and_then(|p| p.controller.as_deref())
                } else {
                    None
                };
                (
                    geography.definition.rgb,
                    Region {
                        state: province.and_then(|p| p.state_id),
                        controller,
                    },
                )
            })
            .collect();
        Self {
            width,
            height,
            pixels,
            stride,
            wrap_x: world.geography.wrap_x,
            regions,
        }
    }

    pub fn full_bounds(&self) -> StateBounds {
        StateBounds {
            left: 0,
            top: 0,
            right: self.width - 1,
            bottom: self.height - 1,
        }
    }

    fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let i = (y as usize * self.width as usize + x as usize) * self.stride;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2]]
    }

    pub fn rgba(&self, bounds: StateBounds) -> Vec<u8> {
        assert!(bounds.left <= bounds.right && bounds.right < self.width);
        assert!(bounds.top <= bounds.bottom && bounds.bottom < self.height);
        let mut overlay = vec![0; bounds.width() as usize * bounds.height() as usize * 4];
        for y in bounds.top..=bounds.bottom {
            for x in bounds.left..=bounds.right {
                let rgb = self.rgb(x, y);
                let Some(region) = self.regions.get(&rgb) else {
                    continue;
                };
                let neighbours = [
                    if x > 0 {
                        Some((x - 1, y))
                    } else if self.wrap_x {
                        Some((self.width - 1, y))
                    } else {
                        None
                    },
                    if x + 1 < self.width {
                        Some((x + 1, y))
                    } else if self.wrap_x {
                        Some((0, y))
                    } else {
                        None
                    },
                    y.checked_sub(1).map(|y| (x, y)),
                    (y + 1 < self.height).then_some((x, y + 1)),
                ];
                let mut country_edge = false;
                let mut state_edge = false;
                let mut province_edge = false;
                for (nx, ny) in neighbours.into_iter().flatten() {
                    let neighbour_rgb = self.rgb(nx, ny);
                    if neighbour_rgb == rgb {
                        continue;
                    }
                    let neighbour = self.regions.get(&neighbour_rgb);
                    if region
                        .controller
                        .zip(neighbour.and_then(|p| p.controller))
                        .is_some_and(|(a, b)| a != b)
                    {
                        country_edge = true;
                    } else if region.state.is_some()
                        && region.state != neighbour.and_then(|p| p.state)
                    {
                        state_edge = true;
                    } else {
                        province_edge = true;
                    }
                }
                let color = if country_edge {
                    [232, 220, 194, 150]
                } else if state_edge {
                    [235, 198, 120, 38]
                } else if province_edge {
                    [20, 25, 31, 34]
                } else {
                    continue;
                };
                let i = ((y - bounds.top) as usize * bounds.width() as usize
                    + (x - bounds.left) as usize)
                    * 4;
                overlay[i..i + 4].copy_from_slice(&color);
            }
        }
        overlay
    }
}

/// Repaint both sides of each affected edge. Include the opposite map seam
/// when a changed province touches a horizontally wrapped edge.
pub fn dirty_regions(world: &World, since_revision: u64) -> Vec<StateBounds> {
    let (width, height) = (world.geography.width, world.geography.height);
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let mut regions = Vec::new();
    for province in world
        .provinces
        .values()
        .filter(|p| p.control_revision > since_revision)
    {
        let Some(b) = world
            .geography
            .provinces
            .get(&province.id)
            .and_then(|p| p.bounds)
        else {
            continue;
        };
        let bounds = StateBounds {
            left: b.min_x.saturating_sub(1),
            top: b.min_y.saturating_sub(1),
            right: b.max_x.saturating_add(1).min(width - 1),
            bottom: b.max_y.saturating_add(1).min(height - 1),
        };
        regions.push(bounds);
        if world.geography.wrap_x && b.min_x == 0 && bounds.right < width - 1 {
            regions.push(StateBounds {
                left: width - 1,
                right: width - 1,
                ..bounds
            });
        }
        if world.geography.wrap_x && b.max_x == width - 1 && bounds.left > 0 {
            regions.push(StateBounds {
                left: 0,
                right: 0,
                ..bounds
            });
        }
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario(wrap_x: bool) -> (World, Vec<u8>) {
        let mut pixels = Vec::new();
        for _ in 0..3 {
            for color in [[10, 0, 0, 255], [20, 0, 0, 255], [30, 0, 0, 255]] {
                pixels.extend_from_slice(&color);
                pixels.extend_from_slice(&color);
            }
        }
        let mut world = World::from_map_data(
            &HashMap::from([(1, 1), (2, 2), (3, 2)]),
            &HashMap::from([(1, "A".into()), (2, "B".into())]),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        world.geography = crate::map_graph::MapGraph::from_rgba(
            6, 3, &pixels,
            "1;10;0;0;land;false;plains;1\n2;20;0;0;land;false;plains;1\n3;30;0;0;land;false;plains;1",
            wrap_x,
        ).unwrap();
        (world, pixels)
    }

    fn render(world: &World, pixels: &[u8], stride: usize) -> Vec<u8> {
        let raster = BorderRaster::new(world, pixels, stride);
        raster.rgba(raster.full_bounds())
    }

    fn apply_dirty(world: &World, pixels: &[u8], stride: usize, since: u64, texture: &mut [u8]) {
        let raster = BorderRaster::new(world, pixels, stride);
        for b in dirty_regions(world, since) {
            let patch = raster.rgba(b);
            for y in b.top..=b.bottom {
                let source = ((y - b.top) * b.width() * 4) as usize;
                let dest = ((y * world.geography.width + b.left) * 4) as usize;
                let bytes = b.width() as usize * 4;
                texture[dest..dest + bytes].copy_from_slice(&patch[source..source + bytes]);
            }
        }
    }

    #[test]
    fn capture_moves_border_within_a_state_without_transferring_legal_ownership() {
        let (mut world, pixels) = scenario(false);
        let before = render(&world, &pixels, 4);
        let alpha = |data: &[u8], x: usize| data[(6 + x) * 4 + 3];
        assert_eq!(alpha(&before, 1), 150);
        assert_eq!(alpha(&before, 2), 150);
        assert_eq!(alpha(&before, 3), 34);
        world.set_province_controller(2, Some("A"));
        let captured = render(&world, &pixels, 4);
        assert_eq!(alpha(&captured, 1), 38);
        assert_eq!(alpha(&captured, 2), 38);
        assert_eq!(alpha(&captured, 3), 150);
        assert_eq!(alpha(&captured, 4), 150);
        assert_eq!(world.states[&2].owner.as_deref(), Some("B"));
        world.set_province_controller(2, Some("B"));
        assert_eq!(render(&world, &pixels, 4), before);
    }

    #[test]
    fn incremental_capture_and_recapture_match_a_full_texture_rebuild() {
        let (mut world, rgba) = scenario(false);
        let rgb: Vec<_> = rgba
            .chunks_exact(4)
            .flat_map(|p| p[..3].iter().copied())
            .collect();
        let mut texture = render(&world, &rgba, 4);
        assert_eq!(render(&world, &rgb, 3), texture);
        for controller in ["A", "B"] {
            let since = world.control_revision;
            world.set_province_controller(2, Some(controller));
            apply_dirty(&world, &rgb, 3, since, &mut texture);
            assert_eq!(texture, render(&world, &rgba, 4));
        }
        assert!(dirty_regions(&world, world.control_revision).is_empty());
    }

    #[test]
    fn capture_at_wrapped_seam_refreshes_the_opposite_edge() {
        let (mut world, pixels) = scenario(true);
        let mut texture = render(&world, &pixels, 4);
        let before = texture.clone();
        world.set_province_controller(1, Some("B"));
        apply_dirty(&world, &pixels, 4, 0, &mut texture);
        assert_ne!(texture, before);
        assert_eq!(texture, render(&world, &pixels, 4));
        assert_ne!(&texture[20..24], &before[20..24]);
    }

    #[test]
    fn several_captures_before_a_frame_do_not_leave_obsolete_borders() {
        let (mut world, pixels) = scenario(true);
        let mut texture = render(&world, &pixels, 4);
        world.set_province_controller(2, Some("A"));
        world.set_province_controller(3, Some("A"));
        apply_dirty(&world, &pixels, 4, 0, &mut texture);
        assert_eq!(texture, render(&world, &pixels, 4));
        assert!(!texture.chunks_exact(4).any(|p| p[3] == 150));
    }
}
