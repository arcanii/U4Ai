//! When to read the usage again: the Mac app's smart mode and its
//! rate-limit backoff (`UserSettings+SmartMode.swift`,
//! `DataRefreshManager.swift`), as plain state the tests step through.
//!
//! Smart mode reads every minute while usage moves, and every 3, 5 and 10
//! minutes after 3, 6 and 12 reads that found it unchanged; any change is
//! every minute again. Fixed mode reads at its own interval. A 429 holds
//! every read until the server's `Retry-After` (kept within 30 s and 30
//! min), else 2 min, then double each time to 30 min, until a read works.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How often to read: smart, or every so many minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Smart,
    Fixed(u32),
}

/// Smart mode's pace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Active,
    IdleShort,
    IdleMedium,
    IdleLong,
}

impl Tier {
    pub fn interval(self) -> Duration {
        Duration::from_secs(60 * match self {
            Tier::Active => 1,
            Tier::IdleShort => 3,
            Tier::IdleMedium => 5,
            Tier::IdleLong => 10,
        })
    }

    /// The tier after `unchanged` reads in a row that found nothing new,
    /// none while it stays.
    fn next(self, unchanged: u32) -> Option<Tier> {
        match self {
            Tier::Active if unchanged >= 3 => Some(Tier::IdleShort),
            Tier::IdleShort if unchanged >= 6 => Some(Tier::IdleMedium),
            Tier::IdleMedium if unchanged >= 12 => Some(Tier::IdleLong),
            _ => None,
        }
    }
}

/// The first wait after a 429 without `Retry-After`, the longest, and the
/// shortest a server's own may make it.
const BASE_BACKOFF: Duration = Duration::from_secs(120);
const MAX_BACKOFF: Duration = Duration::from_secs(1800);
const MIN_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Refresh {
    pub mode: Mode,
    tier: Tier,
    last: Option<f64>,
    unchanged: u32,
    backoff: Option<Duration>,
    limited_until: Option<Instant>,
}

impl Refresh {
    pub fn new(mode: Mode) -> Refresh {
        Refresh { mode, tier: Tier::Active, last: None, unchanged: 0, backoff: None, limited_until: None }
    }

    pub fn tier(&self) -> Tier {
        self.tier
    }

    /// How long from a read to the next.
    pub fn interval(&self) -> Duration {
        match self.mode {
            Mode::Smart => self.tier.interval(),
            Mode::Fixed(minutes) => Duration::from_secs(60 * u64::from(minutes.clamp(1, 60))),
        }
    }

    /// A read worked, finding `percent` on the outer ring: any backoff ends,
    /// and smart mode keeps its pace or changes it.
    pub fn read(&mut self, percent: f64) {
        self.backoff = None;
        self.limited_until = None;
        if self.mode == Mode::Smart {
            let changed = self.last.is_some_and(|last| (percent - last).abs() > 0.01);
            if changed {
                self.tier = Tier::Active;
                self.unchanged = 0;
            } else {
                self.unchanged += 1;
                if let Some(next) = self.tier.next(self.unchanged) {
                    self.tier = next;
                    self.unchanged = 0;
                }
            }
        }
        self.last = Some(percent);
    }

    /// The user asked (Refresh, or switched account): every minute again,
    /// as the Mac's popover does, though a 429's hold stands.
    pub fn hurry(&mut self) {
        self.tier = Tier::Active;
        self.unchanged = 0;
        self.last = None;
    }

    /// A 429: no read until the time this gives back.
    pub fn rate_limited(&mut self, retry_after: Option<Duration>, now: Instant) -> Instant {
        let wait = match retry_after.filter(|r| !r.is_zero()) {
            Some(server) => server.clamp(MIN_BACKOFF, MAX_BACKOFF),
            None => match self.backoff {
                None => BASE_BACKOFF,
                Some(before) => (before * 2).min(MAX_BACKOFF),
            },
        };
        self.backoff = Some(wait);
        let until = now + wait;
        self.limited_until = Some(until);
        until
    }

    /// Until when reads are held, where they are.
    pub fn limited_until(&self, now: Instant) -> Option<Instant> {
        self.limited_until.filter(|until| *until > now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_mode_slows_after_reads_that_find_nothing_new_and_hurries_on_a_change() {
        let mut r = Refresh::new(Mode::Smart);
        r.read(10.0); // the first read compares with nothing
        assert_eq!(r.tier(), Tier::Active);
        for _ in 0..2 {
            r.read(10.0);
        }
        assert_eq!(r.tier(), Tier::IdleShort, "three unchanged in a row");
        for _ in 0..6 {
            r.read(10.0);
        }
        assert_eq!(r.tier(), Tier::IdleMedium);
        for _ in 0..12 {
            r.read(10.0);
        }
        assert_eq!(r.tier(), Tier::IdleLong);
        assert_eq!(r.interval(), Duration::from_secs(600));
        for _ in 0..50 {
            r.read(10.0);
        }
        assert_eq!(r.tier(), Tier::IdleLong, "no slower than every 10 minutes");
        r.read(10.005);
        assert_eq!(r.tier(), Tier::IdleLong, "a hundredth of a percent is no change");
        r.read(11.0);
        assert_eq!((r.tier(), r.interval()), (Tier::Active, Duration::from_secs(60)));
    }

    #[test]
    fn fixed_mode_keeps_its_interval() {
        let mut r = Refresh::new(Mode::Fixed(5));
        for p in [1.0, 1.0, 1.0, 1.0, 2.0] {
            r.read(p);
        }
        assert_eq!(r.interval(), Duration::from_secs(300));
        assert_eq!(Refresh::new(Mode::Fixed(0)).interval(), Duration::from_secs(60), "never every 0 minutes");
    }

    #[test]
    fn a_429_holds_reads_doubling_to_half_an_hour_until_one_works() {
        let now = Instant::now();
        let mut r = Refresh::new(Mode::Smart);
        let waits: Vec<u64> = (0..6).map(|_| (r.rate_limited(None, now) - now).as_secs()).collect();
        assert_eq!(waits, [120, 240, 480, 960, 1800, 1800]);
        assert!(r.limited_until(now).is_some());
        assert!(r.limited_until(now + Duration::from_secs(1801)).is_none());
        r.read(5.0);
        assert!(r.limited_until(now).is_none());
        assert_eq!((r.rate_limited(None, now) - now).as_secs(), 120, "a read that worked starts it over");
    }

    #[test]
    fn the_servers_retry_after_wins_within_bounds() {
        let now = Instant::now();
        let mut r = Refresh::new(Mode::Smart);
        assert_eq!((r.rate_limited(Some(Duration::from_secs(90)), now) - now).as_secs(), 90);
        assert_eq!((r.rate_limited(Some(Duration::from_secs(1)), now) - now).as_secs(), 30, "never a tight loop");
        assert_eq!((r.rate_limited(Some(Duration::from_secs(99_999)), now) - now).as_secs(), 1800);
        assert_eq!((r.rate_limited(Some(Duration::ZERO), now) - now).as_secs(), 1800, "a zero is no Retry-After: doubled from the last");
    }
}
