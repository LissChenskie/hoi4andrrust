//! Static province geography. Built once at import, independent of textures.
//! Raster neighbours and scripted special connections are separate inputs.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvinceKind {
    Land,
    Sea,
    Lake,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvinceDefinition {
    pub id: u32,
    pub rgb: [u8; 3],
    pub kind: ProvinceKind,
    pub coastal: bool,
    pub terrain: String,
    pub continent: u32,
}

pub fn parse_definitions(text: &str) -> Result<BTreeMap<u32, ProvinceDefinition>, String> {
    let delimiter = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| if line.contains(';') { ';' } else { ',' })
        .unwrap_or(';');
    let mut result = BTreeMap::new();
    let mut colors = BTreeSet::new();
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line
            .split('#')
            .next()
            .unwrap_or("")
            .split(delimiter)
            .map(str::trim)
            .collect();
        let Some(id) = fields.first().and_then(|id| id.parse::<u32>().ok()) else {
            continue;
        };
        if id == 0 {
            continue;
        }
        if fields.len() < 4 {
            return Err(format!(
                "Missing RGB at definition.csv line {}",
                line_number + 1
            ));
        }
        let component = |i: usize| {
            fields[i]
                .parse::<u8>()
                .map_err(|_| format!("Invalid RGB at definition.csv line {}", line_number + 1))
        };
        let rgb = [component(1)?, component(2)?, component(3)?];
        if result.contains_key(&id) {
            return Err(format!("Duplicate province ID {id}"));
        }
        if !colors.insert(rgb) {
            return Err("Duplicate province RGB colour".into());
        }
        let kind = match fields.get(4).copied() {
            Some("land") => ProvinceKind::Land,
            Some("sea") => ProvinceKind::Sea,
            Some("lake") => ProvinceKind::Lake,
            _ => ProvinceKind::Unknown,
        };
        result.insert(
            id,
            ProvinceDefinition {
                id,
                rgb,
                kind,
                coastal: fields.get(5).is_some_and(|v| *v == "true" || *v == "yes"),
                terrain: fields.get(6).copied().unwrap_or("unknown").to_owned(),
                continent: fields.get(7).and_then(|v| v.parse().ok()).unwrap_or(0),
            },
        );
    }
    if result.is_empty() {
        return Err("definition.csv contained no valid province definitions".into());
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProvinceGeography {
    pub definition: ProvinceDefinition,
    /// Raster centroid, with circular X averaging on a wrapping world map.
    /// A future movement model must calibrate pixel distances to game distances.
    pub center: Option<MapPosition>,
    pub pixel_count: u64,
    pub bounds: Option<ProvinceBounds>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProvinceBounds {
    pub min_x: u32,
    pub min_y: u32,
    pub max_x: u32,
    pub max_y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossingKind {
    Border,
    Strait,
    Impassable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiverCrossing {
    /// No river raster was loaded, or one endpoint has no map position.
    Unknown,
    None,
    /// Preserve source palette index; small/large thresholds belong to versioned rules.
    PaletteIndex(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub to: u32,
    pub kind: CrossingKind,
    pub through: Option<u32>,
    pub rule: Option<String>,
    pub river: RiverCrossing,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapGraph {
    pub width: u32,
    pub height: u32,
    pub wrap_x: bool,
    pub provinces: BTreeMap<u32, ProvinceGeography>,
    /// Compressed physical land-border segments in raster coordinates. Built
    /// once with the graph, shared by front hit testing and rendering.
    pub border_segments: BTreeMap<(u32, u32), Vec<[u32; 4]>>,
    /// Retained script data, not an automatically granted passage permission.
    pub adjacency_rules_source: Option<String>,
    connections: BTreeMap<u32, Vec<Connection>>,
    source_fingerprint: u64,
}

#[derive(Clone, Default)]
struct PixelTotals {
    count: u64,
    x: u64,
    y: u64,
    sin_x: f64,
    cos_x: f64,
    bounds: Option<ProvinceBounds>,
}

impl MapGraph {
    fn fingerprint(width: u32, height: u32, rgba: &[u8], definition: &str) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        width.hash(&mut h);
        height.hash(&mut h);
        definition.hash(&mut h);
        rgba.hash(&mut h);
        h.finish()
    }

    pub fn snapshot_fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.source_fingerprint.hash(&mut h);
        self.wrap_x.hash(&mut h);
        format!("{:?}", self.connections).hash(&mut h);
        self.adjacency_rules_source.hash(&mut h);
        h.finish()
    }
    pub fn matches_source(&self, width: u32, height: u32, rgba: &[u8], definition: &str) -> bool {
        self.source_fingerprint == Self::fingerprint(width, height, rgba, definition)
    }

    pub fn from_rgba(
        width: u32,
        height: u32,
        rgba: &[u8],
        definition: &str,
        wrap_x: bool,
    ) -> Result<Self, String> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(4));
        if width == 0 || height == 0 || expected != Some(rgba.len()) {
            return Err("Province raster dimensions do not match its RGBA data".into());
        }
        let definitions = parse_definitions(definition)?;
        let definitions: Vec<_> = definitions.into_values().collect();
        let index: HashMap<_, _> = definitions
            .iter()
            .enumerate()
            .map(|(i, d)| (d.rgb, i))
            .collect();
        let mut totals = vec![PixelTotals::default(); definitions.len()];
        // Two rows rather than a second full-world u32 province raster.
        let w = width as usize;
        let mut previous = vec![usize::MAX; w];
        let mut current = vec![usize::MAX; w];
        let angles: Vec<_> = (0..width)
            .map(|x| (f64::from(x) * std::f64::consts::TAU / f64::from(width)).sin_cos())
            .collect();
        let mut borders = BTreeSet::new();
        let mut border_segments = BTreeMap::<(u32, u32), Vec<[u32; 4]>>::new();
        let mut add_border = |a: usize, b: usize, segment: [u32; 4]| {
            if a != usize::MAX && b != usize::MAX && a != b {
                let land = definitions[a].kind == ProvinceKind::Land
                    && definitions[b].kind == ProvinceKind::Land;
                let (a, b) = (definitions[a].id, definitions[b].id);
                let key = (a.min(b), a.max(b));
                borders.insert(key);
                if land {
                    border_segments.entry(key).or_default().push(segment);
                }
            }
        };
        for y in 0..height as usize {
            for x in 0..w {
                let o = (y * w + x) * 4;
                let rgb = [rgba[o], rgba[o + 1], rgba[o + 2]];
                let i = if let Some(&i) = index.get(&rgb) {
                    i
                } else if rgb == [0, 0, 0] {
                    usize::MAX
                } else {
                    return Err(format!("Undefined province color {rgb:?} at ({x}, {y})"));
                };
                current[x] = i;
                if i != usize::MAX {
                    let t = &mut totals[i];
                    if let Some(b) = &mut t.bounds {
                        b.min_x = b.min_x.min(x as u32);
                        b.max_x = b.max_x.max(x as u32);
                        b.min_y = b.min_y.min(y as u32);
                        b.max_y = b.max_y.max(y as u32);
                    } else {
                        t.bounds = Some(ProvinceBounds {
                            min_x: x as u32,
                            max_x: x as u32,
                            min_y: y as u32,
                            max_y: y as u32,
                        });
                    }
                    t.count += 1;
                    t.x += x as u64;
                    t.y += y as u64;
                    if wrap_x {
                        t.sin_x += angles[x].0;
                        t.cos_x += angles[x].1;
                    }
                }
                if x > 0 {
                    add_border(
                        i,
                        current[x - 1],
                        [x as u32, y as u32, x as u32, y as u32 + 1],
                    );
                }
                if y > 0 {
                    add_border(i, previous[x], [x as u32, y as u32, x as u32 + 1, y as u32]);
                }
            }
            if wrap_x && w > 1 {
                add_border(current[0], current[w - 1], [0, y as u32, 0, y as u32 + 1]);
            }
            std::mem::swap(&mut current, &mut previous);
        }
        let provinces = definitions
            .into_iter()
            .zip(totals)
            .map(|(definition, t)| {
                let center = (t.count > 0).then(|| {
                    let x = if wrap_x && t.sin_x.hypot(t.cos_x) > t.count as f64 * 1.0e-9 {
                        (t.sin_x.atan2(t.cos_x) / std::f64::consts::TAU * f64::from(width))
                            .rem_euclid(f64::from(width))
                    } else {
                        t.x as f64 / t.count as f64
                    };
                    MapPosition {
                        x,
                        y: t.y as f64 / t.count as f64,
                    }
                });
                (
                    definition.id,
                    ProvinceGeography {
                        definition,
                        center,
                        pixel_count: t.count,
                        bounds: t.bounds,
                    },
                )
            })
            .collect();
        for segments in border_segments.values_mut() {
            segments.sort_unstable_by_key(|s| {
                if s[0] == s[2] {
                    (0, s[0], s[1])
                } else {
                    (1, s[1], s[0])
                }
            });
            let mut merged: Vec<[u32; 4]> = Vec::new();
            for &s in segments.iter() {
                if let Some(last) = merged.last_mut() {
                    if last[2] == s[0] && last[3] == s[1] && (last[0] == last[2]) == (s[0] == s[2])
                    {
                        last[2] = s[2];
                        last[3] = s[3];
                        continue;
                    }
                }
                merged.push(s);
            }
            *segments = merged;
        }
        let mut graph = Self {
            border_segments,
            width,
            height,
            wrap_x,
            provinces,
            adjacency_rules_source: None,
            connections: BTreeMap::new(),
            source_fingerprint: Self::fingerprint(width, height, rgba, definition),
        };
        for (a, b) in borders {
            graph.connect(a, b, CrossingKind::Border, None, None);
        }
        Ok(graph)
    }

    fn connect(
        &mut self,
        a: u32,
        b: u32,
        kind: CrossingKind,
        through: Option<u32>,
        rule: Option<String>,
    ) {
        for (from, to) in [(a, b), (b, a)] {
            let list = self.connections.entry(from).or_default();
            let edge = Connection {
                to,
                kind,
                through,
                rule: rule.clone(),
                river: RiverCrossing::Unknown,
            };
            match list.binary_search_by_key(&to, |edge| edge.to) {
                Ok(i) => list[i] = edge,
                Err(i) => list.insert(i, edge),
            }
        }
    }

    pub fn connections_from(&self, id: u32) -> &[Connection] {
        self.connections.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn connection(&self, from: u32, to: u32) -> Option<&Connection> {
        self.connections_from(from)
            .iter()
            .find(|edge| edge.to == to)
    }

    /// Geographic candidates only. A strait still requires naval/diplomatic
    /// permission and its named adjacency rule; this does not grant access.
    pub fn land_neighbors(&self, id: u32) -> impl Iterator<Item = &Connection> {
        let land = self
            .provinces
            .get(&id)
            .is_some_and(|p| p.definition.kind == ProvinceKind::Land);
        self.connections_from(id).iter().filter(move |edge| {
            land && edge.kind != CrossingKind::Impassable
                && self
                    .provinces
                    .get(&edge.to)
                    .is_some_and(|p| p.definition.kind == ProvinceKind::Land)
        })
    }

    pub fn distance_in_pixels(&self, a: u32, b: u32) -> Option<f64> {
        let a = self.provinces.get(&a)?.center?;
        let b = self.provinces.get(&b)?.center?;
        let mut dx = (a.x - b.x).abs();
        if self.wrap_x {
            dx = dx.min(f64::from(self.width) - dx);
        }
        Some(dx.hypot(a.y - b.y))
    }

    /// Parse all entries before mutation; reject unknown references/types rather
    /// than opening an unintended route. CSV terminator is not an edge.
    pub fn import_adjacencies(&mut self, source: &str) -> Result<usize, String> {
        let mut additions = Vec::new();
        for (n, line) in source.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<_> = line.split(';').map(str::trim).collect();
            if f[0] == "-1" {
                break;
            }
            let Ok(a) = f[0].parse::<u32>() else {
                if n == 0
                    || f[0].eq_ignore_ascii_case("from")
                    || f[0].eq_ignore_ascii_case("start province id")
                {
                    continue;
                }
                return Err(format!("Invalid adjacency at line {}", n + 1));
            };
            if f.len() < 9 {
                return Err(format!("Truncated adjacency at line {}", n + 1));
            }
            let b = f[1]
                .parse::<u32>()
                .map_err(|_| format!("Invalid adjacency target at line {}", n + 1))?;
            if a == b || !self.provinces.contains_key(&a) || !self.provinces.contains_key(&b) {
                return Err(format!("Unknown/self adjacency {a} -> {b}"));
            }
            let kind = match f[2] {
                // HOI4 uses an omitted type as the default sea connection,
                // including naval rules between sea provinces.
                "sea" | "" => CrossingKind::Strait,
                "impassable" => CrossingKind::Impassable,
                other => {
                    return Err(format!(
                        "Unsupported adjacency type {other:?} at line {} ({a} -> {b}); expected sea, empty, or impassable",
                        n + 1
                    ));
                }
            };
            let through = match f[3] {
                "-1" | "" => None,
                raw => Some(
                    raw.parse::<u32>()
                        .map_err(|_| "Invalid adjacency through province")?,
                ),
            };
            if through.is_some_and(|id| !self.provinces.contains_key(&id)) {
                return Err(format!("Unknown through province in adjacency {a} -> {b}"));
            }
            let rule = (!f[8].is_empty() && f[8] != "-1").then(|| f[8].to_owned());
            additions.push((a, b, kind, through, rule));
        }
        let count = additions.len();
        for (a, b, kind, through, rule) in additions {
            self.connect(a, b, kind, through, rule);
        }
        Ok(count)
    }

    /// Keep indices from an 8-bit BMP, rather than inferring river size from
    /// rendered RGB. Imported crossings follow a straight centroid-to-centroid
    /// segment; exact engine positioning remains a parity-validation task.
    pub fn import_rivers(&mut self, bmp: &[u8], max_river_index: u8) -> Result<(), String> {
        let mask = RiverMask::from_bmp(bmp)?;
        if mask.width != self.width || mask.height != self.height {
            return Err("rivers.bmp dimensions differ from provinces.bmp".into());
        }
        let mut results = Vec::new();
        for (&from, list) in &self.connections {
            for edge in list.iter().filter(|edge| from < edge.to) {
                let crossing = if edge.kind != CrossingKind::Border {
                    RiverCrossing::None
                } else if let (Some(a), Some(b)) = (
                    self.provinces[&from].center,
                    self.provinces[&edge.to].center,
                ) {
                    mask.crossing(a, b, self.wrap_x, max_river_index)
                } else {
                    RiverCrossing::Unknown
                };
                results.push((from, edge.to, crossing));
            }
        }
        for (a, b, crossing) in results {
            for (from, to) in [(a, b), (b, a)] {
                if let Some(edge) = self
                    .connections
                    .get_mut(&from)
                    .and_then(|v| v.iter_mut().find(|e| e.to == to))
                {
                    edge.river = crossing;
                }
            }
        }
        Ok(())
    }
}

struct RiverMask {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RiverMask {
    fn from_bmp(data: &[u8]) -> Result<Self, String> {
        let u16_at = |o| {
            data.get(o..o + 2)
                .and_then(|v| v.try_into().ok())
                .map(u16::from_le_bytes)
        };
        let u32_at = |o| {
            data.get(o..o + 4)
                .and_then(|v| v.try_into().ok())
                .map(u32::from_le_bytes)
        };
        if data.get(..2) != Some(b"BM")
            || u32_at(14).is_none_or(|n| n < 40)
            || u16_at(26) != Some(1)
            || u16_at(28) != Some(8)
            || u32_at(30) != Some(0)
        {
            return Err("rivers.bmp must be an uncompressed 8-bit indexed BMP".into());
        }
        let width = u32_at(18).unwrap_or(0) as i32;
        let height = u32_at(22).unwrap_or(0) as i32;
        if width <= 0 || height == 0 || height == i32::MIN {
            return Err("Invalid rivers.bmp dimensions".into());
        }
        let h = height.unsigned_abs();
        let w = width as u32;
        let offset = u32_at(10).unwrap_or(0) as usize;
        let header_end = 14usize
            .checked_add(u32_at(14).unwrap() as usize)
            .ok_or("Invalid BMP header")?;
        let stride = (w as usize).checked_add(3).ok_or("River row too large")? & !3;
        let size = stride
            .checked_mul(h as usize)
            .ok_or("River raster too large")?;
        if offset < header_end || offset.checked_add(size).is_none_or(|end| end > data.len()) {
            return Err("Truncated rivers.bmp raster".into());
        }
        let mut pixels = Vec::with_capacity(w as usize * h as usize);
        for y in 0..h as usize {
            let row = if height > 0 { h as usize - 1 - y } else { y };
            let start = offset + row * stride;
            pixels.extend_from_slice(&data[start..start + w as usize]);
        }
        Ok(Self {
            width: w,
            height: h,
            pixels,
        })
    }

    fn crossing(
        &self,
        a: MapPosition,
        b: MapPosition,
        wrap_x: bool,
        max_index: u8,
    ) -> RiverCrossing {
        let w = i64::from(self.width);
        let h = i64::from(self.height);
        let (mut x, mut y) = (a.x.round() as i64, (a.y.round() as i64).clamp(0, h - 1));
        let (mut end_x, end_y) = (b.x.round() as i64, (b.y.round() as i64).clamp(0, h - 1));
        if wrap_x {
            if end_x - x > w / 2 {
                end_x -= w;
            }
            if x - end_x > w / 2 {
                end_x += w;
            }
        }
        let dx = (end_x - x).abs();
        let dy = -(end_y - y).abs();
        let sx = if x < end_x { 1 } else { -1 };
        let sy = if y < end_y { 1 } else { -1 };
        let mut error = dx + dy;
        let mut widest: Option<u8> = None;
        loop {
            let px = if wrap_x {
                x.rem_euclid(w)
            } else {
                x.clamp(0, w - 1)
            };
            let index = self.pixels[y as usize * self.width as usize + px as usize];
            if index <= max_index {
                widest = Some(widest.map_or(index, |v| v.max(index)));
            }
            if x == end_x && y == end_y {
                break;
            }
            let twice = 2 * error;
            if twice >= dy {
                error += dy;
                x += sx;
            }
            if twice <= dx {
                error += dx;
                y += sy;
            }
        }
        widest.map_or(RiverCrossing::None, RiverCrossing::PaletteIndex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DEFINITIONS: &str = "province;red;green;blue;type;coastal;terrain;continent\n1;1;0;0;land;false;plains;1\n2;2;0;0;land;true;forest;1\n3;3;0;0;sea;true;ocean;1\n4;4;0;0;land;true;mountain;2\n";
    fn rgba(ids: &[u8]) -> Vec<u8> {
        ids.iter().flat_map(|id| [*id, 0, 0, 255]).collect()
    }

    #[test]
    fn raster_builds_cardinal_neighbors_without_diagonal_or_water_routes() {
        let graph = MapGraph::from_rgba(2, 2, &rgba(&[1, 2, 3, 4]), DEFINITIONS, false).unwrap();
        assert!(graph.connection(1, 2).is_some());
        assert!(graph.connection(2, 1).is_some());
        assert!(graph.connection(1, 4).is_none());
        assert_eq!(
            graph.land_neighbors(1).map(|e| e.to).collect::<Vec<_>>(),
            [2]
        );
        assert_eq!(graph.land_neighbors(3).count(), 0);
        assert_eq!(graph.provinces[&2].definition.terrain, "forest");
        assert_eq!(graph.provinces[&4].definition.continent, 2);
    }

    #[test]
    fn horizontal_seam_is_connected_but_north_and_south_are_not() {
        let graph = MapGraph::from_rgba(4, 1, &rgba(&[1, 2, 3, 4]), DEFINITIONS, true).unwrap();
        assert!(graph.connection(1, 4).is_some());
        assert!((graph.distance_in_pixels(1, 4).unwrap() - 1.0).abs() < 1e-8);
        let graph = MapGraph::from_rgba(1, 4, &rgba(&[1, 2, 3, 4]), DEFINITIONS, true).unwrap();
        assert!(graph.connection(1, 4).is_none());
    }

    #[test]
    fn scripted_crossings_override_borders_and_keep_rule_and_through_metadata() {
        let mut graph =
            MapGraph::from_rgba(4, 1, &rgba(&[1, 2, 3, 4]), DEFINITIONS, false).unwrap();
        graph.import_adjacencies("From;To;Type;Through;x;y;x;y;Rule;Comment\n1;2;impassable;-1;-1;-1;-1;-1;;blocked\n1;4;sea;3;-1;-1;-1;-1;strait_rule;crossing\n-1;-1;-1;-1;-1;-1;-1;-1;-1\n999;2;sea;-1;-1;-1;-1;-1;;ignored").unwrap();
        assert_eq!(
            graph.connection(1, 2).unwrap().kind,
            CrossingKind::Impassable
        );
        let edge = graph.connection(4, 1).unwrap();
        assert_eq!(edge.through, Some(3));
        assert_eq!(edge.rule.as_deref(), Some("strait_rule"));
        assert_eq!(
            graph.land_neighbors(1).map(|e| e.to).collect::<Vec<_>>(),
            [4]
        );
        let before = graph.clone();
        assert!(
            graph
                .import_adjacencies("1;4;impassable;-1;-1;-1;-1;-1;;\n1;99;sea;-1;-1;-1;-1;-1;;")
                .is_err()
        );
        assert_eq!(graph, before);
    }
    #[test]
    fn omitted_adjacency_type_matches_sea_and_keeps_access_rules() {
        let mut implicit =
            MapGraph::from_rgba(4, 1, &rgba(&[1, 2, 3, 4]), DEFINITIONS, false).unwrap();
        let mut explicit = implicit.clone();
        implicit.import_adjacencies("From;To;Type;Through;x;y;x;y;Rule;Comment\n1;4;;3;-1;-1;-1;-1;closed_strait;default type\n-1;-1;;-1;-1;-1;-1;-1;-1\n").unwrap();
        explicit
            .import_adjacencies(
                "1;4;sea;3;-1;-1;-1;-1;closed_strait;explicit type\n-1;-1;-1;-1;-1;-1;-1;-1;-1\n",
            )
            .unwrap();
        assert_eq!(implicit, explicit);
        let edge = implicit.connection(4, 1).unwrap();
        assert_eq!(edge.kind, CrossingKind::Strait);
        assert_eq!(edge.through, Some(3));
        assert_eq!(edge.rule.as_deref(), Some("closed_strait"));
        let before = implicit.clone();
        let error = implicit
            .import_adjacencies("2;4;;3;-1;-1;-1;-1;;\n1;4;invalid;3;-1;-1;-1;-1;;")
            .unwrap_err();
        assert!(
            error.contains("\"invalid\"") && error.contains("line 2") && error.contains("1 -> 4")
        );
        assert_eq!(implicit, before);
    }

    fn bmp(width: u32, height: i32, rows_in_file: &[u8]) -> Vec<u8> {
        let mut data = vec![0u8; 54];
        data[..2].copy_from_slice(b"BM");
        data[10..14].copy_from_slice(&54u32.to_le_bytes());
        data[14..18].copy_from_slice(&40u32.to_le_bytes());
        data[18..22].copy_from_slice(&width.to_le_bytes());
        data[22..26].copy_from_slice(&height.to_le_bytes());
        data[26..28].copy_from_slice(&1u16.to_le_bytes());
        data[28..30].copy_from_slice(&8u16.to_le_bytes());
        data.extend_from_slice(rows_in_file);
        data
    }

    #[test]
    fn river_indices_padding_and_bottom_up_orientation_are_preserved() {
        let mut graph = MapGraph::from_rgba(3, 1, &rgba(&[1, 2, 2]), DEFINITIONS, false).unwrap();
        assert_eq!(
            graph.connection(1, 2).unwrap().river,
            RiverCrossing::Unknown
        );
        graph
            .import_rivers(&bmp(3, 1, &[255, 8, 255, 0]), 11)
            .unwrap();
        assert_eq!(
            graph.connection(1, 2).unwrap().river,
            RiverCrossing::PaletteIndex(8)
        );
        assert_eq!(
            graph.connection(2, 1).unwrap().river,
            RiverCrossing::PaletteIndex(8)
        );
        let mask = RiverMask::from_bmp(&bmp(1, 2, &[8, 0, 0, 0, 255, 0, 0, 0])).unwrap();
        assert_eq!(mask.pixels, [255, 8]);
        let mask = RiverMask::from_bmp(&bmp(1, -2, &[8, 0, 0, 0, 255, 0, 0, 0])).unwrap();
        assert_eq!(mask.pixels, [8, 255]);
        assert!(graph.import_rivers(&bmp(3, 1, &[]), 11).is_err());
    }

    #[test]
    fn source_validation_catches_changed_geometry_and_undefined_colors() {
        let pixels = rgba(&[1, 2]);
        let graph = MapGraph::from_rgba(2, 1, &pixels, DEFINITIONS, false).unwrap();
        assert!(graph.matches_source(2, 1, &pixels, DEFINITIONS));
        assert!(!graph.matches_source(2, 1, &rgba(&[2, 1]), DEFINITIONS));
        assert!(MapGraph::from_rgba(2, 1, &rgba(&[1, 99]), DEFINITIONS, false).is_err());
        assert!(MapGraph::from_rgba(2, 2, &pixels, DEFINITIONS, false).is_err());
    }
}
