//! Claude, the first provider: Anthropic's OAuth endpoints (`api`), its
//! sign-in (`sign_in`), and its usage answer (`usage`), ported from the Mac's
//! U4Claude; here they become a `Reading` of meters: the 5-hour window, the
//! 7-day, each weekly model's, and Extra Usage as a line.

pub mod api;
pub mod sign_in;
pub mod usage;

use super::{Limit, Meter, Reader, Reading, ReadError, SignIn, SignInError, Window};
use crate::i18n::{t, tf};
use usage::{UsageData, WeeklyModel};

pub const STATUS_PAGE: &str = "https://status.claude.com";

pub fn sign_in() -> Result<SignIn, SignInError> {
    let pending = sign_in::start()?;
    let url = pending.url.clone();
    Ok(SignIn::new(url, move |cancel| pending.finish(&api::Endpoints::anthropic(), cancel)))
}

pub fn reader(secret: String) -> Box<dyn Reader> {
    Box::new(api::Session::new(api::Endpoints::anthropic(), secret))
}

impl Reader for api::Session {
    fn read(&mut self) -> Result<Reading, ReadError> {
        api::Session::read(self).map(|data| reading(&data))
    }

    fn take_rotated(&mut self) -> Option<String> {
        api::Session::take_rotated(self)
    }
}

/// Claude's usage as meters and lines.
pub fn reading(data: &UsageData) -> Reading {
    let mut meters = Vec::new();
    let meter = |key: &str, label: String, window: Window, limit: Limit| Meter { key: key.into(), label, window, limit };
    if let Some(five) = data.five_hour {
        meters.push(meter("five_hour", t("usage.five_hour").into(), Window::Short, five));
    }
    meters.push(meter("seven_day", t("usage.seven_day").into(), Window::Long, data.seven_day));
    let weekly = |slot: &str, fallback: &str, model: WeeklyModel| match &model.name {
        Some(name) => Meter { key: format!("model:{name}"), label: tf("usage.model", &[("model", name)]), window: Window::Model, limit: model.limit },
        None => meter(slot, t(fallback).into(), Window::Model, model.limit),
    };
    if let Some(opus) = data.opus() {
        meters.push(weekly("opus", "usage.opus", opus));
    }
    if let Some(sonnet) = data.sonnet() {
        meters.push(weekly("sonnet", "usage.sonnet", sonnet));
    }
    for model in data.overflow() {
        meters.push(weekly("model", "usage.opus", model.clone()));
    }
    let mut lines = Vec::new();
    if let Some(extra) = data.extra.as_ref().filter(|e| e.enabled) {
        let sym = extra.symbol();
        let used = format!("{sym}{:.2}", extra.used.unwrap_or(0.0));
        let total = format!("{sym}{:.0}", extra.limit.unwrap_or(0.0));
        lines.push(tf("usage.extra_row", &[("limit", &t("usage.extra")), ("used", &used), ("total", &total)]));
    }
    Reading { meters, lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::set_language;
    use usage::Extra;

    #[test]
    fn claudes_usage_reads_as_its_windows_its_models_and_extra_usage() {
        set_language("en", &[]);
        let data = UsageData {
            five_hour: Some(Limit { percent: 42.0, resets_at: None }),
            seven_day: Limit { percent: 76.0, resets_at: None },
            legacy_opus: None,
            legacy_sonnet: Some(Limit { percent: 30.0, resets_at: None }),
            scoped: vec![WeeklyModel { name: Some("Fable".into()), limit: Limit { percent: 13.0, resets_at: None } }],
            extra: Some(Extra { enabled: true, used: Some(12.34), limit: Some(50.0), currency: "USD".into() }),
        };
        let r = reading(&data);
        let shown: Vec<(&str, &str, Window)> = r.meters.iter().map(|m| (m.key.as_str(), m.label.as_str(), m.window)).collect();
        assert_eq!(
            shown,
            [
                ("five_hour", "5-Hour Limit", Window::Short),
                ("seven_day", "7-Day Limit", Window::Long),
                ("model:Fable", "7D Fable Limit", Window::Model),
                ("sonnet", "7D Sonnet Limit", Window::Model),
            ]
        );
        assert_eq!(r.lines, ["Extra Usage: $12.34 / $50"]);
        let without = reading(&UsageData { five_hour: None, extra: None, ..data });
        assert_eq!(without.primary().unwrap().key, "seven_day");
        assert!(without.lines.is_empty());
    }
}
