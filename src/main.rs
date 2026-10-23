mod state;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use image::GenericImageView;
use macroquad::prelude::*;
use state::load_state_membership;

const MAP_BITMAP_PATH: &str = "data/hoi4/map/provinces.bmp";
const MAP_DEFINITION_PATH: &str = "data/hoi4/map/definition.csv";
const STATE_DIRECTORY_PATH: &str = "data/hoi4/history/states";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    MainMenu,
    MapViewer,
    Settings,
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
    zoom: f32,
    pan: Vec2,
    last_mouse: Vec2,
    dragging: bool,
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
            zoom: 1.0,
            pan: Vec2::ZERO,
            last_mouse: Vec2::ZERO,
            dragging: false,
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
        self.zoom = (self.zoom * factor).clamp(1.0, 12.0);
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
    texture.set_filter(FilterMode::Nearest);
    texture
}

/// Parse HOI4 definition.csv into an RGB -> province ID index.
/// Vanilla HOI4 files are semicolon-delimited; comma-delimited input is also accepted.
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
    draw_text(label, x + 20.0, y + height * 0.63, 23.0, WHITE);

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
    draw_text("HOI4 MOBILE", 72.0, 110.0, 48.0, WHITE);
    draw_text("Independent Rust strategy prototype", 75.0, 143.0, 19.0, Color::from_rgba(160, 177, 195, 255));

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
    draw_text("MAP PREVIEW", panel_x + 20.0, panel_y + panel_h - 24.0, 16.0, Color::from_rgba(160, 177, 195, 255));

    let button_x = 74.0;
    let button_w = (width * 0.34).clamp(250.0, 360.0);
    if button("Open map viewer", button_x, 220.0, button_w, 54.0) {
        return Screen::MapViewer;
    }
    if button("Map data / import guide", button_x, 290.0, button_w, 54.0) {
        return Screen::Settings;
    }

    draw_text("Prototype milestone 01: menu + local map raster viewer", 75.0, height - 42.0, 16.0, Color::from_rgba(125, 145, 163, 255));
    Screen::MainMenu
}


