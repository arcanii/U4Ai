//! What Claude's `/api/oauth/usage` answers, and the usage it's read into: a port of
//! the Mac app's `ClaudeAPIResponseModels.swift`, whose tests' JSON is
//! these tests' too, so a change in the API shows in both apps.
//!
//! The 5-hour window may be missing (plans without it, and organizations
//! without the usage dashboard). The 7-day window is always there, a 0%
//! placeholder where the API leaves it out. Weekly per-model limits come
//! from the dedicated `seven_day_opus` and `seven_day_sonnet` fields where
//! the API still fills them, and from `limits[]` (Claude 5's, named by the
//! model) for the rest: two slots, each its legacy field first, then the
//! scoped models in the API's order, and any further model a row of its
//! own (`overflow`).

use jiff::{Timestamp, Unit};
use serde::Deserialize;

pub use crate::providers::Limit;

/// One limit window as the API gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct LimitUsage {
    /// 0–100, perhaps fractional. Lenient where the Mac's decoder isn't: a
    /// null reads as 0 rather than failing the whole answer.
    #[serde(default)]
    pub utilization: Option<f64>,
    /// RFC 3339; none before usage starts.
    #[serde(default)]
    pub resets_at: Option<String>,
}

/// An entry in the API's `limits[]`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LimitEntry {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub percent: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub scope: Option<Scope>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Scope {
    #[serde(default)]
    pub model: Option<ScopeModel>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScopeModel {
    #[serde(default)]
    pub display_name: Option<String>,
}

/// The usage answer as the API gives it. Unknown keys (codenamed models'
/// windows) are passed over.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UsageResponse {
    #[serde(default)]
    pub five_hour: Option<LimitUsage>,
    #[serde(default)]
    pub seven_day: Option<LimitUsage>,
    #[serde(default)]
    pub seven_day_oauth_apps: Option<LimitUsage>,
    #[serde(default)]
    pub seven_day_opus: Option<LimitUsage>,
    #[serde(default)]
    pub seven_day_sonnet: Option<LimitUsage>,
    #[serde(default)]
    pub limits: Option<Vec<LimitEntry>>,
    /// Kept for the log only: what it means isn't known for certain, so
    /// whether usage shows goes by the windows themselves.
    #[serde(default)]
    pub member_dashboard_available: Option<bool>,
    /// Extra Usage, read on its own (`extra_from`), so a shape it doesn't
    /// know never loses the limits.
    #[serde(default)]
    pub extra_usage: Option<serde_json::Value>,
}

/// Extra Usage as the API gives it, new names and old.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExtraUsageResponse {
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub monthly_limit: Option<i64>,
    #[serde(default)]
    pub monthly_credit_limit: Option<i64>,
    #[serde(default)]
    pub currency: Option<String>,
    /// Cents, which the API may give as `21.0`.
    #[serde(default)]
    pub used_credits: Option<f64>,
    #[serde(default)]
    pub spend_limit_currency: Option<String>,
    #[serde(default)]
    pub spend_limit_amount_cents: Option<i64>,
    #[serde(default)]
    pub balance_cents: Option<i64>,
}

/// A weekly per-model limit: the model's name where the API gives one
/// (none for the legacy fields, which are shown as Opus and Sonnet).
#[derive(Debug, Clone, PartialEq)]
pub struct WeeklyModel {
    pub name: Option<String>,
    pub limit: Limit,
}

/// Extra Usage in the account's currency.
#[derive(Debug, Clone, PartialEq)]
pub struct Extra {
    pub enabled: bool,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    /// ISO 4217, upper case.
    pub currency: String,
}

impl Extra {
    /// The currency's sign, or its code for one not listed.
    pub fn symbol(&self) -> &str {
        match self.currency.as_str() {
            "USD" => "$",
            "EUR" => "€",
            "GBP" => "£",
            "JPY" => "¥",
            "KRW" => "₩",
            "CAD" => "CA$",
            "AUD" => "A$",
            "BRL" => "R$",
            "INR" => "₹",
            other => other,
        }
    }

