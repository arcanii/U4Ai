//! When to tell of a limit: once it reaches 90%, said once until it falls
//! below 90% again, and once a limit that reached 90% starts again (its
//! reset time moves later and its usage falls, or it falls 30 points).
//! Each account keeps its own, so switching accounts never compares one
//! with another, and an account's first read only notes where it stands.

use std::collections::{HashMap, HashSet};

use crate::providers::{Limit, Meter};

/// Where a limit warns.
pub const WARN_AT: f64 = 90.0;
/// How far a warned limit must fall, without its reset time moving, to count
/// as having started again.
const FALL: f64 = 30.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Alert {
    Warning { limit: String, percent: u32 },
    Reset { limit: String },
}

#[derive(Debug, Default)]
pub struct Alerts {
    /// The limits that have warned, `account/limit`.
    warned: HashSet<String>,
    /// Each limit as the last read had it.
    last: HashMap<String, Limit>,
}

impl Alerts {
    /// What to tell of after a read of `account` (its key, provider and
    /// id) found `meters`.
    pub fn check(&mut self, account: &str, meters: &[Meter]) -> Vec<Alert> {
        let mut out = Vec::new();
        for Meter { key, label, limit: now, .. } in meters {
            let id = format!("{account}/{key}");
            let before = self.last.insert(id.clone(), *now);
            let Some(before) = before else {
                // An account's first read: noted, and warned only where it's
                // already past, once.
                if now.percent >= WARN_AT {
                    self.warned.insert(id);
                }
                continue;
            };
            let started_again = (now.resets_at > before.resets_at && before.resets_at.is_some() && now.percent < before.percent)
                || now.percent <= before.percent - FALL;
            if self.warned.contains(&id) && started_again && now.percent < WARN_AT {
                self.warned.remove(&id);
                out.push(Alert::Reset { limit: label.clone() });
            } else if now.percent >= WARN_AT && self.warned.insert(id.clone()) {
                out.push(Alert::Warning { limit: label.clone(), percent: now.percent.round() as u32 });
            } else if now.percent < WARN_AT && !started_again {
                // Fell below without starting again (a little leeway given
                // back): it may warn again, but there's no reset to tell of.
                self.warned.remove(&id);
            }
        }
        out
    }

    /// Forgets an account signed out of.
    pub fn forget(&mut self, account: &str) {
        let prefix = format!("{account}/");
        self.warned.retain(|k| !k.starts_with(&prefix));
        self.last.retain(|k, _| !k.starts_with(&prefix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;

    fn at(percent: f64, reset: i64) -> Vec<Meter> {
        let limit = Limit { percent, resets_at: Some(Timestamp::from_second(reset).unwrap()) };
        vec![Meter { key: "five_hour".into(), label: "5-Hour Limit".into(), window: crate::providers::Window::Short, limit }]
    }

    #[test]
    fn a_limit_warns_once_at_90_and_tells_of_its_reset() {
        let mut a = Alerts::default();
        assert!(a.check("acc", &at(50.0, 1000)).is_empty(), "the first read only notes");
        assert!(a.check("acc", &at(85.0, 1000)).is_empty());
        assert_eq!(a.check("acc", &at(91.4, 1000)), [Alert::Warning { limit: "5-Hour Limit".into(), percent: 91 }]);
        assert!(a.check("acc", &at(95.0, 1000)).is_empty(), "said once");
        assert_eq!(a.check("acc", &at(2.0, 19000)), [Alert::Reset { limit: "5-Hour Limit".into() }]);
        assert!(a.check("acc", &at(3.0, 19000)).is_empty());
        assert_eq!(a.check("acc", &at(90.0, 19000)).len(), 1, "warns again in the next window");
    }

    #[test]
    fn a_reset_below_90_that_never_warned_says_nothing() {
        let mut a = Alerts::default();
        a.check("acc", &at(40.0, 1000));
        assert!(a.check("acc", &at(1.0, 19000)).is_empty());
    }

    #[test]
    fn a_fall_of_30_points_counts_as_starting_again_even_with_the_same_reset_time() {
        let mut a = Alerts::default();
        a.check("acc", &at(80.0, 1000));
        assert_eq!(a.check("acc", &at(92.0, 1000)).len(), 1);
        assert_eq!(a.check("acc", &at(60.0, 1000)), [Alert::Reset { limit: "5-Hour Limit".into() }]);
    }

    #[test]
    fn each_account_keeps_its_own_and_forgets_it_when_signed_out() {
        let mut a = Alerts::default();
        a.check("one", &at(10.0, 1000));
        a.check("two", &at(95.0, 1000));
        assert!(a.check("one", &at(12.0, 1000)).is_empty(), "the other account's 95% is not this one's");
        assert!(a.check("two", &at(96.0, 1000)).is_empty(), "already past at its first read");
        a.forget("two");
        assert!(a.check("two", &at(97.0, 1000)).is_empty(), "a first read again");
    }
}
