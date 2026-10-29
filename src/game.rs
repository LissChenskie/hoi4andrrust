//! Deterministic game-time state, kept separate from rendering and input.

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct GameDate {
    pub year: u32,
    pub month: u8,
    pub day: u8,
}

impl GameDate {
    pub fn is_valid(self) -> bool {
        let end = match self.month { 1|3|5|7|8|10|12=>31,4|6|9|11=>30,2 if is_leap_year(self.year)=>29,2=>28,_=>return false };
        self.year > 0 && self.year < u32::MAX && self.day > 0 && self.day <= end
    }

    pub const START: Self = Self {
        year: 1936,
        month: 1,
        day: 1,
    };

    pub fn advance_one_day(&mut self) {
        let days_in_month = match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap_year(self.year) => 29,
            2 => 28,
            _ => {
                *self = Self::START;
                return;
            }
        };

        if self.day < days_in_month {
            self.day += 1;
        } else {
            self.day = 1;
            if self.month < 12 {
                self.month += 1;
            } else {
                self.month = 1;
                self.year += 1;
            }
        }
    }

    /// Predicted date for a construction project whose current daily
    /// factory allocation and speed remain unchanged.
    pub fn after_days(self, days: u32) -> Option<Self> {
        // Far-future predictions are not meaningful for the prototype and
        // should not stall a UI frame with millions of date operations.
        if days > 100_000 {
            return None;
        }
        let mut result = self;
        for _ in 0..days {
            result.advance_one_day();
        }
        Some(result)
    }

    pub fn display(self) -> String {
        format!("{:04}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

fn is_leap_year(year: u32) -> bool {
    year % 400 == 0 || (year % 4 == 0 && year % 100 != 0)
}

/// Core simulation clock. Rendering code can read this state but does not own time rules.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GameState {
    pub date: GameDate,
    pub hour: u8,
    pub elapsed_hours: u64,
    pub paused: bool,
    /// Simulation speed: one in-game day every SECONDS_PER_DAY / speed real seconds.
    pub speed: u8,
    #[serde(skip)]
    accumulator: f64,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            date: GameDate::START,
            hour: 0,
            elapsed_hours: 0,
            paused: true,
            speed: 1,
            accumulator: 0.0,
        }
    }
}

impl GameState {
    pub const SECONDS_PER_DAY: f32 = 1.0;
    /// Limit catch-up after backgrounding the game. Work beyond one real
    /// second of lag is deliberately discarded instead of freezing the UI.
    pub const MAX_FRAME_DELTA: f32 = 1.0;
    /// Bound simulation work while retaining unfinished days for later frames.
    pub const HOURS_PER_DAY: u32 = 24;
    pub const MAX_HOURLY_TICKS_PER_FRAME: u32 = 192;
    pub const MAX_BACKLOG_HOURS: f64 = 768.0;

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn set_speed(&mut self, speed: u8) {
        self.speed = speed.clamp(1, 5);
    }

    /// Schedule whole hours, without advancing the calendar ahead of simulation.
    /// The session consumes these in order, committing each hour and midnight once.
    pub fn accumulate_hours(&mut self, delta_seconds: f32) -> u32 {
        if self.paused || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return 0;
        }

        let speed_multiplier = match self.speed {
            1 => 1.0,
            2 => 2.0,
            3 => 4.0,
            4 => 8.0,
            _ => 16.0,
        };
        // Keep existing days/second at each speed. Hourly precision does not
        // make the campaign 24 times slower. Retain bounded unfinished work.
        self.accumulator = (self.accumulator
            + f64::from(delta_seconds.min(Self::MAX_FRAME_DELTA))
                * speed_multiplier
                * f64::from(Self::HOURS_PER_DAY)
                / f64::from(Self::SECONDS_PER_DAY))
        .min(Self::MAX_BACKLOG_HOURS);
        // f32 frame durations can land just below an exact hour boundary.
        let ticks =
            ((self.accumulator + 1.0e-6).floor() as u32).min(Self::MAX_HOURLY_TICKS_PER_FRAME);
        self.accumulator = (self.accumulator - f64::from(ticks)).max(0.0);
        ticks
    }

    /// Returns the completed day at midnight. Daily systems use THAT day's
    /// date for research penalties, including across month/year boundaries.
    pub fn advance_one_hour(&mut self) -> Option<GameDate> {
        self.elapsed_hours += 1;
        self.hour += 1;
        if self.hour < Self::HOURS_PER_DAY as u8 {
            return None;
        }
        self.hour = 0;
        let ended = self.date;
        self.date.advance_one_day();
        Some(ended)
    }

    pub fn display(&self) -> String {
        format!("{} {:02}:00", self.date.display(), self.hour)
    }
}

#[cfg(test)]
mod tests {
    use super::{GameDate, GameState};

    #[test]
    fn starts_at_hoi4_default_bookmark() {
        assert_eq!(GameState::default().date, GameDate::START);
        assert!(GameState::default().paused);
    }

