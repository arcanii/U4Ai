//! Anthropic's OAuth endpoints, as the Mac app reads them
//! (`ClaudeOAuthConfig.swift`, `ClaudeOAuthService.swift`,
//! `ClaudeAPIService.swift`'s OAuth path): a refresh token is traded for an
//! access token (a JSON body, as Claude Code sends it), which reads the
//! usage and the profile with a Bearer token and the OAuth beta header.
//! Neither endpoint is documented, so the decoder is lenient and its tests
//! are the Mac app's.
//!
//! `Session` keeps an account's access token until five minutes before it
//! runs out, trades the refresh token again on a 401 and tries once more,
//! and hands back a refresh token the server turned over, to be kept.

use std::time::{Duration, Instant};

use jiff::Timestamp;
use serde_json::{json, Value};

use super::usage::{self, UsageData};
use crate::http::{agent, classify};
use crate::providers::ReadError;

/// Claude Code's public client (PKCE, no secret), as the Mac app uses it.
pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
/// Read-only usage needs the profile scope alone, never inference.
pub const SCOPE: &str = "user:profile";
/// The beta header the OAuth endpoints want.
const BETA: &str = "oauth-2025-04-20";
/// An access token whose life isn't given is kept this long.
const ASSUMED_LIFE: Duration = Duration::from_secs(30 * 60);
/// An access token is traded again this long before it runs out.
const EARLY: Duration = Duration::from_secs(5 * 60);

/// Where the requests go: Anthropic's, or a test's own server.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub token: String,
    pub usage: String,
    pub profile: String,
}

impl Endpoints {
    pub fn anthropic() -> Endpoints {
        Endpoints {
            token: "https://console.anthropic.com/v1/oauth/token".into(),
            usage: "https://api.anthropic.com/api/oauth/usage".into(),
            profile: "https://api.anthropic.com/api/oauth/profile".into(),
        }
    }
}

/// What the token endpoint gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    pub access: String,
    /// None where it didn't turn the refresh token over.
    pub refresh: Option<String>,
    pub expires_in: Option<u64>,
}

