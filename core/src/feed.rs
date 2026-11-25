//! The update feed: Sparkle's appcast, as WinSparkle and Flow's Linux
//! updater read it, and the Ed25519 check that what it offers was signed
//! with the project's key. Linux's updater reads it here (WinSparkle reads
//! its own); here so the tests run on every system. Ported from Flow's
//! `linux/src/updater.rs`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};

/// A release the feed offers.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// "0.1.0", the version people see.
    pub version: String,
    /// Its build number (`sparkle:version` in Linux's feed), which orders
    /// releases.
    pub build: u64,
    pub url: String,
    pub length: u64,
    /// Ed25519 over the download's bytes, in base64 (`sparkle:edSignature`).
    pub signature: String,
    /// The release's page (`sparkle:releaseNotesLink`).
    pub notes: Option<String>,
}

impl Release {
    /// "0.1.0 (build 160)", as U4Ai writes its own version.
    pub fn display(&self) -> String {
        format!("{} (build {})", self.version, self.build)
    }
}

/// Every release in an appcast: one `<enclosure>` an item, with
/// `sparkle:version` (the build), `sparkle:shortVersionString`,
/// `sparkle:edSignature` and `length`. One missing a part is passed over,
/// and so is one whose version isn't a release's ([`plain_version`]).
pub fn releases(xml: &str) -> Vec<Release> {
    let mut out = Vec::new();
    for item in xml.split("<item").skip(1) {
        let item = item.split("</item>").next().unwrap_or(item);
        let Some(tag) = item.split("<enclosure").nth(1) else { continue };
        let attrs = attributes(&tag[..tag.find('>').unwrap_or(tag.len())]);
        let get = |name: &str| attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        let release = (|| {
            Some(Release {
                version: get("sparkle:shortVersionString")?,
                build: get("sparkle:version")?.trim().parse().ok()?,
                url: get("url")?,
                length: get("length")?.trim().parse().ok()?,
                signature: get("sparkle:edSignature")?,
                notes: element(item, "sparkle:releaseNotesLink"),
            })
        })();
        match release {
            Some(r) if !plain_version(&r.version) => log::warn!("update feed: a release's version isn't numbers and dots: {:?}", r.version),
            Some(r) => out.push(r),
            None => log::warn!("update feed: an enclosure lacks a part: {attrs:?}"),
        }
    }
    out
}

/// The newest release in `xml`, by build.
pub fn newest(xml: &str) -> Option<Release> {
    releases(xml).into_iter().max_by_key(|r| r.build)
}

/// Whether `version` is a release's as the scripts write one: numbers and
/// dots, "0.1.0.1". The feed itself isn't signed, and the version goes
/// into folder names and the menu.
pub fn plain_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    (1..=4).contains(&parts.len()) && parts.iter().all(|p| (1..=9).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// The text of the first `<name>…</name>` in `xml`, entities undone.
fn element(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let text = &xml[start..start + xml[start..].find("</")?];
    let text = unescape(text.trim());
    (!text.is_empty()).then_some(text)
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// `name="value"` pairs (or single-quoted), XML's five entities undone.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim().trim_end_matches('/').rsplit(char::is_whitespace).next().unwrap_or("").to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let Some(close) = after[1..].find(quote) else { break };
        out.push((name, unescape(&after[1..1 + close])));
        rest = &after[close + 2..];
    }
    out
}

/// Whether `signature` (base64) is `key`'s (base64) Ed25519 signature of
/// `bytes`, as Sparkle's `sign_update` and examples/update_key.rs make it.
pub fn signed(bytes: &[u8], signature: &str, key: &str) -> bool {
    let key: Option<[u8; 32]> = STANDARD.decode(key.trim()).ok().and_then(|k| k.try_into().ok());
    let sig: Option<[u8; 64]> = STANDARD.decode(signature.trim()).ok().and_then(|s| s.try_into().ok());
    let (Some(key), Some(sig)) = (key, sig) else { return false };
    VerifyingKey::from_bytes(&key).is_ok_and(|k| k.verify_strict(bytes, &Signature::from_bytes(&sig)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <item>
      <title>Version 0.1.3</title>
      <sparkle:releaseNotesLink>https://github.com/arcanii/U4Ai/releases/tag/v0.1.3-linux</sparkle:releaseNotesLink>
      <enclosure url="https://example.com/U4Ai-0.1.3-linux-x86_64.tar.gz?a=1&amp;b=2"
                 sparkle:version="146"
                 sparkle:shortVersionString="0.1.3"
                 sparkle:edSignature="c2lnbmF0dXJl"
                 length="4000000"
                 type="application/octet-stream"/>
    </item>
    <item>
      <enclosure url='https://example.com/old.tar.gz' sparkle:version="120" sparkle:shortVersionString="0.1.2"
                 sparkle:edSignature="b2xk" length="10"/>
    </item>
    <item><enclosure url="https://example.com/broken.tar.gz" sparkle:version="999"/></item>
    <item><enclosure url="https://example.com/x" sparkle:version="1000" sparkle:shortVersionString="1;rm -rf" sparkle:edSignature="eA==" length="1"/></item>
  </channel>
</rss>"#;

    #[test]
    fn the_feed_gives_each_release_and_the_newest_wins() {
        let found = releases(FEED);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(
            found[0],
            Release {
                version: "0.1.3".into(),
                build: 146,
                url: "https://example.com/U4Ai-0.1.3-linux-x86_64.tar.gz?a=1&b=2".into(),
                length: 4_000_000,
                signature: "c2lnbmF0dXJl".into(),
                notes: Some("https://github.com/arcanii/U4Ai/releases/tag/v0.1.3-linux".into()),
            }
        );
        assert_eq!((found[1].build, found[1].notes.clone()), (120, None));
        assert_eq!(newest(FEED).unwrap().display(), "0.1.3 (build 146)", "the broken and the oddly named are passed over");
        assert!(newest("<rss><channel></channel></rss>").is_none());
    }

    #[test]
    fn a_version_is_numbers_and_dots_alone() {
        for good in ["0.1.0", "0.1.0.1", "1"] {
            assert!(plain_version(good), "{good}");
        }
        for bad in ["", "0.1.0.DEV", "0..1", "1.2.3.4.5", "0.1/../x", "1234567890.1"] {
            assert!(!plain_version(bad), "{bad}");
        }
    }

    #[test]
    fn only_the_keys_own_signature_of_those_bytes_passes() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let public = STANDARD.encode(key.verifying_key().to_bytes());
        let sig = STANDARD.encode(key.sign(b"release").to_bytes());
        assert!(signed(b"release", &sig, &public));
        assert!(!signed(b"releasf", &sig, &public));
        assert!(!signed(b"release", &sig, crate::UPDATE_PUBLIC_KEY));
        assert!(!signed(b"release", "not base64!", &public));
        assert!(!signed(b"release", &STANDARD.encode([0u8; 12]), &public));
    }
}