fn draw_main_menu_compact() -> Screen {
    draw_background();
    draw_text("HOI4 MOBILE", 22.0, 82.0, 36.0, WHITE);
    draw_text("Independent Rust strategy prototype", 24.0, 111.0, 15.0, LIGHTGRAY);

    let button_x = 20.0;
    let button_w = (screen_width() - 40.0).max(180.0);
    let first_y = (screen_height() * 0.40).max(190.0);
    if button("Open map viewer", button_x, first_y, button_w, 54.0) {
        return Screen::MapViewer;
    }
    if button("Map data / import guide", button_x, first_y + 68.0, button_w, 54.0) {
        return Screen::Settings;
    }
    draw_text("Prototype milestone 01", 22.0, screen_height() - 28.0, 14.0, LIGHTGRAY);
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
    draw_text("WORLD MAP", 22.0, 42.0, 25.0, WHITE);

    let compact = screen_width() < 760.0;
    if compact {
        if let Some(map) = map.as_deref_mut() {
            if button("+", screen_width() - 176.0, 15.0, 34.0, 38.0) {
                map.set_zoom(1.25);
            }
            if button("−", screen_width() - 134.0, 15.0, 34.0, 38.0) {
                map.set_zoom(1.0 / 1.25);
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
                map.zoom = (map.zoom * 1.25).clamp(1.0, 12.0);
            }
            if button("Zoom -", 132.0, screen_height() - 174.0, 100.0, 38.0) {
                map.zoom = (map.zoom / 1.25).clamp(1.0, 12.0);
            }
            if button("Fit map", 22.0, screen_height() - 220.0, 210.0, 38.0) {
                map.zoom = 1.0;
                map.pan = Vec2::ZERO;
            }
        }
    }

    if let Some(map) = map.as_deref_mut() {
        let (_, wheel_y) = mouse_wheel();
        let mouse = vec2(mouse_position().0, mouse_position().1);
        if wheel_y != 0.0 && map_view_rect(map).contains(mouse) {
            map.set_zoom(1.12_f32.powf(wheel_y));
        }
    }
    if compact {
        draw_rectangle(12.0, 74.0, screen_width() - 24.0, 116.0, Color::from_rgba(18, 30, 43, 255));
    } else {
        draw_rectangle(12.0, 78.0, 242.0, screen_height() - 92.0, Color::from_rgba(18, 30, 43, 255));
        draw_text("MAP DATA", 28.0, 109.0, 18.0, WHITE);
    }

    match map {
        Some(map) => {
            if compact {
                draw_text(
                    &format!("{} × {}  |  {} provinces", map.width, map.height, map.province_count),
                    24.0,
                    104.0,
                    15.0,
                    LIGHTGRAY,
                );
                if let Some((id, rgb, x, y)) = selected {
                    draw_text(
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
                    draw_text(&state_label, 24.0, 160.0, 13.0, LIGHTGRAY);
                } else {
                    draw_text("Tap a province to inspect its ID and colour", 24.0, 133.0, 13.0, LIGHTGRAY);
                    draw_text(&format!("States loaded: {}", map.state_count), 24.0, 160.0, 13.0, LIGHTGRAY);
                }
            } else {
                draw_text(&format!("Size: {} × {}", map.width, map.height), 28.0, 143.0, 17.0, LIGHTGRAY);
                draw_text(&format!("Defined provinces: {}", map.province_count), 28.0, 170.0, 16.0, LIGHTGRAY);
                draw_text("Source raster: loaded", 28.0, 202.0, 15.0, Color::from_rgba(130, 200, 150, 255));
                draw_text(
                    &format!("States loaded: {}", map.state_count),
                    28.0,
                    228.0,
                    14.0,
                    LIGHTGRAY,
                );

                if let Some((id, rgb, x, y)) = selected {
                    draw_rectangle(26.0, 248.0, 210.0, 154.0, Color::from_rgba(27, 44, 59, 255));
                    draw_text("SELECTED PIXEL", 38.0, 273.0, 15.0, WHITE);
                    draw_text(&format!("Province ID: {id}"), 38.0, 300.0, 17.0, WHITE);
                    draw_text(&format!("Pixel: {x}, {y}"), 38.0, 327.0, 15.0, LIGHTGRAY);
                    draw_text(&format!("RGB: {}, {}, {}", rgb[0], rgb[1], rgb[2]), 38.0, 352.0, 14.0, LIGHTGRAY);
                    let state_label = map
                        .state_by_province
                        .get(&id)
                        .map(|state_id| format!("State ID: {state_id}"))
                        .unwrap_or_else(|| "State ID: not mapped".to_owned());
                    draw_text(&state_label, 38.0, 380.0, 14.0, LIGHTGRAY);
                } else {
                    draw_text("Tap a province to", 28.0, 263.0, 16.0, LIGHTGRAY);
                    draw_text("inspect its ID and RGB.", 28.0, 286.0, 16.0, LIGHTGRAY);
                    let status: String = map.state_load_status.chars().take(27).collect();
                    draw_text(&status, 28.0, 320.0, 12.0, LIGHTGRAY);
                }
            }

            let rect = map_view_rect(map);
            draw_rectangle(rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0, Color::from_rgba(62, 78, 90, 255));
            let source_rect = map.source_rect();
            draw_texture_ex(
                &map.texture,
                rect.x,
                rect.y,
                WHITE,
                DrawTextureParams {
                    source: Some(source_rect),
                    dest_size: Some(vec2(rect.w, rect.h)),
                    ..Default::default()
                },
            );
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
            draw_text("Nearest-neighbour raster view", rect.x, screen_height() - 12.0, 14.0, LIGHTGRAY);
        }
        None => {
            if compact {
                draw_text("Map raster not loaded", 24.0, 104.0, 17.0, Color::from_rgba(236, 187, 126, 255));
                let short_error: String = load_error.chars().take(38).collect();
                draw_text(&short_error, 24.0, 128.0, 12.0, Color::from_rgba(220, 155, 135, 255));
                draw_text("Need provinces.bmp + definition.csv", 24.0, 158.0, 13.0, LIGHTGRAY);
            } else {
                draw_text("Map raster not loaded", 28.0, 145.0, 18.0, Color::from_rgba(236, 187, 126, 255));
                let short_error: String = load_error.chars().take(29).collect();
                draw_text(&short_error, 28.0, 166.0, 13.0, Color::from_rgba(220, 155, 135, 255));
                draw_text("Expected local files:", 28.0, 204.0, 15.0, LIGHTGRAY);
                draw_text("data/hoi4/map/", 28.0, 228.0, 15.0, WHITE);
                draw_text("provinces.bmp", 28.0, 252.0, 15.0, WHITE);
                draw_text("definition.csv", 28.0, 275.0, 15.0, WHITE);
                draw_text("No game map assets are bundled.", 28.0, 320.0, 13.0, LIGHTGRAY);
            }

            let x = if compact { 12.0 } else { 280.0 };
            let y = if compact { 210.0 } else { 92.0 };
            let w = screen_width() - x - if compact { 12.0 } else { 22.0 };
            let h = screen_height() - y - 24.0;
            draw_rectangle(x, y, w, h, Color::from_rgba(17, 31, 43, 255));
            draw_rectangle_lines(x, y, w, h, 1.0, Color::from_rgba(48, 69, 84, 255));
            draw_text("Import a local province raster to view the real map.", x + 20.0, y + h * 0.48, if compact { 16.0 } else { 21.0 }, WHITE);
            draw_text("No fake province data is shown.", x + 20.0, y + h * 0.48 + 32.0, 14.0, LIGHTGRAY);
        }
    }

    Screen::MapViewer
}

fn draw_settings() -> Screen {
    if screen_width() < 760.0 {
        draw_background();
        draw_text("MAP DATA / IMPORT", 20.0, 70.0, 23.0, WHITE);
        draw_text("Create this folder beside Cargo.toml:", 20.0, 119.0, 15.0, LIGHTGRAY);
        draw_text("data/hoi4/map/", 20.0, 147.0, 17.0, WHITE);
        draw_text("Add provinces.bmp and definition.csv.", 20.0, 181.0, 14.0, WHITE);
        draw_text("Only use data you are allowed to use.", 20.0, 211.0, 14.0, LIGHTGRAY);
        draw_text("Android file import is not ready yet.", 20.0, 247.0, 14.0, Color::from_rgba(236, 187, 126, 255));
        if button("Back to menu", 20.0, screen_height() - 82.0, 220.0, 50.0) {
            return Screen::MainMenu;
        }
        return Screen::Settings;
    }
    draw_background();
    draw_text("MAP DATA / IMPORT GUIDE", 48.0, 86.0, 30.0, WHITE);
    draw_text("The viewer reads files from a local folder; the repository does not include game assets.", 50.0, 132.0, 17.0, LIGHTGRAY);
    draw_text("1. Create data/hoi4/map/ beside Cargo.toml.", 50.0, 185.0, 18.0, WHITE);
    draw_text("2. Put your legally obtained provinces.bmp and definition.csv there.", 50.0, 218.0, 18.0, WHITE);
    draw_text("3. Return to the map viewer; it loads the bitmap and resolves pixel colours to province IDs.", 50.0, 251.0, 18.0, WHITE);
    draw_text("State files and state-boundary overlays are a later milestone.", 50.0, 284.0, 17.0, LIGHTGRAY);
    draw_text("Android file picker / SAF import is not implemented yet.", 50.0, 322.0, 17.0, Color::from_rgba(236, 187, 126, 255));

    if button("Back to menu", 50.0, 380.0, 220.0, 52.0) {
        Screen::MainMenu
    } else {
        Screen::Settings
    }
}

#[macroquad::main(window_conf)]
async fn main() {
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

                    if is_mouse_button_down(MouseButton::Left) && inside_map {
                        if current_map.dragging {
                            let delta = mouse - current_map.last_mouse;
                            let source = current_map.source_rect();
                            current_map.pan.x -= delta.x / rect.w * source.w;
                            current_map.pan.y -= delta.y / rect.h * source.h;
                            current_map.clamp_pan();
                        }
                        current_map.dragging = true;
                    } else {
                        current_map.dragging = false;
                    }
                    current_map.last_mouse = mouse;

                    if is_mouse_button_pressed(MouseButton::Left) && inside_map {
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
