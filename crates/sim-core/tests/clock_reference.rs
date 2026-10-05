use sim_core::clock::{ClockError, SimClock};

#[test]
fn tso_and_ts1_have_distinct_source_game_clock_rates() {
    let mut tso = SimClock::new(false, 0);
    let mut ts1 = SimClock::new(true, 0);
    for _ in 0..150 {
        tso.advance().unwrap();
        ts1.advance().unwrap();
    }
    assert_eq!((tso.minutes, ts1.minutes), (1, 5));
    assert_eq!((tso.ticks, ts1.ticks), (150, 150));
    assert_eq!(tso.utc_dotnet_ticks().unwrap(), 50_000_000);
}

#[test]
fn calendar_uses_legacy_thirty_day_months() {
    let mut c = SimClock::new(true, 0);
    c.hours = 23;
    c.minutes = 59;
    c.minute_fractions = 29;
    c.day_of_month = 30;
    c.month = 12;
    c.year = 2001;
    c.advance().unwrap();
    assert_eq!(
        (
            c.year,
            c.month,
            c.day_of_month,
            c.hours,
            c.minutes,
            c.minute_fractions
        ),
        (2002, 1, 1, 0, 0, 0)
    );
}

#[test]
fn fire_recovery_stops_at_the_source_cap() {
    let mut c = SimClock::new(false, 0);
    c.fire_percent = 19999;
    c.advance().unwrap();
    c.advance().unwrap();
    assert_eq!(c.fire_percent, 20000);
    c.fire_percent = 20001;
    c.advance().unwrap();
    assert_eq!(c.fire_percent, 20001);
}

#[test]
fn seconds_are_derived_from_tick_fraction_without_real_time() {
    let mut c = SimClock::new(false, 123);
    for _ in 0..75 {
        c.advance().unwrap();
    }
    assert_eq!(c.seconds().unwrap(), 30);
    assert_eq!(c.utc_dotnet_ticks().unwrap(), 25_000_123);
}

#[test]
fn corrupt_or_overflowing_clocks_fail_atomically() {
    let mut c = SimClock::new(false, 0);
    c.ticks_per_minute = 0;
    let saved = c.clone();
    assert_eq!(c.advance(), Err(ClockError::InvalidState));
    assert_eq!(c, saved);
    c.ticks_per_minute = 150;
    c.ticks = u64::MAX;
    let saved = c.clone();
    assert_eq!(c.advance(), Err(ClockError::Overflow));
    assert_eq!(c, saved);
}

#[test]
fn standard_time_matches_mono_millisecond_rounding() {
    let mut c = SimClock::new(false, 0);
    c.advance().unwrap();
    assert_eq!(c.utc_dotnet_ticks().unwrap(), 330_000);
    c.advance().unwrap();
    assert_eq!(c.utc_dotnet_ticks().unwrap(), 670_000);
    c.advance().unwrap();
    assert_eq!(c.utc_dotnet_ticks().unwrap(), 1_000_000);
}

#[test]
fn standard_time_rejects_dotnet_datetime_domain_overflow() {
    let mut c = SimClock::new(false, 3_155_378_975_999_999_999);
    assert_eq!(c.utc_dotnet_ticks().unwrap(), 3_155_378_975_999_999_999);
    let before = c.clone();
    assert_eq!(c.advance(), Err(ClockError::Overflow));
    assert_eq!(c, before);
    c.utc_start_dotnet_ticks = i64::MAX;
    assert_eq!(c.validate(), Err(ClockError::InvalidState));
}
