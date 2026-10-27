mod air_warfare;
mod army_command_ui;
mod army_controls;
mod army_projection;
mod army_touch;
mod army_ui;
mod campaign_ui;
mod combat_modifiers;
mod combat_rules;
mod command;
mod command_abilities;
mod construction;
mod construction_map;
mod construction_ui;
mod division_designer_ui;
mod logistics;
mod mobile_army_ui;
mod naval_warfare;
mod support_import;
#[cfg(test)]
mod support_tests;
pub use hoi4_mobile::game;
mod battle_plan_render;
mod battle_plan_ui;
mod battle_ui;
mod frontline;
mod history_conditions;
mod hoi_ui;
mod land_combat;
mod map_borders;
mod map_camera;
mod map_graph;
mod map_pixels;
mod military_data;
mod military_history;
mod military_model;
mod operations;
mod production;
mod production_ui;
mod recruitment;
mod recruitment_ui;
mod resource_economy;
mod trade_routes;
pub use hoi4_mobile::pdx;
pub use hoi4_mobile::research;
pub use hoi4_mobile::research_data;
pub use hoi4_mobile::research_script;
mod campaign_save;
mod platform_storage;
mod research_layout;
mod research_ui;
mod scenario;
mod session;
mod spatial_grid;
mod state;
mod ui_icons;
mod ui_layout;
mod warfare;
mod world;

use construction::{BuildingRules, ConstructionQueues};
use game::GameState;
use world::World;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use image::GenericImageView;
use macroquad::prelude::*;
use session::{CountryConstruction, GameSession};

const MAP_BITMAP_PATH: &str = "data/hoi4/map/provinces.bmp";
const MAP_DEFINITION_PATH: &str = "data/hoi4/map/definition.csv";
const STATE_DIRECTORY_PATH: &str = "data/hoi4/history/states";

thread_local! {
    static UI_FONT: RefCell<Option<Font>> = const { RefCell::new(None) };
}

fn init_ui_font() {
    let candidates = [
        "data/hoi4/gfx/fonts/RobotoCondensed-Regular.ttf",
        "data/hoi4/gfx/fonts/RobotoCondensed.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/tahoma.ttf",
        "/system/fonts/Roboto-Regular.ttf",
        "/system/fonts/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(font) = load_ttf_font_from_bytes(&bytes) {
                UI_FONT.with(|slot| *slot.borrow_mut() = Some(font));
                return;
            }
        }
    }
}

fn draw_ui_text(text: &str, x: f32, y: f32, size: f32, color: Color) {
    UI_FONT.with(|slot| {
        if let Some(font) = slot.borrow().as_ref() {
            draw_text_ex(
                text,
                x,
                y,
                TextParams {
                    font: Some(font),
                    font_size: size.round().clamp(1.0, 128.0) as u16,
                    font_scale: 1.0,
                    color,
                    ..Default::default()
                },
            );
        } else {
            draw_text(text, x, y, size, color);
        }
    });
}

fn measure_ui_text(text: &str, size: u16) -> TextDimensions {
    UI_FONT.with(|slot| {
        if let Some(font) = slot.borrow().as_ref() {
            measure_text(text, Some(font), size, 1.0)
        } else {
            measure_text(text, None, size, 1.0)
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    MainMenu,
    CountrySelect,
    MapViewer,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MapMode {
    Political,
    Resources,
    Terrain,
}

#[derive(Clone)]
struct ResourceMarker {
    x: f32,
    y: f32,
    label: &'static str,
    color: Color,
}

#[derive(Clone)]
struct CityMarker {
    province_id: u32,
    victory_points: u32,
    x: f32,
    y: f32,
    name: String,
    display_name: bool,
    label_width: f32,
}

struct ProvinceMap {
    width: u32,
    height: u32,
    rgb_pixels: Vec<u8>,
    id_by_rgb: HashMap<[u8; 3], u32>,
    state_by_province: HashMap<u32, u32>,
    map_border_texture: Texture2D,
    state_ids_by_pixel: map_pixels::StatePixelIds,
    state_bounds: HashMap<u32, map_pixels::StateBounds>,
    highlighted_state_texture: Option<(u32, map_pixels::StateBounds, Texture2D)>,
    construction_overlay_texture: Option<Texture2D>,
    construction_overlay_signature: Option<u64>,
    political_texture: Texture2D,
    control_revision: u64,
    /// One lazily constructed texture for whichever non-political mode is open.
    /// Selecting another mode releases the previous texture.
    active_mode_texture: Option<(MapMode, Texture2D)>,
    color_palette: MapColorPalette,
    resource_markers: Vec<ResourceMarker>,
    city_markers: Vec<CityMarker>,
    resource_grid: spatial_grid::SpatialGrid,
    city_grid: spatial_grid::SpatialGrid,
    zoom: f32,
    target_zoom: f32,
    pan: Vec2,
    /// World-space pixel kept under the given screen pixel while zoom eases.
    zoom_anchor: Option<(Vec2, Vec2)>,
    pinch_tracker: map_camera::PinchTracker,

    map_mode: MapMode,
}

impl ProvinceMap {
    fn load(world: &World) -> Result<Self, String> {
        let bitmap_path = Path::new(MAP_BITMAP_PATH);
        let definition_path = Path::new(MAP_DEFINITION_PATH);

        if !bitmap_path.is_file() {
            return Err(format!("Missing {}", MAP_BITMAP_PATH));
        }
        if !definition_path.is_file() {
            return Err(format!("Missing {}", MAP_DEFINITION_PATH));
        }

        let decoded = image::open(bitmap_path)
            .map_err(|error| format!("Could not decode provinces.bmp: {error}"))?;
        let (width, height) = decoded.dimensions();

        // Macroquad's raw texture API uses u16 dimensions.
        if width == 0 || height == 0 || width > u16::MAX as u32 || height > u16::MAX as u32 {
            return Err(format!("Unsupported map dimensions: {width}x{height}"));
        }

        let rgba = decoded.to_rgba8().into_raw();
        // Release the decoder's original image buffer before allocating map layers.
        // The RGBA buffer is now the single CPU-side source for the remaining load steps.
        drop(decoded);
        let definition_text = fs::read_to_string(definition_path)
            .map_err(|error| format!("Could not read definition.csv: {error}"))?;
        let id_by_rgb = parse_definition_text(&definition_text)?;

        if !world
            .geography
            .matches_source(width, height, &rgba, &definition_text)
        {
            return Err("Map geometry changed. Start a new campaign to import it; the current campaign is preserved.".into());
        }
        let state_by_province: HashMap<_, _> = world
            .provinces
            .iter()
            .filter_map(|(&id, province)| province.state_id.map(|state| (id, state)))
            .collect();
        let owner_by_state: HashMap<_, _> = world
            .states
            .iter()
            .filter_map(|(&id, state)| state.owner.clone().map(|tag| (id, tag)))
            .collect();

        let (state_ids_by_pixel, state_bounds) =
            build_state_ids_by_pixel(width, height, &rgba, &id_by_rgb, &state_by_province);

        let (mut color_palette, _) = build_map_palette(
            &id_by_rgb,
            &state_by_province,
            &definition_text,
            Path::new(STATE_DIRECTORY_PATH),
            Some(&owner_by_state),
        );
        for (&rgb, &id) in &id_by_rgb {
            if let Some(colors) = color_palette.colors_by_rgb.get_mut(&rgb) {
                if world
                    .geography
                    .provinces
                    .get(&id)
                    .is_some_and(|p| p.definition.kind == map_graph::ProvinceKind::Land)
                {
                    colors.political = world
                        .controller_of_province(id)
                        .map(|tag| blend_country(colors.terrain, tag_color(tag), 0.48))
                        .unwrap_or([92, 96, 101]);
                }
            }
        }
        let political_texture =
            color_palette.texture_from_rgb(width, height, &rgba, 4, MapMode::Political);
        // One combined layer; capture updates replace only affected rectangles.
        let map_border_texture = build_map_border_texture(world, &rgba);
        let resource_markers = build_resource_markers(
            width,
            &rgba,
            &id_by_rgb,
            &state_by_province,
            Path::new(STATE_DIRECTORY_PATH),
        );
        let city_markers =
            build_city_markers(width, &rgba, &id_by_rgb, Path::new(STATE_DIRECTORY_PATH));
        // Index static marker positions once at import time. At high zoom the
        // viewport covers just a fraction of the world map.
        let city_grid = spatial_grid::SpatialGrid::build(
            width,
            height,
            256.0,
            city_markers.iter().map(|marker| (marker.x, marker.y)),
        );
        let resource_grid = spatial_grid::SpatialGrid::build(
            width,
            height,
            256.0,
            resource_markers.iter().map(|marker| (marker.x, marker.y)),
        );
        // CPU-side hit testing only needs RGB. Drop the fourth alpha byte per pixel
        // after every map texture and marker has been built.
        let mut rgb_pixels = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for pixel in rgba.chunks_exact(4) {
            rgb_pixels.extend_from_slice(&pixel[..3]);
        }
        drop(rgba);

        Ok(Self {
            width,
            height,
            rgb_pixels,
            id_by_rgb,
            state_by_province,
            map_border_texture,
            state_ids_by_pixel,
            state_bounds,
            highlighted_state_texture: None,
            construction_overlay_texture: None,
            construction_overlay_signature: None,
            political_texture,
            control_revision: world.control_revision,
            active_mode_texture: None,
            color_palette,
            resource_markers,
            city_markers,
            resource_grid,
            city_grid,
            zoom: 1.0,
            target_zoom: 1.0,
            pan: Vec2::ZERO,
            zoom_anchor: None,
            pinch_tracker: map_camera::PinchTracker::default(),

            map_mode: MapMode::Political,
        })
    }

    fn sync_control(&mut self, world: &World) {
        if self.control_revision == world.control_revision {
            return;
        }
        let changed: Vec<_> = world
            .provinces
            .values()
            .filter(|p| p.control_revision > self.control_revision)
            .filter_map(|p| {
                world
                    .geography
                    .provinces
                    .get(&p.id)
                    .and_then(|g| g.bounds.map(|b| (p, g.definition.rgb, b)))
            })
            .collect();
        for (p, rgb, _) in &changed {
            if let Some(colors) = self.color_palette.colors_by_rgb.get_mut(rgb) {
                colors.political = p
                    .controller
                    .as_deref()
                    .map(|tag| blend_country(colors.terrain, tag_color(tag), 0.48))
                    .unwrap_or([92, 96, 101]);
            }
        }
        // Upload just the affected raster rectangle, retaining terrain colors
        // for neighbouring provinces inside that rectangle.
        for (_, _, b) in changed {
            let w = b.max_x - b.min_x + 1;
            let h = b.max_y - b.min_y + 1;
            let mut bytes = Vec::with_capacity((w * h * 4) as usize);
            for y in b.min_y..=b.max_y {
                for x in b.min_x..=b.max_x {
                    let i = ((y * self.width + x) * 3) as usize;
                    let rgb = [
                        self.rgb_pixels[i],
                        self.rgb_pixels[i + 1],
                        self.rgb_pixels[i + 2],
                    ];
                    let color = self
                        .color_palette
                        .colors_by_rgb
                        .get(&rgb)
                        .map(|c| c.political)
                        .unwrap_or([92, 96, 101]);
                    bytes.extend_from_slice(&[color[0], color[1], color[2], 255]);
                }
            }
            self.political_texture.update_part(
                &Image {
                    bytes,
                    width: w as u16,
                    height: h as u16,
                },
                b.min_x as i32,
                b.min_y as i32,
                w as i32,
                h as i32,
            );
        }
        let borders = map_borders::BorderRaster::new(world, &self.rgb_pixels, 3);
        for bounds in map_borders::dirty_regions(world, self.control_revision) {
            let w = bounds.width();
            let h = bounds.height();
            self.map_border_texture.update_part(
                &Image {
                    bytes: borders.rgba(bounds),
                    width: w as u16,
                    height: h as u16,
                },
                bounds.left as i32,
                bounds.top as i32,
                w as i32,
                h as i32,
            );
        }
        self.control_revision = world.control_revision;
    }

    fn source_rect(&self) -> Rect {
        map_camera::source_for_view(
            self.width as f32,
            self.height as f32,
            self.zoom,
            self.pan,
            Rect::new(0.0, 0.0, screen_width(), screen_height()),
        )
    }

    fn set_zoom_at(&mut self, factor: f32, anchor: Vec2, view: Rect) {
        if view.w <= 0.0 || view.h <= 0.0 || !factor.is_finite() || factor <= 0.0 {
            return;
        }
        // The source pixel remains fixed beneath the pointer THROUGH the zoom
        // animation. Setting pan for the target zoom in advance caused jumps.
        let world = map_camera::screen_to_world(self.source_rect(), view, anchor);
        self.target_zoom =
            (self.target_zoom * factor).clamp(map_camera::MIN_ZOOM, map_camera::MAX_ZOOM);
        self.zoom_anchor = Some((world, anchor));
    }

    fn zoom_pinch(&mut self, gesture: map_camera::PinchMotion, view: Rect) {
        let world = map_camera::screen_to_world(self.source_rect(), view, gesture.previous_center);
        self.zoom =
            (self.zoom * gesture.zoom_factor).clamp(map_camera::MIN_ZOOM, map_camera::MAX_ZOOM);
        self.target_zoom = self.zoom;
        self.zoom_anchor = None;
        self.pan = map_camera::anchor_for_view(
            self.width as f32,
            self.height as f32,
            self.zoom,
            world,
            gesture.center,
            view,
        );
    }

    fn update_zoom(&mut self, dt: f32, view: Rect) {
        let zoom_delta = self.target_zoom - self.zoom;
        if zoom_delta.abs() < 0.0005 {
            self.zoom = self.target_zoom;
        } else {
            let easing = 1.0 - (-12.0 * dt.max(0.0)).exp();
            self.zoom += zoom_delta * easing;
        }
        if let Some((world, anchor)) = self.zoom_anchor {
            self.pan = map_camera::anchor_for_view(
                self.width as f32,
                self.height as f32,
                self.zoom,
                world,
                anchor,
                view,
            );
            if self.zoom == self.target_zoom {
                self.zoom_anchor = None;
            }
        }
        self.clamp_pan();
    }

    fn clamp_pan(&mut self) {
        self.pan = map_camera::clamp_for_view(
            self.width as f32,
            self.height as f32,
            self.zoom,
            self.pan,
            Rect::new(0.0, 0.0, screen_width(), screen_height()),
        );
    }

    fn province_at(&self, x: u32, y: u32) -> Option<(u32, [u8; 3])> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let offset = ((y as usize * self.width as usize) + x as usize) * 3;
        let rgb = [
            self.rgb_pixels[offset],
            self.rgb_pixels[offset + 1],
            self.rgb_pixels[offset + 2],
        ];
        self.id_by_rgb.get(&rgb).copied().map(|id| (id, rgb))
    }

    fn center_on(&mut self, point: Vec2) {
        let view = map_view_rect(self, false);
        self.pan = map_camera::anchor_for_view(
            self.width as f32,
            self.height as f32,
            self.zoom,
            point,
            vec2(view.w * 0.5, view.h * 0.5),
            view,
        );
        self.zoom_anchor = None;
    }
    fn focus_country(&mut self, world: &World, tag: &str) {
        let capital = fs::read_dir("data/hoi4/history/countries")
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.starts_with(&format!("{tag} -")) || n == format!("{tag}.txt")
                })
            })
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| research_data::parse_pdx(&s).ok())
            .and_then(|e| e.into_iter().find(|e| e.key == "capital"))
            .and_then(|e| e.atom().and_then(|a| a.parse::<u32>().ok()));
        let state = capital.and_then(|id| world.states.get(&id)).or_else(|| {
            world
                .countries
                .get(tag)
                .and_then(|c| c.state_ids.first())
                .and_then(|id| world.states.get(id))
        });
        let point = state.and_then(|s| {
            s.province_ids
                .iter()
                .find_map(|id| world.geography.provinces.get(id).and_then(|p| p.center))
        });
        if let Some(p) = point {
            if self.width > 2000 {
                self.zoom = 2.0;
                self.target_zoom = 2.0;
            }
            self.center_on(vec2(p.x as f32, p.y as f32));
        }
    }
}

