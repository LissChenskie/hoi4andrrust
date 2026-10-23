//! Deterministic game-time state, kept separate from rendering and input.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameDate {
    pub year: u32,
    pub month: u8,
    pub day: u8,
}

impl GameDate {
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

    pub fn display(self) -> String {
        format!("{:04}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

fn is_leap_year(year: u32) -> bool {
    year % 400 == 0 || (year % 4 == 0 && year % 100 != 0)
}

/// Core simulation clock. Rendering code can read this state but does not own time rules.
#[derive(Debug, Clone)]
pub struct GameState {
    pub date: GameDate,
    pub paused: bool,
    /// Simulation speed: one in-game day every SECONDS_PER_DAY / speed real seconds.
    pub speed: u8,
    accumulator: f32,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            date: GameDate::START,
            paused: true,
            speed: 1,
            accumulator: 0.0,
        }
    }
}

impl GameState {
    pub const SECONDS_PER_DAY: f32 = 1.0;

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn set_speed(&mut self, speed: u8) {
        self.speed = speed.clamp(1, 5);
    }

    /// Advance using elapsed real time. A frame hitch is capped to avoid huge catch-up loops.
    pub fn update(&mut self, delta_seconds: f32) {
        if self.paused || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }

        self.accumulator += delta_seconds.min(0.25);
        let speed_multiplier = match self.speed { 1 => 1.0, 2 => 2.0, 3 => 4.0, 4 => 8.0, _ => 16.0 };
        let day_duration = Self::SECONDS_PER_DAY / speed_multiplier;
        while self.accumulator >= day_duration {
            self.accumulator -= day_duration;
            self.date.advance_one_day();
        }
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
        let mut date = GameDate { year: 1936, month: 1, day: 31 };
        date.advance_one_day();
        assert_eq!(date, GameDate { year: 1936, month: 2, day: 1 });

        let mut date = GameDate { year: 1936, month: 12, day: 31 };
        date.advance_one_day();
        assert_eq!(date, GameDate { year: 1937, month: 1, day: 1 });
    }

    #[test]
    fn handles_leap_years() {
        let mut leap = GameDate { year: 1936, month: 2, day: 28 };
        leap.advance_one_day();
        assert_eq!(leap.day, 29);
        leap.advance_one_day();
        assert_eq!(leap, GameDate { year: 1936, month: 3, day: 1 });

        let mut ordinary = GameDate { year: 1937, month: 2, day: 28 };
        ordinary.advance_one_day();
        assert_eq!(ordinary, GameDate { year: 1937, month: 3, day: 1 });
    }

    #[test]
    fn clock_advances_only_when_unpaused_and_respects_speed() {
        let mut game = GameState::default();
        game.update(1.0);
        assert_eq!(game.date, GameDate::START);

        game.paused = false;
        game.update(0.25);
        assert_eq!(game.date, GameDate::START);

        game.set_speed(3);
        game.update(0.25);
        assert_eq!(game.date, GameDate { year: 1936, month: 1, day: 2 });
    }
}
