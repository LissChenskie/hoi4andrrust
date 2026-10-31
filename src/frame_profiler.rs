//! Opt-in per-frame CPU profiler. It measures only our own work between
//! `begin_frame` and `end_frame`; time spent waiting for vsync inside
//! `next_frame().await` is deliberately excluded, so these numbers say where
//! the game spends CPU, not what the display refresh rate is.
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::Write as _;
use std::time::Instant;

const WINDOW: usize = 120;
const LOG_EVERY_FRAMES: u64 = 300;

#[derive(Default)]
struct Section {
    name: &'static str,
    current: f64,
    recent: VecDeque<f64>,
}

pub struct FrameProfiler {
    pub enabled: bool,
    last: Instant,
    frame_start: Instant,
    sections: Vec<Section>,
    totals: VecDeque<f64>,
    frames: u64,
}

impl Default for FrameProfiler {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            enabled: std::env::var_os("HOI4_PROFILE").is_some_and(|v| v != "0"),
            last: now,
            frame_start: now,
            sections: Vec::new(),
            totals: VecDeque::new(),
            frames: 0,
        }
    }
}

fn push(window: &mut VecDeque<f64>, value: f64) {
    if window.len() >= WINDOW {
        window.pop_front();
    }
    window.push_back(value);
}

fn mean(w: &VecDeque<f64>) -> f64 {
    if w.is_empty() {
        0.0
    } else {
        w.iter().sum::<f64>() / w.len() as f64
    }
}

fn max(w: &VecDeque<f64>) -> f64 {
    w.iter().copied().fold(0.0, f64::max)
}

impl FrameProfiler {
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn begin_frame(&mut self) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        self.frame_start = now;
        self.last = now;
        for s in &mut self.sections {
            s.current = 0.0;
        }
    }

    /// Attribute the time since the previous mark to `name`.
    pub fn mark(&mut self, name: &'static str) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        let ms = now.duration_since(self.last).as_secs_f64() * 1000.0;
        self.last = now;
        self.add_ms(name, ms);
    }

    pub fn add_ms(&mut self, name: &'static str, ms: f64) {
        match self.sections.iter_mut().find(|s| s.name == name) {
            Some(s) => s.current += ms,
            None => self.sections.push(Section {
                name,
                current: ms,
                recent: VecDeque::new(),
            }),
        }
    }

    pub fn end_frame(&mut self) {
        if !self.enabled {
            return;
        }
        let total = self.frame_start.elapsed().as_secs_f64() * 1000.0;
        self.finish_frame(total);
    }

    pub fn finish_frame(&mut self, total_ms: f64) {
        for s in &mut self.sections {
            let value = std::mem::take(&mut s.current);
            push(&mut s.recent, value);
        }
        push(&mut self.totals, total_ms);
        self.frames += 1;
        if self.frames % LOG_EVERY_FRAMES == 0 {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("perf.log")
            {
                let _ = writeln!(f, "--- frame {}\n{}", self.frames, self.report().join("\n"));
            }
        }
    }

    /// Lines for the overlay and the log: average and worst over the last
    /// `WINDOW` frames, per section, sorted by average cost.
    pub fn report(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "CPU/frame avg {:.2} ms, worst {:.2} ms (last {} frames)",
            mean(&self.totals),
            max(&self.totals),
            self.totals.len()
        )];
        let mut rows: Vec<_> = self
            .sections
            .iter()
            .map(|s| (s.name, mean(&s.recent), max(&s.recent)))
            .collect();
        rows.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (name, avg, worst) in rows {
            let mut line = String::new();
            let _ = write!(line, "{name:<14} avg {avg:6.2}  worst {worst:7.2}");
            lines.push(line);
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> FrameProfiler {
        FrameProfiler {
            enabled: true,
            ..FrameProfiler::default()
        }
    }

    #[test]
    fn averages_and_worst_are_reported_per_section_sorted_by_cost() {
        let mut p = on();
        for (sim, map) in [(1.0, 5.0), (3.0, 7.0)] {
            p.add_ms("sim", sim);
            p.add_ms("map", map);
            p.finish_frame(sim + map);
        }
        let r = p.report();
        assert!(
            r[0].contains("avg 8.00") && r[0].contains("worst 10.00"),
            "{r:?}"
        );
        assert!(
            r[1].starts_with("map") && r[1].contains("avg   6.00"),
            "{r:?}"
        );
        assert!(
            r[2].starts_with("sim") && r[2].contains("worst    3.00"),
            "{r:?}"
        );
    }

    #[test]
    fn window_is_bounded_and_disabled_profiler_records_nothing() {
        let mut p = on();
        for _ in 0..(WINDOW * 3) {
            p.add_ms("x", 1.0);
            p.finish_frame(1.0);
        }
        assert_eq!(p.totals.len(), WINDOW);
        let mut off = FrameProfiler {
            enabled: false,
            ..FrameProfiler::default()
        };
        off.mark("x");
        off.end_frame();
        assert!(off.sections.is_empty() && off.totals.is_empty());
    }
}
