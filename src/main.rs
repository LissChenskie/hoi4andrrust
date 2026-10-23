mod state;
mod game;
mod world;
mod construction;

use game::GameState;
use world::World;
use construction::{BuildingRules, BuildingType, ConstructionQueues};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use image::GenericImageView;
use macroquad::prelude::*;
use state::load_state_membership;

const MAP_BITMAP_PATH: &str = "data/hoi4/map/provinces.bmp";
const MAP_DEFINITION_PATH: &str = "data/hoi4/map/definition.csv";
const STATE_DIRECTORY_PATH: &str = "data/hoi4/history/states";
const STATE_CATEGORY_DIRECTORY_PATH: &str = "data/hoi4/common/state_category";

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
            draw_text_ex(text, x, y, TextParams {
                font: Some(font),
                font_size: size.round().clamp(1.0, 128.0) as u16,
                font_scale: 1.0,
                color,
                ..Default::default()
            });
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

impl MapMode {
    fn label(self) -> &'static str {
        match self {
            Self::Political => "Political",
            Self::Resources => "Resources",
            Self::Terrain => "Terrain",
        }
    }
}

struct ProvinceMap {
    width: u32,
    height: u32,
    rgb_pixels: Vec<u8>,
    id_by_rgb: HashMap<[u8; 3], u32>,
    province_count: usize,
    state_by_province: HashMap<u32, u32>,
    owner_by_state: HashMap<u32, String>,
    world: World,
    state_count: usize,
    state_load_status: String,
    map_border_texture: Texture2D,
    state_ids_by_pixel: Vec<u32>,
    highlighted_state_texture: Option<(u32, Texture2D)>,
    political_texture: Texture2D,
    resources_texture: Texture2D,
    terrain_texture: Texture2D,
    resource_markers: Vec<ResourceMarker>,
    city_markers: Vec<CityMarker>,
    city_named_count: usize,
    localisation_file_count: usize,
    zoom: f32,
    target_zoom: f32,
    pan: Vec2,
    last_mouse: Vec2,
    dragging: bool,
    drag_moved: bool,
    map_mode: MapMode,
}

impl ProvinceMap {
    fn load() -> Result<Self, String> {
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

        let (state_by_province, economy_by_state, shared_slots_by_state, state_count, state_load_status) =
            match load_state_membership(
                Path::new(STATE_DIRECTORY_PATH),
                Path::new(STATE_CATEGORY_DIRECTORY_PATH),
            ) {
                Ok(membership) => (
                    membership.state_by_province,
                    membership.economy_by_state,
                    membership.shared_slots_by_state,
                    membership.state_count,
                    "State membership and category slots loaded".to_owned(),
                ),
                Err(error) => (HashMap::new(), HashMap::new(), HashMap::new(), 0, error),
            };

        let state_ids_by_pixel = build_state_ids_by_pixel(width, height, &rgba, &id_by_rgb, &state_by_province);

        let state_name_by_id = load_state_names(Path::new(STATE_DIRECTORY_PATH));
        let (political_texture, resources_texture, terrain_texture, owner_by_state) = build_map_mode_textures(
            width,
            height,
            &rgba,
            &id_by_rgb,
            &state_by_province,
            &definition_text,
            Path::new(STATE_DIRECTORY_PATH),
        );
        // One combined static border layer reduces GPU texture memory and draw calls.
        let map_border_texture = build_map_border_texture(
            width, height, &rgba, &id_by_rgb, &state_ids_by_pixel, &owner_by_state,
        );
        let resource_markers = build_resource_markers(
            width, &rgba, &id_by_rgb, &state_by_province, Path::new(STATE_DIRECTORY_PATH),
        );
        let city_markers = build_city_markers(width, &rgba, &id_by_rgb, Path::new(STATE_DIRECTORY_PATH));
        let world = World::from_map_data(
            &state_by_province, &owner_by_state, &state_name_by_id, &economy_by_state,
            &shared_slots_by_state,
        );

        // CPU-side hit testing only needs RGB. Drop the fourth alpha byte per pixel
        // after every map texture and marker has been built.
        let mut rgb_pixels = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for pixel in rgba.chunks_exact(4) {
            rgb_pixels.extend_from_slice(&pixel[..3]);
        }
        drop(rgba);
        let city_named_count = city_markers.iter().filter(|city| !city.name.is_empty()).count();
        let localisation_file_count = localisation_roots()
            .iter()
            .map(|root| count_localisation_files(root))
            .sum();

        Ok(Self {
            width,
            height,
            rgb_pixels,
            province_count: id_by_rgb.len(),
            id_by_rgb,
            state_by_province,
            owner_by_state,
            world,
            state_count,
            state_load_status,
            map_border_texture,
            state_ids_by_pixel,
            highlighted_state_texture: None,
            political_texture,
            resources_texture,
            terrain_texture,
            resource_markers,
            city_markers,
            city_named_count,
            localisation_file_count,
            zoom: 1.0,
            target_zoom: 1.0,
            pan: Vec2::ZERO,
            last_mouse: Vec2::ZERO,
            dragging: false,
            drag_moved: false,
            map_mode: MapMode::Political,
        })
    }

    fn source_rect(&self) -> Rect {
        let source_width = self.width as f32 / self.zoom;
        let source_height = self.height as f32 / self.zoom;
        let max_x = self.width as f32 - source_width;
        let max_y = self.height as f32 - source_height;
        Rect::new(
            (max_x * 0.5 + self.pan.x).clamp(0.0, max_x),
            (max_y * 0.5 + self.pan.y).clamp(0.0, max_y),
            source_width,
            source_height,
        )
    }

    fn set_zoom_at(&mut self, factor: f32, anchor: Vec2, view: Rect) {
        if view.w <= 0.0 || view.h <= 0.0 {
            return;
        }

        let old = self.source_rect();
        let u = ((anchor.x - view.x) / view.w).clamp(0.0, 1.0);
        let v = ((anchor.y - view.y) / view.h).clamp(0.0, 1.0);
        let map_x = old.x + u * old.w;
        let map_y = old.y + v * old.h;

        self.target_zoom = (self.target_zoom * factor).clamp(1.0, 24.0);
        let new_w = self.width as f32 / self.target_zoom;
        let new_h = self.height as f32 / self.target_zoom;
        self.pan.x = map_x - u * new_w - (self.width as f32 - new_w) * 0.5;
        self.pan.y = map_y - v * new_h - (self.height as f32 - new_h) * 0.5;
        self.clamp_pan();
    }

    fn clamp_pan(&mut self) {
        let max_x = self.width as f32 - self.width as f32 / self.zoom;
        let max_y = self.height as f32 - self.height as f32 / self.zoom;
        self.pan.x = self.pan.x.clamp(-max_x * 0.5, max_x * 0.5);
        self.pan.y = self.pan.y.clamp(-max_y * 0.5, max_y * 0.5);
    }

    fn province_at(&self, x: u32, y: u32) -> Option<(u32, [u8; 3])> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let offset = ((y as usize * self.width as usize) + x as usize) * 3;
        let rgb = [self.rgb_pixels[offset], self.rgb_pixels[offset + 1], self.rgb_pixels[offset + 2]];
        self.id_by_rgb.get(&rgb).copied().map(|id| (id, rgb))
    }
}


fn build_state_ids_by_pixel(
    width: u32,
    height: u32,
    rgba: &[u8],
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_by_province: &HashMap<u32, u32>,
) -> Vec<u32> {
    let mut state_ids = vec![0u32; width as usize * height as usize];
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        if let Some(province_id) = id_by_rgb.get(&rgb) {
            state_ids[index] = state_by_province.get(province_id).copied().unwrap_or(0);
        }
    }
    state_ids
}

fn build_map_border_texture(
    width: u32,
    height: u32,
    rgba: &[u8],
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_ids: &[u32],
    owner_by_state: &HashMap<u32, String>,
) -> Texture2D {
    let w = width as usize;
    let h = height as usize;
    let pixel_count = w * h;

    let mut province_ids = vec![0u32; pixel_count];

    // Assign compact, collision-free owner IDs once. A u32 per pixel is half
    // the memory of the old u64 owner layer, without hash lookups in the edge loop.
    let mut owner_ids_by_tag = HashMap::<&str, u32>::new();
    let mut owner_ids_by_state = HashMap::<u32, u32>::new();
    for (&state_id, owner) in owner_by_state {
        let next_id = owner_ids_by_tag.len() as u32 + 1;
        let owner_id = *owner_ids_by_tag.entry(owner.as_str()).or_insert(next_id);
        owner_ids_by_state.insert(state_id, owner_id);
    }

    let mut owners = vec![0u32; pixel_count];
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        if let Some(province_id) = id_by_rgb.get(&rgb) {
            province_ids[index] = *province_id;
            let state_id = state_ids.get(index).copied().unwrap_or(0);
            owners[index] = owner_ids_by_state.get(&state_id).copied().unwrap_or(0);
        }
    }

    let mut overlay = vec![0u8; pixel_count * 4];
    for y in 0..h {
        for x in 0..w {
            let index = y * w + x;
            let province_id = province_ids[index];
            if province_id == 0 { continue; }
            let state_id = state_ids.get(index).copied().unwrap_or(0);
            let owner = owners[index];

            let mut country_edge = false;
            let mut state_edge = false;
            let mut province_edge = false;
            let neighbours = [
                if x > 0 { Some(index - 1) } else { None },
                if x + 1 < w { Some(index + 1) } else { None },
                if y > 0 { Some(index - w) } else { None },
                if y + 1 < h { Some(index + w) } else { None },
            ];
            for neighbour in neighbours.into_iter().flatten() {
                if owner != 0 && owners[neighbour] != 0 && owners[neighbour] != owner {
                    country_edge = true;
                } else if state_id != 0 && state_ids.get(neighbour).copied().unwrap_or(0) != state_id {
                    state_edge = true;
                } else if province_ids[neighbour] != province_id {
                    province_edge = true;
                }
            }

            let (color, alpha) = if country_edge {
                ([232u8, 220u8, 194u8], 150u8)
            } else if state_edge {
                ([235u8, 198u8, 120u8], 38u8)
            } else if province_edge {
                ([20u8, 25u8, 31u8], 34u8)
            } else {
                continue;
            };
            let o = index * 4;
            overlay[o] = color[0];
            overlay[o + 1] = color[1];
            overlay[o + 2] = color[2];
            overlay[o + 3] = alpha;
        }
    }

    let texture = Texture2D::from_rgba8(width as u16, height as u16, &overlay);
    texture.set_filter(FilterMode::Linear);
    texture
}

