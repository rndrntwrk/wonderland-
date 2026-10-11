//! Simulation time; no wall-clock access. Source: FreeSO VMClock.cs.
use serde::{Deserialize, Serialize};

pub const DOTNET_DATETIME_MAX: i64 = 3_155_378_975_999_999_999;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimClock {
    pub ticks: u64,
    pub minute_fractions: i32,
    pub ticks_per_minute: i32,
    pub minutes: i32,
    pub hours: i32,
    pub day_of_month: i32,
    pub month: i32,
    pub year: i32,
    pub fire_percent: i32,
    pub utc_start_dotnet_ticks: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    InvalidState,
    Overflow,
}

impl SimClock {
    pub fn new(ts1: bool, utc_start_dotnet_ticks: i64) -> Self {
        Self {
            ticks: 0,
            minute_fractions: 0,
            ticks_per_minute: if ts1 { 30 } else { 150 },
            minutes: 0,
            hours: 0,
            day_of_month: 1,
            month: 6,
            year: 1997,
            fire_percent: 20000,
            utc_start_dotnet_ticks,
        }
    }
    pub fn advance(&mut self) -> Result<(), ClockError> {
        self.validate()?;
        let next_tick = self.ticks.checked_add(1).ok_or(ClockError::Overflow)?;
        self.utc_at_tick(next_tick)?;
        // Validate the rare year rollover before changing any field.
        if self.minute_fractions + 1 == self.ticks_per_minute
            && self.minutes == 59
            && self.hours == 23
            && self.day_of_month == 30
            && self.month == 12
            && self.year == i32::MAX
        {
            return Err(ClockError::Overflow);
        }
        if self.fire_percent < 20_000 {
            self.fire_percent += 1;
        }
        self.minute_fractions += 1;
        if self.minute_fractions >= self.ticks_per_minute {
            self.minute_fractions = 0;
            self.minutes += 1;
            if self.minutes >= 60 {
                self.minutes = 0;
                self.hours += 1;
                if self.hours >= 24 {
                    self.hours = 0;
                    self.day_of_month += 1;
                    if self.day_of_month > 30 {
                        self.day_of_month = 1;
                        self.month += 1;
                        if self.month > 12 {
                            self.month = 1;
                            self.year += 1;
                        }
                    }
                }
            }
        }
        self.ticks = next_tick;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), ClockError> {
        if self.ticks_per_minute <= 0
            || !(0..self.ticks_per_minute).contains(&self.minute_fractions)
            || !(0..60).contains(&self.minutes)
            || !(0..24).contains(&self.hours)
            || !(1..=30).contains(&self.day_of_month)
            || !(1..=12).contains(&self.month)
            || self.year < 1
            || !(0..=DOTNET_DATETIME_MAX).contains(&self.utc_start_dotnet_ticks)
        {
            return Err(ClockError::InvalidState);
        }
        self.utc_at_tick(self.ticks)?;
        Ok(())
    }
    pub fn seconds(&self) -> Result<i32, ClockError> {
        self.validate()?;
        Ok(((i64::from(self.minute_fractions) * 60) / i64::from(self.ticks_per_minute)) as i32)
    }
    pub fn utc_dotnet_ticks(&self) -> Result<i64, ClockError> {
        self.validate()?;
        self.utc_at_tick(self.ticks)
    }
    fn utc_at_tick(&self, tick: u64) -> Result<i64, ClockError> {
        // The pinned Mono DateTime.AddSeconds implementation rounds to whole
        // milliseconds. At 30Hz the remainder is 0, 1/3 or 2/3 millisecond;
        // exact integer rounding reproduces that result throughout DateTime's
        // valid range without depending on the host floating-point library.
        let elapsed = ((i128::from(tick) * 1000 + 15) / 30) * 10_000;
        let result = i128::from(self.utc_start_dotnet_ticks) + elapsed;
        if result > i128::from(DOTNET_DATETIME_MAX) {
            return Err(ClockError::Overflow);
        }
        i64::try_from(result).map_err(|_| ClockError::Overflow)
    }
    /// Standard-time memory uses the Gregorian UTC calendar; the game calendar
    /// above deliberately retains the source's thirty-day months.
    pub fn standard_component(&self, index: i16) -> Result<i16, ClockError> {
        let ticks = self.utc_dotnet_ticks()?;
        let seconds = ticks / 10_000_000;
        match index {
            0 => Ok((seconds % 60) as i16),
            1 => Ok((seconds / 60 % 60) as i16),
            2 => Ok((seconds / 3600 % 24) as i16),
            3..=5 => {
                let mut day = seconds / 86400;
                let y400 = day / 146097;
                day %= 146097;
                let y100 = (day / 36524).min(3);
                day -= y100 * 36524;
                let y4 = day / 1461;
                day %= 1461;
                let y1 = (day / 365).min(3);
                day -= y1 * 365;
                let year = y400 * 400 + y100 * 100 + y4 * 4 + y1 + 1;
                let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
                let lengths = [
                    31,
                    if leap { 29 } else { 28 },
                    31,
                    30,
                    31,
                    30,
                    31,
                    31,
                    30,
                    31,
                    30,
                    31,
                ];
                let mut month = 0;
                while day >= lengths[month] {
                    day -= lengths[month];
                    month += 1;
                }
                Ok(match index {
                    3 => (day + 1) as i16,
                    4 => (month + 1) as i16,
                    _ => year as i16,
                })
            }
            _ => Ok(0),
        }
    }
}
