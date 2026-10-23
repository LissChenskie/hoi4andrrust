mod state;

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
    texture: Texture2D,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    id_by_rgb: HashMap<[u8; 3], u32>,
    province_count: usize,
    state_by_province: HashMap<u32, u32>,
    state_count: usize,
    state_load_status: String,
    state_border_texture: Option<Texture2D>,
    political_texture: Texture2D,
    resources_texture: Texture2D,
    terrain_texture: Texture2D,
    resource_markers: Vec<ResourceMarker>,
    city_markers: Vec<CityMarker>,
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
        let texture = Texture2D::from_rgba8(width as u16, height as u16, &rgba);
        texture.set_filter(FilterMode::Linear);

        let definition_text = fs::read_to_string(definition_path)
            .map_err(|error| format!("Could not read definition.csv: {error}"))?;
        let id_by_rgb = parse_definition_text(&definition_text)?;

        let (state_by_province, state_count, state_load_status) =
            match load_state_membership(Path::new(STATE_DIRECTORY_PATH)) {
                Ok(membership) => (
                    membership.state_by_province,
                    membership.state_count,
                    "State membership loaded".to_owned(),
                ),
                Err(error) => (HashMap::new(), 0, error),
            };

        let state_border_texture = if state_count > 0 {
            Some(build_state_border_texture(
                width,
                height,
                &rgba,
                &id_by_rgb,
                &state_by_province,
            ))
        } else {
            None
        };

        let (political_texture, resources_texture, terrain_texture) = build_map_mode_textures(
            width,
            height,
            &rgba,
            &id_by_rgb,
            &state_by_province,
            &definition_text,
            Path::new(STATE_DIRECTORY_PATH),
        );
        let resource_markers = build_resource_markers(
            width, height, &rgba, &id_by_rgb, &state_by_province, Path::new(STATE_DIRECTORY_PATH),
        );
        let city_markers = build_city_markers(width, height, &rgba, &id_by_rgb, Path::new(STATE_DIRECTORY_PATH));

        Ok(Self {
            texture,
            width,
            height,
            rgba,
            province_count: id_by_rgb.len(),
            id_by_rgb,
            state_by_province,
            state_count,
            state_load_status,
            state_border_texture,
            political_texture,
            resources_texture,
            terrain_texture,
            resource_markers,
            city_markers,
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

    fn set_zoom(&mut self, factor: f32) {
        self.target_zoom = (self.target_zoom * factor).clamp(1.0, 12.0);
        self.clamp_pan();
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

        let offset = ((y as usize * self.width as usize) + x as usize) * 4;
        let rgb = [self.rgba[offset], self.rgba[offset + 1], self.rgba[offset + 2]];
        self.id_by_rgb.get(&rgb).copied().map(|id| (id, rgb))
    }
}


fn build_state_border_texture(
    width: u32,
    height: u32,
    rgba: &[u8],
    id_by_rgb: &HashMap<[u8; 3], u32>,
    state_by_province: &HashMap<u32, u32>,
) -> Texture2D {
    let pixel_count = width as usize * height as usize;
    let mut state_ids = vec![0u32; pixel_count];

    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        if let Some(province_id) = id_by_rgb.get(&rgb) {
            state_ids[index] = state_by_province.get(province_id).copied().unwrap_or(0);
        }
    }

    let mut overlay = vec![0u8; pixel_count * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let index = y * width as usize + x;
            let state_id = state_ids[index];
            if state_id == 0 {
                continue;
            }

            let differs = (x > 0 && state_ids[index - 1] != state_id)
                || (x + 1 < width as usize && state_ids[index + 1] != state_id)
                || (y > 0 && state_ids[index - width as usize] != state_id)
                || (y + 1 < height as usize && state_ids[index + width as usize] != state_id);
            if differs {
                let offset = index * 4;
                overlay[offset] = 235;
                overlay[offset + 1] = 198;
                overlay[offset + 2] = 120;
                overlay[offset + 3] = 230;
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
    let mut owners = HashMap::new();
    let mut resources = HashMap::new();
    let Ok(entries) = fs::read_dir(directory) else { return (owners, resources) };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") { continue; }
        let Ok(source) = fs::read_to_string(path) else { continue };
        let mut state_id = None;
        let mut in_history = false;
        let mut history_depth = 0i32;
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
            if clean.starts_with("history") && clean.contains('{') {
                in_history = true;
                history_depth = clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if let Some((_, tail)) = clean.split_once('{') {
                    if let Some((_, owner)) = tail.split_once("owner") {
                        if let Some((_, tag)) = owner.split_once('=') {
                            let tag = tag.trim().trim_matches(|c: char| c == '}' || c.is_whitespace() || c == '"');
                            let tag = tag.split_whitespace().next().unwrap_or("").trim_matches('}');
                            if !tag.is_empty() { owners.insert(id, tag.to_owned()); }
                        }
                    }
                }
            } else if in_history {
                // Owner may be nested inside a dated history block, e.g.
                // 1936.1.1 = { owner = TAG }. Do not restrict it to line start.
                if let Some((_, assignment)) = clean.split_once("owner") {
                    if let Some((_, raw_tag)) = assignment.split_once('=') {
                        let tag = raw_tag.trim().trim_matches(|c: char| c == '}' || c.is_whitespace() || c == '"');
                        let tag = tag.split_whitespace().next().unwrap_or("").trim_matches('}');
                        if !tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                            owners.insert(id, tag.to_owned());
                        }
                    }
                }
                history_depth += clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
                if history_depth <= 0 { in_history = false; }
            }
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
) -> (Texture2D, Texture2D, Texture2D) {
    let (owner_by_state, resources_by_state) = parse_state_visual_data(state_directory);
    let mut terrain_by_province = HashMap::<u32, String>::new();
    let delimiter = definition_text.lines().find(|l| !l.trim().is_empty())
        .map(|l| if l.contains(';') { ';' } else { ',' }).unwrap_or(';');
    for line in definition_text.lines().skip(1) {
        let fields: Vec<&str> = line.split(delimiter).collect();
        if fields.len() < 7 { continue; }
        if let Ok(id) = fields[0].trim().trim_start_matches('\u{feff}').parse::<u32>() {
            terrain_by_province.insert(id, fields[6].trim().to_owned());
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
        // Political mode keeps terrain details visible under a translucent national tint.
        // States with no parsed owner remain terrain-coloured instead of becoming grey blocks.
        let political_color = owner_color
            .map(|country| blend_country(base_terrain, country, 0.48))
            .unwrap_or(base_terrain);
        political[offset..offset + 3].copy_from_slice(&political_color);
        political[offset + 3] = 255;

        // Terrain mode is terrain-first; do not tint it with country colors.
        terrain[offset..offset + 3].copy_from_slice(&base_terrain);
        terrain[offset + 3] = 255;

        let amount = resources_by_state.get(&state_id).copied().unwrap_or(0.0);
        let strength = (amount / max_resources).clamp(0.0, 1.0);
        // Dark neutral map with a clear low-to-high resource heat ramp (not all-green).
        let resource_color = [
            (44.0 + strength * 190.0) as u8,
            (49.0 + (1.0 - strength) * 65.0) as u8,
            (58.0 + (1.0 - strength) * 40.0) as u8,
        ];
        resources[offset..offset + 3].copy_from_slice(&resource_color);
        resources[offset + 3] = 255;
    }
    let make_texture = |bytes: &[u8]| {
        let texture = Texture2D::from_rgba8(width as u16, height as u16, bytes);
        texture.set_filter(FilterMode::Linear);
        texture
    };
    (make_texture(&political), make_texture(&resources), make_texture(&terrain))
}


fn build_resource_markers(
    width: u32,
    height: u32,
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
    height: u32,
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
        markers.push(CityMarker {
            province_id,
            victory_points: points[&province_id],
            x: (sx / count as f64) as f32,
            y: (sy / count as f64) as f32,
            name: names.get(&province_id).cloned().unwrap_or_default(),
        });
    }
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

fn load_victory_point_names() -> HashMap<u32, String> {
    let mut names = HashMap::new();
    let roots = [
        Path::new("data/hoi4/localisation"),
        Path::new("data/hoi4/localisation/english"),
    ];
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let Ok(files) = fs::read_dir(path) else { continue };
                for file in files.flatten() {
                    let p = file.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("yml") {
                        if let Ok(source) = fs::read_to_string(p) { parse_vp_localisation(&source, &mut names); }
                    }
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("yml") {
                if let Ok(source) = fs::read_to_string(path) { parse_vp_localisation(&source, &mut names); }
            }
        }
    }
    names
}

fn parse_vp_localisation(source: &str, names: &mut HashMap<u32, String>) {
    for line in source.lines() {
        let Some((key, value)) = line.trim().split_once(':') else { continue };
        let key = key.trim().trim_start_matches('\u{feff}');
        let Some(raw_id) = key.strip_prefix("VICTORY_POINTS_") else { continue };
        let Ok(id) = raw_id.parse::<u32>() else { continue };
        let mut value = value.trim();
        // HOI4 localisation rows commonly use KEY:0 \"Name\".
        if value.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            if let Some((_, rest)) = value.split_once(' ') { value = rest.trim(); }
        }
        let value = value.trim_matches('"');
        if !value.is_empty() { names.entry(id).or_insert_with(|| value.to_owned()); }
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
    if button("Open map viewer", button_x, 220.0, button_w, 54.0) {
        return Screen::MapViewer;
    }
    if button("Map data / import guide", button_x, 290.0, button_w, 54.0) {
        return Screen::Settings;
    }

    draw_ui_text("Prototype milestone 01: menu + local map raster viewer", 75.0, height - 42.0, 16.0, Color::from_rgba(125, 145, 163, 255));
    Screen::MainMenu
}


fn draw_main_menu_compact() -> Screen {
    draw_background();
    draw_ui_text("HOI4 MOBILE", 22.0, 82.0, 36.0, WHITE);
    draw_ui_text("Independent Rust strategy prototype", 24.0, 111.0, 15.0, LIGHTGRAY);

    let button_x = 20.0;
    let button_w = (screen_width() - 40.0).max(180.0);
    let first_y = (screen_height() * 0.40).max(190.0);
    if button("Open map viewer", button_x, first_y, button_w, 54.0) {
        return Screen::MapViewer;
    }
    if button("Map data / import guide", button_x, first_y + 68.0, button_w, 54.0) {
        return Screen::Settings;
    }
    draw_ui_text("Prototype milestone 01", 22.0, screen_height() - 28.0, 14.0, LIGHTGRAY);
    Screen::MainMenu
}

fn map_view_rect(map: &ProvinceMap) -> Rect {
    let compact = screen_width() < 760.0;
    let left = if compact { 12.0 } else { 270.0 };
    let top = if compact { 210.0 } else { 82.0 };
    let available_w = (screen_width() - left - 22.0).max(1.0);
    let bottom_reserve = if compact { 90.0 } else { 24.0 };
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
    selected: Option<(u32, [u8; 3], u32, u32)>,
) -> Screen {
    draw_background();
    draw_rectangle(0.0, 5.0, screen_width(), 58.0, Color::from_rgba(20, 32, 46, 255));
    draw_ui_text("WORLD MAP", 22.0, 42.0, 25.0, WHITE);

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

    match map {
        Some(map) => {
            if compact {
                draw_ui_text(
                    &format!("{} × {}  |  {} provinces", map.width, map.height, map.province_count),
                    24.0,
                    104.0,
                    15.0,
                    LIGHTGRAY,
                );
                if let Some((id, rgb, x, y)) = selected {
                    draw_ui_text(
                        &format!("Province {id} at {x},{y}  RGB {},{},{}", rgb[0], rgb[1], rgb[2]),
                        24.0,
                        133.0,
                        13.0,
                        WHITE,
                    );
                    let state_label = map
                        .state_by_province
                        .get(&id)
                        .map(|state_id| format!("State ID: {state_id}"))
                        .unwrap_or_else(|| "State ID: not mapped".to_owned());
                    draw_ui_text(&state_label, 24.0, 160.0, 13.0, LIGHTGRAY);
                } else {
                    draw_ui_text("Tap a province to inspect its ID and colour", 24.0, 133.0, 13.0, LIGHTGRAY);
                    draw_ui_text(&format!("States loaded: {}", map.state_count), 24.0, 160.0, 13.0, LIGHTGRAY);
                }
            } else {
                draw_ui_text(&format!("Size: {} × {}", map.width, map.height), 28.0, 143.0, 17.0, LIGHTGRAY);
                draw_ui_text(&format!("Defined provinces: {}", map.province_count), 28.0, 170.0, 16.0, LIGHTGRAY);
                draw_ui_text("Source raster: loaded", 28.0, 202.0, 15.0, Color::from_rgba(130, 200, 150, 255));
                draw_ui_text(
                    &format!("States loaded: {}", map.state_count),
                    28.0,
                    228.0,
                    14.0,
                    LIGHTGRAY,
                );

                if let Some((id, rgb, x, y)) = selected {
                    draw_rectangle(26.0, 248.0, 210.0, 154.0, Color::from_rgba(27, 44, 59, 255));
                    draw_ui_text("SELECTED PIXEL", 38.0, 273.0, 15.0, WHITE);
                    draw_ui_text(&format!("Province ID: {id}"), 38.0, 300.0, 17.0, WHITE);
                    draw_ui_text(&format!("Pixel: {x}, {y}"), 38.0, 327.0, 15.0, LIGHTGRAY);
                    draw_ui_text(&format!("RGB: {}, {}, {}", rgb[0], rgb[1], rgb[2]), 38.0, 352.0, 14.0, LIGHTGRAY);
                    let state_label = map
                        .state_by_province
                        .get(&id)
                        .map(|state_id| format!("State ID: {state_id}"))
                        .unwrap_or_else(|| "State ID: not mapped".to_owned());
                    draw_ui_text(&state_label, 38.0, 380.0, 14.0, LIGHTGRAY);
                } else {
                    draw_ui_text("Tap a province to", 28.0, 263.0, 16.0, LIGHTGRAY);
                    draw_ui_text("inspect its ID and RGB.", 28.0, 286.0, 16.0, LIGHTGRAY);
                    let status: String = map.state_load_status.chars().take(27).collect();
                    draw_ui_text(&status, 28.0, 320.0, 12.0, LIGHTGRAY);
                }
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
                MapMode::Resources => &map.political_texture,
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
            if map.zoom >= 1.15 {
                let label_scale = (map.zoom.sqrt() * 0.78).clamp(0.65, 1.35);
                for city in &map.city_markers {
                    if city.x < source_rect.x || city.x > source_rect.x + source_rect.w
                        || city.y < source_rect.y || city.y > source_rect.y + source_rect.h { continue; }
                    let x = rect.x + (city.x - source_rect.x) / source_rect.w * rect.w;
                    let y = rect.y + (city.y - source_rect.y) / source_rect.h * rect.h;
                    draw_circle(x, y, 3.0 * label_scale, Color::from_rgba(245, 224, 168, 255));
                    draw_circle_lines(x, y, 4.3 * label_scale, 1.0, Color::from_rgba(25, 31, 38, 255));
                    if map.zoom >= 1.8 {
                        let size = (11.0 * label_scale).clamp(9.0, 15.0);
                        let label = if city.name.is_empty() { format!("VP {}", city.victory_points) } else { city.name.clone() };
                        let tw = measure_ui_text(&label, size as u16).width;
                        draw_rectangle(x + 6.0, y - size - 3.0, tw + 7.0, size + 5.0, Color::from_rgba(15, 22, 29, 220));
                        draw_ui_text(&label, x + 9.0, y - 2.0, size, WHITE);
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
            if let Some(state_texture) = &map.state_border_texture {
                draw_texture_ex(
                    state_texture,
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
            draw_ui_text(
                &format!("{} mode  |  zoom {:.1}x  |  drag to pan  |  wheel to zoom at cursor", map.map_mode.label(), map.zoom),
                rect.x,
                screen_height() - 12.0,
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

    loop {
        let next_screen = match screen {
            Screen::MainMenu => draw_main_menu(),
            Screen::MapViewer => {
                let rendered_screen = draw_map_viewer(map.as_mut(), &load_error, selected);
                if let Some(current_map) = map.as_mut() {
                    let (mouse_x, mouse_y) = mouse_position();
                    let mouse = vec2(mouse_x, mouse_y);
                    let rect = map_view_rect(current_map);
                    let inside_map = rect.contains(mouse);

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

                if button("Reload local map", 22.0, screen_height() - 72.0, 210.0, 42.0) {
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