    /// How much of the allowance is used, 0–100, where there is one.
    pub fn percent(&self) -> Option<f64> {
        match (self.used, self.limit) {
            (Some(used), Some(limit)) if limit > 0.0 => Some(used / limit * 100.0),
            _ => None,
        }
    }
}

/// Usage as U4Ai shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageData {
    pub five_hour: Option<Limit>,
    pub seven_day: Limit,
    pub legacy_opus: Option<Limit>,
    pub legacy_sonnet: Option<Limit>,
    /// From `limits[]`, in the API's order, never cut.
    pub scoped: Vec<WeeklyModel>,
    pub extra: Option<Extra>,
}

impl UsageData {
    /// Weekly slot 0: the legacy Opus field, else the first scoped model.
    pub fn opus(&self) -> Option<WeeklyModel> {
        match self.legacy_opus {
            Some(limit) => Some(WeeklyModel { name: None, limit }),
            None => self.scoped.first().cloned(),
        }
    }

    /// Weekly slot 1: the legacy Sonnet field, else the next scoped model
    /// slot 0 didn't take.
    pub fn sonnet(&self) -> Option<WeeklyModel> {
        match self.legacy_sonnet {
            Some(limit) => Some(WeeklyModel { name: None, limit }),
            None => self.scoped.get(if self.legacy_opus.is_none() { 1 } else { 0 }).cloned(),
        }
    }

    /// Scoped models past both slots, each a row of its own.
    pub fn overflow(&self) -> &[WeeklyModel] {
        let taken = usize::from(self.legacy_opus.is_none()) + usize::from(self.legacy_sonnet.is_none());
        &self.scoped[taken.min(self.scoped.len())..]
    }

    /// The limit the tray's outer ring shows: the 5-hour window where the
    /// plan has one, else the 7-day.
    pub fn primary(&self) -> Limit {
        self.five_hour.unwrap_or(self.seven_day)
    }
}

/// An RFC 3339 time rounded to the second; none where it doesn't read.
fn reset_time(text: Option<&str>) -> Option<Timestamp> {
    let ts: Timestamp = text?.parse().ok()?;
    ts.round(Unit::Second).ok()
}

fn limit_of(usage: &LimitUsage) -> Limit {
    Limit { percent: usage.utilization.unwrap_or(0.0), resets_at: reset_time(usage.resets_at.as_deref()) }
}

/// A legacy weekly field, none where it's the API's "nothing yet" (0%
/// with no reset time).
fn legacy_weekly(usage: Option<&LimitUsage>) -> Option<Limit> {
    let usage = usage?;
    if usage.utilization.unwrap_or(0.0) == 0.0 && usage.resets_at.is_none() {
        return None;
    }
    Some(limit_of(usage))
}

impl UsageResponse {
    /// Whether the answer holds no limit at all: every window null and
    /// `limits[]` empty or missing. That's a plan without usage data, not
    /// a sign-in that failed.
    pub fn has_no_limit_data(&self) -> bool {
        self.five_hour.is_none()
            && self.seven_day.is_none()
            && self.seven_day_oauth_apps.is_none()
            && self.seven_day_opus.is_none()
            && self.seven_day_sonnet.is_none()
            && self.limits.as_ref().is_none_or(Vec::is_empty)
    }

    pub fn to_usage(&self) -> UsageData {
        let scoped = self
            .limits
            .iter()
            .flatten()
            .filter_map(|entry| {
                let name = entry.scope.as_ref()?.model.as_ref()?.display_name.clone().filter(|n| !n.is_empty())?;
                let percent = entry.percent?;
                Some(WeeklyModel { name: Some(name), limit: Limit { percent, resets_at: reset_time(entry.resets_at.as_deref()) } })
            })
            .collect();
        UsageData {
            five_hour: self.five_hour.as_ref().map(limit_of),
            seven_day: self.seven_day.as_ref().map(limit_of).unwrap_or(Limit { percent: 0.0, resets_at: None }),
            legacy_opus: legacy_weekly(self.seven_day_opus.as_ref()),
            legacy_sonnet: legacy_weekly(self.seven_day_sonnet.as_ref()),
            scoped,
            extra: self.extra_usage.as_ref().and_then(extra_from),
        }
    }
}