/// Fill the compact state raster and its per-state pixel footprint in
/// one pass. No second full-image scan is needed to build the outline index.
fn build_state_ids_by_pixel(
    width: u32,
    height: u32,
    rgba: &[u8],
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_by_province: &HashMap<u32, u32>,
) -> (
    map_pixels::StatePixelIds,
    HashMap<u32, map_pixels::StateBounds>,
) {
    let max_state_id = state_by_province.values().copied().max().unwrap_or(0);
    let mut state_ids =
        map_pixels::StatePixelIds::new(width as usize * height as usize, max_state_id);
    let mut bounds = HashMap::<u32, map_pixels::StateBounds>::new();
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        if let Some(province_id) = id_by_rgb.get(&rgb) {
            let id = state_by_province.get(province_id).copied().unwrap_or(0);
            state_ids.set(index, id);
            if id != 0 {
                let x = index as u32 % width;
                let y = index as u32 / width;
                bounds
                    .entry(id)
                    .and_modify(|box_| box_.include(x, y))
                    .or_insert_with(|| map_pixels::StateBounds::new(x, y));
            }
        }
    }
    (state_ids, bounds)
}

/// Use current control for both initial rendering and later capture updates.
fn build_map_border_texture(world: &World, rgba: &[u8]) -> Texture2D {
    let borders = map_borders::BorderRaster::new(world, rgba, 4);
    let overlay = borders.rgba(borders.full_bounds());
    let texture = Texture2D::from_rgba8(
        world.geography.width as u16,
        world.geography.height as u16,
        &overlay,
    );
    texture.set_filter(FilterMode::Linear);
    texture
}

/// Upload only the state's bounding box, rather than a full 5632x2048
/// texture every time the player clicks a new province.
fn build_highlighted_state_texture(
    width: u32,
    height: u32,
    state_ids: &map_pixels::StatePixelIds,
    selected_state: u32,
    bounds: map_pixels::StateBounds,
) -> Texture2D {
    let overlay = map_pixels::outline_rgba(state_ids, width, height, selected_state, bounds);
    let texture = Texture2D::from_rgba8(bounds.width() as u16, bounds.height() as u16, &overlay);
    texture.set_filter(FilterMode::Linear);
    texture
}

/// Parse HOI4 definition.csv into an RGB -> province ID index.
/// Vanilla HOI4 files are semicolon-delimited; comma-delimited input is also accepted.

fn parse_state_visual_data(directory: &Path) -> (HashMap<u32, String>, HashMap<u32, f32>) {
    const MAP_DATE: (u32, u32, u32) = (1936, 1, 1);
    let mut owners = HashMap::new();
    let mut resources = HashMap::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return (owners, resources);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };

        // Reuse the Clausewitz parser that reads state membership so line layout
        // cannot cause owner assignments to be silently skipped.
        if let Ok(file_owners) = state::parse_state_owners(&source, MAP_DATE) {
            owners.extend(file_owners);
        }

        let mut state_id = None;
        let mut in_resources = false;
        let mut resource_depth = 0i32;
        for line in source.lines() {
            let clean = line.split('#').next().unwrap_or("").trim();
            if clean.is_empty() {
                continue;
            }
            if state_id.is_none() && clean.starts_with("id") {
                if let Some(value) = clean
                    .split('=')
                    .nth(1)
                    .and_then(|v| v.trim().parse::<u32>().ok())
                {
                    state_id = Some(value);
                }
            }
            let Some(id) = state_id else { continue };

            if clean.starts_with("resources") && clean.contains('{') {
                in_resources = true;
                resource_depth =
                    clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if let Some((_, tail)) = clean.split_once('{') {
                    parse_resource_values(tail, id, &mut resources);
                }
                if resource_depth <= 0 {
                    in_resources = false;
                }
            } else if in_resources {
                parse_resource_values(clean, id, &mut resources);
                resource_depth +=
                    clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if resource_depth <= 0 {
                    in_resources = false;
                }
            }
        }
    }
    (owners, resources)
}

fn parse_resource_values(line: &str, state_id: u32, resources: &mut HashMap<u32, f32>) {
    for key in [
        "steel",
        "aluminium",
        "aluminum",
        "rubber",
        "tungsten",
        "chromium",
        "oil",
    ] {
        if let Some((_, tail)) = line.split_once(key) {
            if let Some((_, value)) = tail.split_once('=') {
                let value = value
                    .trim()
                    .trim_matches(|c: char| c == '}' || c.is_whitespace() || c == '"');
                if let Ok(amount) = value.split_whitespace().next().unwrap_or("").parse::<f32>() {
                    *resources.entry(state_id).or_insert(0.0) += amount;
                }
            }
        }
    }
}

fn tag_color(tag: &str) -> [u8; 3] {
    // HOI4-inspired fallback palette. Real country colors can replace these once
    // common/countries/*.txt is imported; avoid hash colors that make neighbors indistinguishable.
    match tag {
        "GER" => [105, 112, 119],
        "ENG" | "GBR" => [190, 52, 57],
        "FRA" => [62, 105, 190],
        "SOV" | "RUS" => [190, 62, 55],
        "USA" => [65, 119, 190],
        "ITA" => [73, 150, 91],
        "JAP" => [205, 120, 135],
        "CHI" => [205, 174, 95],
        "POL" => [205, 145, 160],
        "AUS" => [195, 185, 150],
        "CAN" => [185, 60, 66],
        "BRA" => [80, 155, 91],
        "SPR" | "SPB" => [210, 170, 75],
        "TUR" => [180, 80, 70],
        "HOL" => [230, 135, 65],
        "BEL" => [215, 195, 85],
        "CZE" => [90, 145, 180],
        "ROM" => [110, 155, 80],
        "YUG" => [150, 160, 180],
        "GRE" => [80, 155, 185],
        "NOR" => [100, 155, 195],
        "SWE" => [75, 135, 190],
        "FIN" => [175, 190, 200],
        "POR" => [65, 145, 105],
        "MEX" => [75, 145, 95],
        "ARG" => [100, 170, 195],
        _ => {
            let hash = tag
                .bytes()
                .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
            // Keep all channels bright enough and separate hues more strongly.
            let hue = (hash % 360) as f32;
            let c = 0.62_f32;
            let x = c * (1.0 - (((hue / 60.0) % 2.0) - 1.0).abs());
            let (r, g, b) = match hue as u32 / 60 {
                0 => (c, x, 0.0),
                1 => (x, c, 0.0),
                2 => (0.0, c, x),
                3 => (0.0, x, c),
                4 => (x, 0.0, c),
                _ => (c, 0.0, x),
            };
            [
                ((r + 0.18) * 255.0) as u8,
                ((g + 0.18) * 255.0) as u8,
                ((b + 0.18) * 255.0) as u8,
            ]
        }
    }
}

fn terrain_color(name: &str) -> [u8; 3] {
    match name.to_ascii_lowercase().as_str() {
        "plains" => [190, 176, 112],
        "forest" | "forests" => [74, 119, 75],
        "hills" => [147, 132, 91],
        "mountain" | "mountains" => [133, 128, 122],
        "desert" => [215, 184, 113],
        "marsh" => [92, 127, 105],
        "jungle" => [48, 105, 63],
        "urban" => [135, 132, 137],
        "ocean" | "sea" => [55, 105, 155],
        "lakes" | "lake" => [70, 130, 174],
        "river" => [76, 143, 185],
        "unknown" => [95, 101, 107],
        _ => [115, 123, 100],
    }
}

fn blend_country(base: [u8; 3], country: [u8; 3], alpha: f32) -> [u8; 3] {
    [
        (base[0] as f32 * (1.0 - alpha) + country[0] as f32 * alpha) as u8,
        (base[1] as f32 * (1.0 - alpha) + country[1] as f32 * alpha) as u8,
        (base[2] as f32 * (1.0 - alpha) + country[2] as f32 * alpha) as u8,
    ]
}

/// Source colors for each unique province RGB. We only store a few bytes
/// per province, not three additional full-screen images on the GPU.
#[derive(Clone, Copy)]
struct ProvinceColors {
    political: [u8; 3],
    resources: [u8; 3],
    terrain: [u8; 3],
}

struct MapColorPalette {
    colors_by_rgb: HashMap<[u8; 3], ProvinceColors>,
}

impl MapColorPalette {
    fn texture_from_rgb(
        &self,
        width: u32,
        height: u32,
        pixels: &[u8],
        stride: usize,
        mode: MapMode,
    ) -> Texture2D {
        assert!(stride >= 3 && pixels.len() == width as usize * height as usize * stride);
        // Only ONE RGBA scratch buffer is allocated for the active mode; all
        // inactive modes exist as tiny province palettes in CPU memory.
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        let neutral = ProvinceColors {
            political: [92, 96, 101],
            resources: [54, 59, 65],
            terrain: [95, 101, 107],
        };
        for (index, rgb_pixel) in pixels.chunks_exact(stride).enumerate() {
            let rgb = [rgb_pixel[0], rgb_pixel[1], rgb_pixel[2]];
            let colors = self.colors_by_rgb.get(&rgb).copied().unwrap_or(neutral);
            let selected = match mode {
                MapMode::Political => colors.political,
                MapMode::Resources => colors.resources,
                MapMode::Terrain => colors.terrain,
            };
            let offset = index * 4;
            rgba[offset..offset + 3].copy_from_slice(&selected);
            rgba[offset + 3] = 255;
        }
        let texture = Texture2D::from_rgba8(width as u16, height as u16, &rgba);
        texture.set_filter(FilterMode::Linear);
        texture
    }
}

fn build_map_palette(
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_by_province: &HashMap<u32, u32>,
    definition_text: &str,
    state_directory: &Path,
    owner_override: Option<&HashMap<u32, String>>,
) -> (MapColorPalette, HashMap<u32, String>) {
    let (loaded_owners, resources_by_state) = parse_state_visual_data(state_directory);
    let owner_by_state = owner_override.cloned().unwrap_or(loaded_owners);
    let mut terrain_by_province = HashMap::<u32, String>::new();
    let mut water_provinces = HashSet::<u32>::new();
    let delimiter = definition_text
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| if l.contains(';') { ';' } else { ',' })
        .unwrap_or(';');
    for line in definition_text.lines().skip(1) {
        let fields: Vec<&str> = line.split(delimiter).collect();
        if fields.len() < 7 {
            continue;
        }
        if let Ok(id) = fields[0]
            .trim()
            .trim_start_matches('\u{feff}')
            .parse::<u32>()
        {
            let province_type = fields[4].trim().to_ascii_lowercase();
            let terrain_name = fields[6].trim().to_owned();
            let terrain_lower = terrain_name.to_ascii_lowercase();
            if matches!(province_type.as_str(), "sea" | "lake")
                || matches!(terrain_lower.as_str(), "ocean" | "sea" | "lake" | "lakes")
            {
                water_provinces.insert(id);
            }
            terrain_by_province.insert(id, terrain_name);
        }
    }
    let max_resources = resources_by_state
        .values()
        .copied()
        .fold(0.0f32, f32::max)
        .max(1.0);
    let mut colors_by_rgb = HashMap::with_capacity(id_by_rgb.len());

    for (&rgb, &province_id) in id_by_rgb {
        let state_id = state_by_province.get(&province_id).copied().unwrap_or(0);
        let base_terrain = terrain_by_province
            .get(&province_id)
            .map(|n| terrain_color(n))
            .unwrap_or([95, 101, 107]);
        let water = water_provinces.contains(&province_id);
        let political = if water {
            [55, 105, 155]
        } else {
            owner_by_state
                .get(&state_id)
                .map(|owner| blend_country(base_terrain, tag_color(owner), 0.48))
                .unwrap_or([92, 96, 101])
        };
        let amount = resources_by_state.get(&state_id).copied().unwrap_or(0.0);
        let resources = if water {
            base_terrain
        } else if amount <= 0.0 {
            [54, 59, 65]
        } else {
            let strength = (amount / max_resources).clamp(0.0, 1.0);
            if strength < 0.5 {
                let t = strength * 2.0;
                [
                    (45.0 + t * 190.0) as u8,
                    (90.0 + t * 95.0) as u8,
                    (155.0 - t * 115.0) as u8,
                ]
            } else {
                let t = (strength - 0.5) * 2.0;
                [235, (185.0 - t * 135.0) as u8, (40.0 - t * 5.0) as u8]
            }
        };
        colors_by_rgb.insert(
            rgb,
            ProvinceColors {
                political,
                resources,
                terrain: base_terrain,
            },
        );
    }
    (MapColorPalette { colors_by_rgb }, owner_by_state)
}

