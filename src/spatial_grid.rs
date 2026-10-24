//! A simple static uniform-grid spatial index for map markers.
//! Lookup cost scales with visible map cells instead of all world markers.
use macroquad::prelude::Rect;

#[derive(Debug, Default)]
pub struct SpatialGrid {
    columns: usize,
    rows: usize,
    cell: f32,
    buckets: Vec<Vec<usize>>,
    points: Vec<(f32, f32)>,
}

impl SpatialGrid {
    pub fn build<I>(width: u32, height: u32, cell: f32, markers: I) -> Self
    where I: IntoIterator<Item = (f32, f32)> {
        let cell = if cell.is_finite() && cell >= 1.0 { cell } else { 256.0 };
        let columns = ((width.max(1) as f32 / cell).ceil() as usize).max(1);
        let rows = ((height.max(1) as f32 / cell).ceil() as usize).max(1);
        let mut grid = Self {
            columns, rows, cell, buckets: vec![Vec::new(); columns * rows], points: Vec::new(),
        };
        for (index, point) in markers.into_iter().enumerate() {
            // Preserve original indices (e.g. cities sorted by VP priority).
            grid.points.push(point);
            if !point.0.is_finite() || !point.1.is_finite() { continue; }
            if point.0 < 0.0 || point.1 < 0.0
                || point.0 >= width as f32 || point.1 >= height as f32 { continue; }
            let col = (point.0 / cell).floor() as usize;
            let row = (point.1 / cell).floor() as usize;
            grid.buckets[row * columns + col].push(index);
        }
        grid
    }

    pub fn visible(&self, area: Rect) -> Vec<usize> {
        if self.points.is_empty() || !area.x.is_finite() || !area.y.is_finite()
            || !area.w.is_finite() || !area.h.is_finite()
            || area.w <= 0.0 || area.h <= 0.0 { return Vec::new(); }
        let first_x = ((area.x / self.cell).floor() as isize).clamp(0, self.columns as isize - 1) as usize;
        let last_x = (((area.x + area.w) / self.cell).floor() as isize)
            .clamp(0, self.columns as isize - 1) as usize;
        let first_y = ((area.y / self.cell).floor() as isize).clamp(0, self.rows as isize - 1) as usize;
        let last_y = (((area.y + area.h) / self.cell).floor() as isize)
            .clamp(0, self.rows as isize - 1) as usize;
        if area.x + area.w < 0.0 || area.y + area.h < 0.0
            || area.x > self.columns as f32 * self.cell
            || area.y > self.rows as f32 * self.cell {
            return Vec::new();
        }
        let mut result = Vec::new();
        for y in first_y..=last_y {
            for x in first_x..=last_x {
                for &index in &self.buckets[y * self.columns + x] {
                    let (px, py) = self.points[index];
                    if px >= area.x && py >= area.y
                        && px <= area.x + area.w && py <= area.y + area.h {
                        result.push(index);
                    }
                }
            }
        }
        // Critical: keep caller ordering, so high-priority city labels still
        // win when the per-frame label budget is exhausted.
        result.sort_unstable();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_only_visible_markers_across_cell_boundaries() {
        let grid = SpatialGrid::build(1024, 1024, 128.0, [
            (20.0, 20.0), (250.0, 252.0), (995.0, 995.0), (120.0, 190.0),
        ]);
        assert_eq!(grid.visible(Rect::new(100.0, 120.0, 170.0, 150.0)), vec![1, 3]);
        assert_eq!(grid.visible(Rect::new(980.0, 980.0, 44.0, 44.0)), vec![2]);
        assert_eq!(grid.visible(Rect::new(-100.0, -100.0, 110.0, 110.0)), vec![]);
    }

    #[test]
    fn spatial_culling_keeps_original_draw_and_label_priority() {
        let grid = SpatialGrid::build(500, 500, 100.0, [
            (190.0, 200.0), (400.0, 450.0), (170.0, 160.0), (180.0, 190.0),
        ]);
        assert_eq!(grid.visible(Rect::new(150.0, 150.0, 55.0, 60.0)), vec![0, 2, 3]);
    }

    #[test]
    fn invalid_positions_never_enter_grid() {
        let grid = SpatialGrid::build(100, 100, 25.0, [
            (f32::NAN, 12.0), (-10.0, 10.0), (10.0, f32::INFINITY), (50.0, 50.0),
        ]);
        assert_eq!(grid.visible(Rect::new(0.0, 0.0, 100.0, 100.0)), vec![3]);
        assert!(grid.visible(Rect::new(0.0, 0.0, f32::NAN, 100.0)).is_empty());
    }
}