fn build_highlighted_state_texture(width: u32, height: u32, state_ids: &[u32], selected_state: u32) -> Texture2D {
    let w = width as usize;
    let h = height as usize;
    let mut overlay = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let index = y * w + x;
            if state_ids[index] != selected_state { continue; }
            let differs = (x > 0 && state_ids[index - 1] != selected_state)
                || (x + 1 < w && state_ids[index + 1] != selected_state)
                || (y > 0 && state_ids[index - w] != selected_state)
                || (y + 1 < h && state_ids[index + w] != selected_state);
            if differs {
                let o = index * 4;
                overlay[o] = 255;
                overlay[o + 1] = 218;
                overlay[o + 2] = 105;
                overlay[o + 3] = 235;
            }
        }
    }
    let texture = Texture2D::from_rgba8(width as u16, height as u16, &overlay);
    texture.set_filter(FilterMode::Linear);
    texture
}

/// Parse HOI4 definition.csv into an RGB -> province ID index.
/// Vanilla HOI4 files are semicolon-delimited; comma-delimited input is also accepted.

fn parse_state_visual_data(directory: &Path) -> (HashMap<u32, String>, HashMap<u32, f32>) {
    const MAP_DATE: (u32, u32, u32) = (1936, 1, 1);
    let mut owners = HashMap::new();
    let mut resources = HashMap::new();
    let Ok(entries) = fs::read_dir(directory) else { return (owners, resources) };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") { continue; }
        let Ok(source) = fs::read_to_string(&path) else { continue };

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
            if clean.is_empty() { continue; }
            if state_id.is_none() && clean.starts_with("id") {
                if let Some(value) = clean.split('=').nth(1).and_then(|v| v.trim().parse::<u32>().ok()) {
                    state_id = Some(value);
                }
            }
            let Some(id) = state_id else { continue };

            if clean.starts_with("resources") && clean.contains('{') {
                in_resources = true;
                resource_depth = clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if let Some((_, tail)) = clean.split_once('{') {
                    parse_resource_values(tail, id, &mut resources);
                }
                if resource_depth <= 0 { in_resources = false; }
            } else if in_resources {
                parse_resource_values(clean, id, &mut resources);
                resource_depth += clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if resource_depth <= 0 { in_resources = false; }
            }
        }
    }
    (owners, resources)
}

