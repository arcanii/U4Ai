//! "Sign in with Claude", as the Mac app does it: PKCE in the system
//! browser with Claude Code's public client, the redirect caught on
//! `http://localhost:1456/callback` (or 1458), the code traded for tokens,
//! and the account named from its profile.

use std::sync::atomic::AtomicBool;

use super::api::{self, Endpoints, CLIENT_ID, SCOPE};
use crate::i18n::t;
use crate::oauth::{encode, listen, redirect_uri, Callback, Pkce, WAIT};
use crate::providers::{NewAccount, Provider, SignInError};

/// The redirect's ports: the first that's free. Both are below the
/// ephemeral range, so the system won't have handed them to an outgoing
/// connection, and clear of Codex's 1455 and 1457.
pub const PORTS: [u16; 2] = [1456, 1458];
/// The page that signs in.
pub fn authorize_url(pkce: &Pkce, redirect: &str) -> String {
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        api::AUTHORIZE_URL,
        encode(CLIENT_ID),
        encode(redirect),
        encode(SCOPE),
        encode(&pkce.challenge),
        encode(&pkce.state)
    )
}

fn failed(why: &dyn std::fmt::Display) -> SignInError {
    log::warn!("sign-in failed: {why}");
    SignInError::Failed(t("sign_in.failed").into())
}

/// A sign-in begun: the page to open in the browser (`url`), and the
/// listener waiting for it to come back.
pub struct Pending {
    pkce: Pkce,
    callback: Callback,
    redirect: String,
    pub url: String,
}

/// Begins a sign-in: the browser is to open `Pending::url`, then `finish`
/// waits for it on a thread of its own.
pub fn start() -> Result<Pending, SignInError> {
    let pkce = Pkce::new().map_err(|e| failed(&e))?;
    let callback = listen(&PORTS).ok_or_else(|| SignInError::Failed(t("sign_in.port_busy").into()))?;
    let redirect = redirect_uri(callback.port);
    let url = authorize_url(&pkce, &redirect);
    Ok(Pending { pkce, callback, redirect, url })
}

impl Pending {
    /// Waits for the browser, trades the code for tokens and names the
    /// account from its profile; given up when `cancel` is set.
    pub fn finish(self, ends: &Endpoints, cancel: &AtomicBool) -> Result<NewAccount, SignInError> {
        let code = self.callback.wait_for_code(&self.pkce.state, cancel, WAIT)?;
        drop(self.callback);
        let tokens = api::exchange_code(ends, &code, &self.pkce.state, &self.pkce.verifier, &self.redirect).map_err(|e| failed(&format!("{e:?}")))?;
        let refresh = tokens.refresh.ok_or_else(|| failed(&"no refresh_token"))?;
        // The profile names the account; without it the sign-in still stands.
        let profile = api::profile(ends, &tokens.access).unwrap_or_else(|e| {
            log::warn!("no profile: {e:?}");
            Default::default()
        });
        Ok(account_from(&profile, refresh))
    }
}

/// The account a profile names: keyed by its organization, else its email.
fn account_from(profile: &api::Profile, refresh: String) -> NewAccount {
    let name = if profile.email.is_empty() { "Claude".to_string() } else { profile.email.clone() };
    let id = [profile.org_id.as_str(), profile.email.as_str()].into_iter().find(|s| !s.is_empty()).map_or_else(|| name.clone(), String::from);
    NewAccount { provider: Provider::Claude, id, name, secret: refresh }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::tests::get;

    #[test]
    fn the_page_asks_for_the_profile_alone() {
        let pkce = Pkce::new().unwrap();
        let url = authorize_url(&pkce, &redirect_uri(1456));
        assert!(url.starts_with("https://claude.ai/oauth/authorize?response_type=code&client_id=9d1c250a-"), "{url}");
        assert!(url.contains("&redirect_uri=http%3A%2F%2Flocalhost%3A1456%2Fcallback&scope=user%3Aprofile&"), "{url}");
        assert!(url.contains(&format!("&code_challenge={}&code_challenge_method=S256&state={}", pkce.challenge, pkce.state)));
    }

    /// The whole sign-in against a stand-in for Anthropic's endpoints, the
    /// browser played by a request to the redirect.
    #[test]
    fn a_sign_in_trades_the_code_and_names_the_account_by_its_organization() {
        let (base, seen) = crate::http::tests::serve(|_, path, _, body| match path {
            "/token" => {
                assert!(body.contains(r#""grant_type":"authorization_code""#) && body.contains(r#""code":"c0de""#) && body.contains("code_verifier"), "{body}");
                (200, String::new(), r#"{"access_token": "a", "refresh_token": "r", "expires_in": 3600}"#.into())
            }
            "/profile" => (200, String::new(), r#"{"account": {"email": "me@example.com"}, "organization": {"uuid": "org-1"}}"#.into()),
            _ => (404, String::new(), String::new()),
        });
        // Another test may hold 1456 and 1458; any port does for this one.
        let pkce = Pkce::new().unwrap();
        let callback = listen(&[0]).unwrap();
        let redirect = redirect_uri(callback.port);
        let pending = Pending { url: authorize_url(&pkce, &redirect), pkce, callback, redirect };
        let (port, state) = (pending.callback.port, pending.pkce.state.clone());
        let browser = std::thread::spawn(move || get(port, &format!("/callback?code=c0de&state={state}")));
        let account = pending.finish(&crate::providers::claude::api::tests::ends(&base), &AtomicBool::new(false)).unwrap();
        browser.join().unwrap();
        assert_eq!(account, NewAccount { provider: Provider::Claude, id: "org-1".into(), name: "me@example.com".into(), secret: "r".into() });
        assert_eq!(seen.lock().unwrap().len(), 2);
        let bare = account_from(&api::Profile::default(), "r".into());
        assert_eq!((bare.id.as_str(), bare.name.as_str()), ("Claude", "Claude"));
    }

}