fn build_resource_markers(
    width: u32,
    rgba: &[u8],
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_by_province: &HashMap<u32, u32>,
    state_directory: &Path,
) -> Vec<ResourceMarker> {
    let mut resource_types = HashMap::<u32, Vec<(String, f32)>>::new();
    let Ok(entries) = fs::read_dir(state_directory) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let mut state_id = None;
        let mut in_resources = false;
        let mut depth = 0i32;
        for line in source.lines() {
            let clean = line.split('#').next().unwrap_or("").trim();
            if clean.starts_with("id") {
                state_id = clean
                    .split_once('=')
                    .and_then(|(_, v)| v.trim().parse::<u32>().ok())
                    .or(state_id);
            }
            if clean.starts_with("resources") && clean.contains('{') {
                in_resources = true;
                depth = clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if let Some(id) = state_id {
                    for (key, value) in resource_pairs(clean) {
                        if let Ok(amount) = value.parse::<f32>() {
                            if amount > 0.0 {
                                resource_types.entry(id).or_default().push((key, amount));
                            }
                        }
                    }
                }
                if depth <= 0 {
                    in_resources = false;
                }
                continue;
            }
            if in_resources {
                if let Some(id) = state_id {
                    for (key, value) in resource_pairs(clean) {
                        if let Ok(amount) = value.parse::<f32>() {
                            if amount > 0.0 {
                                resource_types.entry(id).or_default().push((key, amount));
                            }
                        }
                    }
                }
                depth += clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if depth <= 0 {
                    in_resources = false;
                }
            }
        }
    }

    let mut sums = HashMap::<u32, (f64, f64, u32)>::new();
    for (pixel_index, pixel) in rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let Some(province) = id_by_rgb.get(&rgb) else {
            continue;
        };
        let Some(state_id) = state_by_province.get(province) else {
            continue;
        };
        let x = (pixel_index % width as usize) as f64;
        let y = (pixel_index / width as usize) as f64;
        let item = sums.entry(*state_id).or_insert((0.0, 0.0, 0));
        item.0 += x;
        item.1 += y;
        item.2 += 1;
    }

    let mut markers = Vec::new();
    let mut states: Vec<u32> = resource_types.keys().copied().collect();
    states.sort_unstable();
    for state_id in states {
        let Some((sx, sy, count)) = sums.get(&state_id).copied() else {
            continue;
        };
        if count == 0 {
            continue;
        }
        let (cx, cy) = ((sx / count as f64) as f32, (sy / count as f64) as f32);
        let mut values = resource_types.remove(&state_id).unwrap_or_default();
        values.sort_by(|a, b| b.1.total_cmp(&a.1));
        // Keep every resource type visible, offset around the state's center to reduce overlap.
        let take = values.len().min(6);
        for (index, (name, _amount)) in values.into_iter().take(take).enumerate() {
            let (label, color) = match name.as_str() {
                "oil" => ("OIL", Color::from_rgba(55, 55, 62, 255)),
                "steel" => ("Fe", Color::from_rgba(110, 145, 165, 255)),
                "aluminium" | "aluminum" => ("Al", Color::from_rgba(170, 175, 185, 255)),
                "rubber" => ("R", Color::from_rgba(70, 145, 85, 255)),
                "tungsten" => ("W", Color::from_rgba(160, 125, 75, 255)),
                "chromium" => ("Cr", Color::from_rgba(145, 105, 160, 255)),
                _ => continue,
            };
            let angle = index as f32 * 1.15;
            markers.push(ResourceMarker {
                x: cx + angle.cos() * 5.0,
                y: cy + angle.sin() * 5.0,
                label,
                color,
            });
        }
    }
    markers
}

fn build_city_markers(
    width: u32,
    rgba: &[u8],
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_directory: &Path,
) -> Vec<CityMarker> {
    let mut points = HashMap::<u32, u32>::new();
    let Ok(entries) = fs::read_dir(state_directory) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();
        let mut in_vp = false;
        let mut depth = 0i32;
        for line in lines {
            let clean = line.split('#').next().unwrap_or("").trim();
            if clean.starts_with("victory_points") && clean.contains('{') {
                in_vp = true;
                depth = clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if let Some((_, tail)) = clean.split_once('{') {
                    parse_victory_point_pairs(tail, &mut points);
                }
                if depth <= 0 {
                    in_vp = false;
                }
                continue;
            }
            if in_vp {
                parse_victory_point_pairs(clean, &mut points);
                depth += clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if depth <= 0 {
                    in_vp = false;
                }
            }
        }
    }

    let mut sums = HashMap::<u32, (f64, f64, u32)>::new();
    for (pixel_index, pixel) in rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let Some(province_id) = id_by_rgb.get(&rgb) else {
            continue;
        };
        if !points.contains_key(province_id) {
            continue;
        }
        let x = (pixel_index % width as usize) as f64;
        let y = (pixel_index / width as usize) as f64;
        let item = sums.entry(*province_id).or_insert((0.0, 0.0, 0));
        item.0 += x;
        item.1 += y;
        item.2 += 1;
    }

    let names = load_victory_point_names();
    let mut markers = Vec::new();
    let mut ids: Vec<u32> = points.keys().copied().collect();
    ids.sort_unstable();
    for province_id in ids {
        let Some((sx, sy, count)) = sums.get(&province_id).copied() else {
            continue;
        };
        if count == 0 {
            continue;
        }
        let name = names.get(&province_id).cloned().unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        let display_name = !name.trim().is_empty()
            && !lower.starts_with("vp_")
            && !lower.starts_with("victory_points_")
            && !lower.starts_with("victory_points.");
        // Cache text measurement at the maximum label size; never measure text in the frame loop.
        let label_width = if display_name {
            measure_ui_text(name.trim(), 13).width
        } else {
            0.0
        };
        markers.push(CityMarker {
            province_id,
            victory_points: points[&province_id],
            x: (sx / count as f64) as f32,
            y: (sy / count as f64) as f32,
            name,
            display_name,
            label_width,
        });
    }
    // High-value cities get label priority when the visible-label budget is reached.
    markers.sort_by(|a, b| {
        b.victory_points
            .cmp(&a.victory_points)
            .then_with(|| a.province_id.cmp(&b.province_id))
    });
    markers
}

fn parse_victory_point_pairs(line: &str, points: &mut HashMap<u32, u32>) {
    // HOI4 state files usually encode victory points as "province_id value"
    // (for example: victory_points = { 1234 5 }), not "province_id = value".
    let tokens: Vec<&str> = line
        .split(|c: char| c.is_whitespace() || c == '{' || c == '}')
        .filter(|s| !s.is_empty())
        .collect();

    // Support both the vanilla pair format and explicit key=value variants.
    let mut i = 0;
    while i < tokens.len() {
        if i + 2 < tokens.len() && tokens[i + 1] == "=" {
            if let (Ok(province), Ok(value)) =
                (tokens[i].parse::<u32>(), tokens[i + 2].parse::<u32>())
            {
                points.insert(province, value);
                i += 3;
                continue;
            }
        }
        if i + 1 < tokens.len() {
            if let (Ok(province), Ok(value)) =
                (tokens[i].parse::<u32>(), tokens[i + 1].parse::<u32>())
            {
                points.insert(province, value);
                i += 2;
                continue;
            }
        }
        i += 1;
    }
}

fn localisation_roots() -> Vec<std::path::PathBuf> {
    // Prefer the project's copied game data, but also support running with the
    // original HOI4 installation present on the user's machine.
    let candidates = [
        "data/hoi4/localisation",
        "data/hoi4/localization",
        "E:/перекид с ссд/games/Hearts of Iron IV/localisation",
        "E:/перекид с ссд/games/Hearts of Iron IV/localization",
    ];
    let mut roots = Vec::new();
    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_dir() && !roots.iter().any(|existing| existing == path) {
            roots.push(path.to_path_buf());
        }
    }
    roots
}

fn load_state_names(state_directory: &Path) -> HashMap<u32, String> {
    fn visit_localisation(directory: &Path, names: &mut HashMap<String, String>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                visit_localisation(&path, names);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("yml"))
            {
                let Ok(source) = fs::read_to_string(&path) else {
                    continue;
                };
                for line in source.lines() {
                    let line = line.trim().trim_start_matches('\u{feff}');
                    let Some((key, raw_value)) = line.split_once(':') else {
                        continue;
                    };
                    let key = key.trim().trim_matches('"');
                    if !key.starts_with("STATE_") {
                        continue;
                    }
                    let raw_value = raw_value.split('#').next().unwrap_or("").trim();
                    let value = raw_value
                        .strip_prefix("0")
                        .map(str::trim_start)
                        .unwrap_or(raw_value);
                    let value = value.trim().trim_matches('"').trim();
                    if !value.is_empty() && !value.starts_with("STATE_") {
                        names.insert(key.to_owned(), value.to_owned());
                    }
                }
            }
        }
    }

    let mut key_by_id = HashMap::<u32, String>::new();
    let Ok(entries) = fs::read_dir(state_directory) else {
        return HashMap::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let mut state_id = None;
        let mut state_key = None;
        for line in source.lines() {
            let clean = line.split('#').next().unwrap_or("").trim();
            if state_id.is_none() && clean.starts_with("id") {
                state_id = clean
                    .split_once('=')
                    .and_then(|(_, value)| value.trim().parse::<u32>().ok());
            }
            if state_key.is_none() && clean.starts_with("name") {
                state_key = clean
                    .split_once('=')
                    .map(|(_, value)| value.trim().trim_matches('"').to_owned());
            }
            if state_id.is_some() && state_key.is_some() {
                break;
            }
        }
        if let (Some(id), Some(key)) = (state_id, state_key) {
            key_by_id.insert(id, key);
        }
    }

    let mut localised = HashMap::<String, String>::new();
    for root in localisation_roots() {
        visit_localisation(&root, &mut localised);
    }

    key_by_id
        .into_iter()
        .map(|(id, key)| {
            let name = localised
                .get(&key)
                .cloned()
                .unwrap_or_else(|| format!("State {id}"));
            (id, name)
        })
        .collect()
}

fn load_victory_point_names() -> HashMap<u32, String> {
    fn visit(directory: &Path, names: &mut HashMap<u32, String>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                visit(&path, names);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("yml"))
            {
                if let Ok(source) = fs::read_to_string(&path) {
                    parse_vp_localisation(&source, names);
                }
            }
        }
    }

    let mut names = HashMap::new();
    for root in localisation_roots() {
        visit(&root, &mut names);
    }
    names
}

fn parse_vp_localisation(source: &str, names: &mut HashMap<u32, String>) {
    for line in source.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches('"');
        let Some(raw_id) = key.strip_prefix("VICTORY_POINTS_") else {
            continue;
        };
        let Ok(id) = raw_id.parse::<u32>() else {
            continue;
        };

        // Accept HOI4 localisation lines such as VICTORY_POINTS_123:0 "City".
        // Ignore trailing comments and remove the numeric localisation version.
        let value = value.split('#').next().unwrap_or("").trim();
        let value = value
            .strip_prefix("0")
            .map(str::trim_start)
            .unwrap_or(value);
        let value = value.trim_matches('"').trim();
        if value.is_empty() {
            continue;
        }

        // Do not treat untranslated/internal keys as readable city names.
        let lower = value.to_ascii_lowercase();
        if lower.starts_with("vp_") || lower.starts_with("victory_points_") {
            continue;
        }
        names.entry(id).or_insert_with(|| value.to_owned());
    }
}

fn resource_pairs(line: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for part in line.split(|c: char| c == '{' || c == '}' || c.is_whitespace()) {
        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim().to_owned();
            let value = value.trim().trim_matches('"').to_owned();
            if matches!(
                key.as_str(),
                "oil" | "steel" | "aluminium" | "aluminum" | "rubber" | "tungsten" | "chromium"
            ) {
                pairs.push((key, value));
            }
        }
    }
    // Clausewitz usually separates key and '=' with spaces, so parse token triplets too.
    let tokens: Vec<&str> = line.split_whitespace().collect();
    for window in tokens.windows(3) {
        if window[1] == "="
            && matches!(
                window[0],
                "oil" | "steel" | "aluminium" | "aluminum" | "rubber" | "tungsten" | "chromium"
            )
        {
            pairs.push((
                window[0].to_owned(),
                window[2]
                    .trim_matches(|c: char| c == '}' || c == '{' || c == '"')
                    .to_owned(),
            ));
        }
    }
    pairs
}