fn parse_resource_values(line: &str, state_id: u32, resources: &mut HashMap<u32, f32>) {
    for key in ["steel", "aluminium", "aluminum", "rubber", "tungsten", "chromium", "oil"] {
        if let Some((_, tail)) = line.split_once(key) {
            if let Some((_, value)) = tail.split_once('=') {
                let value = value.trim().trim_matches(|c: char| c == '}' || c.is_whitespace() || c == '"');
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
        "GER" => [105, 112, 119], "ENG" | "GBR" => [190, 52, 57],
        "FRA" => [62, 105, 190], "SOV" | "RUS" => [190, 62, 55],
        "USA" => [65, 119, 190], "ITA" => [73, 150, 91],
        "JAP" => [205, 120, 135], "CHI" => [205, 174, 95],
        "POL" => [205, 145, 160], "AUS" => [195, 185, 150],
        "CAN" => [185, 60, 66], "BRA" => [80, 155, 91],
        "SPR" | "SPB" => [210, 170, 75], "TUR" => [180, 80, 70],
        "HOL" => [230, 135, 65], "BEL" => [215, 195, 85],
        "CZE" => [90, 145, 180], "ROM" => [110, 155, 80],
        "YUG" => [150, 160, 180], "GRE" => [80, 155, 185],
        "NOR" => [100, 155, 195], "SWE" => [75, 135, 190],
        "FIN" => [175, 190, 200], "POR" => [65, 145, 105],
        "MEX" => [75, 145, 95], "ARG" => [100, 170, 195],
        _ => {
            let hash = tag.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
            // Keep all channels bright enough and separate hues more strongly.
            let hue = (hash % 360) as f32;
            let c = 0.62_f32;
            let x = c * (1.0 - (((hue / 60.0) % 2.0) - 1.0).abs());
            let (r, g, b) = match hue as u32 / 60 {
                0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x),
                3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x),
            };
            [((r + 0.18) * 255.0) as u8, ((g + 0.18) * 255.0) as u8, ((b + 0.18) * 255.0) as u8]
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

fn build_map_mode_textures(
    width: u32,
    height: u32,
    source_rgba: &[u8],
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_by_province: &HashMap<u32, u32>,
    definition_text: &str,
    state_directory: &Path,
) -> (Texture2D, Texture2D, Texture2D, HashMap<u32, String>) {
    let (owner_by_state, resources_by_state) = parse_state_visual_data(state_directory);
    let mut terrain_by_province = HashMap::<u32, String>::new();
    let mut water_provinces = HashSet::<u32>::new();
    let delimiter = definition_text.lines().find(|l| !l.trim().is_empty())
        .map(|l| if l.contains(';') { ';' } else { ',' }).unwrap_or(';');
    for line in definition_text.lines().skip(1) {
        let fields: Vec<&str> = line.split(delimiter).collect();
        if fields.len() < 7 { continue; }
        if let Ok(id) = fields[0].trim().trim_start_matches('\u{feff}').parse::<u32>() {
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
    let max_resources = resources_by_state.values().copied().fold(0.0f32, f32::max).max(1.0);
    let mut political = source_rgba.to_vec();
    let mut resources = vec![0u8; source_rgba.len()];
    let mut terrain = vec![0u8; source_rgba.len()];
    for (pixel_index, pixel) in source_rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let province_id = id_by_rgb.get(&rgb).copied().unwrap_or(0);
        let state_id = state_by_province.get(&province_id).copied().unwrap_or(0);
        let owner = owner_by_state.get(&state_id);
        let owner_color = owner.map(|tag| tag_color(tag));
        let base_terrain = terrain_by_province.get(&province_id).map(|n| terrain_color(n)).unwrap_or([95, 101, 107]);
        let offset = pixel_index * 4;
        // Political mode blends the terrain palette with the owning country's color.
        // Unowned/unparsed provinces stay neutral gray, and water remains blue.
        let political_color = if water_provinces.contains(&province_id) {
            [55, 105, 155]
        } else {
            owner_color
                .map(|country| blend_country(base_terrain, country, 0.48))
                .unwrap_or([92, 96, 101])
        };
        political[offset..offset + 3].copy_from_slice(&political_color);
        political[offset + 3] = 255;

        // Terrain mode is terrain-first; do not tint it with country colors.
        terrain[offset..offset + 3].copy_from_slice(&base_terrain);
        terrain[offset + 3] = 255;

        let amount = resources_by_state.get(&state_id).copied().unwrap_or(0.0);
        // Keep sea and lake pixels in their normal water colours in resource mode.
        let resource_color = if water_provinces.contains(&province_id) {
            base_terrain
        } else if amount <= 0.0 {
            [54, 59, 65]
        } else {
            let strength = (amount / max_resources).clamp(0.0, 1.0);
            if strength < 0.5 {
                // Blue -> yellow for low to medium resource states.
                let t = strength * 2.0;
                [
                    (45.0 + t * 190.0) as u8,
                    (90.0 + t * 95.0) as u8,
                    (155.0 - t * 115.0) as u8,
                ]
            } else {
                // Yellow -> red for the richest states.
                let t = (strength - 0.5) * 2.0;
                [
                    235,
                    (185.0 - t * 135.0) as u8,
                    (40.0 - t * 5.0) as u8,
                ]
            }
        };
        resources[offset..offset + 3].copy_from_slice(&resource_color);
        resources[offset + 3] = 255;
    }
    let make_texture = |bytes: &[u8]| {
        let texture = Texture2D::from_rgba8(width as u16, height as u16, bytes);
        texture.set_filter(FilterMode::Linear);
        texture
    };
    (make_texture(&political), make_texture(&resources), make_texture(&terrain), owner_by_state)
}


fn build_resource_markers(
    width: u32,
    rgba: &[u8],
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_by_province: &HashMap<u32, u32>,
    state_directory: &Path,
) -> Vec<ResourceMarker> {
    let mut resource_types = HashMap::<u32, Vec<(String, f32)>>::new();
    let Ok(entries) = fs::read_dir(state_directory) else { return Vec::new() };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") { continue; }
        let Ok(source) = fs::read_to_string(path) else { continue };
        let mut state_id = None;
        let mut in_resources = false;
        let mut depth = 0i32;
        for line in source.lines() {
            let clean = line.split('#').next().unwrap_or("").trim();
            if clean.starts_with("id") {
                state_id = clean.split_once('=').and_then(|(_, v)| v.trim().parse::<u32>().ok()).or(state_id);
            }
            if clean.starts_with("resources") && clean.contains('{') {
                in_resources = true;
                depth = clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if let Some(id) = state_id {
                    for (key, value) in resource_pairs(clean) {
                        if let Ok(amount) = value.parse::<f32>() {
                            if amount > 0.0 { resource_types.entry(id).or_default().push((key, amount)); }
                        }
                    }
                }
                if depth <= 0 { in_resources = false; }
                continue;
            }
            if in_resources {
                if let Some(id) = state_id {
                    for (key, value) in resource_pairs(clean) {
                        if let Ok(amount) = value.parse::<f32>() {
                            if amount > 0.0 { resource_types.entry(id).or_default().push((key, amount)); }
                        }
                    }
                }
                depth += clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if depth <= 0 { in_resources = false; }
            }
        }
    }

    let mut sums = HashMap::<u32, (f64, f64, u32)>::new();
    for (pixel_index, pixel) in rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let Some(province) = id_by_rgb.get(&rgb) else { continue };
        let Some(state_id) = state_by_province.get(province) else { continue };
        let x = (pixel_index % width as usize) as f64;
        let y = (pixel_index / width as usize) as f64;
        let item = sums.entry(*state_id).or_insert((0.0, 0.0, 0));
        item.0 += x; item.1 += y; item.2 += 1;
    }

    let mut markers = Vec::new();
    let mut states: Vec<u32> = resource_types.keys().copied().collect();
    states.sort_unstable();
    for state_id in states {
        let Some((sx, sy, count)) = sums.get(&state_id).copied() else { continue };
        if count == 0 { continue; }
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
    let Ok(entries) = fs::read_dir(state_directory) else { return Vec::new() };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") { continue; }
        let Ok(source) = fs::read_to_string(path) else { continue };
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
                if depth <= 0 { in_vp = false; }
                continue;
            }
            if in_vp {
                parse_victory_point_pairs(clean, &mut points);
                depth += clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if depth <= 0 { in_vp = false; }
            }
        }
    }

    let mut sums = HashMap::<u32, (f64, f64, u32)>::new();
    for (pixel_index, pixel) in rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let Some(province_id) = id_by_rgb.get(&rgb) else { continue };
        if !points.contains_key(province_id) { continue; }
        let x = (pixel_index % width as usize) as f64;
        let y = (pixel_index / width as usize) as f64;
        let item = sums.entry(*province_id).or_insert((0.0, 0.0, 0));
        item.0 += x; item.1 += y; item.2 += 1;
    }

    let names = load_victory_point_names();
    let mut markers = Vec::new();
    let mut ids: Vec<u32> = points.keys().copied().collect();
    ids.sort_unstable();
    for province_id in ids {
        let Some((sx, sy, count)) = sums.get(&province_id).copied() else { continue };
        if count == 0 { continue; }
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
    markers.sort_by(|a, b| b.victory_points.cmp(&a.victory_points).then_with(|| a.province_id.cmp(&b.province_id)));
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

fn count_localisation_files(root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(root) else { return 0 };
    entries.flatten().map(|entry| {
        let path = entry.path();
        if path.is_dir() {
            count_localisation_files(&path)
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("yml")) {
            1
        } else {
            0
        }
    }).sum()
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
        let Ok(entries) = fs::read_dir(directory) else { return };
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                visit_localisation(&path, names);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("yml")) {
                let Ok(source) = fs::read_to_string(&path) else { continue };
                for line in source.lines() {
                    let line = line.trim().trim_start_matches('\u{feff}');
                    let Some((key, raw_value)) = line.split_once(':') else { continue };
                    let key = key.trim().trim_matches('"');
                    if !key.starts_with("STATE_") { continue; }
                    let raw_value = raw_value.split('#').next().unwrap_or("").trim();
                    let value = raw_value.strip_prefix("0").map(str::trim_start).unwrap_or(raw_value);
                    let value = value.trim().trim_matches('"').trim();
                    if !value.is_empty() && !value.starts_with("STATE_") {
                        names.insert(key.to_owned(), value.to_owned());
                    }
                }
            }
        }
    }

    let mut key_by_id = HashMap::<u32, String>::new();
    let Ok(entries) = fs::read_dir(state_directory) else { return HashMap::new() };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") { continue; }
        let Ok(source) = fs::read_to_string(path) else { continue };
        let mut state_id = None;
        let mut state_key = None;
        for line in source.lines() {
            let clean = line.split('#').next().unwrap_or("").trim();
            if state_id.is_none() && clean.starts_with("id") {
                state_id = clean.split_once('=').and_then(|(_, value)| value.trim().parse::<u32>().ok());
            }
            if state_key.is_none() && clean.starts_with("name") {
                state_key = clean.split_once('=').map(|(_, value)| value.trim().trim_matches('"').to_owned());
            }
            if state_id.is_some() && state_key.is_some() { break; }
        }
        if let (Some(id), Some(key)) = (state_id, state_key) {
            key_by_id.insert(id, key);
        }
    }

    let mut localised = HashMap::<String, String>::new();
    for root in localisation_roots() {
        visit_localisation(&root, &mut localised);
    }

    key_by_id.into_iter().map(|(id, key)| {
        let name = localised.get(&key).cloned().unwrap_or_else(|| format!("State {id}"));
        (id, name)
    }).collect()
}

fn load_victory_point_names() -> HashMap<u32, String> {
    fn visit(directory: &Path, names: &mut HashMap<u32, String>) {
        let Ok(entries) = fs::read_dir(directory) else { return };
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                visit(&path, names);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("yml")) {
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
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim().trim_matches('"');
        let Some(raw_id) = key.strip_prefix("VICTORY_POINTS_") else { continue };
        let Ok(id) = raw_id.parse::<u32>() else { continue };

        // Accept HOI4 localisation lines such as VICTORY_POINTS_123:0 "City".
        // Ignore trailing comments and remove the numeric localisation version.
        let value = value.split('#').next().unwrap_or("").trim();
        let value = value.strip_prefix("0").map(str::trim_start).unwrap_or(value);
        let value = value.trim_matches('"').trim();
        if value.is_empty() { continue; }

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
            if matches!(key.as_str(), "oil" | "steel" | "aluminium" | "aluminum" | "rubber" | "tungsten" | "chromium") {
                pairs.push((key, value));
            }
        }
    }
    // Clausewitz usually separates key and '=' with spaces, so parse token triplets too.
    let tokens: Vec<&str> = line.split_whitespace().collect();
    for window in tokens.windows(3) {
        if window[1] == "=" && matches!(window[0], "oil" | "steel" | "aluminium" | "aluminum" | "rubber" | "tungsten" | "chromium") {
            pairs.push((window[0].to_owned(), window[2].trim_matches(|c: char| c == '}' || c == '{' || c == '"').to_owned()));
        }
    }
    pairs
}

fn parse_definition_text(text: &str) -> Result<HashMap<[u8; 3], u32>, String> {
    let delimiter = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| if line.contains(';') { ';' } else { ',' })
        .unwrap_or(';');

    let mut id_by_rgb = HashMap::new();
    let mut ids_seen = HashSet::new();

    for (line_index, line) in text.lines().enumerate() {
        let mut fields = line.split(delimiter);
        let Some(id_text) = fields.next() else { continue };
        let Some(r_text) = fields.next() else { continue };
        let Some(g_text) = fields.next() else { continue };
        let Some(b_text) = fields.next() else { continue };

        let parsed = (
            id_text.trim().trim_start_matches('\u{feff}').parse::<u32>(),
            r_text.trim().parse::<u8>(),
            g_text.trim().parse::<u8>(),
            b_text.trim().parse::<u8>(),
        );
        let (Ok(id), Ok(r), Ok(g), Ok(b)) = parsed else {
            // Skip the header and malformed rows in this initial parser.
            continue;
        };
        if id == 0 {
            continue;
        }
        if !ids_seen.insert(id) {
            return Err(format!("Duplicate province ID {id} at definition.csv line {}", line_index + 1));
        }
        let rgb = [r, g, b];
        if id_by_rgb.insert(rgb, id).is_some() {
            return Err(format!("Duplicate province RGB colour at definition.csv line {}", line_index + 1));
        }
    }

    if id_by_rgb.is_empty() {
        return Err("definition.csv contained no valid province definitions".to_owned());
    }
    Ok(id_by_rgb)
}

#[cfg(test)]
mod tests {
    use super::parse_definition_text;

    #[test]
    fn parses_hoi4_semicolon_delimited_definitions() {
        let source = "province;red;green;blue;type;coastal;terrain;continent\n1;10;20;30;land;false;plains;1\n2;40;50;60;sea;true;ocean;1\n";
        let index = parse_definition_text(source).expect("valid HOI4 definition");
        assert_eq!(index.get(&[10, 20, 30]), Some(&1));
        assert_eq!(index.get(&[40, 50, 60]), Some(&2));
    }

