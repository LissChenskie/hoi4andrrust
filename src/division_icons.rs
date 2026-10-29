//! Optional, locally imported HOI4 unit icon preview textures.
//! No proprietary textures are committed to this repository. The user imports
//! their own installed game resources with tools/import_hoi4_division_icons.py.
use macroquad::prelude::*;
use std::{cell::RefCell, collections::BTreeMap};

thread_local! {
    static ICONS: RefCell<BTreeMap<String, Option<Texture2D>>> =
        const { RefCell::new(BTreeMap::new()) };
}
fn safe(stem: &str) -> bool {
    !stem.is_empty()
        && stem.len() < 110
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn paint(stem: &str, rect: Rect) -> bool {
    if !safe(stem) || rect.w < 5. || rect.h < 5. {
        return false;
    }
    ICONS.with(|cell| {
        let mut cache = cell.borrow_mut();
        if !cache.contains_key(stem) {
            // Bound GPU memory and negative lookup cache for modded game data.
            if cache.len() >= 320 {
                return false;
            }
            let path = format!("data/hoi4/ui/division_icons/{stem}.png");
            let texture = image::open(path).ok().and_then(|im| {
                let rgba = im.to_rgba8();
                if rgba.width() > 256
                    || rgba.height() > 256
                    || rgba.width() == 0
                    || rgba.height() == 0
                {
                    return None;
                }
                let t =
                    Texture2D::from_rgba8(rgba.width() as u16, rgba.height() as u16, rgba.as_raw());
                t.set_filter(FilterMode::Linear);
                Some(t)
            });
            cache.insert(stem.to_owned(), texture);
        }
        if let Some(Some(texture)) = cache.get(stem) {
            let scale = (rect.w / texture.width()).min(rect.h / texture.height());
            let w = texture.width() * scale;
            let h = texture.height() * scale;
            draw_texture_ex(
                texture,
                rect.x + (rect.w - w) * 0.5,
                rect.y + (rect.h - h) * 0.5,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(w, h)),
                    ..Default::default()
                },
            );
            true
        } else {
            false
        }
    })
}
/// Draw a user's imported sprite if found; otherwise show an original
/// procedural symbol, never a broken file name or a misleading missing texture.
pub fn draw(unit: &str, group: &str, r: Rect) {
    if paint(unit, r) || paint(&format!("group_{group}"), r) {
        return;
    }
    let fallback = if group == "support" {
        crate::ui_icons::Icon::Supply
    } else if group.contains("armor") || group.contains("tank") {
        crate::ui_icons::Icon::Factory
    } else if group.contains("mobile") || group.contains("motor") {
        crate::ui_icons::Icon::Convoy
    } else {
        crate::ui_icons::Icon::Army
    };
    crate::ui_icons::draw(
        fallback,
        Rect::new(r.x + 5., r.y + 5., (r.w - 10.).max(1.), (r.h - 10.).max(1.)),
        crate::hoi_ui::MUTED,
    );
}
pub fn draw_template(icon: &str, r: Rect) {
    if paint(&format!("division_{icon}"), r) || paint(icon, r) {
        return;
    }
    crate::ui_icons::draw(crate::ui_icons::Icon::Army, r, crate::hoi_ui::GOLD);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_filenames_cannot_traverse_game_directories() {
        assert!(safe("light_armor"));
        assert!(!safe("../../private"));
        assert!(!safe("unit:bad"));
    }
}