    #[test]
    fn date_rolls_over_month_and_year() {
        let mut date = GameDate {
            year: 1936,
            month: 1,
            day: 31,
        };
        date.advance_one_day();
        assert_eq!(
            date,
            GameDate {
                year: 1936,
                month: 2,
                day: 1
            }
        );

        let mut date = GameDate {
            year: 1936,
            month: 12,
            day: 31,
        };
        date.advance_one_day();
        assert_eq!(
            date,
            GameDate {
                year: 1937,
                month: 1,
                day: 1
            }
        );
    }

    #[test]
    fn completion_date_prediction_advances_across_leap_february() {
        let day = GameDate {
            year: 1936,
            month: 2,
            day: 27,
        };
        assert_eq!(
            day.after_days(3),
            Some(GameDate {
                year: 1936,
                month: 3,
                day: 1
            })
        );
        assert_eq!(day.after_days(0), Some(day));
        assert_eq!(day.after_days(100_001), None);
    }

    #[test]
    fn handles_leap_years() {
        let mut leap = GameDate {
            year: 1936,
            month: 2,
            day: 28,
        };
        leap.advance_one_day();
        assert_eq!(leap.day, 29);
        leap.advance_one_day();
        assert_eq!(
            leap,
            GameDate {
                year: 1936,
                month: 3,
                day: 1
            }
        );

        let mut ordinary = GameDate {
            year: 1937,
            month: 2,
            day: 28,
        };
        ordinary.advance_one_day();
        assert_eq!(
            ordinary,
            GameDate {
                year: 1937,
                month: 3,
                day: 1
            }
        );
    }

    fn consume(game: &mut GameState, delta: f32) -> u32 {
        let hours = game.accumulate_hours(delta);
        for _ in 0..hours {
            game.advance_one_hour();
        }
        hours
    }

    #[test]
    fn hours_advance_only_when_unpaused_and_keep_existing_speed() {
        let mut game = GameState::default();
        assert_eq!(consume(&mut game, 1.0), 0);
        game.paused = false;
        assert_eq!(consume(&mut game, 0.25), 6);
        assert_eq!(game.hour, 6);
        assert_eq!(game.date, GameDate::START);
        game.set_speed(3);
        assert_eq!(consume(&mut game, 0.25), 24);
        assert_eq!(game.hour, 6);
        assert_eq!(
            game.date,
            GameDate {
                year: 1936,
                month: 1,
                day: 2
            }
        );
    }

    #[test]
    fn scheduling_does_not_advance_calendar_before_simulation_runs() {
        let mut game = GameState::default();
        game.paused = false;
        assert_eq!(game.accumulate_hours(1.0), 24);
        assert_eq!(game.hour, 0);
        assert_eq!(game.date, GameDate::START);
        for _ in 0..23 {
            assert_eq!(game.advance_one_hour(), None);
        }
        assert_eq!(game.advance_one_hour(), Some(GameDate::START));
        assert_eq!(game.hour, 0);
        assert_eq!(game.elapsed_hours, 24);
    }

    #[test]
    fn hour_rollover_handles_leap_day_and_year_end() {
        let mut game = GameState::default();
        game.date = GameDate {
            year: 1936,
            month: 2,
            day: 28,
        };
        game.hour = 23;
        game.advance_one_hour();
        assert_eq!(
            game.date,
            GameDate {
                year: 1936,
                month: 2,
                day: 29
            }
        );
        game.date = GameDate {
            year: 1936,
            month: 12,
            day: 31,
        };
        game.hour = 23;
        game.advance_one_hour();
        assert_eq!(
            game.date,
            GameDate {
                year: 1937,
                month: 1,
                day: 1
            }
        );
        assert_eq!(game.display(), "1937.01.01 00:00");
    }

    #[test]
    fn large_frame_spikes_keep_bounded_backlog_and_pause_it() {
        let mut game = GameState::default();
        game.paused = false;
        game.set_speed(5);
        assert_eq!(consume(&mut game, 100.0), 192);
        game.paused = true;
        assert_eq!(consume(&mut game, 0.01), 0);
        game.paused = false;
        assert_eq!(consume(&mut game, 0.01), 192);
        assert_eq!(game.elapsed_hours, 384);
        assert_eq!(
            game.date,
            GameDate {
                year: 1936,
                month: 1,
                day: 17
            }
        );
    }

    #[test]
    fn frame_slicing_preserves_whole_hours() {
        let mut a = GameState::default();
        let mut b = GameState::default();
        a.paused = false;
        b.paused = false;
        a.set_speed(3);
        b.set_speed(3);
        for _ in 0..2 {
            consume(&mut a, 0.5);
        }
        for _ in 0..10 {
            consume(&mut b, 0.1);
        }
        assert_eq!(a.elapsed_hours, b.elapsed_hours);
        assert_eq!(a.hour, b.hour);
        assert_eq!(a.date, b.date);
    }

    #[test]
    fn rejects_invalid_deltas_without_changing_time() {
        let mut game = GameState::default();
        game.paused = false;
        for delta in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            assert_eq!(consume(&mut game, delta), 0);
        }
        assert_eq!(game.date, GameDate::START);
        assert_eq!(game.elapsed_hours, 0);
    }
}
