//! What any provider's OAuth sign-in needs, on Flow's loopback listener:
//! PKCE and `state`, the redirect caught on a `localhost` port, and the
//! callback link a user pastes where the redirect never came back. The
//! listener binds the loopback addresses alone (127.0.0.1, and ::1 where
//! there is one, since a browser may take `localhost` for either), so
//! nothing off this computer can reach it, where the Mac app's binds every
//! interface and drops other peers.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::i18n::t;
pub use crate::providers::SignInError;

/// How long the browser has to come back.
pub const WAIT: Duration = Duration::from_secs(5 * 60);

/// PKCE's verifier and challenge (S256), and the `state` that ties the
/// redirect to this sign-in.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

fn random(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| e.to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

/// PKCE's S256 challenge for `verifier` (RFC 7636).
pub fn challenge_of(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

impl Pkce {
    /// From the system's random numbers: a failure there is a failed
    /// sign-in, never a guessable verifier.
    pub fn new() -> Result<Pkce, String> {
        let verifier = random(64)?;
        Ok(Pkce { challenge: challenge_of(&verifier), state: random(32)?, verifier })
    }
}

/// `text` percent-encoded for a query, all but the unreserved.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A query's fields, decoded.
pub fn query_params(text: &str) -> BTreeMap<String, String> {
    let decode = |s: &str| {
        let bytes = s.as_bytes();
        let hex = |b: u8| (b as char).to_digit(16);
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    out.push((high * 16 + low) as u8);
                    i += 3;
                    continue;
                }
            }
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    };
    text.split('&').filter(|p| !p.is_empty()).map(|p| p.split_once('=').unwrap_or((p, ""))).map(|(k, v)| (decode(k), decode(v))).collect()
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/callback")
}

/// Where the browser comes back: a port on the loopback addresses.
pub struct Callback {
    listeners: Vec<TcpListener>,
    pub port: u16,
}

/// Listens on the first of `ports` free on 127.0.0.1, and on ::1 too where
/// the system has it; none where every one is taken.
pub fn listen(ports: &[u16]) -> Option<Callback> {
    for &port in ports {
        let Ok(v4) = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))) else { continue };
        // The port the system gave, where 0 asked for any (the tests).
        let port = v4.local_addr().map_or(port, |a| a.port());
        let mut listeners = vec![v4];
        match TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port))) {
            Ok(v6) => listeners.push(v6),
            Err(err) => log::debug!("no ::1 listener on {port}: {err}"),
        }
        for l in &listeners {
            if l.set_nonblocking(true).is_err() {
                return None;
            }
        }
        return Some(Callback { listeners, port });
    }
    None
}