fn token_request(agent: &ureq::Agent, url: &str, body: Value) -> Result<Tokens, ReadError> {
    let response = agent.post(url).header("content-type", "application/json").header("accept", "application/json").send(body.to_string()).map_err(|e| ReadError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response.into_body().read_to_string().unwrap_or_default();
    if !(200..300).contains(&status) {
        log::warn!("token endpoint answered {status}: {}", text.chars().take(200).collect::<String>());
        // A refresh token it no longer takes comes back 400 (invalid_grant)
        // or 401: either way the user signs in again.
        return Err(if matches!(status, 400 | 401) { ReadError::Unauthorized } else { ReadError::Http(status) });
    }
    let said: Value = serde_json::from_str(&text).map_err(|e| ReadError::Decoding(e.to_string()))?;
    let access = said.get("access_token").and_then(Value::as_str).filter(|a| !a.is_empty()).ok_or_else(|| ReadError::Decoding("no access_token".into()))?;
    Ok(Tokens {
        access: access.into(),
        refresh: said.get("refresh_token").and_then(Value::as_str).filter(|r| !r.is_empty()).map(String::from),
        expires_in: said.get("expires_in").and_then(Value::as_u64),
    })
}

/// Trades a sign-in's code for tokens.
pub fn exchange_code(ends: &Endpoints, code: &str, state: &str, verifier: &str, redirect: &str) -> Result<Tokens, ReadError> {
    let body = json!({"grant_type": "authorization_code", "code": code, "state": state, "redirect_uri": redirect, "client_id": CLIENT_ID, "code_verifier": verifier});
    token_request(&agent(), &ends.token, body)
}

/// Trades a refresh token for a new access token.
pub fn refresh(ends: &Endpoints, refresh_token: &str) -> Result<Tokens, ReadError> {
    let body = json!({"grant_type": "refresh_token", "refresh_token": refresh_token, "client_id": CLIENT_ID});
    token_request(&agent(), &ends.token, body)
}

/// A GET with the Bearer token: the body, or why not.
fn get(url: &str, access: &str) -> Result<String, ReadError> {
    let response = agent()
        .get(url)
        .header("authorization", &format!("Bearer {access}"))
        .header("anthropic-beta", BETA)
        .header("accept", "application/json")
        .call()
        .map_err(|e| ReadError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let retry = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(String::from);
    if let Some(error) = classify(status, retry.as_deref(), Timestamp::now()) {
        return Err(error);
    }
    response.into_body().with_config().limit(1 << 20).read_to_string().map_err(|e| ReadError::Network(e.to_string()))
}

/// Who signed in: their email, and their organization's id and name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Profile {
    pub email: String,
    pub org_id: String,
    pub org_name: String,
}

pub fn profile(ends: &Endpoints, access: &str) -> Result<Profile, ReadError> {
    let said: Value = serde_json::from_str(&get(&ends.profile, access)?).map_err(|e| ReadError::Decoding(e.to_string()))?;
    let account = said.get("account").ok_or_else(|| ReadError::Decoding("no account".into()))?;
    let field = |v: Option<&Value>, k: &str| v.and_then(|v| v.get(k)).and_then(Value::as_str).unwrap_or("").to_string();
    let email = field(Some(account), "email");
    let org = said.get("organization");
    Ok(Profile { org_id: field(org, "uuid"), org_name: org.and_then(|o| o.get("name")).and_then(Value::as_str).map(String::from).unwrap_or_else(|| email.clone()), email })
}

pub fn usage(ends: &Endpoints, access: &str) -> Result<UsageData, ReadError> {
    let body = get(&ends.usage, access)?;
    match usage::parse(&body) {
        Ok(Some(data)) => Ok(data),
        Ok(None) => Err(ReadError::NoDashboard),
        Err(err) => {
            log::warn!("usage didn't read ({err}): {}", body.chars().take(300).collect::<String>());
            Err(ReadError::Decoding(err.to_string()))
        }
    }
}

/// An account's access to its usage.
pub struct Session {
    ends: Endpoints,
    refresh: String,
    access: Option<(String, Instant)>,
    /// A refresh token the server turned over since the last look, to keep.
    rotated: Option<String>,
}

impl Session {
    pub fn new(ends: Endpoints, refresh: String) -> Session {
        Session { ends, refresh, access: None, rotated: None }
    }

    /// The access token, traded for afresh where it's about to run out.
    fn bearer(&mut self) -> Result<String, ReadError> {
        if let Some((token, until)) = &self.access {
            if *until > Instant::now() + EARLY {
                return Ok(token.clone());
            }
        }
        let got = refresh(&self.ends, &self.refresh)?;
        let life = got.expires_in.map(Duration::from_secs).unwrap_or(ASSUMED_LIFE);
        if let Some(new) = got.refresh.filter(|r| *r != self.refresh) {
            log::info!("the refresh token turned over");
            self.refresh = new.clone();
            self.rotated = Some(new);
        }
        self.access = Some((got.access.clone(), Instant::now() + life));
        Ok(got.access)
    }

    /// Reads the usage; a 401 trades the refresh token again and tries once
    /// more, so a revoked access token doesn't wait for the next read.
    pub fn read(&mut self) -> Result<UsageData, ReadError> {
        let access = self.bearer()?;
        match usage(&self.ends, &access) {
            Err(ReadError::Unauthorized) => {
                log::info!("usage answered 401: trading the refresh token again");
                self.access = None;
                let access = self.bearer()?;
                usage(&self.ends, &access)
            }
            other => other,
        }
    }

    /// A refresh token the server turned over, once.
    pub fn take_rotated(&mut self) -> Option<String> {
        self.rotated.take()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::http::tests::serve;
    use std::sync::{Arc, Mutex};

    pub fn ends(base: &str) -> Endpoints {
        Endpoints { token: format!("{base}/token"), usage: format!("{base}/usage"), profile: format!("{base}/profile") }
    }

    const USAGE: &str = r#"{"five_hour": {"utilization": 42, "resets_at": "2026-05-01T15:00:00Z"}, "seven_day": {"utilization": 7, "resets_at": null}}"#;

    #[test]
    fn a_session_keeps_its_access_token_and_keeps_a_turned_over_refresh_token() {
        let (base, seen) = serve(|method, path, headers, body| match (method, path) {
            ("POST", "/token") => {
                assert!(body.contains(r#""grant_type":"refresh_token""#) && body.contains(CLIENT_ID), "{body}");
                (200, String::new(), r#"{"access_token": "a1", "refresh_token": "r2", "expires_in": 3600}"#.into())
            }
            ("GET", "/usage") => {
                assert!(headers.contains("authorization: Bearer a1") && headers.contains("anthropic-beta: oauth-2025-04-20"), "{headers}");
                (200, String::new(), USAGE.into())
            }
            _ => (404, String::new(), String::new()),
        });
        let mut session = Session::new(ends(&base), "r1".into());
        assert_eq!(session.read().unwrap().five_hour.unwrap().percent, 42.0);
        assert_eq!(session.take_rotated().as_deref(), Some("r2"));
        assert_eq!(session.take_rotated(), None, "handed back once");
        session.read().unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.iter().filter(|r| r.starts_with("POST /token")).count(), 1, "the access token is kept: {seen:?}");
        assert!(seen.last().unwrap().starts_with("GET /usage"));
    }

    #[test]
    fn a_401_trades_the_refresh_token_again_and_tries_once_more() {
        let calls = Arc::new(Mutex::new(0));
        let count = calls.clone();
        let (base, _) = serve(move |_, path, headers, _| match path {
            "/token" => {
                let mut n = count.lock().unwrap();
                *n += 1;
                (200, String::new(), format!(r#"{{"access_token": "a{n}"}}"#))
            }
            "/usage" if headers.contains("Bearer a1") => (401, String::new(), "{}".into()),
            "/usage" => (200, String::new(), USAGE.into()),
            _ => (404, String::new(), String::new()),
        });
        let mut session = Session::new(ends(&base), "r1".into());
        assert!(session.read().is_ok());
        assert_eq!(*calls.lock().unwrap(), 2);
        assert_eq!(session.take_rotated(), None, "no refresh token given back is no turnover");
    }

    #[test]
    fn a_refused_refresh_token_a_429_and_a_plan_without_data_say_so() {
        let (base, _) = serve(|_, path, _, body| match path {
            "/token" if body.contains("dead") => (400, String::new(), r#"{"error": "invalid_grant"}"#.into()),
            "/token" => (200, String::new(), r#"{"access_token": "a"}"#.into()),
            "/usage" => (429, "retry-after: 90\r\n".into(), "{}".into()),
            "/profile" => (200, String::new(), r#"{"account": {"email": "me@example.com"}, "organization": {"uuid": "org-1", "name": "Mine"}}"#.into()),
            _ => (404, String::new(), String::new()),
        });
        assert_eq!(Session::new(ends(&base), "dead".into()).read(), Err(ReadError::Unauthorized));
        assert_eq!(Session::new(ends(&base), "r".into()).read(), Err(ReadError::RateLimited(Some(Duration::from_secs(90)))));
        assert_eq!(profile(&ends(&base), "a").unwrap(), Profile { email: "me@example.com".into(), org_id: "org-1".into(), org_name: "Mine".into() });
        let (base, _) = serve(|_, path, _, _| match path {
            "/token" => (200, String::new(), r#"{"access_token": "a"}"#.into()),
            _ => (200, String::new(), r#"{"five_hour": null, "seven_day": null}"#.into()),
        });
        assert_eq!(Session::new(ends(&base), "r".into()).read(), Err(ReadError::NoDashboard));
    }
}
