//! What every provider's requests share: the HTTP client, how a status
//! reads as a `ReadError`, and `Retry-After` in either of its forms.

use std::time::Duration;

use jiff::Timestamp;

use crate::providers::ReadError;

/// The HTTP client: statuses are answers, not errors; Windows' TLS on
/// Windows (core/Cargo.toml says why).
pub fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(30)))
        .user_agent(format!("U4Ai/{} ({})", crate::version::VERSION, std::env::consts::OS));
    #[cfg(windows)]
    let config = config.tls_config(ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build());
    config.build().into()
}

/// How long `Retry-After` says to wait, in either of RFC 9110's forms:
/// seconds, or an HTTP date (`now` measures it).
pub fn retry_after(text: &str, now: Timestamp) -> Option<Duration> {
    let text = text.trim();
    if let Ok(seconds) = text.parse::<f64>() {
        return (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds.min(86_400.0)));
    }
    let when = jiff::fmt::rfc2822::DateTimeParser::new().parse_timestamp(text).ok()?;
    let seconds = when.as_second() - now.as_second();
    Some(Duration::from_secs(seconds.max(0) as u64))
}

/// What a status means for a read: none for success.
pub fn classify(status: u16, retry: Option<&str>, now: Timestamp) -> Option<ReadError> {
    match status {
        200..=299 => None,
        401 => Some(ReadError::Unauthorized),
        429 => Some(ReadError::RateLimited(retry.and_then(|r| retry_after(r, now)))),
        other => Some(ReadError::Http(other)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A server on a free loopback port answering each request with
    /// `answer(method, path, headers, body)`'s status, headers and body,
    /// noting every request. Its base URL.
    pub fn serve(answer: impl Fn(&str, &str, &str, &str) -> (u16, String, String) + Send + 'static) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let noted = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut headers = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    headers.push_str(&line);
                }
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).unwrap();
                let body = String::from_utf8_lossy(&body).to_string();
                let mut parts = first.split_whitespace();
                let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                noted.lock().unwrap().push(format!("{method} {path} {body}"));
                let (status, extra, text) = answer(method, path, &headers, &body);
                let mut out = stream;
                let _ = write!(out, "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{extra}\r\n{text}", text.len());
            }
        });
        (base, seen)
    }

    #[test]
    fn retry_after_reads_seconds_and_http_dates() {
        let now: Timestamp = "2026-11-25T10:00:00Z".parse().unwrap();
        assert_eq!(retry_after(" 120 ", now), Some(Duration::from_secs(120)));
        assert_eq!(retry_after("Wed, 25 Nov 2026 10:05:00 GMT", now), Some(Duration::from_secs(300)));
        assert_eq!(retry_after("Wed, 25 Nov 2026 09:00:00 GMT", now), Some(Duration::ZERO), "a date gone by is now");
        assert_eq!(retry_after("soon", now), None);
        assert_eq!(retry_after("-5", now), None);
        assert_eq!(classify(429, Some("60"), now), Some(ReadError::RateLimited(Some(Duration::from_secs(60)))));
        assert_eq!(classify(429, None, now), Some(ReadError::RateLimited(None)));
        assert_eq!((classify(204, None, now), classify(401, None, now), classify(503, None, now)), (None, Some(ReadError::Unauthorized), Some(ReadError::Http(503))));
    }

}
