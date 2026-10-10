//! Weekly time windows (the speed-limit and queue schedules) and the clock they are read with.
//!
//! A window that crosses midnight (`from` later than `to`, e.g. 22:00–06:00) belongs to the
//! day it STARTS on: with only Monday ticked, it covers Monday 22:00 to Tuesday 06:00, so
//! Tuesday 03:00 is inside and Monday 03:00 is not. `from == to` means the whole day.

/// The local time the schedules are read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalNow {
    /// Milliseconds since the Unix epoch.
    pub unix_ms: i64,
    /// Monday = 0 … Sunday = 6.
    pub weekday: u8,
    /// Minutes after local midnight, 0..1440.
    pub minute: u16,
}

/// Where the manager reads the time from. Tests use a fake one.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> LocalNow;
}

/// Window days and times. `days` is Monday (0) to Sunday (6); times are minutes after midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub days: [bool; 7],
    pub from: u16,
    pub to: u16,
}

/// Whether `now` falls inside `w`.
pub fn active(now: LocalNow, w: &Window) -> bool {
    let today = w.days[(now.weekday % 7) as usize];
    let yesterday = w.days[((now.weekday + 6) % 7) as usize];
    let m = now.minute;
    match w.from.cmp(&w.to) {
        std::cmp::Ordering::Equal => today,
        std::cmp::Ordering::Less => today && m >= w.from && m < w.to,
        // Crosses midnight: the evening part is today's, the morning part yesterday's.
        std::cmp::Ordering::Greater => (today && m >= w.from) || (yesterday && m < w.to),
    }
}

/// The real local time of this PC.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> LocalNow {
        use chrono::{Datelike, Timelike};
        let t = chrono::Local::now();
        LocalNow {
            unix_ms: t.timestamp_millis(),
            weekday: t.weekday().num_days_from_monday() as u8,
            minute: (t.hour() * 60 + t.minute()) as u16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: u8 = 0;
    const TUE: u8 = 1;
    const SUN: u8 = 6;

    fn at(weekday: u8, h: u16, m: u16) -> LocalNow {
        LocalNow {
            unix_ms: 0,
            weekday,
            minute: h * 60 + m,
        }
    }

    fn only(day: u8, from: u16, to: u16) -> Window {
        let mut days = [false; 7];
        days[day as usize] = true;
        Window { days, from, to }
    }

    #[test]
    fn window_across_midnight() {
        // Monday 22:00–06:00: it belongs to Monday, the day it starts on.
        let w = only(MON, 22 * 60, 6 * 60);
        assert!(active(at(MON, 22, 0), &w), "start is inside");
        assert!(active(at(MON, 23, 59), &w));
        assert!(active(at(TUE, 0, 0), &w), "midnight is inside");
        assert!(
            active(at(TUE, 3, 0), &w),
            "Tuesday 03:00 is Monday's window"
        );
        assert!(!active(at(TUE, 6, 0), &w), "the stop time is outside");
        assert!(
            !active(at(MON, 3, 0), &w),
            "Monday 03:00 is Sunday's window"
        );
        assert!(!active(at(MON, 21, 59), &w));
        assert!(!active(at(TUE, 22, 0), &w), "Tuesday is not ticked");
        // Sunday's window runs into Monday morning (the week wraps).
        let w = only(SUN, 23 * 60, 2 * 60);
        assert!(active(at(MON, 1, 0), &w));
        assert!(!active(at(SUN, 1, 0), &w));
    }

    #[test]
    fn day_filter() {
        let w = only(TUE, 9 * 60, 17 * 60);
        assert!(active(at(TUE, 9, 0), &w));
        assert!(active(at(TUE, 16, 59), &w));
        assert!(!active(at(TUE, 17, 0), &w));
        assert!(!active(at(TUE, 8, 59), &w));
        for d in [0, 2, 3, 4, 5, 6] {
            assert!(!active(at(d, 12, 0), &w), "day {d} is not ticked");
        }
        let all = Window {
            days: [true; 7],
            from: 9 * 60,
            to: 17 * 60,
        };
        assert!((0..7).all(|d| active(at(d, 12, 0), &all)));
        let none = Window {
            days: [false; 7],
            ..all
        };
        assert!((0..7).all(|d| !active(at(d, 12, 0), &none)));
    }

    #[test]
    fn same_from_and_to_is_the_whole_day() {
        let w = only(MON, 8 * 60, 8 * 60);
        assert!(active(at(MON, 0, 0), &w));
        assert!(active(at(MON, 23, 59), &w));
        assert!(!active(at(TUE, 8, 0), &w));
    }
}