impl Callback {
    /// Waits up to `wait` for the browser to come back with the code whose
    /// `state` is this sign-in's, answering it with a page in the user's
    /// words.
    pub fn wait_for_code(&self, state: &str, cancel: &AtomicBool, wait: Duration) -> Result<String, SignInError> {
        let until = Instant::now() + wait;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(SignInError::Cancelled);
            }
            if Instant::now() > until {
                return Err(SignInError::Failed(t("sign_in.timed_out").into()));
            }
            let mut idle = true;
            for listener in &self.listeners {
                match listener.accept() {
                    Ok((stream, _)) => {
                        idle = false;
                        if let Some(outcome) = answer(stream, state) {
                            return outcome;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => log::warn!("the sign-in's listener: {e}"),
                }
            }
            if idle {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// One request to the loopback port: the code where it's the sign-in
/// coming back with the right `state`, its refusal, or none (a favicon,
/// say), which leaves the sign-in waiting.
fn answer(stream: TcpStream, state: &str) -> Option<Result<String, SignInError>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut first = String::new();
    reader.read_line(&mut first).ok()?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
    }
    let target = first.split_whitespace().nth(1).unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut out = stream;
    if path != "/callback" {
        let _ = write!(out, "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        return None;
    }
    let page = |out: &mut TcpStream, words: &str| {
        let escaped = words.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let body = format!("<!doctype html><meta charset=\"utf-8\"><title>U4Ai</title><body style=\"font: 16px system-ui, sans-serif; margin: 3em auto; max-width: 32em; padding: 0 1em\"><p>{escaped}</p></body>");
        let _ = write!(out, "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
    };
    let params = query_params(query);
    if params.get("state").map(String::as_str) != Some(state) {
        // Not this sign-in's: someone else's page, or an old tab.
        page(&mut out, t("sign_in.failed"));
        log::warn!("a sign-in came back with another state");
        return None;
    }
    if let Some(error) = params.get("error") {
        log::warn!("the sign-in was refused: {error} {:?}", params.get("error_description"));
        page(&mut out, t("sign_in.failed"));
        return Some(Err(SignInError::Failed(t("sign_in.failed").into())));
    }
    let code = params.get("code").filter(|c| !c.is_empty())?.clone();
    page(&mut out, t("sign_in.page_done"));
    Some(Ok(code))
}

/// The code, state and error in what the user pasted from the browser's
/// address bar, for a redirect that never reached the listener: a whole
/// callback URL, `code#state`, or the code alone (as the Mac app takes).
pub fn pasted(text: &str) -> BTreeMap<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return BTreeMap::new();
    }
    if let Some((_, query)) = text.split_once('?') {
        let query = query.split('#').next().unwrap_or(query);
        return query_params(query).into_iter().filter(|(k, v)| matches!(k.as_str(), "code" | "state" | "error") && !v.is_empty()).collect();
    }
    if !text.contains('/') {
        if let Some((code, state)) = text.split_once('#') {
            let mut out = BTreeMap::from([("code".to_string(), code.to_string())]);
            if !state.is_empty() {
                out.insert("state".into(), state.into());
            }
            return out;
        }
    }
    BTreeMap::from([("code".to_string(), text.to_string())])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Read;

    pub(crate) fn get(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nhost: localhost\r\n\r\n").unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    }

    #[test]
    fn the_listener_takes_the_code_with_this_sign_ins_state_alone() {
        let callback = listen(&[0]).expect("a free port");
        let port = callback.listeners[0].local_addr().unwrap().port();
        let cancel = AtomicBool::new(false);
        let browser = std::thread::spawn(move || {
            let favicon = get(port, "/favicon.ico");
            let other = get(port, "/callback?code=x&state=someone-elses");
            let ours = get(port, "/callback?code=the%20code&state=ours");
            (favicon, other, ours)
        });
        let code = callback.wait_for_code("ours", &cancel, Duration::from_secs(10)).unwrap();
        let (favicon, other, ours) = browser.join().unwrap();
        assert_eq!(code, "the code");
        assert!(favicon.starts_with("HTTP/1.1 404"));
        assert!(other.contains("200 OK") && ours.contains("U4Ai"), "{ours}");
    }

    #[test]
    fn a_refused_sign_in_and_a_cancelled_one_end_the_wait() {
        let callback = listen(&[0]).unwrap();
        let port = callback.listeners[0].local_addr().unwrap().port();
        let browser = std::thread::spawn(move || get(port, "/callback?error=access_denied&state=s"));
        let refused = callback.wait_for_code("s", &AtomicBool::new(false), Duration::from_secs(10));
        browser.join().unwrap();
        assert!(matches!(refused, Err(SignInError::Failed(_))));
        let callback = listen(&[0]).unwrap();
        assert_eq!(callback.wait_for_code("s", &AtomicBool::new(true), Duration::from_secs(10)), Err(SignInError::Cancelled));
        assert!(matches!(callback.wait_for_code("s", &AtomicBool::new(false), Duration::ZERO), Err(SignInError::Failed(_))));
    }

    #[test]
    fn a_port_already_taken_moves_to_the_next() {
        let first = listen(&[0]).unwrap();
        let taken = first.listeners[0].local_addr().unwrap().port();
        let second = listen(&[taken, 0]).expect("the next port");
        assert_ne!(second.listeners[0].local_addr().unwrap().port(), taken);
        assert!(listen(&[taken]).is_none());
        assert!(second.listeners.iter().all(|l| l.local_addr().unwrap().ip().is_loopback()), "loopback alone");
    }

    #[test]
    fn the_challenge_is_s256_and_each_sign_in_has_its_own() {
        // SHA-256 in base64url without padding, as Python's hashlib and
        // base64 give it for the same verifier.
        assert_eq!(challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r2wW1gFWFOEjXk"), "dZrJz5b___3oA5uVny1DZqEO9NM98brdg_f4AEY03Bc");
        let pkce = Pkce::new().unwrap();
        assert_eq!(pkce.verifier.len(), 86, "64 bytes in base64url");
        assert_ne!(Pkce::new().unwrap().state, pkce.state);
    }

    #[test]
    fn a_pasted_callback_reads_as_a_url_a_code_and_state_or_a_code() {
        let p = pasted(" http://localhost:1456/callback?code=abc&state=xyz&extra=1 ");
        assert_eq!(p, BTreeMap::from([("code".into(), "abc".into()), ("state".into(), "xyz".into())]));
        assert_eq!(pasted("http://localhost:1456/callback?error=access_denied&state=s").get("error").map(String::as_str), Some("access_denied"));
        assert_eq!(pasted("abc#xyz"), BTreeMap::from([("code".into(), "abc".into()), ("state".into(), "xyz".into())]));
        assert_eq!(pasted("abc"), BTreeMap::from([("code".into(), "abc".into())]));
        assert!(pasted("  ").is_empty());
    }
}
