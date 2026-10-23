use std::collections::HashMap;
use std::fs;
use std::path::Path;

use image::GenericImageView;
use macroquad::prelude::*;

const MAP_BITMAP_PATH: &str = "data/hoi4/map/provinces.bmp";
const MAP_DEFINITION_PATH: &str = "data/hoi4/map/definition.csv";

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
        texture.set_filter(FilterMode::Nearest);

        let definition_text = fs::read_to_string(definition_path)
            .map_err(|error| format!("Could not read definition.csv: {error}"))?;
        let mut id_by_rgb = HashMap::new();

        for (line_index, line) in definition_text.lines().enumerate() {
            let mut fields = line.split(',');
            let Some(id_text) = fields.next() else { continue };
            let Some(r_text) = fields.next() else { continue };
            let Some(g_text) = fields.next() else { continue };
            let Some(b_text) = fields.next() else { continue };

            let parsed = (
                id_text.trim().parse::<u32>(),
                r_text.trim().parse::<u8>(),
                g_text.trim().parse::<u8>(),
                b_text.trim().parse::<u8>(),
            );

            let (Ok(id), Ok(r), Ok(g), Ok(b)) = parsed else {
                // The header or a malformed row is ignored for this first prototype.
                continue;
            };

            if id == 0 {
                continue;
            }

            let rgb = [r, g, b];
            if id_by_rgb.insert(rgb, id).is_some() {
                return Err(format!(
                    "Duplicate province RGB colour at definition.csv line {}",
                    line_index + 1
                ));
            }
        }

        if id_by_rgb.is_empty() {
            return Err("definition.csv contained no valid province definitions".to_owned());
        }

        Ok(Self {
            texture,
            width,
            height,
            rgba,
            province_count: id_by_rgb.len(),
            id_by_rgb,
        })
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

fn map_view_rect(map: &ProvinceMap) -> Rect {
    let left = 270.0_f32.min(screen_width() * 0.30);
    let top = 82.0;
    let available_w = (screen_width() - left - 22.0).max(1.0);
    let available_h = (screen_height() - top - 24.0).max(1.0);
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
    map: Option<&ProvinceMap>,
    load_error: &str,
    selected: Option<(u32, [u8; 3], u32, u32)>,
) -> Screen {
    draw_background();
    draw_rectangle(0.0, 5.0, screen_width(), 58.0, Color::from_rgba(20, 32, 46, 255));
    draw_text("WORLD MAP", 22.0, 42.0, 25.0, WHITE);

    if button("Main menu", screen_width() - 160.0, 15.0, 138.0, 38.0) {
        return Screen::MainMenu;
    }

    draw_rectangle(12.0, 78.0, 242.0, screen_height() - 92.0, Color::from_rgba(18, 30, 43, 255));
    draw_text("MAP DATA", 28.0, 109.0, 18.0, WHITE);

    match map {
        Some(map) => {
            draw_text(&format!("Size: {} × {}", map.width, map.height), 28.0, 143.0, 17.0, LIGHTGRAY);
            draw_text(&format!("Defined provinces: {}", map.province_count), 28.0, 170.0, 16.0, LIGHTGRAY);
            draw_text("Source raster: loaded", 28.0, 202.0, 15.0, Color::from_rgba(130, 200, 150, 255));

            if let Some((id, rgb, x, y)) = selected {
                draw_rectangle(26.0, 238.0, 210.0, 124.0, Color::from_rgba(27, 44, 59, 255));
                draw_text("SELECTED PIXEL", 38.0, 263.0, 15.0, WHITE);
                draw_text(&format!("Province ID: {id}"), 38.0, 290.0, 17.0, WHITE);
                draw_text(&format!("Pixel: {x}, {y}"), 38.0, 317.0, 15.0, LIGHTGRAY);
                draw_text(&format!("RGB: {}, {}, {}", rgb[0], rgb[1], rgb[2]), 38.0, 342.0, 14.0, LIGHTGRAY);
            } else {
                draw_text("Tap a province to", 28.0, 252.0, 16.0, LIGHTGRAY);
                draw_text("inspect its ID and RGB.", 28.0, 275.0, 16.0, LIGHTGRAY);
            }

            let rect = map_view_rect(map);
            draw_rectangle(rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0, Color::from_rgba(62, 78, 90, 255));
            draw_texture_ex(
                &map.texture,
                rect.x,
                rect.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(rect.w, rect.h)),
                    ..Default::default()
                },
            );
            draw_text("Nearest-neighbour raster view", rect.x, screen_height() - 12.0, 14.0, LIGHTGRAY);
        }
        None => {
            draw_text("Map raster not loaded", 28.0, 145.0, 18.0, Color::from_rgba(236, 187, 126, 255));
            draw_text("Expected local files:", 28.0, 184.0, 15.0, LIGHTGRAY);
            draw_text("data/hoi4/map/", 28.0, 208.0, 15.0, WHITE);
            draw_text("provinces.bmp", 28.0, 232.0, 15.0, WHITE);
            draw_text("definition.csv", 28.0, 255.0, 15.0, WHITE);
            draw_text("No game map assets are bundled.", 28.0, 300.0, 13.0, LIGHTGRAY);

            let x = 280.0;
            let y = 92.0;
            let w = screen_width() - x - 22.0;
            let h = screen_height() - y - 24.0;
            draw_rectangle(x, y, w, h, Color::from_rgba(17, 31, 43, 255));
            draw_rectangle_lines(x, y, w, h, 1.0, Color::from_rgba(48, 69, 84, 255));
            draw_text("Import a local province raster to view the real map.", x + 28.0, y + h * 0.48, 21.0, WHITE);
            draw_text("This empty panel is intentional; no fake province data is shown.", x + 28.0, y + h * 0.48 + 32.0, 15.0, LIGHTGRAY);
        }
    }

    Screen::MapViewer
}

fn draw_settings() -> Screen {
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
    let mut map = ProvinceMap::load().ok();
    let mut load_error = map.as_ref().map(|_| String::new()).unwrap_or_else(|| {
        format!("Map not loaded. Expected {} and {}", MAP_BITMAP_PATH, MAP_DEFINITION_PATH)
    });
    let mut selected: Option<(u32, [u8; 3], u32, u32)> = None;

    loop {
        let next_screen = match screen {
            Screen::MainMenu => draw_main_menu(),
            Screen::MapViewer => {
                let rendered_screen = draw_map_viewer(map.as_ref(), &load_error, selected);
                if let Some(current_map) = map.as_ref() {
                    if is_mouse_button_pressed(MouseButton::Left) {
                        let (mouse_x, mouse_y) = mouse_position();
                        let rect = map_view_rect(current_map);
                        if mouse_x >= rect.x && mouse_x <= rect.x + rect.w
                            && mouse_y >= rect.y && mouse_y <= rect.y + rect.h
                        {
                            let pixel_x = (((mouse_x - rect.x) / rect.w) * current_map.width as f32)
                                .floor() as u32;
                            let pixel_y = (((mouse_y - rect.y) / rect.h) * current_map.height as f32)
                                .floor() as u32;
                            selected = current_map.province_at(pixel_x, pixel_y)
                                .map(|(id, rgb)| (id, rgb, pixel_x, pixel_y));
                        }
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
