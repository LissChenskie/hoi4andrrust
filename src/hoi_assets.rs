//! Optional HOI4 graphics imported locally from a user's installed copy.
//! Only preview images under data/hoi4/ui are loaded; no proprietary images
//! are committed with this independent Rust project.
use macroquad::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

thread_local! {
    static PREVIEWS: RefCell<HashMap<String,(Option<Texture2D>,u64)>> =
        RefCell::new(HashMap::new());
    static ACCESS: Cell<u64> = const { Cell::new(0) };
}
fn safe(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn path(kind: &str, key: &str) -> Option<String> {
    if !matches!(
        kind,
        "research" | "equipment" | "portraits" | "flags" | "focus"
    ) || !safe(key)
    {
        return None;
    }
    Some(format!("data/hoi4/ui/{kind}/{key}.png"))
}
fn get(kind: &str, key: &str) -> Option<Texture2D> {
    let file = path(kind, key)?;
    let tick = ACCESS.with(|access| {
        let value = access.get().wrapping_add(1);
        access.set(value);
        value
    });
    PREVIEWS.with(|cell| {
        let mut previews = cell.borrow_mut();
        if let Some((texture, access)) = previews.get_mut(&file) {
            *access = tick;
            return texture.clone();
        }
        // Bound GPU memory on older mobile devices. Evict least-recently-used
        // icons instead of permanently disabling graphics after 320 lookups.
        if previews.len() >= 320 {
            if let Some(oldest) = previews
                .iter()
                .min_by_key(|(_, (_, last))| *last)
                .map(|(name, _)| name.clone())
            {
                previews.remove(&oldest);
            }
        }
        let texture = image::open(&file).ok().and_then(|image| {
            let rgba = image.to_rgba8();
            if rgba.width() == 0 || rgba.height() == 0 || rgba.width() > 256 || rgba.height() > 256
            {
                return None;
            }
            let t = Texture2D::from_rgba8(rgba.width() as u16, rgba.height() as u16, rgba.as_raw());
            t.set_filter(FilterMode::Linear);
            Some(t)
        });
        previews.insert(file, (texture.clone(), tick));
        texture
    })
}
pub fn draw(kind: &str, key: &str, r: Rect) -> bool {
    if r.w <= 1. || r.h <= 1. {
        return false;
    }
    let Some(texture) = get(kind, key) else {
        return false;
    };
    let scale = (r.w / texture.width()).min(r.h / texture.height());
    let w = texture.width() * scale;
    let h = texture.height() * scale;
    draw_texture_ex(
        &texture,
        r.x + (r.w - w) * 0.5,
        r.y + (r.h - h) * 0.5,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(w, h)),
            ..Default::default()
        },
    );
    true
}
/// Candidate aliases cover GFX_name, GFX_name_medium and GFX_name_small.
pub fn draw_equipment(key: &str, r: Rect) -> bool {
    draw("equipment", key, r) || draw("equipment", &format!("{key}_medium"), r)
}
pub fn draw_technology(key: &str, r: Rect) -> bool {
    draw("research", key, r) || draw("research", &format!("{key}_medium"), r)
}
pub fn portrait(character_id: &str, r: Rect) -> bool {
    draw("portraits", character_id, r)
}
pub fn country_portrait(tag: &str, r: Rect) -> bool {
    draw("portraits", &format!("country_{tag}"), r)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_image_keys_are_sandboxed() {
        assert!(path("research", "basic_machine_tools").is_some());
        assert!(path("portraits", "GER_erwin_rommel").is_some());
        assert!(path("research", "../../etc/passwd").is_none());
        assert!(path("gfx", "something").is_none());
    }
}