/// Extra Usage from its JSON, none where it doesn't read as one (logged).
fn extra_from(value: &serde_json::Value) -> Option<Extra> {
    if value.is_null() {
        return None;
    }
    match serde_json::from_value::<ExtraUsageResponse>(value.clone()) {
        Ok(extra) => Some(extra.to_extra()),
        Err(err) => {
            let keys: Vec<&String> = value.as_object().map(|o| o.keys().collect()).unwrap_or_default();
            log::warn!("extra_usage didn't read ({err}); its keys: {keys:?}");
            None
        }
    }
}

impl ExtraUsageResponse {
    pub fn to_extra(&self) -> Extra {
        let currency = self.currency.clone().or_else(|| self.spend_limit_currency.clone()).unwrap_or_else(|| "USD".into()).to_uppercase();
        let limit_cents = self.monthly_limit.or(self.monthly_credit_limit).or(self.spend_limit_amount_cents);
        let used_cents = self.used_credits.or(self.balance_cents.map(|c| c as f64));
        let enabled = self.is_enabled.unwrap_or(limit_cents.is_some_and(|l| l > 0));
        match limit_cents.filter(|l| enabled && *l > 0) {
            Some(limit) => Extra { enabled: true, used: Some(used_cents.unwrap_or(0.0) / 100.0), limit: Some(limit as f64 / 100.0), currency },
            None => Extra { enabled: false, used: None, limit: None, currency },
        }
    }
}