fn parse_definition_text(text: &str) -> Result<HashMap<[u8; 3], u32>, String> {
    Ok(map_graph::parse_definitions(text)?
        .into_values()
        .map(|d| (d.rgb, d.id))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{build_map_palette, parse_definition_text};
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn parses_hoi4_semicolon_delimited_definitions() {
        let source = "province;red;green;blue;type;coastal;terrain;continent\n1;10;20;30;land;false;plains;1\n2;40;50;60;sea;true;ocean;1\n";
        let index = parse_definition_text(source).expect("valid HOI4 definition");
        assert_eq!(index.get(&[10, 20, 30]), Some(&1));
        assert_eq!(index.get(&[40, 50, 60]), Some(&2));
    }

    #[test]
    fn accepts_comma_delimited_definitions() {
        let source =
            "province,red,green,blue,type,coastal,terrain,continent\n7,1,2,3,land,false,plains,1\n";
        let index = parse_definition_text(source).expect("valid comma-delimited definition");
        assert_eq!(index.get(&[1, 2, 3]), Some(&7));
    }

    #[test]
    fn palette_keeps_original_political_terrain_and_resource_colours() {
        let definition = "province;red;green;blue;type;coastal;terrain;continent\\n1;10;20;30;sea;false;ocean;1\\n2;40;50;60;land;false;plains;1\\n";
        let definition = definition.replace("\\n", "\n");
        let id_by_rgb = parse_definition_text(&definition).unwrap();
        let (palette, owners) = build_map_palette(
            &id_by_rgb,
            &HashMap::new(),
            &definition,
            Path::new("this-test-directory-intentionally-does-not-exist"),
            None,
        );
        assert!(owners.is_empty());
        let water = palette.colors_by_rgb.get(&[10, 20, 30]).unwrap();
        assert_eq!(water.political, [55, 105, 155]);
        assert_eq!(water.terrain, [55, 105, 155]);
        assert_eq!(water.resources, [55, 105, 155]);
        let land = palette.colors_by_rgb.get(&[40, 50, 60]).unwrap();
        assert_eq!(land.political, [92, 96, 101]);
        assert_eq!(land.terrain, [190, 176, 112]);
        assert_eq!(land.resources, [54, 59, 65]);
    }

    #[test]
    fn rejects_duplicate_province_ids() {
        let source = "province;red;green;blue\n4;1;2;3\n4;4;5;6\n";
        assert!(
            parse_definition_text(source)
                .unwrap_err()
                .contains("Duplicate province ID 4")
        );
    }

    #[test]
    fn rejects_duplicate_rgb_colours() {
        let source = "province;red;green;blue\n4;1;2;3\n5;1;2;3\n";
        assert!(
            parse_definition_text(source)
                .unwrap_err()
                .contains("Duplicate province RGB colour")
        );
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "HOI4 Mobile — prototype".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

fn draw_time_controls(game: &GameState, _date_label: &str) -> (bool, Option<u8>) {
    let r = ui_layout::Layout::current().time();
    draw_rectangle(r.x, r.y, r.w, r.h, hoi_ui::BASE);
    let date = Rect::new(r.x + 4.0, r.y + 2.0, r.w - 82.0, r.h - 4.0);
    let months = [
        "янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
    ];
    let month = months
        .get(game.date.month.saturating_sub(1) as usize)
        .unwrap_or(&"—");
    hoi_ui::label(
        &format!("{} {} {}", game.date.day, month, game.date.year),
        Rect::new(date.x, date.y, date.w, 17.0),
        12.0,
        hoi_ui::TEXT,
    );
    hoi_ui::label(
        &format!(
            "{:02}:00  {}",
            game.hour,
            if game.paused { "ПАУЗА" } else { "" }
        ),
        Rect::new(date.x, date.y + 15.0, date.w, 15.0),
        10.0,
        hoi_ui::GOLD,
    );
    hoi_ui::tooltip(
        date,
        "Время · Пробел",
        "Нажмите на дату, чтобы поставить игру на паузу или продолжить.",
    );
    let pause = hoi_ui::inside(date) && is_mouse_button_pressed(MouseButton::Left);
    let mut speed = None;
    for n in 1..=5 {
        let b = Rect::new(
            r.right() - 78.0 + (n - 1) as f32 * 15.0,
            r.y + 5.0,
            12.0,
            r.h - 9.0,
        );
        let color = if n <= game.speed {
            if game.paused {
                hoi_ui::GOLD
            } else {
                hoi_ui::GREEN
            }
        } else {
            hoi_ui::EDGE
        };
        draw_rectangle(b.x, b.y, b.w, b.h, color);
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, hoi_ui::BASE);
        hoi_ui::tooltip(
            b,
            &format!("Скорость {n} · {n}"),
            "Выбрать скорость и продолжить симуляцию.",
        );
        if hoi_ui::inside(b) && is_mouse_button_pressed(MouseButton::Left) {
            speed = Some(n);
        }
    }
    (pause, speed)
}

fn button(label: &str, x: f32, y: f32, width: f32, height: f32) -> bool {
    hoi_ui::small_button(label, Rect::new(x, y, width, height), false)
}

fn draw_background() {
    clear_background(hoi_ui::BASE);
    // Fine etched horizontal lines instead of flat prototype-blue rectangles.
    for i in 0..20 {
        let y = (screen_height() / 20.0) * i as f32;
        let tone = 0.075 + i as f32 * 0.0016;
        draw_rectangle(
            0.0,
            y,
            screen_width(),
            (screen_height() / 20.0) + 1.0,
            Color::new(tone, tone + 0.009, tone + 0.010, 1.0),
        );
    }
    for i in 0..8 {
        let y = i as f32 * 100.0 + 52.0;
        if y < screen_height() {
            draw_line(
                0.0,
                y,
                screen_width(),
                y,
                1.0,
                Color::new(0.18, 0.19, 0.18, 0.17),
            );
        }
    }
    draw_rectangle(0.0, 0.0, screen_width(), 4.0, hoi_ui::GOLD);
}

fn draw_loading(stage: &str, progress: f32) {
    draw_background();
    let x = (screen_width() - 440.0).max(32.0) * 0.5;
    draw_ui_text("HOI4 MOBILE", x, screen_height() * 0.4, 36.0, hoi_ui::GOLD);
    hoi_ui::label(
        stage,
        Rect::new(
            x,
            screen_height() * 0.4 + 24.0,
            (screen_width() - 2.0 * x).max(1.0),
            30.0,
        ),
        14.0,
        hoi_ui::TEXT,
    );
    hoi_ui::progress_bar(
        Rect::new(
            x,
            screen_height() * 0.4 + 76.0,
            (screen_width() - 2.0 * x).min(440.0),
            10.0,
        ),
        progress,
    );
}

fn draw_main_menu(
    map: Option<&ProvinceMap>,
    can_continue: bool,
    date: game::GameDate,
    load: &mut bool,
) -> Screen {
    draw_background();
    let compact = screen_width() < 760.0;
    if let Some(map) = map {
        draw_texture_ex(
            &map.political_texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                ..Default::default()
            },
        );
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            Color::new(0.02, 0.035, 0.04, 0.77),
        );
    }
    let x = if compact { 22.0 } else { 72.0 };
    let width = if compact {
        (screen_width() - 44.0).min(360.0)
    } else {
        350.0
    };
    draw_ui_text(
        "HOI4 MOBILE",
        x,
        if compact { 96.0 } else { 112.0 },
        if compact { 37.0 } else { 51.0 },
        hoi_ui::GOLD,
    );
    draw_ui_text(
        "ГЛОБАЛЬНАЯ СТРАТЕГИЯ",
        x + 3.0,
        if compact { 127.0 } else { 151.0 },
        16.0,
        hoi_ui::TEXT,
    );
    let y = if compact { 167.0 } else { 198.0 };
    hoi_ui::panel(x - 10.0, y - 12.0, width + 20.0, 320.0);
    if button("Одиночная игра", x, y, width, 45.0) {
        return Screen::CountrySelect;
    }
    if can_continue {
        if button("Продолжить кампанию", x, y + 55.0, width, 45.0) {
            return Screen::MapViewer;
        }
    } else {
        hoi_ui::disabled(
            "Продолжить кампанию",
            Rect::new(x, y + 55.0, width, 45.0),
            "Кампания ещё не начата. Выберите страну в одиночной игре.",
        );
    }
    hoi_ui::placeholder(Rect::new(x, y + 110.0, width, 45.0), "Сетевая игра");
    if button("Настройки", x, y + 165.0, width, 45.0) {
        return Screen::Settings;
    }
    if button("Загрузить сохранение", x, y + 220.0, width, 45.0) {
        *load = true;
        return Screen::MapViewer;
    }
    draw_ui_text(
        &format!("{} · КАМПАНИЯ", date.year),
        x,
        screen_height() - 54.0,
        16.0,
        hoi_ui::GOLD,
    );
    draw_ui_text(
        "Независимый прототип · Rust / Android",
        x,
        screen_height() - 28.0,
        12.0,
        hoi_ui::MUTED,
    );
    Screen::MainMenu
}

fn country_display_name(tag: &str) -> &str {
    match tag {
        "GER" => "Германский рейх",
        "ENG" | "GBR" => "Великобритания",
        "FRA" => "Франция",
        "SOV" | "RUS" => "Советский Союз",
        "USA" => "Соединённые Штаты",
        "ITA" => "Италия",
        "JAP" => "Япония",
        "CHI" => "Китай",
        "POL" => "Польша",
        "AUS" => "Австрия",
        "CAN" => "Канада",
        "BRA" => "Бразилия",
        "SPR" => "Испания",
        "SPB" => "Националистическая Испания",
        "TUR" => "Турция",
        "HOL" => "Нидерланды",
        "BEL" => "Бельгия",
        "CZE" => "Чехословакия",
        "ROM" => "Румыния",
        "YUG" => "Югославия",
        "GRE" => "Греция",
        "NOR" => "Норвегия",
        "SWE" => "Швеция",
        "FIN" => "Финляндия",
        "POR" => "Португалия",
        "MEX" => "Мексика",
        "ARG" => "Аргентина",
        _ => tag,
    }
}

fn ordered_country_tags(world: &World) -> Vec<String> {
    let priority = [
        "GER", "ENG", "GBR", "FRA", "SOV", "RUS", "USA", "ITA", "JAP", "CHI", "POL", "CAN", "AUS",
        "BRA", "SPR", "SPB", "TUR", "HOL", "BEL", "CZE",
    ];
    let mut tags: Vec<String> = world.countries.keys().cloned().collect();
    tags.sort_by(|a, b| {
        let rank = |tag: &str| {
            priority
                .iter()
                .position(|candidate| *candidate == tag)
                .unwrap_or(usize::MAX)
        };
        rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
    });
    tags
}

fn draw_country_select(
    world: &World,
    military: &warfare::Warfare,
    map: Option<&ProvinceMap>,
    selected_country: &mut Option<String>,
    page: &mut usize,
    date: &mut game::GameDate,
    bookmarks: &[game::GameDate],
    load_error: &str,
    retry_requested: &mut bool,
) -> Screen {
    draw_background();
    if let Some(map) = map {
        draw_texture_ex(
            &map.political_texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                ..Default::default()
            },
        );
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            Color::new(0.02, 0.035, 0.04, 0.73),
        );
    }
    let compact = screen_width() < 760.0;
    let width = screen_width();
    let height = screen_height();
    draw_ui_text("ВЫБОР СТРАНЫ", 24.0, 42.0, 26.0, hoi_ui::GOLD);
    if button("Назад", width - 108.0, 15.0, 86.0, 31.0) || is_key_pressed(KeyCode::Escape) {
        return Screen::MainMenu;
    }
    hoi_ui::section("СЦЕНАРИЙ", 24.0, 61.0, width - 225.0);
    let dates: Vec<_> = bookmarks
        .iter()
        .filter(|d| matches!(d.year, 1936 | 1939))
        .copied()
        .collect();
    if hoi_ui::small_button(
        &format!("{} ▸", date.display()),
        Rect::new(width - 192.0, 58.0, 168.0, 28.0),
        false,
    ) && dates.len() > 1
    {
        let next = dates
            .iter()
            .position(|d| d == date)
            .map_or(0, |i| (i + 1) % dates.len());
        *date = dates[next];
    }
    let tags = ordered_country_tags(world);
    if tags.is_empty() || !load_error.is_empty() {
        let card_width = (width - 32.0).min(800.0);
        let card = Rect::new(
            (width - card_width) * 0.5,
            106.0,
            card_width,
            (height - 132.0).clamp(150.0, 300.0),
        );
        hoi_ui::panel(card.x, card.y, card.w, card.h);
        hoi_ui::label(
            if load_error.is_empty() {
                "Страны не найдены"
            } else {
                "Не удалось загрузить карту"
            },
            Rect::new(card.x + 16.0, card.y + 15.0, card.w - 32.0, 30.0),
            if card.w < 420.0 { 18.0 } else { 20.0 },
            hoi_ui::GOLD,
        );
        hoi_ui::paragraph(
            if load_error.is_empty() {
                "В данных выбранного сценария нет стран."
            } else {
                load_error
            },
            Rect::new(
                card.x + 16.0,
                card.y + 62.0,
                card.w - 32.0,
                (card.h - 128.0).max(22.0),
            ),
            15.0,
            hoi_ui::TEXT,
        );
        if hoi_ui::small_button(
            "Повторить загрузку",
            Rect::new(
                card.x + 16.0,
                card.bottom() - 52.0,
                (card.w - 32.0).min(260.0),
                34.0,
            ),
            false,
        ) {
            *retry_requested = true;
        }
        return Screen::CountrySelect;
    }
    if !selected_country.as_ref().is_some_and(|t| tags.contains(t)) {
        *selected_country = tags.first().cloned();
    }
    let grid_width = if compact { width - 32.0 } else { width - 390.0 };
    let cols = if compact { 2 } else { 3 };
    let rows = if compact { 3 } else { 4 };
    let per = cols * rows;
    let pages = tags.len().div_ceil(per);
    *page = (*page).min(pages - 1);
    let cw = (grid_width - 12.0 * (cols - 1) as f32) / cols as f32;
    let ch = if compact { 56.0 } else { 74.0 };
    for (slot, tag) in tags.iter().skip(*page * per).take(per).enumerate() {
        let r = Rect::new(
            16.0 + (slot % cols) as f32 * (cw + 12.0),
            106.0 + (slot / cols) as f32 * (ch + 10.0),
            cw,
            ch,
        );
        let chosen = selected_country.as_deref() == Some(tag);
        hoi_ui::row_bg(r, chosen, slot % 2 == 1);
        hoi_ui::flag(tag, Rect::new(r.x + 9.0, r.y + 12.0, 47.0, 30.0));
        hoi_ui::label(
            country_display_name(tag),
            Rect::new(r.x + 67.0, r.y + 12.0, cw - 74.0, 26.0),
            if compact { 12.0 } else { 15.0 },
            hoi_ui::TEXT,
        );
        if !compact {
            hoi_ui::label(
                tag,
                Rect::new(r.x + 67.0, r.y + 40.0, cw - 74.0, 18.0),
                11.0,
                hoi_ui::MUTED,
            );
        }
        if hoi_ui::inside(r) && is_mouse_button_pressed(MouseButton::Left) {
            *selected_country = Some(tag.clone());
        }
    }
    let py = 106.0 + rows as f32 * (ch + 10.0) + 6.0;
    if button("◀", 16.0, py, 38.0, 28.0) {
        *page = page.saturating_sub(1);
    }
    hoi_ui::label(
        &format!("{} / {pages}", *page + 1),
        Rect::new(65.0, py + 4.0, 80.0, 22.0),
        13.0,
        hoi_ui::TEXT,
    );
    if button("▶", grid_width - 22.0, py, 38.0, 28.0) {
        *page = (*page + 1).min(pages - 1);
    }
    let info = if compact {
        Rect::new(
            16.0,
            py + 42.0,
            width - 32.0,
            (height - py - 126.0).min(245.0),
        )
    } else {
        Rect::new(width - 350.0, 106.0, 325.0, height - 212.0)
    };
    hoi_ui::panel(info.x, info.y, info.w, info.h);
    let tag = selected_country.as_deref().unwrap_or("");
    let economy = world.country_economy(tag).unwrap_or_default();
    hoi_ui::header(
        info.x + 4.0,
        info.y + 4.0,
        info.w - 8.0,
        country_display_name(tag),
    );
    let n = military
        .divisions
        .values()
        .filter(|d| d.country == tag && !d.destroyed)
        .count();
    hoi_ui::flag(tag, Rect::new(info.x + 14.0, info.y + 51.0, 63.0, 40.0));
    hoi_ui::label(
        &format!(
            "Дивизии: {} · Регионы: {}",
            if n == 0
                && military
                    .countries
                    .get(tag)
                    .is_some_and(|a| !a.history_errors.is_empty())
            {
                "ошибка загрузки".into()
            } else {
                n.to_string()
            },
            economy.states
        ),
        Rect::new(info.x + 92.0, info.y + 56.0, info.w - 103.0, 30.0),
        13.0,
        hoi_ui::GOLD,
    );
    if let Some(army) = military
        .countries
        .get(tag)
        .filter(|a| !a.history_errors.is_empty())
    {
        hoi_ui::tooltip(
            Rect::new(info.x + 92.0, info.y + 56.0, info.w - 103.0, 30.0),
            "Ошибка загрузки армии",
            &format!(
                "{}\nПодробности: data/hoi4/military-import-report.json",
                army.history_errors.join("\n")
            ),
        );
    }
    hoi_ui::label(
        &format!(
            "Фабрики {}   Заводы {}   Верфи {}",
            economy.civilian_factories, economy.military_factories, economy.dockyards
        ),
        Rect::new(info.x + 14.0, info.y + 106.0, info.w - 28.0, 24.0),
        13.0,
        hoi_ui::TEXT,
    );
    if info.h > 210.0 {
        hoi_ui::section(
            "ЛИДЕР И ПРАВИТЕЛЬСТВО",
            info.x + 11.0,
            info.y + 148.0,
            info.w - 22.0,
        );
        ui_icons::draw(
            ui_icons::Icon::Officer,
            Rect::new(info.x + 22.0, info.y + 184.0, 58.0, 75.0),
            hoi_ui::MUTED,
        );
        hoi_ui::label(
            "Политическая система",
            Rect::new(info.x + 100.0, info.y + 187.0, info.w - 115.0, 22.0),
            13.0,
            hoi_ui::TEXT,
        );
        hoi_ui::label(
            "ЗАГЛУШКА",
            Rect::new(info.x + 100.0, info.y + 215.0, info.w - 115.0, 20.0),
            11.0,
            hoi_ui::GOLD,
        );
        if info.h > 340.0 {
            hoi_ui::placeholder(
                Rect::new(info.x + 14.0, info.bottom() - 91.0, info.w - 28.0, 31.0),
                "Исторические национальные фокусы",
            );
            hoi_ui::placeholder(
                Rect::new(info.x + 14.0, info.bottom() - 49.0, info.w - 28.0, 31.0),
                "Настройка сложности",
            );
        }
    }
    let bw = if compact { width - 32.0 } else { 300.0 };
    if button("НАЧАТЬ ИГРУ", (width - bw) * 0.5, height - 78.0, bw, 48.0) {
        return Screen::MapViewer;
    }
    Screen::CountrySelect
}

