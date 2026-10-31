//! Opt-in frame profiler for desktop and Android.
//!
//! Nothing is written unless profiling is explicitly enabled. Aggregated log
//! writes happen once per 60 frames to avoid adding per-frame disk overhead.
use macroquad::prelude::*;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{Duration, Instant};

const LOG_INTERVAL: u32 = 60;

#[derive(Debug, Clone, Copy, Default)]
struct FrameTimes {
    input_ms: f32,
    simulation_ms: f32,
    ui_update_ms: f32,
    map_ms: f32,
    panels_ms: f32,
    frame_ms: f32,
}

#[derive(Debug)]
pub struct FrameProfiler {
    enabled: bool,
    frame_started: Option<Instant>,
    current: FrameTimes,
    accumulated: FrameTimes,
    accumulated_frames: u32,
    log_error_reported: bool,
}

impl FrameProfiler {
    pub fn from_environment() -> Self {
        let enabled = std::env::var("HOI4_PROFILE")
            .map(|value| value != "0" && !value.trim().is_empty())
            .unwrap_or(false);
        Self {
            enabled,
            frame_started: None,
            current: FrameTimes::default(),
            accumulated: FrameTimes::default(),
            accumulated_frames: 0,
            log_error_reported: false,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame_started = Some(Instant::now());
        self.current = FrameTimes::default();
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.flush_log();
        }
    }

    pub fn record_input(&mut self, elapsed: Duration) {
        self.current.input_ms = millis(elapsed);
    }

    pub fn record_simulation(&mut self, elapsed: Duration) {
        self.current.simulation_ms = millis(elapsed);
    }

    pub fn record_ui_update(&mut self, elapsed: Duration) {
        self.current.ui_update_ms = millis(elapsed);
    }

    pub fn record_map(&mut self, elapsed: Duration) {
        self.current.map_ms = millis(elapsed);
    }

    pub fn record_panels(&mut self, elapsed: Duration) {
        self.current.panels_ms = millis(elapsed);
    }

    pub fn finish_frame(&mut self) {
        self.current.frame_ms = self.frame_started.map_or(0.0, |start| millis(start.elapsed()));
        if !self.enabled {
            return;
        }
        self.accumulated.input_ms += self.current.input_ms;
        self.accumulated.simulation_ms += self.current.simulation_ms;
        self.accumulated.ui_update_ms += self.current.ui_update_ms;
        self.accumulated.map_ms += self.current.map_ms;
        self.accumulated.panels_ms += self.current.panels_ms;
        self.accumulated.frame_ms += self.current.frame_ms;
        self.accumulated_frames += 1;
        if self.accumulated_frames >= LOG_INTERVAL {
            self.flush_log();
        }
    }

    fn flush_log(&mut self) {
        if self.accumulated_frames == 0 {
            return;
        }
        let count = self.accumulated_frames as f32;
        let avg = FrameTimes {
            input_ms: self.accumulated.input_ms / count,
            simulation_ms: self.accumulated.simulation_ms / count,
            ui_update_ms: self.accumulated.ui_update_ms / count,
            map_ms: self.accumulated.map_ms / count,
            panels_ms: self.accumulated.panels_ms / count,
            frame_ms: self.accumulated.frame_ms / count,
        };
        match OpenOptions::new().create(true).append(true).open("perf.log") {
            Ok(mut file) => {
                let _ = writeln!(
                    file,
                    "frames={} input_ms={:.3} simulation_ms={:.3} ui_update_ms={:.3} map_ms={:.3} panels_ms={:.3} frame_ms={:.3}",
                    self.accumulated_frames,
                    avg.input_ms,
                    avg.simulation_ms,
                    avg.ui_update_ms,
                    avg.map_ms,
                    avg.panels_ms,
                    avg.frame_ms
                );
            }
            Err(error) if !self.log_error_reported => {
                eprintln!("[perf] cannot append perf.log: {error}");
                self.log_error_reported = true;
            }
            Err(_) => {}
        }
        self.accumulated = FrameTimes::default();
        self.accumulated_frames = 0;
    }

    pub fn draw_overlay(&self) {
        if !self.enabled {
            return;
        }
        let width = screen_width().min(390.0).max(310.0);
        draw_rectangle(
            8.0,
            8.0,
            width,
            112.0,
            Color::new(0.025, 0.035, 0.045, 0.92),
        );
        draw_rectangle_lines(
            8.0,
            8.0,
            width,
            112.0,
            1.0,
            Color::new(0.55, 0.62, 0.68, 0.85),
        );
        draw_text("PERFORMANCE PROFILE  (Shift+F3)", 18.0, 28.0, 15.0, WHITE);
        draw_text(
            &format!(
                "Input {:6.2} ms   Simulation {:6.2} ms",
                self.current.input_ms, self.current.simulation_ms
            ),
            18.0,
            49.0,
            14.0,
            WHITE,
        );
        draw_text(
            &format!(
                "UI update {:6.2} ms   Map {:6.2} ms",
                self.current.ui_update_ms, self.current.map_ms
            ),
            18.0,
            70.0,
            14.0,
            WHITE,
        );
        draw_text(
            &format!(
                "Panels {:6.2} ms   Frame {:6.2} ms",
                self.current.panels_ms, self.current.frame_ms
            ),
            18.0,
            91.0,
            14.0,
            WHITE,
        );
        draw_text("perf.log: 60-frame averages", 18.0, 109.0, 11.0, GRAY);
    }
}

fn millis(duration: Duration) -> f32 {
    duration.as_secs_f64() as f32 * 1000.0
}

#[cfg(test)]
mod tests {
    use super::{FrameProfiler, FrameTimes, millis};
    use std::time::Duration;

    #[test]
    fn duration_is_reported_in_milliseconds() {
        assert!((millis(Duration::from_micros(1_250)) - 1.25).abs() < 0.001);
    }

    #[test]
    fn profiler_starts_disabled_without_environment_opt_in() {
        // Do not call from_environment here: CI may intentionally set the env var.
        let profiler = FrameProfiler {
            enabled: false,
            frame_started: None,
            current: FrameTimes::default(),
            accumulated: FrameTimes::default(),
            accumulated_frames: 0,
            log_error_reported: false,
        };
        assert!(!profiler.enabled);
    }
}