/// The usage answer read, or why not: `Ok(None)` is a plan without usage
/// data (every window null).
pub fn parse(body: &str) -> Result<Option<UsageData>, serde_json::Error> {
    let response: UsageResponse = serde_json::from_str(body)?;
    if response.has_no_limit_data() {
        log::info!("usage holds no limit (member_dashboard_available: {:?})", response.member_dashboard_available);
        return Ok(None);
    }
    Ok(Some(response.to_usage()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(json: &str) -> UsageData {
        parse(json).expect("reads").expect("has limits")
    }

    #[test]
    fn the_five_hour_window_reads_whole_and_fractional_percentages() {
        let u = usage(r#"{"five_hour": {"utilization": 42, "resets_at": "2026-05-01T15:00:00.000Z"}, "seven_day": null}"#);
        assert_eq!(u.five_hour.unwrap().percent, 42.0);
        assert_eq!(u.five_hour.unwrap().resets_at.unwrap().to_string(), "2026-05-01T15:00:00Z");
        let u = usage(r#"{"five_hour": {"utilization": 42.7, "resets_at": null}}"#);
        assert_eq!(u.five_hour.unwrap().percent, 42.7);
        assert_eq!(u.five_hour.unwrap().resets_at, None);
    }

    #[test]
    fn a_missing_seven_day_window_is_a_zero_placeholder() {
        let u = usage(r#"{"five_hour": {"utilization": 10, "resets_at": null}}"#);
        assert_eq!(u.seven_day, Limit { percent: 0.0, resets_at: None });
        let u = usage(r#"{"five_hour": null, "seven_day": {"utilization": 76, "resets_at": "2026-08-27T23:00:00.027333+00:00"}}"#);
        assert_eq!(u.five_hour, None);
        assert_eq!(u.seven_day.percent, 76.0);
        assert_eq!(u.primary(), u.seven_day, "the 7-day takes the outer ring without a 5-hour window");
    }

    #[test]
    fn reset_times_round_to_the_nearest_second() {
        let up = usage(r#"{"five_hour": {"utilization": 1, "resets_at": "2026-05-01T05:59:59.645+00:00"}}"#);
        let down = usage(r#"{"five_hour": {"utilization": 1, "resets_at": "2026-05-01T06:00:00.159+00:00"}}"#);
        assert_eq!(up.five_hour.unwrap().resets_at, down.five_hour.unwrap().resets_at);
        assert_eq!(up.five_hour.unwrap().resets_at.unwrap().to_string(), "2026-05-01T06:00:00Z");
    }

    #[test]
    fn legacy_weekly_fields_at_zero_with_no_reset_are_nothing_yet() {
        let u = usage(r#"{"five_hour": {"utilization": 1, "resets_at": null},
            "seven_day_opus": {"utilization": 0, "resets_at": null},
            "seven_day_sonnet": {"utilization": 50, "resets_at": "2026-05-01T15:00:00.000Z"}}"#);
        assert_eq!(u.legacy_opus, None);
        assert_eq!(u.opus(), None);
        // Sonnet keeps its own slot rather than moving into Opus's.
        assert_eq!(u.sonnet().unwrap().limit.percent, 50.0);
        assert_eq!(u.sonnet().unwrap().name, None);
    }

    /// The live answer of 2026-08-26 the Mac's tests keep: the per-model
    /// weekly limit comes in `limits[]` alone, named, and unknown windows
    /// sit beside the known ones.
    #[test]
    fn the_live_shape_fills_the_first_weekly_slot_from_limits() {
        let u = usage(
            r#"{
            "five_hour": { "utilization": 71.0, "resets_at": "2026-08-26T14:20:00.027310+00:00" },
            "seven_day": { "utilization": 76.0, "resets_at": "2026-08-27T23:00:00.027333+00:00" },
            "seven_day_oauth_apps": null, "seven_day_opus": null, "seven_day_sonnet": null, "seven_day_cowork": null,
            "nimbus_quill": { "utilization": 0.0, "resets_at": null },
            "limits": [
                { "kind": "session", "group": "session", "percent": 71, "severity": "normal",
                  "resets_at": "2026-08-26T14:20:00.027310+00:00", "scope": null, "is_active": false },
                { "kind": "weekly_all", "group": "weekly", "percent": 76, "severity": "warning",
                  "resets_at": "2026-08-27T23:00:00.027333+00:00", "scope": null, "is_active": true },
                { "kind": "weekly_scoped", "group": "weekly", "percent": 13, "severity": "normal",
                  "resets_at": "2026-08-27T23:00:00.027631+00:00",
                  "scope": { "model": { "id": null, "display_name": "Fable" }, "surface": null }, "is_active": false }
            ]}"#,
        );
        assert_eq!(u.five_hour.unwrap().percent, 71.0);
        assert_eq!(u.seven_day.percent, 76.0);
        let opus = u.opus().unwrap();
        assert_eq!((opus.name.as_deref(), opus.limit.percent), (Some("Fable"), 13.0));
        assert_eq!(u.sonnet(), None, "session and weekly_all have no model and are no weekly model's");
        assert!(u.overflow().is_empty());
    }

    #[test]
    fn a_third_model_past_both_legacy_slots_gets_its_own_row() {
        let u = usage(
            r#"{"five_hour": {"utilization": 10, "resets_at": null},
            "seven_day_opus": {"utilization": 40, "resets_at": "2026-05-01T15:00:00.000Z"},
            "seven_day_sonnet": {"utilization": 50, "resets_at": "2026-05-01T15:00:00.000Z"},
            "limits": [{"kind": "weekly_scoped", "percent": 13, "resets_at": null, "scope": {"model": {"id": null, "display_name": "Fable"}}}]}"#,
        );
        assert_eq!(u.opus().unwrap().limit.percent, 40.0);
        assert_eq!(u.sonnet().unwrap().limit.percent, 50.0);
        assert_eq!(u.overflow().len(), 1);
        assert_eq!(u.overflow()[0].name.as_deref(), Some("Fable"));
    }

    #[test]
    fn scoped_models_fill_both_slots_in_the_apis_order() {
        let u = usage(
            r#"{"five_hour": {"utilization": 10, "resets_at": null}, "limits": [
                {"kind": "weekly_scoped", "percent": 11, "scope": {"model": {"display_name": "Fable"}}},
                {"kind": "weekly_scoped", "percent": 22, "scope": {"model": {"display_name": "Opus"}}},
                {"kind": "weekly_scoped", "percent": 33, "scope": {"model": {"display_name": "Sonnet"}}}]}"#,
        );
        assert_eq!(u.opus().unwrap().name.as_deref(), Some("Fable"));
        assert_eq!(u.sonnet().unwrap().name.as_deref(), Some("Opus"));
        assert_eq!(u.overflow().iter().map(|m| m.name.as_deref()).collect::<Vec<_>>(), [Some("Sonnet")]);
    }

    #[test]
    fn a_legacy_field_wins_its_slot_and_limits_fill_the_other() {
        let u = usage(
            r#"{"five_hour": {"utilization": 10, "resets_at": null},
            "seven_day_opus": {"utilization": 80, "resets_at": "2026-05-01T15:00:00.000Z"}, "seven_day_sonnet": null,
            "limits": [{"kind": "weekly_scoped", "percent": 12, "scope": {"model": {"id": "fable", "display_name": "Fable"}}}]}"#,
        );
        assert_eq!(u.opus().unwrap().limit.percent, 80.0);
        assert_eq!(u.sonnet().unwrap().limit.percent, 12.0);
    }

    #[test]
    fn limits_without_a_model_name_are_no_weekly_models() {
        let u = usage(r#"{"five_hour": {"utilization": 10, "resets_at": null}, "limits": [{"kind": "weekly_all", "percent": 50, "scope": {"surface": "web"}}]}"#);
        assert_eq!((u.opus(), u.sonnet()), (None, None));
    }

    #[test]
    fn every_window_null_is_a_plan_without_usage_data_not_a_failure() {
        let all_null = r#"{"five_hour": null, "seven_day": null, "seven_day_oauth_apps": null, "seven_day_opus": null,
            "seven_day_sonnet": null, "member_dashboard_available": false, "extra_usage": {"is_enabled": true, "monthly_limit": 5000}}"#;
        assert!(parse(all_null).unwrap().is_none());
        assert!(parse(r#"{"limits": []}"#).unwrap().is_none());
        // The dashboard flag alone decides nothing.
        let u = usage(r#"{"five_hour": {"utilization": 5, "resets_at": null}, "member_dashboard_available": false}"#);
        assert_eq!(u.five_hour.unwrap().percent, 5.0);
        assert!(parse("not json").is_err());
    }

    #[test]
    fn extra_usage_reads_new_and_old_names_in_its_own_currency() {
        let u = usage(r#"{"five_hour": {"utilization": 1, "resets_at": null},
            "extra_usage": {"is_enabled": true, "monthly_limit": 5000, "used_credits": 1234.0, "currency": "eur"}}"#);
        let extra = u.extra.unwrap();
        assert_eq!(extra, Extra { enabled: true, used: Some(12.34), limit: Some(50.0), currency: "EUR".into() });
        assert_eq!(extra.symbol(), "€");
        assert!((extra.percent().unwrap() - 24.68).abs() < 1e-9);
        let old = ExtraUsageResponse { spend_limit_amount_cents: Some(2000), balance_cents: Some(500), spend_limit_currency: Some("krw".into()), ..Default::default() }.to_extra();
        assert_eq!((old.enabled, old.used, old.limit, old.symbol()), (true, Some(5.0), Some(20.0), "₩"));
        let off = ExtraUsageResponse { is_enabled: Some(false), monthly_limit: Some(5000), ..Default::default() }.to_extra();
        assert!(!off.enabled && off.percent().is_none());
        // A shape it doesn't know loses Extra Usage alone, never the limits.
        let u = usage(r#"{"five_hour": {"utilization": 3, "resets_at": null}, "extra_usage": {"monthly_limit": "lots"}}"#);
        assert_eq!((u.five_hour.unwrap().percent, u.extra.clone()), (3.0, None));
        assert_eq!(Extra { currency: "CHF".into(), ..off }.symbol(), "CHF");
    }

    #[test]
    fn a_null_utilization_reads_as_zero_rather_than_failing_the_answer() {
        let u = usage(r#"{"five_hour": {"utilization": null, "resets_at": null}, "seven_day": {"utilization": 9}}"#);
        assert_eq!((u.five_hour.unwrap().percent, u.seven_day.percent), (0.0, 9.0));
    }
}