fn map_view_rect(_map: &ProvinceMap, _placing: bool) -> Rect {
    Rect::new(0.0, 0.0, screen_width(), screen_height())
}

fn province_inspector_bounds(
    selected: bool,
    construction: bool,
    production: bool,
    research: bool,
    army: &army_ui::ArmyView,
) -> Option<Rect> {
    if !selected
        || construction
        || production
        || research
        || army.open
        || army.deployment_pick
        || !army.selected.is_empty()
    {
        return None;
    }
    Some(campaign_ui::state_bounds())
}

fn handle_hud_action(
    action: campaign_ui::Action,
    hud: &mut campaign_ui::Hud,
    world: &World,
    tag: Option<&str>,
    military: &mut warfare::Warfare,
    army: &mut army_ui::ArmyView,
    construction: &mut construction_ui::ConstructionView,
    production: &mut production_ui::ProductionView,
    research: &mut research_ui::ResearchView,
    clock: &mut GameState,
    map: &mut Option<&mut ProvinceMap>,
) {
    use campaign_ui::{Action, Panel};
    match action {
        Action::Open(panel) => {
            hud.toggle(panel, construction, production, research, army);
            if panel == Panel::Construction {
                construction.map_placement = true;
            }
        }
        Action::Select(id) => {
            hud.open(None, construction, production, research, army);
            army.selected.clear();
            army.selected.insert(id);
            army.sync_selection(military);
            if let Some(d) = military.divisions.get(&id) {
                if let Some((map, p)) = map.as_deref_mut().zip(
                    world
                        .geography
                        .provinces
                        .get(&d.province)
                        .and_then(|p| p.center),
                ) {
                    map.center_on(vec2(p.x as f32, p.y as f32));
                }
            }
        }
        Action::SelectAll => {
            hud.open(None, construction, production, research, army);
            army.selected = military
                .divisions
                .values()
                .filter(|d| Some(d.country.as_str()) == tag && !d.destroyed)
                .map(|d| d.id)
                .collect();
            army.sync_selection(military);
        }
        Action::SelectArmy(id, add) => {
            let selected = if add {
                army.selected.clone()
            } else {
                Default::default()
            };
            hud.open(None, construction, production, research, army);
            army.show_army(id, military);
            army.selected.extend(selected);
        }
        Action::AssignArmy(id) => {
            if let Some(tag) = tag {
                let ids = army.selected.iter().copied().collect::<Vec<_>>();
                match military.assign_army(tag, id, &ids) {
                    Ok(()) => {
                        army.sync_selection(military);
                        army.notice = "Дивизии переведены в армию".into();
                    }
                    Err(e) => army.notice = e,
                }
            }
        }
        Action::ToggleArmy(id) => {
            if let Some(tag) = tag {
                let active = military
                    .plans
                    .plans
                    .values()
                    .any(|p| p.army == id && p.activation == operations::Activation::Executing);
                army.notice = military.set_army_execution(tag, id, !active).map_or_else(
                    |e| e,
                    |_| {
                        if active {
                            "Выполнение планов армии остановлено".into()
                        } else {
                            "Планы армии включены · снимите паузу для исполнения".into()
                        }
                    },
                );
            }
        }

        Action::CreateArmy => {
            if let Some(tag) = tag {
                let ids: Vec<_> = army.selected.iter().copied().collect();
                if ids.is_empty() {
                    army.notice = "Сначала выделите дивизии на карте".into();
                } else {
                    match military.create_army(tag, &ids, "") {
                        Ok(id) => {
                            hud.open(None, construction, production, research, army);
                            army.show_army(id, military);
                            army.notice="Армия создана. Портрет — выбор генерала; «Фронт» — назначение границы".into();
                        }
                        Err(e) => army.notice = e,
                    }
                }
            }
        }
        Action::Commander => {
            if let Some(tag) = tag {
                match army.ensure_army(tag, military) {
                    Ok(_) => {
                        hud.open(
                            Some(Panel::Officer),
                            construction,
                            production,
                            research,
                            army,
                        );
                        army.show_command(true);
                    }
                    Err(e) => army.notice = e,
                }
            }
        }
        Action::Designer(index) => {
            hud.open(
                Some(Panel::Recruitment),
                construction,
                production,
                research,
                army,
            );
            army.show_template(index);
        }
        Action::Stop => {
            if let Some(tag) = tag {
                army.notice =
                    army_controls::stop_selection(military, world, tag, &army.selected).message();
            }
        }
        Action::Redeploy => army.strategic = !army.strategic,
        Action::MapMode(index) => {
            if let Some(map) = map.as_deref_mut() {
                map.map_mode = match index {
                    1 => MapMode::Resources,
                    2 => MapMode::Terrain,
                    _ => MapMode::Political,
                };
            }
        }
        Action::Zoom(factor) => {
            if let Some(map) = map.as_deref_mut() {
                let view = map_view_rect(map, false);
                map.set_zoom_at(factor, vec2(view.w * 0.5, view.h * 0.5), view);
            }
        }
        Action::Fit => {
            if let Some(map) = map.as_deref_mut() {
                map.zoom = 1.0;
                map.target_zoom = 1.0;
                map.pan = Vec2::ZERO;
                map.zoom_anchor = None;
            }
        }
        Action::Menu => {
            hud.menu_was_paused = clock.paused;
            clock.paused = true;
            hud.menu = true;
        }
        Action::Resume => {
            hud.menu = false;
            clock.paused = hud.menu_was_paused;
        }
        Action::Reload => {
            hud.reload_requested = true;
            hud.menu = false;
            clock.paused = hud.menu_was_paused;
        }
        Action::DeclareWar(target) => {
            if let Some(tag) = tag {
                hud.notice = military
                    .declare_war(tag, &target, world)
                    .map_or_else(|e| e, |_| "Война объявлена".into());
            }
        }
        Action::NavalOrder(id, mission, region) => {
            if let Some(tag) = tag {
                let mut navy = std::mem::take(&mut military.naval);
                hud.notice = navy
                    .assign_mission(tag, id, mission, region, world, military)
                    .map_or_else(|e| e, |_| format!("Приказ флоту: {}", mission.label()));
                military.naval = navy;
            }
        }
        Action::NavalQueueShip(model, base) => {
            if let Some(tag) = tag {
                let mut navy = std::mem::take(&mut military.naval);
                hud.notice = navy
                    .queue_ship(tag, &model, base, 5, world, military)
                    .map_or_else(|e| e, |_| format!("Корабль {model} поставлен на верфи"));
                military.naval = navy;
            }
        }
        Action::NavalCancelShip(id) => {
            if let Some(tag) = tag {
                hud.notice = if military.naval.cancel_ship(tag, id) {
                    "Строительство корабля отменено".into()
                } else {
                    "Производственная линия не найдена".into()
                };
            }
        }
        Action::Save => hud.save_requested = true,
        Action::Load => hud.load_requested = true,
        Action::MainMenu => {}
    }
}