    #[test]
    fn accepts_comma_delimited_definitions() {
        let source = "province,red,green,blue,type,coastal,terrain,continent\n7,1,2,3,land,false,plains,1\n";
        let index = parse_definition_text(source).expect("valid comma-delimited definition");
        assert_eq!(index.get(&[1, 2, 3]), Some(&7));
    }

    #[test]
    fn rejects_duplicate_province_ids() {
        let source = "province;red;green;blue\n4;1;2;3\n4;4;5;6\n";
        assert!(parse_definition_text(source).unwrap_err().contains("Duplicate province ID 4"));
    }

    #[test]
    fn rejects_duplicate_rgb_colours() {
        let source = "province;red;green;blue\n4;1;2;3\n5;1;2;3\n";
        assert!(parse_definition_text(source).unwrap_err().contains("Duplicate province RGB colour"));
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

fn time_control_button(label: &str, x: f32, y: f32, width: f32, height: f32, active: bool) -> bool {
    let (mouse_x, mouse_y) = mouse_position();
    let hovered = mouse_x >= x
        && mouse_x <= x + width
        && mouse_y >= y
        && mouse_y <= y + height;
    let background = if active {
        Color::from_rgba(111, 52, 46, 255)
    } else if hovered {
        Color::from_rgba(57, 92, 125, 255)
    } else {
        Color::from_rgba(35, 54, 76, 255)
    };
    draw_rectangle(x, y, width, height, background);
    draw_rectangle_lines(x, y, width, height, 1.0, Color::from_rgba(91, 125, 154, 255));
    let font_size = if width < 40.0 { 13.0 } else { 15.0 };
    let text_width = measure_ui_text(label, font_size as u16).width;
    draw_ui_text(label, x + (width - text_width) * 0.5, y + height * 0.64, font_size, WHITE);
    hovered && is_mouse_button_pressed(MouseButton::Left)
}

fn draw_time_controls(game: &GameState, date_label: &str) -> (bool, Option<u8>) {
    let width = screen_width();
    let compact = width < 760.0;
    let (panel_x, panel_y, panel_w, panel_h) = if compact {
        (8.0, screen_height() - 62.0, (width - 16.0).max(1.0), 54.0)
    } else {
        ((width - 405.0).max(265.0), 70.0, 395.0, 54.0)
    };
    draw_rectangle(panel_x, panel_y, panel_w, panel_h, Color::from_rgba(18, 30, 43, 248));
    draw_rectangle_lines(panel_x, panel_y, panel_w, panel_h, 1.0, Color::from_rgba(62, 86, 106, 255));

    let date_x = panel_x + 8.0;
    let date_y = panel_y + 33.0;
    let mut speed_clicked = None;

    let pause_clicked = if compact {
        let date_width = (panel_w * 0.255).clamp(70.0, 91.0);
        draw_ui_text(date_label, date_x, date_y, 12.0, WHITE);
        let pause_x = panel_x + date_width;
        let clicked = time_control_button(
            if game.paused { ">" } else { "||" },
            pause_x,
            panel_y + 10.0,
            39.0,
            34.0,
            false,
        );
        let gap = 4.0;
        let speed_x = pause_x + 39.0 + gap;
        let available = (panel_x + panel_w - 6.0 - speed_x - gap * 4.0).max(5.0);
        let speed_w = (available / 5.0).min(39.0);
        for speed in 1..=5 {
            let x = speed_x + (speed - 1) as f32 * (speed_w + gap);
            if time_control_button(&speed.to_string(), x, panel_y + 10.0, speed_w, 34.0, game.speed == speed) {
                speed_clicked = Some(speed);
            }
        }
        clicked
    } else {
        draw_ui_text(date_label, date_x, date_y, 14.0, WHITE);
        let pause_x = panel_x + 112.0;
        let clicked = time_control_button(
            if game.paused { "PLAY" } else { "PAUSE" },
            pause_x,
            panel_y + 9.0,
            66.0,
            36.0,
            false,
        );
        let speed_x = pause_x + 72.0;
        for speed in 1..=5 {
            let x = speed_x + (speed - 1) as f32 * 43.0;
            if time_control_button(&speed.to_string(), x, panel_y + 9.0, 39.0, 36.0, game.speed == speed) {
                speed_clicked = Some(speed);
            }
        }
        clicked
    };
    (pause_clicked, speed_clicked)
}

fn button(label: &str, x: f32, y: f32, width: f32, height: f32) -> bool {
    let (mouse_x, mouse_y) = mouse_position();
    let hovered = mouse_x >= x
        && mouse_x <= x + width
        && mouse_y >= y
        && mouse_y <= y + height;

    let background = if hovered {
        Color::from_rgba(57, 92, 125, 255)
    } else {
        Color::from_rgba(35, 54, 76, 255)
    };

    draw_rectangle(x, y, width, height, background);
    draw_rectangle_lines(x, y, width, height, 1.0, Color::from_rgba(91, 125, 154, 255));
    draw_ui_text(label, x + 20.0, y + height * 0.63, 23.0, WHITE);

    hovered && is_mouse_button_pressed(MouseButton::Left)
}

fn draw_background() {
    clear_background(Color::from_rgba(12, 20, 31, 255));
    draw_rectangle(0.0, 0.0, screen_width(), 5.0, Color::from_rgba(111, 52, 46, 255));
}

fn draw_main_menu() -> Screen {
    if screen_width() < 760.0 {
        return draw_main_menu_compact();
    }
    draw_background();

    let width = screen_width();
    let height = screen_height();
    draw_ui_text("HOI4 MOBILE", 72.0, 110.0, 48.0, WHITE);
    draw_ui_text("Independent Rust strategy prototype", 75.0, 143.0, 19.0, Color::from_rgba(160, 177, 195, 255));

    // Decorative map-like shapes only; these are not real HOI4 geography.
    let panel_x = width * 0.48;
    let panel_y = 72.0;
    let panel_w = width * 0.46;
    let panel_h = height - 125.0;
    draw_rectangle(panel_x, panel_y, panel_w, panel_h, Color::from_rgba(17, 31, 43, 255));
    draw_rectangle_lines(panel_x, panel_y, panel_w, panel_h, 1.0, Color::from_rgba(48, 69, 84, 255));
    draw_ellipse(panel_x + panel_w * 0.29, panel_y + panel_h * 0.32, panel_w * 0.22, panel_h * 0.12, 0.2, Color::from_rgba(57, 80, 72, 255));
    draw_ellipse(panel_x + panel_w * 0.55, panel_y + panel_h * 0.48, panel_w * 0.25, panel_h * 0.17, -0.3, Color::from_rgba(57, 80, 72, 255));
    draw_ellipse(panel_x + panel_w * 0.70, panel_y + panel_h * 0.75, panel_w * 0.11, panel_h * 0.08, 0.1, Color::from_rgba(57, 80, 72, 255));
    draw_ui_text("MAP PREVIEW", panel_x + 20.0, panel_y + panel_h - 24.0, 16.0, Color::from_rgba(160, 177, 195, 255));

    let button_x = 74.0;
    let button_w = (width * 0.34).clamp(250.0, 360.0);
    if button("New game / Select country", button_x, 205.0, button_w, 54.0) {
        return Screen::CountrySelect;
    }
    if button("Open map viewer", button_x, 275.0, button_w, 54.0) {
        return Screen::MapViewer;
    }
    if button("Map data / import guide", button_x, 345.0, button_w, 54.0) {
        return Screen::Settings;
    }

    draw_ui_text("Prototype milestone 02: country selection + state economy", 75.0, height - 42.0, 16.0, Color::from_rgba(125, 145, 163, 255));
    Screen::MainMenu
}


fn draw_main_menu_compact() -> Screen {
    draw_background();
    draw_ui_text("HOI4 MOBILE", 22.0, 82.0, 36.0, WHITE);
    draw_ui_text("Independent Rust strategy prototype", 24.0, 111.0, 15.0, LIGHTGRAY);

    let button_x = 20.0;
    let button_w = (screen_width() - 40.0).max(180.0);
    let first_y = (screen_height() * 0.40).max(190.0);
    if button("New game / Select country", button_x, first_y, button_w, 54.0) {
        return Screen::CountrySelect;
    }
    if button("Open map viewer", button_x, first_y + 68.0, button_w, 54.0) {
        return Screen::MapViewer;
    }
    if button("Map data / import guide", button_x, first_y + 136.0, button_w, 54.0) {
        return Screen::Settings;
    }
    draw_ui_text("Prototype milestone 02", 22.0, screen_height() - 28.0, 14.0, LIGHTGRAY);
    Screen::MainMenu
}


fn country_display_name(tag: &str) -> &str {
    match tag {
        "GER" => "German Reich", "ENG" | "GBR" => "United Kingdom",
        "FRA" => "France", "SOV" | "RUS" => "Soviet Union", "USA" => "United States",
        "ITA" => "Italy", "JAP" => "Japan", "CHI" => "China", "POL" => "Poland",
        "AUS" => "Austria", "CAN" => "Canada", "BRA" => "Brazil", "SPR" => "Spain",
        "SPB" => "Nationalist Spain", "TUR" => "Turkey", "HOL" => "Netherlands",
        "BEL" => "Belgium", "CZE" => "Czechoslovakia", "ROM" => "Romania",
        "YUG" => "Yugoslavia", "GRE" => "Greece", "NOR" => "Norway", "SWE" => "Sweden",
        "FIN" => "Finland", "POR" => "Portugal", "MEX" => "Mexico", "ARG" => "Argentina",
        _ => tag,
    }
}

fn ordered_country_tags(map: &ProvinceMap) -> Vec<String> {
    let priority = [
        "GER", "ENG", "GBR", "FRA", "SOV", "RUS", "USA", "ITA", "JAP", "CHI",
        "POL", "CAN", "AUS", "BRA", "SPR", "SPB", "TUR", "HOL", "BEL", "CZE",
    ];
    let mut tags: Vec<String> = map.world.countries.keys().cloned().collect();
    tags.sort_by(|a, b| {
        let rank = |tag: &str| priority.iter().position(|candidate| *candidate == tag).unwrap_or(usize::MAX);
        rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
    });
    tags
}

fn draw_country_select(
    map: Option<&ProvinceMap>,
    selected_country: &mut Option<String>,
    page: &mut usize,
) -> Screen {
    draw_background();
    draw_rectangle(0.0, 5.0, screen_width(), 58.0, Color::from_rgba(20, 32, 46, 255));
    draw_ui_text("SELECT COUNTRY", 22.0, 42.0, 25.0, WHITE);
    if screen_width() < 760.0 {
        if button("Back", screen_width() - 92.0, 15.0, 76.0, 38.0) { return Screen::MainMenu; }
    } else if button("Back to menu", screen_width() - 160.0, 15.0, 138.0, 38.0) {
        return Screen::MainMenu;
    }

    let Some(map) = map else {
        draw_ui_text("Map data is unavailable. Add the local HOI4 map files first.", 24.0, 108.0, 16.0, LIGHTGRAY);
        return Screen::CountrySelect;
    };
    let tags = ordered_country_tags(map);
    if tags.is_empty() {
        draw_ui_text("No countries found. Check data/hoi4/history/states and state owners.", 24.0, 108.0, 16.0, LIGHTGRAY);
        return Screen::CountrySelect;
    }
    if selected_country.as_ref().is_none_or(|tag| !map.world.countries.contains_key(tag)) {
        *selected_country = tags.first().cloned();
    }

    let compact = screen_width() < 760.0;
    let columns = if compact { 2usize } else { 3usize };
    let rows = 5usize;
    let per_page = columns * rows;
    let page_count = tags.len().div_ceil(per_page).max(1);
    *page = (*page).min(page_count - 1);
    let start = *page * per_page;
    let end = (start + per_page).min(tags.len());
    draw_ui_text("Choose a country to begin. The list is built from loaded state ownership data.",
        24.0, 91.0, if compact { 13.0 } else { 16.0 }, LIGHTGRAY);

    let margin = if compact { 14.0 } else { 34.0 };
    let gap = if compact { 8.0 } else { 12.0 };
    let grid_y = if compact { 130.0 } else { 138.0 };
    let cell_w = ((screen_width() - margin * 2.0 - gap * (columns as f32 - 1.0)) / columns as f32).max(90.0);
    let cell_h = if compact { 48.0 } else { 54.0 };
    for (slot, index) in (start..end).enumerate() {
        let tag = &tags[index];
        let col = slot % columns;
        let row = slot / columns;
        let x = margin + col as f32 * (cell_w + gap);
        let y = grid_y + row as f32 * (cell_h + 9.0);
        let label = format!("{}  ({})", country_display_name(tag), tag);
        let chosen = selected_country.as_deref() == Some(tag.as_str());
        let (mx, my) = mouse_position();
        let hovered = mx >= x && mx <= x + cell_w && my >= y && my <= y + cell_h;
        let bg = if chosen { Color::from_rgba(111, 52, 46, 255) }
            else if hovered { Color::from_rgba(57, 92, 125, 255) }
            else { Color::from_rgba(25, 41, 56, 255) };
        draw_rectangle(x, y, cell_w, cell_h, bg);
        draw_rectangle_lines(x, y, cell_w, cell_h, if chosen { 2.0 } else { 1.0 },
            if chosen { Color::from_rgba(220, 184, 119, 255) } else { Color::from_rgba(76, 100, 121, 255) });
        draw_ui_text(&label, x + 9.0, y + cell_h * 0.62, if compact { 12.0 } else { 15.0 }, WHITE);
        if hovered && is_mouse_button_pressed(MouseButton::Left) { *selected_country = Some(tag.clone()); }
    }

    let controls_y = grid_y + rows as f32 * (cell_h + 9.0) + 2.0;
    if button("Previous", margin, controls_y, 105.0, 36.0) && *page > 0 { *page -= 1; }
    draw_ui_text(&format!("Page {} / {}", *page + 1, page_count), margin + 116.0, controls_y + 24.0, 13.0, LIGHTGRAY);
    if button("Next", screen_width() - margin - 105.0, controls_y, 105.0, 36.0) && *page + 1 < page_count { *page += 1; }

    let chosen_tag = selected_country.as_deref().unwrap_or("");
    if let Some(totals) = map.world.country_economy(chosen_tag) {
        draw_ui_text(&format!("{} ({})  ·  States {}  ·  CIV {}  ·  MIL {}  ·  Dockyards {}",
            country_display_name(chosen_tag), chosen_tag, totals.states, totals.civilian_factories,
            totals.military_factories, totals.dockyards),
            margin, controls_y + 49.0, if compact { 12.0 } else { 15.0 },
            Color::from_rgba(195, 210, 190, 255));
    }

    let start_y = screen_height() - if compact { 65.0 } else { 78.0 };
    let start_w = if compact { screen_width() - 28.0 } else { 250.0 };
    if button("Start game", (screen_width() - start_w) * 0.5, start_y, start_w, 48.0) {
        return Screen::MapViewer;
    }
    Screen::CountrySelect
}

fn map_view_rect(map: &ProvinceMap) -> Rect {
    let compact = screen_width() < 760.0;
    let left = if compact { 12.0 } else { 270.0 };
    let top = if compact { 210.0 } else { 82.0 };
    let available_w = (screen_width() - left - 22.0).max(1.0);
    // Keep a dedicated lower strip for the HOI4-style state/province inspector.
    let bottom_reserve = if compact { 270.0 } else { 260.0 };
    let available_h = (screen_height() - top - bottom_reserve).max(1.0);
    let scale = (available_w / map.width as f32).min(available_h / map.height as f32);
    let width = map.width as f32 * scale;
    let height = map.height as f32 * scale;
    Rect::new(
        left + (available_w - width) * 0.5,
        top + (available_h - height) * 0.5,
        width,
        height,
    )
}

fn draw_map_viewer(
    mut map: Option<&mut ProvinceMap>,
    load_error: &str,
    selected: &mut Option<(u32, [u8; 3], u32, u32)>,
    player_country: Option<&str>,
    construction_queue: &mut ConstructionQueues,
    construction_menu: &mut Option<u32>,
    building_rules: &BuildingRules,
) -> Screen {
    draw_background();
    draw_rectangle(0.0, 5.0, screen_width(), 58.0, Color::from_rgba(20, 32, 46, 255));
    let title = player_country.map(|tag| format!("WORLD MAP  ·  {} ({tag})", country_display_name(tag)))
        .unwrap_or_else(|| "WORLD MAP".to_owned());
    draw_ui_text(&title, 22.0, 42.0, if screen_width() < 760.0 { 17.0 } else { 25.0 }, WHITE);

    let compact = screen_width() < 760.0;
    if compact {
        if let Some(map) = map.as_deref_mut() {
            if button("+", screen_width() - 176.0, 15.0, 34.0, 38.0) {
                map.set_zoom_at(1.25, vec2(screen_width() * 0.5, screen_height() * 0.55), map_view_rect(map));
            }
            if button("−", screen_width() - 134.0, 15.0, 34.0, 38.0) {
                map.set_zoom_at(1.0 / 1.25, vec2(screen_width() * 0.5, screen_height() * 0.55), map_view_rect(map));
            }
        }
        if button("Back", screen_width() - 86.0, 15.0, 72.0, 38.0) {
            return Screen::MainMenu;
        }
    } else {
        if button("Main menu", screen_width() - 160.0, 15.0, 138.0, 38.0) {
            return Screen::MainMenu;
        }
        if let Some(map) = map.as_deref_mut() {
            if button("Zoom +", 22.0, screen_height() - 174.0, 96.0, 38.0) {
                map.set_zoom_at(1.25, vec2(screen_width() * 0.5, screen_height() * 0.55), map_view_rect(map));
            }
            if button("Zoom -", 132.0, screen_height() - 174.0, 100.0, 38.0) {
                map.set_zoom_at(1.0 / 1.25, vec2(screen_width() * 0.5, screen_height() * 0.55), map_view_rect(map));
            }
            if button("Fit map", 22.0, screen_height() - 220.0, 210.0, 38.0) {
                map.zoom = 1.0;
                map.target_zoom = 1.0;
                map.pan = Vec2::ZERO;
            }
        }
    }

    if let Some(map) = map.as_deref_mut() {
        let (_, wheel_y) = mouse_wheel();
        let mouse = vec2(mouse_position().0, mouse_position().1);
        let view = map_view_rect(map);
        if wheel_y != 0.0 && view.contains(mouse) {
            let wheel_step = wheel_y.clamp(-3.0, 3.0);
            // Small exponential steps: each wheel notch changes scale by only a few percent.
            map.set_zoom_at((wheel_step * 0.035).exp(), mouse, view);
        }
    }
    if compact {
        draw_rectangle(12.0, 74.0, screen_width() - 24.0, 116.0, Color::from_rgba(18, 30, 43, 255));
    } else {
        draw_rectangle(12.0, 78.0, 242.0, screen_height() - 92.0, Color::from_rgba(18, 30, 43, 255));
        draw_ui_text("MAP DATA", 28.0, 109.0, 18.0, WHITE);
    }

    match map.as_deref_mut() {
        Some(map) => {
            if compact {
                draw_ui_text(
                    &format!("{} × {}  |  {} provinces", map.width, map.height, map.province_count),
                    24.0,
                    104.0,
                    15.0,
                    LIGHTGRAY,
                );
                draw_ui_text(
                    &format!("States: {}  |  owners: {}", map.state_count, map.owner_by_state.len()),
                    24.0,
                    133.0,
                    13.0,
                    LIGHTGRAY,
                );
                draw_ui_text("Select a state to inspect it", 24.0, 160.0, 13.0, LIGHTGRAY);
            } else {
                draw_ui_text(&format!("Size: {} × {}", map.width, map.height), 28.0, 143.0, 17.0, LIGHTGRAY);
                draw_ui_text(&format!("Defined provinces: {}", map.province_count), 28.0, 170.0, 16.0, LIGHTGRAY);
                draw_ui_text("Source raster: loaded", 28.0, 202.0, 15.0, Color::from_rgba(130, 200, 150, 255));
                draw_ui_text(
                    &format!("States: {}  |  owners: {}", map.state_count, map.owner_by_state.len()),
                    28.0,
                    228.0,
                    14.0,
                    LIGHTGRAY,
                );
                draw_ui_text(
                    &format!("Cities: {}  |  Named: {}", map.city_markers.len(), map.city_named_count),
                    28.0,
                    450.0,
                    13.0,
                    LIGHTGRAY,
                );
                draw_ui_text(
                    &format!("Localisation .yml files: {}", map.localisation_file_count),
                    28.0,
                    470.0,
                    13.0,
                    LIGHTGRAY,
                );

                let status: String = map.state_load_status.chars().take(27).collect();
                draw_ui_text(&status, 28.0, 263.0, 12.0, LIGHTGRAY);
                draw_ui_text("Select a state on the map", 28.0, 290.0, 14.0, LIGHTGRAY);
            }

            // HOI4-style map-mode selector. The data-driven overlays are added in the next layer.
            let mode_y = if compact { 174.0 } else { 18.0 };
            let mode_x = if compact { 12.0 } else { 280.0 };
            let mode_w = if compact { ((screen_width() - 36.0) / 3.0).max(72.0) } else { 126.0 };
            for (index, mode) in [MapMode::Political, MapMode::Resources, MapMode::Terrain].into_iter().enumerate() {
                let x = mode_x + index as f32 * (mode_w + 8.0);
                let active = map.map_mode == mode;
                let color = if active {
                    Color::from_rgba(111, 52, 46, 255)
                } else {
                    Color::from_rgba(35, 54, 76, 255)
                };
                draw_rectangle(x, mode_y, mode_w, 34.0, color);
                draw_rectangle_lines(x, mode_y, mode_w, 34.0, 1.0, Color::from_rgba(91, 125, 154, 255));
                let label = match mode {
                    MapMode::Political => "Political",
                    MapMode::Resources => "Resources",
                    MapMode::Terrain => "Terrain",
                };
                let text_size = if compact { 14.0 } else { 17.0 };
                let text_w = measure_ui_text(label, text_size as u16).width;
                draw_ui_text(label, x + (mode_w - text_w) * 0.5, mode_y + 22.0, text_size, WHITE);
                let (mx, my) = mouse_position();
                if is_mouse_button_pressed(MouseButton::Left)
                    && mx >= x && mx <= x + mode_w && my >= mode_y && my <= mode_y + 34.0
                {
                    map.map_mode = mode;
                }
            }

            let rect = map_view_rect(map);
            draw_rectangle(rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0, Color::from_rgba(62, 78, 90, 255));
            let source_rect = map.source_rect();
            let mode_texture = match map.map_mode {
                MapMode::Political => &map.political_texture,
                // Resource mode keeps country colors and adds resource badges above them.
                MapMode::Resources => &map.resources_texture,
                MapMode::Terrain => &map.terrain_texture,
            };
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
            // Province, state and country borders share one texture to reduce GPU memory.
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
            // Show city markers from zoom 8.5; city names appear only at zoom 20.
            if map.zoom >= 8.5 {
                let label_scale = (map.zoom.sqrt() * 0.72).clamp(0.65, 1.25);
                // Text rendering is much more expensive than map sprites, especially on mobile GPUs.
                // Keep markers visible, but cap actual city-name labels per frame.
                const MAX_CITY_LABELS_PER_FRAME: usize = 72;
                let mut city_labels_drawn = 0usize;
                for city in &map.city_markers {
                    if city.x < source_rect.x || city.x > source_rect.x + source_rect.w
                        || city.y < source_rect.y || city.y > source_rect.y + source_rect.h { continue; }
                    let x = rect.x + (city.x - source_rect.x) / source_rect.w * rect.w;
                    let y = rect.y + (city.y - source_rect.y) / source_rect.h * rect.h;
                    draw_circle(x, y, 3.0 * label_scale, Color::from_rgba(245, 224, 168, 255));
                    draw_circle_lines(x, y, 4.3 * label_scale, 1.0, Color::from_rgba(25, 31, 38, 255));
                    if map.zoom >= 20.0
                        && city.display_name
                        && city_labels_drawn < MAX_CITY_LABELS_PER_FRAME
                    {
                        let name = city.name.trim();
                        let size = (10.0 * label_scale).clamp(9.0, 13.0);
                        let width_scale = size / 13.0;
                        let text_width = city.label_width * width_scale;
                        draw_rectangle(x + 6.0, y - size - 3.0, text_width + 7.0, size + 5.0, Color::from_rgba(15, 22, 29, 220));
                        draw_ui_text(name, x + 9.0, y - 2.0, size, WHITE);
                        city_labels_drawn += 1;
                    }
                }
            }
            if map.map_mode == MapMode::Resources {
                // HOI4-like resource badges are drawn in map space, so they follow pan and zoom.
                let scale = (map.zoom.sqrt() * 0.85).clamp(0.75, 1.35);
                for marker in &map.resource_markers {
                    if marker.x < source_rect.x || marker.x > source_rect.x + source_rect.w
                        || marker.y < source_rect.y || marker.y > source_rect.y + source_rect.h { continue; }
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
            if let Some((state_id, highlight_texture)) = &map.highlighted_state_texture {
                let _ = state_id;
                draw_texture_ex(
                    highlight_texture,
                    rect.x, rect.y, WHITE,
                    DrawTextureParams { source: Some(source_rect), dest_size: Some(vec2(rect.w, rect.h)), ..Default::default() },
                );
            }
            draw_ui_text(
                &format!("{} mode  |  zoom {:.1}x  |  drag to pan  |  wheel to zoom at cursor", map.map_mode.label(), map.zoom),
                rect.x,
                rect.y + rect.h - 8.0,
                14.0,
                LIGHTGRAY,
            );
        }
        None => {
            if compact {
                draw_ui_text("Map raster not loaded", 24.0, 104.0, 17.0, Color::from_rgba(236, 187, 126, 255));
                let short_error: String = load_error.chars().take(38).collect();
                draw_ui_text(&short_error, 24.0, 128.0, 12.0, Color::from_rgba(220, 155, 135, 255));
                draw_ui_text("Need provinces.bmp + definition.csv", 24.0, 158.0, 13.0, LIGHTGRAY);
            } else {
                draw_ui_text("Map raster not loaded", 28.0, 145.0, 18.0, Color::from_rgba(236, 187, 126, 255));
                let short_error: String = load_error.chars().take(29).collect();
                draw_ui_text(&short_error, 28.0, 166.0, 13.0, Color::from_rgba(220, 155, 135, 255));
                draw_ui_text("Expected local files:", 28.0, 204.0, 15.0, LIGHTGRAY);
                draw_ui_text("data/hoi4/map/", 28.0, 228.0, 15.0, WHITE);
                draw_ui_text("provinces.bmp", 28.0, 252.0, 15.0, WHITE);
                draw_ui_text("definition.csv", 28.0, 275.0, 15.0, WHITE);
                draw_ui_text("No game map assets are bundled.", 28.0, 320.0, 13.0, LIGHTGRAY);
            }

            let x = if compact { 12.0 } else { 280.0 };
            let y = if compact { 210.0 } else { 92.0 };
            let w = screen_width() - x - if compact { 12.0 } else { 22.0 };
            let h = screen_height() - y - 24.0;
            draw_rectangle(x, y, w, h, Color::from_rgba(17, 31, 43, 255));
            draw_rectangle_lines(x, y, w, h, 1.0, Color::from_rgba(48, 69, 84, 255));
            draw_ui_text("Import a local province raster to view the real map.", x + 20.0, y + h * 0.48, if compact { 16.0 } else { 21.0 }, WHITE);
            draw_ui_text("No fake province data is shown.", x + 20.0, y + h * 0.48 + 32.0, 14.0, LIGHTGRAY);
        }
    }

    // HOI4-inspired bottom state inspector. Closing it also clears the map selection.
    if let (Some(map), Some((province_id, _, _, _))) = (map.as_deref_mut(), *selected) {
        let panel_x = if compact { 12.0 } else { 270.0 };
        let panel_w = screen_width() - panel_x - 22.0;
        let panel_h = if compact { 242.0 } else { 232.0 };
        let panel_y = screen_height() - panel_h - 14.0;
        draw_rectangle(panel_x, panel_y, panel_w, panel_h, Color::from_rgba(19, 31, 43, 248));
        draw_rectangle_lines(panel_x, panel_y, panel_w, panel_h, 1.5, Color::from_rgba(105, 126, 142, 255));
        draw_rectangle(panel_x, panel_y, panel_w, 30.0, Color::from_rgba(39, 52, 65, 255));

        let state_id = map.state_by_province.get(&province_id).copied();
        let state_name = state_id
            .and_then(|id| map.world.states.get(&id))
            .and_then(|state| state.name.as_deref())
            .unwrap_or("Unknown state");
        let owner = map.world.owner_of_province(province_id).unwrap_or("—");
        let heading = if state_id.is_some() {
            format!("{state_name}  ·  {owner}")
        } else {
            format!("Province {province_id}  ·  State not mapped")
        };
        draw_ui_text(&heading, panel_x + 14.0, panel_y + 21.0, if compact { 15.0 } else { 18.0 }, WHITE);

        let close_x = panel_x + panel_w - 34.0;
        if button("X", close_x, panel_y + 3.0, 28.0, 24.0) {
            *selected = None;
            map.highlighted_state_texture = None;
        }

        let info_y = panel_y + 52.0;
        draw_ui_text(&format!("State ID: {}  ·  Province: {province_id}  ·  Owner: {owner}",
            state_id.map(|id| id.to_string()).unwrap_or_else(|| "—".to_owned())),
            panel_x + 14.0, info_y, if compact { 11.0 } else { 13.0 }, LIGHTGRAY);

        let table_x = panel_x + 14.0;
        let table_y = panel_y + 72.0;
        let table_w = (panel_w - 28.0).max(120.0);
        let selected_state = state_id.and_then(|id| map.world.states.get(&id));
        let economy = selected_state.map(|state| state.economy).unwrap_or_default();

        // Shared factory slots come from the state's actual state_category definition.
        // Infrastructure remains separate: it changes construction speed, not slot capacity.
        let built = usize::from(economy.occupied_shared_slots());
        let state_queue = state_id.and_then(|id| construction_queue.get(&id));
        let queued_count = state_queue.map_or(0, |queue| queue.len());
        let base_slots = selected_state.map(|state| usize::from(state.shared_building_slots)).unwrap_or(0);
        // Until technology/focus slot bonuses are simulated, never hide starting factories
        // if they exceed the base category value in a modded or modified save.
        let slot_count = base_slots.max(built).min(30);
        let free_slots = slot_count.saturating_sub(built + queued_count);
        draw_ui_text(
            &format!("SHARED SLOTS  ·  {built} built  ·  {queued_count} queued  ·  {free_slots} empty  ·  INFRA {}", economy.infrastructure),
            table_x, table_y + 10.0, if compact { 10.0 } else { 12.0 },
            Color::from_rgba(194, 205, 216, 255),
        );

        let grid_y = table_y + 19.0;
        let gap = if compact { 4.0 } else { 6.0 };
        let cell_w = ((table_w - gap * 14.0) / 15.0).clamp(18.0, 54.0);
        let cell_h = if compact { 43.0 } else { 50.0 };
        let total_grid_w = cell_w * 15.0 + gap * 14.0;
        let grid_x = table_x + (table_w - total_grid_w).max(0.0) * 0.5;
        let mut slot_types = Vec::with_capacity(slot_count);
        slot_types.extend(std::iter::repeat_n(("C", Color::from_rgba(224, 139, 45, 255)), usize::from(economy.civilian_factories)));
        slot_types.extend(std::iter::repeat_n(("M", Color::from_rgba(67, 157, 88, 255)), usize::from(economy.military_factories)));
        slot_types.extend(std::iter::repeat_n(("N", Color::from_rgba(80, 158, 133, 255)), usize::from(economy.dockyards)));
        slot_types.extend(std::iter::repeat_n(("S", Color::from_rgba(146, 111, 190, 255)), usize::from(economy.synthetic_refineries)));
        slot_types.extend(std::iter::repeat_n(("F", Color::from_rgba(185, 147, 65, 255)), usize::from(economy.fuel_silos)));
        slot_types.extend(std::iter::repeat_n(("R", Color::from_rgba(182, 93, 154, 255)), usize::from(economy.rocket_sites)));
        slot_types.extend(std::iter::repeat_n(("A", Color::from_rgba(105, 143, 185, 255)), usize::from(economy.nuclear_reactors)));
        if let Some(queue) = state_queue {
            for project in queue {
                let color = Color::from_rgba(112, 126, 139, 255);
                slot_types.push((project.building.glyph(), color));
            }
        }
        for index in 0..slot_count {
            let col = index % 15;
            let row = index / 15;
            let x = grid_x + col as f32 * (cell_w + gap);
            let y = grid_y + row as f32 * (cell_h + gap);
            let filled = slot_types.get(index);
            let bg = filled.map(|(_, color)| *color)
                .unwrap_or_else(|| Color::from_rgba(29, 43, 56, 255));
            draw_rectangle(x, y, cell_w, cell_h, bg);
            draw_rectangle_lines(x, y, cell_w, cell_h, 1.0,
                if filled.is_some() { Color::from_rgba(207, 216, 223, 190) }
                else { Color::from_rgba(79, 100, 117, 255) });

            // Small factory pictogram: roof, main building, and smokestack.
            if filled.is_some() {
                let icon_color = WHITE;
                let icon_w = (cell_w * 0.44).clamp(8.0, 19.0);
                let icon_h = (cell_h * 0.34).clamp(8.0, 16.0);
                let icon_x = x + (cell_w - icon_w) * 0.5;
                let icon_y = y + cell_h * 0.19;
                draw_rectangle(icon_x, icon_y + icon_h * 0.28, icon_w, icon_h * 0.72, icon_color);
                draw_rectangle(icon_x + icon_w * 0.12, icon_y, icon_w * 0.18, icon_h * 0.48, icon_color);
                draw_rectangle(icon_x + icon_w * 0.46, icon_y + icon_h * 0.56, icon_w * 0.12, icon_h * 0.18, bg);
                draw_rectangle(icon_x + icon_w * 0.70, icon_y + icon_h * 0.56, icon_w * 0.12, icon_h * 0.18, bg);
                let label = filled.map(|(glyph, _)| *glyph).unwrap_or("");
                let label_w = measure_ui_text(label, 9).width;
                draw_ui_text(label, x + (cell_w - label_w) * 0.5, y + cell_h - 5.0, 9.0, WHITE);
            } else {
                let plus_w = measure_ui_text("+", 16).width;
                draw_ui_text("+", x + (cell_w - plus_w) * 0.5, y + cell_h * 0.62, 16.0,
                    Color::from_rgba(111, 137, 155, 255));
                let (mx, my) = mouse_position();
                if is_mouse_button_pressed(MouseButton::Left)
                    && mx >= x && mx <= x + cell_w && my >= y && my <= y + cell_h
                {
                    *construction_menu = state_id;
                }
            }
        }

        let legend_y = grid_y + cell_h + gap + (if slot_count > 15 { cell_h + gap } else { 0.0 }) + 13.0;
        let legend_size = if compact { 9.0 } else { 11.0 };
        draw_ui_text(&format!("C Civ: {}  M Mil: {}  N Dock: {}  S Synth: {}  F Fuel: {}",
            economy.civilian_factories, economy.military_factories, economy.dockyards,
            economy.synthetic_refineries, economy.fuel_silos),
            table_x, legend_y, legend_size, LIGHTGRAY);
        draw_ui_text(&format!("R Rocket: {}  A Reactor: {}  INF: {}",
            economy.rocket_sites, economy.nuclear_reactors, economy.infrastructure),
            table_x, legend_y + if compact { 12.0 } else { 15.0 }, legend_size, LIGHTGRAY);

        if let Some(id) = state_id {
            if *construction_menu == Some(id) {
                let menu_y = panel_y + panel_h - 58.0;
                draw_rectangle(table_x, menu_y - 5.0, table_w, 55.0, Color::from_rgba(13, 22, 31, 250));
                let options = BuildingType::ALL;
                let columns = if compact { 4 } else { 7 };
                let rows = (options.len() + columns - 1) / columns;
                let option_gap = 4.0;
                let option_w = (table_w - option_gap * (columns as f32 - 1.0)) / columns as f32;
                for (index, building) in options.iter().enumerate() {
                    let col = index % columns;
                    let row = index / columns;
                    let bx = table_x + col as f32 * (option_w + option_gap);
                    let by = menu_y + row as f32 * 25.0;
                    if button(building.short_label(), bx, by, option_w, 22.0) {
                        let queue = construction_queue.entry(id).or_default();
                        if built + queue.len() < slot_count {
                            let level = selected_state
                                .map(|state| building.built_count(state.economy))
                                .unwrap_or(0)
                                .saturating_add(queue.iter().filter(|p| p.building == *building).count() as u16);
                            let controlled_count = player_country
                                .and_then(|tag| map.world.countries.get(tag))
                                .map(|country| country.state_ids.iter()
                                    .filter_map(|state_id| map.world.states.get(state_id))
                                    .map(|state| u32::from(building.built_count(state.economy)))
                                    .sum::<u32>())
                                .unwrap_or(0);
                            let infrastructure = selected_state.map(|state| state.economy.infrastructure).unwrap_or(0);
                            queue.push_back(building_rules.project(*building, level, controlled_count, infrastructure));
                        }
                        *construction_menu = None;
                    }
                }
                let _ = rows;
                if button("Cancel", panel_x + panel_w - 76.0, panel_y + 34.0, 62.0, 24.0) {
                    *construction_menu = None;
                }
            } else if queued_count > 0 {
                if let Some(project) = state_queue.and_then(|queue| queue.front()) {
                    draw_ui_text(
                        &format!(
                            "Queue {} · {} · {:.0}% ({:.0}/{:.0} IC)",
                            queued_count, project.building.label(),
                            project.progress_fraction() * 100.0,
                            project.progress, project.cost,
                        ),
                        table_x, panel_y + panel_h - 12.0, legend_size,
                        Color::from_rgba(220, 190, 125, 255),
                    );
                }
            }
        }
    }

    Screen::MapViewer
}

fn draw_settings() -> Screen {
    if screen_width() < 760.0 {
        draw_background();
        draw_ui_text("MAP DATA / IMPORT", 20.0, 70.0, 23.0, WHITE);
        draw_ui_text("Create this folder beside Cargo.toml:", 20.0, 119.0, 15.0, LIGHTGRAY);
        draw_ui_text("data/hoi4/map/", 20.0, 147.0, 17.0, WHITE);
        draw_ui_text("Add provinces.bmp and definition.csv.", 20.0, 181.0, 14.0, WHITE);
        draw_ui_text("Only use data you are allowed to use.", 20.0, 211.0, 14.0, LIGHTGRAY);
        draw_ui_text("Android file import is not ready yet.", 20.0, 247.0, 14.0, Color::from_rgba(236, 187, 126, 255));
        if button("Back to menu", 20.0, screen_height() - 82.0, 220.0, 50.0) {
            return Screen::MainMenu;
        }
        return Screen::Settings;
    }
    draw_background();
    draw_ui_text("MAP DATA / IMPORT GUIDE", 48.0, 86.0, 30.0, WHITE);
    draw_ui_text("The viewer reads files from a local folder; the repository does not include game assets.", 50.0, 132.0, 17.0, LIGHTGRAY);
    draw_ui_text("1. Create data/hoi4/map/ beside Cargo.toml.", 50.0, 185.0, 18.0, WHITE);
    draw_ui_text("2. Put your legally obtained provinces.bmp and definition.csv there.", 50.0, 218.0, 18.0, WHITE);
    draw_ui_text("3. Return to the map viewer; it loads the bitmap and resolves pixel colours to province IDs.", 50.0, 251.0, 18.0, WHITE);
    draw_ui_text("State files and state-boundary overlays are a later milestone.", 50.0, 284.0, 17.0, LIGHTGRAY);
    draw_ui_text("Android file picker / SAF import is not implemented yet.", 50.0, 322.0, 17.0, Color::from_rgba(236, 187, 126, 255));

    if button("Back to menu", 50.0, 380.0, 220.0, 52.0) {
        Screen::MainMenu
    } else {
        Screen::Settings
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    init_ui_font();
    let mut screen = Screen::MainMenu;
    let (mut map, mut load_error) = match ProvinceMap::load() {
        Ok(loaded) => (Some(loaded), String::new()),
        Err(error) => (None, error),
    };
    let mut selected: Option<(u32, [u8; 3], u32, u32)> = None;
    let mut selected_country: Option<String> = None;
    let mut country_page = 0usize;
    let mut game = GameState::default();
    let mut construction_queue: ConstructionQueues = ConstructionQueues::new();
    let building_rules = BuildingRules::load(construction::BUILDING_DEFINITIONS_DIRECTORY);
    let mut construction_menu: Option<u32> = None;
    // Rebuild the date label only when the simulation date changes, not every rendered frame.
    let mut cached_date = game.date;
    let mut cached_date_label = game.date.display();

    loop {
        let next_screen = match screen {
            Screen::MainMenu => draw_main_menu(),
            Screen::CountrySelect => draw_country_select(map.as_ref(), &mut selected_country, &mut country_page),
            Screen::MapViewer => {
                // Optional desktop keyboard shortcuts; touch buttons below are the primary controls.
                if is_key_pressed(KeyCode::Space) {
                    game.toggle_pause();
                }
                for (key, speed) in [
                    (KeyCode::Key1, 1),
                    (KeyCode::Key2, 2),
                    (KeyCode::Key3, 3),
                    (KeyCode::Key4, 4),
                    (KeyCode::Key5, 5),
                ] {
                    if is_key_pressed(key) {
                        game.set_speed(speed);
                    }
                }
                let daily_ticks = game.update(get_frame_time());
                if daily_ticks > 0 {
                    if let (Some(tag), Some(current_map)) = (selected_country.as_deref(), map.as_mut()) {
                        for _ in 0..daily_ticks {
                            construction::advance_day(&mut current_map.world, tag, &mut construction_queue, &building_rules);
                        }
                    }
                }
                if game.date != cached_date {
                    cached_date = game.date;
                    cached_date_label = game.date.display();
                }

                let rendered_screen = draw_map_viewer(
                    map.as_mut(), &load_error, &mut selected, selected_country.as_deref(),
                    &mut construction_queue, &mut construction_menu, &building_rules,
                );
                let (pause_clicked, speed_clicked) = draw_time_controls(&game, &cached_date_label);
                if pause_clicked {
                    game.toggle_pause();
                }
                if let Some(speed) = speed_clicked {
                    game.set_speed(speed);
                    // As in HOI4, choosing a speed starts the simulation; pause remains
                    // available as a separate control.
                    game.paused = false;
                }
                if let Some(current_map) = map.as_mut() {
                    let (mouse_x, mouse_y) = mouse_position();
                    let mouse = vec2(mouse_x, mouse_y);
                    let rect = map_view_rect(current_map);
                    let time_controls_overlay = screen_width() >= 760.0
                        && mouse.x >= screen_width() - 405.0
                        && mouse.y >= 70.0
                        && mouse.y <= 124.0;
                    let inside_map = rect.contains(mouse) && !time_controls_overlay;

                    if is_mouse_button_pressed(MouseButton::Left) && inside_map {
                        current_map.dragging = true;
                        current_map.drag_moved = false;
                        current_map.last_mouse = mouse;
                    }

                    if is_mouse_button_down(MouseButton::Left) && current_map.dragging {
                        let delta = mouse - current_map.last_mouse;
                        if delta.length() > 1.5 {
                            current_map.drag_moved = true;
                        }
                        if current_map.drag_moved {
                            let source = current_map.source_rect();
                            current_map.pan.x -= delta.x / rect.w * source.w;
                            current_map.pan.y -= delta.y / rect.h * source.h;
                            current_map.clamp_pan();
                        }
                    }

                    if is_mouse_button_released(MouseButton::Left) && current_map.dragging {
                        if !current_map.drag_moved && inside_map {
                            let source = current_map.source_rect();
                            let pixel_x = (source.x + ((mouse_x - rect.x) / rect.w) * source.w)
                                .floor() as u32;
                            let pixel_y = (source.y + ((mouse_y - rect.y) / rect.h) * source.h)
                                .floor() as u32;
                            selected = current_map
                                .province_at(
                                    pixel_x.min(current_map.width - 1),
                                    pixel_y.min(current_map.height - 1),
                                )
                                .map(|(id, rgb)| (id, rgb, pixel_x, pixel_y));
                            let selected_state_id = selected.and_then(|(province_id, _, _, _)| current_map.state_by_province.get(&province_id).copied());
                            current_map.highlighted_state_texture = selected_state_id.map(|state_id| {
                                (state_id, build_highlighted_state_texture(current_map.width, current_map.height, &current_map.state_ids_by_pixel, state_id))
                            });
                        }
                        current_map.dragging = false;
                    }

                    if current_map.dragging {
                        current_map.last_mouse = mouse;
                    }

                    // Frame-rate-independent easing keeps zoom smooth rather than snapping.
                    let zoom_delta = current_map.target_zoom - current_map.zoom;
                    if zoom_delta.abs() < 0.0005 {
                        current_map.zoom = current_map.target_zoom;
                    } else {
                        let easing = 1.0 - (-12.0 * get_frame_time()).exp();
                        current_map.zoom += zoom_delta * easing;
                    }
                    current_map.clamp_pan();
                }

                let compact_reload = screen_width() < 760.0;
                let reload_clicked = if compact_reload {
                    button("Reload map", 12.0, screen_height() - 108.0, 130.0, 34.0)
                } else {
                    button("Reload local map", 22.0, screen_height() - 72.0, 210.0, 42.0)
                };
                if reload_clicked {
                    match ProvinceMap::load() {
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

        screen = next_screen;
        next_frame().await;
    }
}