fn draw_map_viewer(
    mut map: Option<&mut ProvinceMap>,
    world: &World,
    load_error: &str,
    selected: &mut Option<(u32, [u8; 3], u32, u32)>,
    player_country: Option<&str>,
    construction_queue: &mut ConstructionQueues,
    construction_priority: &mut Vec<u32>,
    construction_order: &mut construction::ConstructionOrder,
    construction_notice: &mut String,
    construction_policy: &mut construction::CountryConstructionPolicy,
    law_preset: &mut construction::EconomicLawPreset,
    building_rules: &BuildingRules,
    construction_view: &mut construction_ui::ConstructionView,
    production_view: &mut production_ui::ProductionView,
    production_state: &mut production::ProductionState,
    resource_state: &mut resource_economy::ResourceEconomy,
    production_rules: &production::ProductionRules,
    research_world: &mut research::ResearchWorld,
    research_catalogue: &research::ResearchCatalogue,
    research_rules: &research::ResearchRules,
    research_view: &mut research_ui::ResearchView,
    game: &mut GameState,
    hud: &mut campaign_ui::Hud,
    military: &mut warfare::Warfare,
    army_view: &mut army_ui::ArmyView,
) -> Screen {
    army_view.orders_enabled = matches!(
        hud.active,
        None | Some(campaign_ui::Panel::Army | campaign_ui::Panel::Officer)
    );
    let effective_building_rules = building_rules.for_research(
        player_country.and_then(|t| research_world.country(t)),
        research_catalogue,
    );
    let building_rules = &effective_building_rules;
    let effective_production_rules =
        production_rules.for_research(player_country.and_then(|t| research_world.country(t)));
    let production_rules = &effective_production_rules;
    let research_construction_modifiers = construction::ConstructionModifiers::from_research(
        player_country.and_then(|t| research_world.country(t)),
    );
    let compact = screen_width() < 760.0;
    let game_date = game.date;
    clear_background(Color::from_rgba(24, 36, 43, 255));
    if let Some(map) = map.as_deref_mut() {
        let mouse = vec2(mouse_position().0, mouse_position().1);
        let rect = map_view_rect(map, construction_view.map_placement);
        if mouse_wheel().1 != 0.0
            && !hud.blocked(mouse)
            && !construction_ui::is_over_panel(mouse, construction_view)
            && !production_ui::is_over_panel(mouse, production_view)
            && !research_ui::is_over_panel(mouse, research_view)
            && !army_ui::is_over_panel(mouse, army_view)
            && !province_inspector_bounds(
                selected.is_some(),
                construction_view.open,
                production_view.open,
                research_view.open,
                army_view,
            )
            .is_some_and(|r| r.contains(mouse))
        {
            map.set_zoom_at(
                (mouse_wheel().1.clamp(-3.0, 3.0) * 0.035).exp(),
                mouse,
                rect,
            );
        }
        let rect = map_view_rect(
            map,
            construction_view.open && construction_view.map_placement,
        );
        draw_rectangle(
            rect.x - 1.0,
            rect.y - 1.0,
            rect.w + 2.0,
            rect.h + 2.0,
            Color::from_rgba(62, 78, 90, 255),
        );
        let source_rect = map.source_rect();
        let placing = construction_view.open && construction_view.map_placement;
        if placing {
            if let Some(tag) = player_country {
                let signature = construction_map::signature(
                    world,
                    tag,
                    construction_view.selected_building,
                    construction_queue,
                    building_rules,
                );
                if map.construction_overlay_signature != Some(signature) {
                    let colors = construction_map::state_colors(
                        world,
                        tag,
                        construction_view.selected_building,
                        construction_queue,
                        building_rules,
                    );
                    let rgba = construction_map::rgba_pixels(&map.state_ids_by_pixel, &colors);
                    let texture = Texture2D::from_rgba8(map.width as u16, map.height as u16, &rgba);
                    texture.set_filter(FilterMode::Nearest);
                    map.construction_overlay_texture = Some(texture);
                    map.construction_overlay_signature = Some(signature);
                }
            }
        } else {
            // The original map texture is never altered: leaving placement
            // restores political/resource/terrain colors on this very frame.
            map.construction_overlay_texture = None;
            map.construction_overlay_signature = None;
        }
        // GPU memory budget: the political map and borders are persistent,
        // but terrain/resources share a single optional texture. Construction
        // placement releases it while displaying the political base layer.
        if placing || map.map_mode == MapMode::Political {
            map.active_mode_texture = None;
        } else if map.active_mode_texture.as_ref().map(|(mode, _)| *mode) != Some(map.map_mode) {
            map.active_mode_texture = Some((
                map.map_mode,
                map.color_palette.texture_from_rgb(
                    map.width,
                    map.height,
                    &map.rgb_pixels,
                    3,
                    map.map_mode,
                ),
            ));
        }
        map.sync_control(world);
        let mode_texture = map
            .active_mode_texture
            .as_ref()
            .map(|(_, texture)| texture)
            .unwrap_or(&map.political_texture);
        draw_texture_ex(
            mode_texture,
            rect.x,
            rect.y,
            WHITE,
            DrawTextureParams {
                source: Some(source_rect),
                dest_size: Some(vec2(rect.w, rect.h)),
                ..Default::default()
            },
        );
        if placing {
            if let Some(overlay) = &map.construction_overlay_texture {
                draw_texture_ex(
                    overlay,
                    rect.x,
                    rect.y,
                    WHITE,
                    DrawTextureParams {
                        source: Some(source_rect),
                        dest_size: Some(vec2(rect.w, rect.h)),
                        ..Default::default()
                    },
                );
            }
        }
        // Keep state/country boundaries crisp above the color heatmap.
        draw_texture_ex(
            &map.map_border_texture,
            rect.x,
            rect.y,
            WHITE,
            DrawTextureParams {
                source: Some(source_rect),
                dest_size: Some(vec2(rect.w, rect.h)),
                ..Default::default()
            },
        );
        battle_plan_ui::draw_map(
            world,
            military,
            army_view,
            player_country,
            source_rect,
            rect,
            game.paused,
        );
        army_ui::draw_counters(
            world,
            military,
            player_country,
            army_view,
            source_rect,
            rect,
            hud.blocked(mouse)
                || army_ui::is_over_panel(mouse, army_view)
                || province_inspector_bounds(
                    selected.is_some(),
                    construction_view.open,
                    production_view.open,
                    research_view.open,
                    army_view,
                )
                .is_some_and(|r| r.contains(mouse)),
        );
        // Show city markers from zoom 8.5; city names appear only at zoom 20.
        if map.zoom >= 8.5 {
            let label_scale = (map.zoom.sqrt() * 0.72).clamp(0.65, 1.25);
            // Text rendering is much more expensive than map sprites, especially on mobile GPUs.
            // Keep markers visible, but cap actual city-name labels per frame.
            const MAX_CITY_LABELS_PER_FRAME: usize = 72;
            let mut city_labels_drawn = 0usize;
            for marker_index in map.city_grid.visible(source_rect) {
                let city = &map.city_markers[marker_index];
                if city.x < source_rect.x
                    || city.x > source_rect.x + source_rect.w
                    || city.y < source_rect.y
                    || city.y > source_rect.y + source_rect.h
                {
                    continue;
                }
                let x = rect.x + (city.x - source_rect.x) / source_rect.w * rect.w;
                let y = rect.y + (city.y - source_rect.y) / source_rect.h * rect.h;
                draw_circle(
                    x,
                    y,
                    3.0 * label_scale,
                    Color::from_rgba(245, 224, 168, 255),
                );
                draw_circle_lines(
                    x,
                    y,
                    4.3 * label_scale,
                    1.0,
                    Color::from_rgba(25, 31, 38, 255),
                );
                if map.zoom >= 20.0
                    && city.display_name
                    && city_labels_drawn < MAX_CITY_LABELS_PER_FRAME
                {
                    let name = city.name.trim();
                    let size = (10.0 * label_scale).clamp(9.0, 13.0);
                    let width_scale = size / 13.0;
                    let text_width = city.label_width * width_scale;
                    draw_rectangle(
                        x + 6.0,
                        y - size - 3.0,
                        text_width + 7.0,
                        size + 5.0,
                        Color::from_rgba(15, 22, 29, 220),
                    );
                    draw_ui_text(name, x + 9.0, y - 2.0, size, WHITE);
                    city_labels_drawn += 1;
                }
            }
        }
        if map.map_mode == MapMode::Resources {
            // HOI4-like resource badges are drawn in map space, so they follow pan and zoom.
            let scale = (map.zoom.sqrt() * 0.85).clamp(0.75, 1.35);
            for marker_index in map.resource_grid.visible(source_rect) {
                let marker = &map.resource_markers[marker_index];
                if marker.x < source_rect.x
                    || marker.x > source_rect.x + source_rect.w
                    || marker.y < source_rect.y
                    || marker.y > source_rect.y + source_rect.h
                {
                    continue;
                }
                let x = rect.x + (marker.x - source_rect.x) / source_rect.w * rect.w;
                let y = rect.y + (marker.y - source_rect.y) / source_rect.h * rect.h;
                let radius = 10.0 * scale;
                draw_circle(x, y, radius, marker.color);
                draw_circle_lines(x, y, radius, 1.2, Color::from_rgba(225, 225, 215, 255));
                let size = (10.0 * scale).clamp(8.0, 13.0);
                let tw = measure_text(marker.label, None, size as u16, 1.0).width;
                draw_ui_text(marker.label, x - tw * 0.5, y + size * 0.35, size, WHITE);
            }
        }
        // Army selection must not leave a bright administrative outline
        // over the moving control border. Construction still needs it.
        if army_view.selected.is_empty() || placing {
            if let Some((_, bounds, highlight_texture)) = &map.highlighted_state_texture {
                if let Some((src, dest)) = map_pixels::visible_outline(*bounds, source_rect, rect) {
                    draw_texture_ex(
                        highlight_texture,
                        dest.x,
                        dest.y,
                        WHITE,
                        DrawTextureParams {
                            source: Some(src),
                            dest_size: Some(vec2(dest.w, dest.h)),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        if placing {
            if let Some(tag) = player_country {
                let (mouse_x, mouse_y) = mouse_position();
                let mouse = vec2(mouse_x, mouse_y);
                if rect.contains(mouse)
                    && !hud.blocked(mouse)
                    && !construction_ui::is_over_panel(mouse, construction_view)
                {
                    let px = (source_rect.x + ((mouse_x - rect.x) / rect.w) * source_rect.w)
                        .floor()
                        .clamp(0.0, map.width.saturating_sub(1) as f32)
                        as u32;
                    let py = (source_rect.y + ((mouse_y - rect.y) / rect.h) * source_rect.h)
                        .floor()
                        .clamp(0.0, map.height.saturating_sub(1) as f32)
                        as u32;
                    if let Some((province, _)) = map.province_at(px, py) {
                        if let Some(&state_id) = map.state_by_province.get(&province) {
                            if let (Some(state), Some(capacity)) = (
                                world.states.get(&state_id),
                                construction_map::availability(
                                    world,
                                    tag,
                                    state_id,
                                    construction_view.selected_building,
                                    construction_queue,
                                    building_rules,
                                ),
                            ) {
                                let tooltip_w = 228.0;
                                let tooltip_h = 75.0;
                                let tx = (mouse_x + 13.0)
                                    .min(screen_width() - tooltip_w - 6.0)
                                    .max(5.0);
                                let ty = (mouse_y + 15.0)
                                    .min(screen_height() - tooltip_h - 8.0)
                                    .max(5.0);
                                hoi_ui::panel(tx, ty, tooltip_w, tooltip_h);
                                let name = state.name.as_deref().unwrap_or("Регион без названия");
                                draw_ui_text(
                                    &name.chars().take(27).collect::<String>(),
                                    tx + 8.0,
                                    ty + 19.0,
                                    14.0,
                                    hoi_ui::GOLD,
                                );
                                let building = construction_view.selected_building;
                                let built = building.built_count(state.economy);
                                let pending = construction_queue.get(&state_id).map_or(0, |q| {
                                    q.iter()
                                        .filter(|project| project.building == building)
                                        .count()
                                });
                                let details = if capacity.eligible {
                                    format!(
                                        "Свободно {}/{} · Построено {} · В очереди {}",
                                        capacity.free, capacity.capacity, built, pending
                                    )
                                } else {
                                    "Регион принадлежит другой стране".to_owned()
                                };
                                draw_ui_text(&details, tx + 8.0, ty + 40.0, 11.0, hoi_ui::TEXT);
                                draw_ui_text(
                                    if capacity.eligible && capacity.free > 0 {
                                        "Нажмите для добавления постройки"
                                    } else {
                                        "Строительство здесь недоступно"
                                    },
                                    tx + 8.0,
                                    ty + 60.0,
                                    11.0,
                                    hoi_ui::MUTED,
                                );
                            }
                        }
                    }
                }
            }
        }

        if let Some(drag) = army_view.map_drag.filter(|d| d.box_select && d.moved()) {
            let end = vec2(
                mouse.x.clamp(rect.x, rect.right()),
                mouse.y.clamp(rect.y, rect.bottom()),
            );
            let box_rect = army_controls::selection_rect(drag.start, end);
            draw_rectangle(
                box_rect.x,
                box_rect.y,
                box_rect.w,
                box_rect.h,
                Color::new(0.8, 0.7, 0.4, 0.1),
            );
            draw_rectangle_lines(
                box_rect.x,
                box_rect.y,
                box_rect.w,
                box_rect.h,
                1.0,
                hoi_ui::GOLD,
            );
        }
    } else {
        hoi_ui::label(
            "Карта не загружена",
            Rect::new(30.0, 160.0, 500.0, 36.0),
            24.0,
            hoi_ui::GOLD,
        );
        hoi_ui::label(
            load_error,
            Rect::new(30.0, 205.0, screen_width() - 60.0, 28.0),
            14.0,
            hoi_ui::TEXT,
        );
    }
    let mode = map.as_deref().map_or(0, |m| match m.map_mode {
        MapMode::Political => 0,
        MapMode::Resources => 1,
        MapMode::Terrain => 2,
    });
    hoi_ui::block_input(hud.menu || hud.detail.is_some() || hud.map_palette);
    let actions = campaign_ui::draw_chrome(
        hud,
        world,
        player_country,
        military,
        production_state,
        research_world,
        army_view,
        mode,
    );
    for action in actions {
        handle_hud_action(
            action,
            hud,
            world,
            player_country,
            military,
            army_view,
            construction_view,
            production_view,
            research_view,
            game,
            &mut map,
        );
    }
    army_view.orders_enabled = matches!(
        hud.active,
        None | Some(campaign_ui::Panel::Army | campaign_ui::Panel::Officer)
    );
    if hud.active.is_none() && army_view.selected.is_empty() && !army_view.deployment_pick {
        if let Some((province, _, _, _)) = *selected {
            let close = campaign_ui::state_bounds();
            let actions = campaign_ui::draw_state(hud, world, military, province);
            if hoi_ui::small_button(
                "×",
                Rect::new(close.right() - 34.0, close.y + 7.0, 26.0, 24.0),
                false,
            ) {
                *selected = None;
                if let Some(m) = map.as_deref_mut() {
                    m.highlighted_state_texture = None;
                }
            }
            for action in actions {
                handle_hud_action(
                    action,
                    hud,
                    world,
                    player_country,
                    military,
                    army_view,
                    construction_view,
                    production_view,
                    research_view,
                    game,
                    &mut map,
                );
            }
        }
    }
    let actions = campaign_ui::draw_panel(
        hud,
        world,
        player_country,
        military,
        production_state,
        resource_state,
        selected.map(|p| p.0),
    );
    for action in actions {
        handle_hud_action(
            action,
            hud,
            world,
            player_country,
            military,
            army_view,
            construction_view,
            production_view,
            research_view,
            game,
            &mut map,
        );
    }
    if construction_view.open {
        construction_ui::draw(
            Some(world),
            player_country,
            construction_queue,
            construction_priority,
            construction_order,
            building_rules,
            construction_policy,
            law_preset,
            construction_notice,
            construction_view,
            game_date,
            &research_construction_modifiers,
        );
    }
    if production_view.open {
        production_ui::draw(
            Some(world),
            player_country,
            production_state,
            production_view,
            production_rules,
            military,
        );
    }
    if research_view.open {
        research_ui::draw(
            player_country,
            research_world,
            research_catalogue,
            research_rules,
            game_date,
            research_view,
        );
    }
    army_ui::draw(
        world,
        player_country,
        military,
        production_state,
        production_rules,
        selected.map(|p| p.0),
        army_view,
    );
    if army_view.open
        && matches!(
            hud.active,
            Some(
                campaign_ui::Panel::Army
                    | campaign_ui::Panel::Recruitment
                    | campaign_ui::Panel::Officer
                    | campaign_ui::Panel::Air
            )
        )
    {
        hud.active = Some(army_view.navigation_panel());
    }
    if hud.active.is_none() {
        army_ui::draw_selection(world, player_country, military, army_view);
        if army_view.open {
            hud.active = Some(army_view.navigation_panel());
        }
    }
    hud.orders_layout = army_view.orders_visible();
    if army_view.orders_enabled {
        if let Some(tag) = player_country {
            battle_plan_ui::draw_toolbar(world, tag, military, army_view, game.paused);
        }
    }
    hud.reconcile(construction_view, production_view, research_view, army_view);
    if hud.minimap
        && !compact
        && !research_view.browsing
        && hud.active != Some(campaign_ui::Panel::Focus)
    {
        if let Some(m) = map.as_deref_mut() {
            let r = hud.minimap_bounds();
            hoi_ui::panel(r.x, r.y, r.w, r.h);
            let inner = Rect::new(r.x + 4.0, r.y + 4.0, r.w - 8.0, r.h - 8.0);
            draw_texture_ex(
                &m.political_texture,
                inner.x,
                inner.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(inner.w, inner.h)),
                    ..Default::default()
                },
            );
            let source = m.source_rect();
            draw_rectangle_lines(
                inner.x + source.x / m.width as f32 * inner.w,
                inner.y + source.y / m.height as f32 * inner.h,
                source.w / m.width as f32 * inner.w,
                source.h / m.height as f32 * inner.h,
                1.0,
                hoi_ui::TEXT,
            );
            hoi_ui::tooltip(
                inner,
                "Обзорная карта",
                "Нажмите, чтобы переместить камеру.",
            );
            if hoi_ui::inside(inner)
                && is_mouse_button_down(MouseButton::Left)
                && !hud.menu
                && hud.detail.is_none()
            {
                let mouse = vec2(mouse_position().0, mouse_position().1);
                let point = vec2(
                    (mouse.x - inner.x) / inner.w * m.width as f32,
                    (mouse.y - inner.y) / inner.h * m.height as f32,
                );
                m.pan = map_camera::anchor_for_view(
                    m.width as f32,
                    m.height as f32,
                    m.zoom,
                    point,
                    vec2(screen_width() * 0.5, screen_height() * 0.5),
                    map_view_rect(m, false),
                );
                m.zoom_anchor = None;
            }
        }
    }
    let (pause, speed) = draw_time_controls(game, &game.date.display());
    if pause && !hud.menu && hud.detail.is_none() {
        game.toggle_pause();
    }
    if let Some(speed) = speed {
        if !hud.menu && hud.detail.is_none() {
            game.set_speed(speed);
            game.paused = false;
        }
    }
    hoi_ui::block_input(false);
    let actions = campaign_ui::draw_map_palette(hud, mode);
    for action in actions {
        handle_hud_action(
            action,
            hud,
            world,
            player_country,
            military,
            army_view,
            construction_view,
            production_view,
            research_view,
            game,
            &mut map,
        );
    }
    if let Some(action) = campaign_ui::draw_modal(hud) {
        if matches!(action, campaign_ui::Action::MainMenu) {
            hud.menu = false;
            return Screen::MainMenu;
        }
        handle_hud_action(
            action,
            hud,
            world,
            player_country,
            military,
            army_view,
            construction_view,
            production_view,
            research_view,
            game,
            &mut map,
        );
    }
    Screen::MapViewer
}

fn draw_settings() -> Screen {
    draw_background();
    let width = screen_width();
    let x = 20.0;
    let w = (width - 40.0).min(660.0);
    hoi_ui::panel(x, 28.0, w, (screen_height() - 106.0).max(360.0));
    hoi_ui::header(x + 4.0, 32.0, w - 8.0, "НАСТРОЙКИ");
    hoi_ui::section("ОТОБРАЖЕНИЕ", x + 12.0, 83.0, w - 24.0);
    hoi_ui::label(
        "Интерфейс автоматически подстраивается под размер окна.",
        Rect::new(x + 18.0, 119.0, w - 36.0, 24.0),
        13.0,
        hoi_ui::TEXT,
    );
    hoi_ui::placeholder(Rect::new(x + 15.0, 157.0, w - 30.0, 32.0), "Звук и музыка");
    hoi_ui::placeholder(
        Rect::new(x + 15.0, 198.0, w - 30.0, 32.0),
        "Переназначение клавиш",
    );
    hoi_ui::section("ЛОКАЛЬНЫЕ ДАННЫЕ", x + 12.0, 257.0, w - 24.0);
    for (i, text) in [
        "Данные игры: data/hoi4 рядом с Cargo.toml",
        "Карта: map/provinces.bmp и map/definition.csv",
        "Состав стран: common и history",
        "Флаги: gfx/flags/*.tga (необязательно)",
        "Автоматический выбор папки пока не реализован.",
    ]
    .iter()
    .enumerate()
    {
        hoi_ui::label(
            text,
            Rect::new(x + 17.0, 296.0 + i as f32 * 30.0, w - 34.0, 25.0),
            13.0,
            hoi_ui::TEXT,
        );
    }
    if button("Назад", 20.0, screen_height() - 61.0, 180.0, 38.0) || is_key_pressed(KeyCode::Escape)
    {
        Screen::MainMenu
    } else {
        Screen::Settings
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    init_ui_font();
    draw_loading("Загрузка определений и технологий…", 0.05);
    next_frame().await;
    let mut screen = Screen::MainMenu;
    let mut selected: Option<(u32, [u8; 3], u32, u32)> = None;
    let mut country_choice: Option<String> = None;
    let mut country_page = 0usize;
    let mut construction_notice = String::new();
    let building_rules = BuildingRules::load(construction::BUILDING_DEFINITIONS_DIRECTORY);
    let mut construction_view = construction_ui::ConstructionView::default();
    let mut production_view = production_ui::ProductionView::default();
    let mut army_view = army_ui::ArmyView::default();
    let mut hud = campaign_ui::Hud::default();
    let mut production_rules;
    // Use the player's own versioned game files if present. Never redistribute
    // proprietary technology files or pretend demo nodes are the vanilla tree.
    let research_dataset =
        match hoi4_mobile::research_import::ResearchDataset::load(Path::new("data/hoi4"), true) {
            Ok(data) => {
                for diagnostic in &data.diagnostics {
                    eprintln!("[research-import] {diagnostic}");
                }
                Some(data)
            }
            Err(error) => {
                eprintln!("[research-import] {error}");
                None
            }
        };
    let empty_research_catalogue = research::ResearchCatalogue::default();
    let research_catalogue = research_dataset
        .as_ref()
        .map(|d| &d.registry)
        .unwrap_or(&empty_research_catalogue);
    let research_rules = research_dataset
        .as_ref()
        .map(|d| d.rules)
        .unwrap_or_default();
    let research_origin = research_dataset.as_ref().map_or_else(
        || "Нет локальных технологий. Запустите tools/Export-Hoi4Research.ps1".into(),
        |d| {
            format!(
                "Локальные определения: {} технологий, {} историй стран{}",
                d.registry.technologies.len(),
                d.histories.len(),
                if d.from_cache { " · кэш" } else { "" }
            )
        },
    );
    let mut research_view = research_ui::ResearchView::default();
    research_view.notice = research_origin.clone();
    let initial_start = research_dataset
        .as_ref()
        .map(|d| d.start_date())
        .transpose()
        .map(|d| d.unwrap_or(game::GameDate::START));
    let mut scenario_choice = initial_start
        .as_ref()
        .copied()
        .unwrap_or(game::GameDate::START);
    let start_session = |start_date: game::GameDate| -> Result<GameSession, String> {
        let names = load_state_names(Path::new(STATE_DIRECTORY_PATH));
        let loaded = scenario::load_at(Path::new("data/hoi4"), &names, start_date)?;
        for diagnostic in &loaded.diagnostics {
            eprintln!("[scenario] {diagnostic}");
        }
        let mut session = GameSession::new(loaded.world);
        session.clock.date = start_date;
        if let Some(data) = &research_dataset {
            session.initialize_research_dataset(data, Path::new("data/hoi4"));
        } else {
            for diagnostic in session.initialize_research(
                Path::new("data/hoi4/history/countries"),
                &research_catalogue,
                &research_rules,
            ) {
                eprintln!("[research] {diagnostic}");
            }
        }
        session.initialize_military(Path::new("data/hoi4"));
        session
            .military
            .use_research_registry(&research_catalogue, &session.research);
        let mut report = military_history::import_report(&session.military, start_date);
        report["production"] = serde_json::json!(
            session
                .production
                .by_country
                .iter()
                .map(|(tag, c)| (
                    tag,
                    serde_json::json!({
                        "starting_data_applied": c.starting_data_applied,
                        "lines":c.lines,
                        "model_stockpile":c.model_stockpile,
                    })
                ))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
        match serde_json::to_vec_pretty(&report)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                fs::write("data/hoi4/military-import-report.json", bytes).map_err(|e| e.to_string())
            }) {
            Ok(()) => {}
            Err(error) => eprintln!("[military-report] {error}"),
        }
        for diagnostic in &session.military.diagnostics {
            eprintln!("[military] {diagnostic}");
        }
        Ok(session)
    };
    draw_loading("Импорт стран, карты и стартовых армий…", 0.4);
    next_frame().await;
    let (mut session, mut load_error) = match initial_start.and_then(|date| start_session(date)) {
        Ok(session) => (session, String::new()),
        Err(error) => (GameSession::new(World::default()), error),
    };
    production_rules =
        production::ProductionRules::from_defines(&session.military.catalogue.literal_defines);
    draw_loading("Подготовка карты и интерфейса…", 0.85);
    next_frame().await;
    let mut map = if session.world.geography.width > 0 {
        match ProvinceMap::load(&session.world) {
            Ok(map) => Some(map),
            Err(error) => {
                load_error = error;
                None
            }
        }
    } else {
        None
    };

    loop {
        hoi_ui::begin_frame();
        hoi_ui::block_input(false);
        let next_screen = match screen {
            Screen::MainMenu => draw_main_menu(
                map.as_ref(),
                session.player_country.is_some(),
                session.clock.date,
                &mut hud.load_requested,
            ),
            Screen::CountrySelect => {
                let old_scenario = scenario_choice;
                let mut retry_requested = false;
                let next = draw_country_select(
                    &session.world,
                    &session.military,
                    map.as_ref(),
                    &mut country_choice,
                    &mut country_page,
                    &mut scenario_choice,
                    research_dataset
                        .as_ref()
                        .map_or(&[], |d| d.bookmarks.as_slice()),
                    &load_error,
                    &mut retry_requested,
                );
                if old_scenario != scenario_choice || retry_requested {
                    match start_session(scenario_choice) {
                        Ok(fresh) => {
                            session = fresh;
                            production_rules = production::ProductionRules::from_defines(
                                &session.military.catalogue.literal_defines,
                            );
                            drop(map.take());
                            match ProvinceMap::load(&session.world) {
                                Ok(m) => {
                                    map = Some(m);
                                    load_error.clear();
                                }
                                Err(e) => load_error = e,
                            }
                            country_page = 0;
                        }
                        Err(e) => load_error = e,
                    }
                }
                if next == Screen::MapViewer {
                    match start_session(scenario_choice) {
                        Ok(mut fresh) => {
                            fresh.player_country = country_choice.clone();
                            session = fresh;
                            production_rules = production::ProductionRules::from_defines(
                                &session.military.catalogue.literal_defines,
                            );
                            drop(map.take());
                            match ProvinceMap::load(&session.world) {
                                Ok(mut loaded) => {
                                    if let Some(tag) = country_choice.as_deref() {
                                        loaded.focus_country(&session.world, tag);
                                    }
                                    map = Some(loaded);
                                    load_error.clear();
                                }
                                Err(error) => load_error = error,
                            }
                            selected = None;
                            construction_notice.clear();
                            construction_view = construction_ui::ConstructionView::default();
                            production_view = production_ui::ProductionView::default();
                            army_view = army_ui::ArmyView::default();
                            hud = campaign_ui::Hud::default();
                            research_view = research_ui::ResearchView {
                                notice: research_origin.clone(),
                                ..Default::default()
                            };
                            Screen::MapViewer
                        }
                        Err(error) => {
                            load_error = error;
                            Screen::CountrySelect
                        }
                    }
                } else {
                    next
                }
            }
            Screen::MapViewer => {
                use campaign_ui::Panel;
                if hud.save_requested {
                    hud.save_requested = false;
                    hud.notice = platform_storage::game_file("campaign-save.json")
                        .and_then(|path| session.save_game(&path, &research_catalogue))
                        .map_or_else(
                            |e| format!("Сохранение: {e}"),
                            |_| "Кампания сохранена".into(),
                        );
                }
                if hud.load_requested {
                    hud.load_requested = false;
                    match platform_storage::game_file("campaign-save.json")
                        .and_then(|path| session.restore_game(&path, &research_catalogue))
                    {
                        Ok(()) => {
                            army_view = Default::default();
                            selected = None;
                            hud.menu = false;
                            hud.notice = "Кампания загружена на паузе".into();
                            if let Some(m) = map.as_mut() {
                                match ProvinceMap::load(&session.world) {
                                    Ok(fresh) => *m = fresh,
                                    Err(e) => load_error = e,
                                }
                            }
                        }
                        Err(e) => hud.notice = format!("Загрузка: {e}"),
                    }
                }
                if is_key_pressed(KeyCode::Escape) {
                    if hud.detail.take().is_some() {
                    } else if hud.map_palette {
                        hud.map_palette = false;
                    } else if hud.menu {
                        hud.menu = false;
                        session.clock.paused = hud.menu_was_paused;
                    } else if research_view.open && research_view.browsing {
                        research_view.browsing = false;
                    } else if army_view.plan_tool.is_some() {
                        army_view.cancel_tool();
                    } else if army_view.deployment_pick {
                        army_view.deployment_pick = false;
                        hud.open(
                            Some(Panel::Recruitment),
                            &mut construction_view,
                            &mut production_view,
                            &mut research_view,
                            &mut army_view,
                        );
                    } else if hud.active.is_some() {
                        hud.open(
                            None,
                            &mut construction_view,
                            &mut production_view,
                            &mut research_view,
                            &mut army_view,
                        );
                    } else if !army_view.selected.is_empty() || selected.is_some() {
                        army_view.selected.clear();
                        selected = None;
                        if let Some(m) = map.as_mut() {
                            m.highlighted_state_texture = None;
                        }
                    } else {
                        hud.menu_was_paused = session.clock.paused;
                        session.clock.paused = true;
                        hud.menu = true;
                    }
                }
                if !hud.menu
                    && hud.detail.is_none()
                    && !hud.map_palette
                    && !army_view.text_input_active()
                {
                    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                    for panel in
                        Panel::NAV
                            .into_iter()
                            .chain([Panel::Army, Panel::Navy, Panel::Air])
                    {
                        if panel.binding().is_some_and(|(key, need_shift)| {
                            is_key_pressed(key) && shift == need_shift
                        }) {
                            hud.toggle(
                                panel,
                                &mut construction_view,
                                &mut production_view,
                                &mut research_view,
                                &mut army_view,
                            );
                            if panel == Panel::Construction {
                                construction_view.map_placement = construction_view.open;
                            }
                        }
                    }
                    if is_key_pressed(KeyCode::A) && !shift {
                        hud.toggle(
                            Panel::Army,
                            &mut construction_view,
                            &mut production_view,
                            &mut research_view,
                            &mut army_view,
                        );
                    }
                    if is_key_pressed(KeyCode::F2) {
                        hud.toggle(
                            Panel::Navy,
                            &mut construction_view,
                            &mut production_view,
                            &mut research_view,
                            &mut army_view,
                        );
                    }
                    if is_key_pressed(KeyCode::F3) {
                        hud.toggle(
                            Panel::Air,
                            &mut construction_view,
                            &mut production_view,
                            &mut research_view,
                            &mut army_view,
                        );
                    }
                    if is_key_pressed(KeyCode::KpAdd) {
                        session
                            .clock
                            .set_speed(session.clock.speed.saturating_add(1));
                    }
                    if is_key_pressed(KeyCode::KpSubtract) {
                        session
                            .clock
                            .set_speed(session.clock.speed.saturating_sub(1));
                    }
                    if is_key_pressed(KeyCode::Space) {
                        session.clock.toggle_pause();
                    }
                    for (key, speed) in [
                        (KeyCode::Key1, 1),
                        (KeyCode::Key2, 2),
                        (KeyCode::Key3, 3),
                        (KeyCode::Key4, 4),
                        (KeyCode::Key5, 5),
                    ] {
                        if is_key_pressed(key) {
                            session.clock.set_speed(speed);
                        }
                    }
                    if let Some(m) = map.as_mut() {
                        for (key, mode) in [
                            (KeyCode::F1, MapMode::Political),
                            (KeyCode::F8, MapMode::Resources),
                            (KeyCode::F5, MapMode::Terrain),
                        ] {
                            if is_key_pressed(key) {
                                m.map_mode = mode;
                            }
                        }
                    }
                    for (key, title) in [
                        (KeyCode::F4, "Карта снабжения"),
                        (KeyCode::F6, "Карта сопротивления"),
                        (KeyCode::F7, "Карта подчинения"),
                        (KeyCode::F9, "Карта инфраструктуры"),
                        (KeyCode::F10, "Карта альянсов"),
                    ] {
                        if is_key_pressed(key) {
                            hud.detail = Some(title.into());
                        }
                    }
                    if is_key_pressed(KeyCode::Home) {
                        if let Some(m) = map.as_mut() {
                            m.zoom = 1.0;
                            m.target_zoom = 1.0;
                            m.pan = Vec2::ZERO;
                            m.zoom_anchor = None;
                        }
                    }
                    if is_key_pressed(KeyCode::H)
                        && !construction_view.open
                        && !production_view.open
                        && !research_view.open
                    {
                        if let Some(tag) = session.player_country.as_deref() {
                            army_view.notice = army_controls::stop_selection(
                                &mut session.military,
                                &session.world,
                                tag,
                                &army_view.selected,
                            )
                            .message();
                        }
                    }
                    if is_key_pressed(KeyCode::B)
                        && (is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl))
                    {
                        army_view.strategic = !army_view.strategic;
                    }
                }
                session.advance_frame(
                    get_frame_time(),
                    &building_rules,
                    &production_rules,
                    &research_catalogue,
                    &research_rules,
                );
                let GameSession {
                    world,
                    clock: game,
                    player_country: selected_country,
                    construction_queues: construction_queue,
                    construction: country_construction,
                    production: production_state,
                    resources: resource_state,
                    research: research_world,
                    military,
                    ..
                } = &mut session;
                let country = country_construction
                    .entry(selected_country.clone().unwrap_or_default())
                    .or_default();
                let CountryConstruction {
                    order: construction_order,
                    state_priority: construction_priority,
                    policy: construction_policy,
                    law: construction_law_preset,
                } = country;

                // Capture placement mode before UI click handlers run. Selecting a
                // building may collapse a panel over the map in the same frame:
                // that click must never leak through and place the building.
                let placement_before_ui = construction_view.open && construction_view.map_placement;
                let research_before_ui = research_view.open && research_view.browsing;
                let hud_before_ui = hud.regions();
                let army_before_ui = army_ui::interactive_bounds(&army_view);
                let inspector_before_ui = province_inspector_bounds(
                    selected.is_some(),
                    construction_view.open,
                    production_view.open,
                    research_view.open,
                    &army_view,
                );
                let rendered_screen = draw_map_viewer(
                    map.as_mut(),
                    world,
                    &load_error,
                    &mut selected,
                    selected_country.as_deref(),
                    construction_queue,
                    construction_priority,
                    construction_order,
                    &mut construction_notice,
                    construction_policy,
                    construction_law_preset,
                    &building_rules,
                    &mut construction_view,
                    &mut production_view,
                    production_state,
                    resource_state,
                    &production_rules,
                    research_world,
                    &research_catalogue,
                    &research_rules,
                    &mut research_view,
                    game,
                    &mut hud,
                    military,
                    &mut army_view,
                );
                if let Some(current_map) = map.as_mut() {
                    let (mouse_x, mouse_y) = mouse_position();
                    let mouse = vec2(mouse_x, mouse_y);
                    let rect = map_view_rect(
                        current_map,
                        construction_view.open && construction_view.map_placement,
                    );
                    let time_controls_overlay = ui_layout::Layout::current().hud_contains(mouse);
                    let inside_map = rect.contains(mouse)
                        && !time_controls_overlay
                        && !hud.blocked(mouse)
                        && !hud_before_ui.iter().any(|r| r.contains(mouse))
                        && !construction_ui::is_over_panel(mouse, &construction_view)
                        && !production_ui::is_over_panel(mouse, &production_view)
                        && !research_ui::is_over_panel(mouse, &research_view)
                        && !army_ui::is_over_panel(mouse, &army_view)
                        && !battle_plan_ui::blocked(&army_view, mouse)
                        && !army_before_ui.is_some_and(|r| r.contains(mouse))
                        && !inspector_before_ui.is_some_and(|r| r.contains(mouse))
                        && !province_inspector_bounds(
                            selected.is_some(),
                            construction_view.open,
                            production_view.open,
                            research_view.open,
                            &army_view,
                        )
                        .is_some_and(|r| r.contains(mouse))
                        && !research_before_ui
                        && !(research_view.open && research_view.browsing)
                        && (!construction_view.open || placement_before_ui);

                    // Macroquad sends touch events AND simulated mouse events.
                    // Keep the mouse/UI path for a single finger, but handle
                    // two-finger gestures independently to avoid ghost taps.
                    let contact_points: Vec<map_camera::TouchPoint> = touches()
                        .into_iter()
                        .filter(|touch| {
                            !matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled)
                        })
                        .map(|touch| map_camera::TouchPoint {
                            id: touch.id,
                            position: touch.position,
                        })
                        .collect();
                    let multitouch = contact_points.len() >= 2;
                    if !touches().is_empty() {
                        army_view.touch_ui = true;
                    }
                    if multitouch && army_view.pending_order.is_none() {
                        army_view.front_start = None;
                        army_view.front_preview.clear();
                        army_view.front_preview_end = None;
                        army_view.line_preview.clear();
                        army_view.line_preview_end = None;
                    }
                    if multitouch || hud.menu || hud.detail.is_some() || hud.map_palette {
                        army_view.map_drag = None;
                        army_view.right_down = None;
                        army_view.pan_last = None;
                    }
                    let pinch_blocked = research_before_ui
                        || (research_view.open && research_view.browsing)
                        || contact_points.iter().any(|point| {
                            hud.blocked(point.position)
                                || hud_before_ui.iter().any(|r| r.contains(point.position))
                                || construction_ui::is_over_panel(
                                    point.position,
                                    &construction_view,
                                )
                                || production_ui::is_over_panel(point.position, &production_view)
                                || research_ui::is_over_panel(point.position, &research_view)
                                || army_ui::is_over_panel(point.position, &army_view)
                                || inspector_before_ui.is_some_and(|r| r.contains(point.position))
                        });
                    if let Some(gesture) =
                        current_map
                            .pinch_tracker
                            .update(&contact_points, rect, pinch_blocked)
                    {
                        current_map.zoom_pinch(gesture, rect);
                    }

                    let military_mode = !construction_view.open
                        && !production_view.open
                        && !research_view.open
                        && !research_before_ui;
                    army_view.order_hover = if inside_map && military_mode {
                        army_view.order_map.hit(
                            world,
                            military,
                            current_map.source_rect(),
                            rect,
                            mouse,
                            army_view.selected_plan,
                        )
                    } else {
                        None
                    };
                    if military_mode
                        && !army_view.text_input_active()
                        && is_key_pressed(KeyCode::Delete)
                    {
                        if let Some(tag) = selected_country.as_deref() {
                            army_view.notice =
                                battle_plan_ui::delete_selected(&mut army_view, military, tag)
                                    .map_or_else(|e| e, |_| "Приказ удалён".into());
                        }
                    }
                    if military_mode
                        && inside_map
                        && army_view.plan_tool.is_none()
                        && is_mouse_button_pressed(MouseButton::Left)
                    {
                        if let Some(tag) = selected_country.as_deref() {
                            let counters = army_controls::counters(
                                world,
                                military,
                                current_map.source_rect(),
                                rect,
                            );
                            if army_controls::hit(&counters, tag, mouse).is_none() {
                                if let Some(handle) = army_view.order_map.handle(
                                    world,
                                    current_map.source_rect(),
                                    rect,
                                    mouse,
                                    army_view.selected_front,
                                    army_view.selected_plan,
                                ) {
                                    battle_plan_ui::begin_handle(&mut army_view, handle, military);
                                }
                            }
                        }
                    }
                    let mut drawing_plan = army_view.plan_tool.is_some() && military_mode;
                    if drawing_plan && inside_map && is_mouse_button_pressed(MouseButton::Left) {
                        if let Some(tag) = selected_country.as_deref() {
                            let counters = army_controls::counters(
                                world,
                                military,
                                current_map.source_rect(),
                                rect,
                            );
                            if army_controls::hit(&counters, tag, mouse).is_some() {
                                army_view.cancel_tool();
                                drawing_plan = false;
                            }
                        }
                    }
                    if drawing_plan && !multitouch {
                        let p = map_camera::screen_to_world(current_map.source_rect(), rect, mouse);
                        let province = if p.x >= 0.0 && p.y >= 0.0 {
                            current_map.province_at(p.x as u32, p.y as u32).map(|p| p.0)
                        } else {
                            None
                        };
                        if let Some(tag) = selected_country.as_deref() {
                            battle_plan_ui::pointer(
                                &mut army_view,
                                military,
                                world,
                                tag,
                                province,
                                map_graph::MapPosition {
                                    x: p.x as f64,
                                    y: p.y as f64,
                                },
                                24.0 * current_map.source_rect().w as f64 / rect.w as f64,
                                inside_map,
                            );
                        }
                        army_view.map_drag = None;
                    }
                    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                    let control =
                        is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
                    let touch_input = !touches().is_empty();
                    if !multitouch && is_mouse_button_pressed(MouseButton::Middle) && inside_map {
                        army_view.pan_last = Some(mouse);
                    }
                    if let Some(last) = army_view.pan_last {
                        if is_mouse_button_down(MouseButton::Middle) {
                            let delta = mouse - last;
                            let source = current_map.source_rect();
                            current_map.zoom_anchor = None;
                            current_map.pan.x -= delta.x / rect.w * source.w;
                            current_map.pan.y -= delta.y / rect.h * source.h;
                            current_map.clamp_pan();
                            army_view.pan_last = Some(mouse);
                        } else {
                            army_view.pan_last = None;
                        }
                    }
                    if !multitouch
                        && !drawing_plan
                        && is_mouse_button_pressed(MouseButton::Left)
                        && inside_map
                    {
                        army_view.map_drag = Some(army_controls::Drag::new(
                            mouse,
                            touch_input,
                            shift || army_view.touch_add,
                            military_mode && (!touch_input || army_view.touch_box),
                        ));
                    }
                    if let Some(drag) = army_view.map_drag.as_mut() {
                        let delta = drag.update(mouse);
                        if is_mouse_button_down(MouseButton::Left)
                            && drag.moved()
                            && !drag.box_select
                        {
                            let source = current_map.source_rect();
                            current_map.zoom_anchor = None;
                            current_map.pan.x -= delta.x / rect.w * source.w;
                            current_map.pan.y -= delta.y / rect.h * source.h;
                            current_map.clamp_pan();
                        }
                    }
                    let mut click = None;
                    if !multitouch && is_mouse_button_released(MouseButton::Left) {
                        if let Some(drag) = army_view.map_drag.take() {
                            if drag.moved() && drag.box_select && military_mode {
                                if let Some(tag) = selected_country.as_deref() {
                                    let counters = army_controls::counters(
                                        world,
                                        military,
                                        current_map.source_rect(),
                                        rect,
                                    );
                                    let mut blocked = army_ui::interactive_bounds(&army_view)
                                        .into_iter()
                                        .collect::<Vec<_>>();
                                    blocked.extend(hud.regions());
                                    army_controls::select_box(
                                        &mut army_view.selected,
                                        &counters,
                                        tag,
                                        army_controls::selection_rect(drag.start, mouse),
                                        drag.add,
                                        &blocked,
                                    );
                                    army_view.sync_selection(military);
                                }
                            } else if !drag.moved() && inside_map {
                                click = Some(drag);
                            }
                        }
                    }
                    if let Some(drag) = click {
                        let source = current_map.source_rect();
                        let pixel = map_camera::screen_to_world(source, rect, mouse);
                        let pixel_x = pixel
                            .x
                            .floor()
                            .clamp(0.0, current_map.width.saturating_sub(1) as f32)
                            as u32;
                        let pixel_y = pixel
                            .y
                            .floor()
                            .clamp(0.0, current_map.height.saturating_sub(1) as f32)
                            as u32;
                        selected = current_map
                            .province_at(pixel_x, pixel_y)
                            .map(|(id, rgb)| (id, rgb, pixel_x, pixel_y));
                        if military_mode {
                            if let Some(tag) = selected_country.as_deref() {
                                let counters =
                                    army_controls::counters(world, military, source, rect);
                                let battle_hit = army_controls::battle_markers(
                                    world, military, tag, source, rect,
                                )
                                .into_iter()
                                .find(|b| b.rect.contains(mouse));
                                if army_view.deployment_pick {
                                    if let Some((province, _, _, _)) = selected
                                        .filter(|p| world.controller_of_province(p.0) == Some(tag))
                                    {
                                        army_view.deployment_province = Some(province);
                                        army_view.deployment_pick = false;
                                        hud.open(
                                            Some(Panel::Recruitment),
                                            &mut construction_view,
                                            &mut production_view,
                                            &mut research_view,
                                            &mut army_view,
                                        );
                                    } else {
                                        army_view.notice =
                                            "Выберите свою сухопутную провинцию".into();
                                    }
                                } else if let Some(marker) = battle_hit {
                                    hud.open(
                                        Some(Panel::Army),
                                        &mut construction_view,
                                        &mut production_view,
                                        &mut research_view,
                                        &mut army_view,
                                    );
                                    army_view.show_battle(marker.id, military);
                                } else if army_view.touch_move && !army_view.selected.is_empty() {
                                    if let Some((province, _, _, _)) = selected {
                                        army_ui::issue_order(
                                            &mut army_view,
                                            military,
                                            world,
                                            tag,
                                            province,
                                            false,
                                        );
                                        army_view.touch_move = false;
                                    }
                                } else if let Some(counter) =
                                    army_controls::hit(&counters, tag, mouse)
                                {
                                    if let Some(g) =
                                        world.geography.provinces.get(&counter.province)
                                    {
                                        selected = Some((
                                            counter.province,
                                            g.definition.rgb,
                                            pixel_x,
                                            pixel_y,
                                        ));
                                    }
                                    army_controls::select_counter(
                                        &mut army_view.selected,
                                        counter,
                                        drag.add,
                                        control,
                                    );
                                    army_view.sync_selection(military);
                                    hud.open(
                                        None,
                                        &mut construction_view,
                                        &mut production_view,
                                        &mut research_view,
                                        &mut army_view,
                                    );
                                } else if battle_plan_ui::choose_order(
                                    &mut army_view,
                                    military,
                                    world,
                                    tag,
                                    source,
                                    rect,
                                    mouse,
                                    control,
                                    drag.add,
                                ) {
                                    hud.open(
                                        None,
                                        &mut construction_view,
                                        &mut production_view,
                                        &mut research_view,
                                        &mut army_view,
                                    );
                                } else if !drag.add && !drag.touch {
                                    army_view.selected.clear();
                                }
                            }
                        }
                        if hud.active == Some(Panel::Diplomacy) {
                            hud.diplomacy_country = selected
                                .and_then(|p| world.controller_of_province(p.0))
                                .map(str::to_owned);
                        }
                        let selected_state_id = selected.and_then(|(province_id, _, _, _)| {
                            current_map.state_by_province.get(&province_id).copied()
                        });
                        if construction_view.open {
                            construction_view.focused_state = selected_state_id;
                            if construction_view.map_placement {
                                if let Some((tag, state_id)) =
                                    selected_country.as_deref().zip(selected_state_id)
                                {
                                    match construction::enqueue_project(
                                        world,
                                        tag,
                                        state_id,
                                        construction_queue,
                                        &building_rules,
                                        construction_view.selected_building,
                                    ) {
                                        Ok(()) => {
                                            if !construction_priority.contains(&state_id) {
                                                construction_priority.push(state_id);
                                            }
                                            construction_order.sync(
                                                world,
                                                tag,
                                                &construction_queue,
                                            );
                                            let state_name = world
                                                .states
                                                .get(&state_id)
                                                .and_then(|s| s.name.as_deref())
                                                .unwrap_or("State");
                                            construction_notice = format!(
                                                "{}: {} в очереди",
                                                state_name,
                                                construction_view.selected_building.label(),
                                            );
                                        }
                                        Err(reason) => construction_notice = reason.to_owned(),
                                    }
                                }
                            }
                        }
                        current_map.highlighted_state_texture =
                            selected_state_id.and_then(|state_id| {
                                current_map
                                    .state_bounds
                                    .get(&state_id)
                                    .copied()
                                    .map(|bounds| {
                                        let texture = build_highlighted_state_texture(
                                            current_map.width,
                                            current_map.height,
                                            &current_map.state_ids_by_pixel,
                                            state_id,
                                            bounds,
                                        );
                                        (state_id, bounds, texture)
                                    })
                            });
                    }
                    if !multitouch
                        && military_mode
                        && is_mouse_button_pressed(MouseButton::Right)
                        && inside_map
                    {
                        army_view.right_down = Some(mouse);
                    }
                    if !multitouch && is_mouse_button_released(MouseButton::Right) {
                        if army_view
                            .right_down
                            .take()
                            .is_some_and(|start| start.distance_squared(mouse) < 16.0)
                            && inside_map
                            && military_mode
                        {
                            let pixel =
                                map_camera::screen_to_world(current_map.source_rect(), rect, mouse);
                            let counters = army_controls::counters(
                                world,
                                military,
                                current_map.source_rect(),
                                rect,
                            );
                            let province =
                                army_controls::target(&counters, mouse).or_else(|| {
                                    current_map
                                        .province_at(
                                            pixel.x.floor().clamp(
                                                0.0,
                                                current_map.width.saturating_sub(1) as f32,
                                            ) as u32,
                                            pixel.y.floor().clamp(
                                                0.0,
                                                current_map.height.saturating_sub(1) as f32,
                                            ) as u32,
                                        )
                                        .map(|p| p.0)
                                });
                            if let Some((tag, province)) = selected_country.as_deref().zip(province)
                            {
                                if army_view.selected.is_empty() {
                                    hud.diplomacy_country =
                                        world.controller_of_province(province).map(str::to_owned);
                                    hud.open(
                                        Some(Panel::Diplomacy),
                                        &mut construction_view,
                                        &mut production_view,
                                        &mut research_view,
                                        &mut army_view,
                                    );
                                } else {
                                    army_ui::issue_order(
                                        &mut army_view,
                                        military,
                                        world,
                                        tag,
                                        province,
                                        shift,
                                    );
                                }
                            }
                        }
                    }

                    current_map.update_zoom(get_frame_time(), rect);
                }

                let reload_clicked = std::mem::take(&mut hud.reload_requested);
                if reload_clicked {
                    // Drop the old world's GPU textures before allocating the new
                    // view. The campaign world, clock, orders and stockpiles live in session.
                    drop(map.take());
                    match ProvinceMap::load(world) {
                        Ok(loaded) => {
                            map = Some(loaded);
                            load_error.clear();
                            selected = None;
                        }
                        Err(error) => {
                            load_error = error;
                            map = None;
                            selected = None;
                        }
                    }
                }
                rendered_screen
            }
            Screen::Settings => draw_settings(),
        };
        // Files can be added after launch. Retry the headless import when the
        // user returns to a campaign screen from the menu, preserving a live party.
        if screen == Screen::MainMenu
            && matches!(next_screen, Screen::CountrySelect | Screen::MapViewer)
            && session.world.geography.width == 0
        {
            match start_session(scenario_choice) {
                Ok(fresh) => {
                    session = fresh;
                    match ProvinceMap::load(&session.world) {
                        Ok(loaded) => {
                            map = Some(loaded);
                            load_error.clear();
                        }
                        Err(error) => load_error = error,
                    }
                }
                Err(error) => load_error = error,
            }
        }
        screen = next_screen;
        hoi_ui::draw_tooltips();
        next_frame().await;
    }
}
