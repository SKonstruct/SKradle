//! The signed runtime and module index: which Java runtimes and optional modules (for example the graphics agent)
//! the launcher may download, with URL, size and SHA-256 for each.
//!
//! The index is served by the launcher's backend but signed OFFLINE by the owner; the launcher trusts only keys
//! pinned in the program, so a compromised server cannot make it download something else. The envelope is
//! `{"payload": "<the document as a string>", "signature": "<base64 Ed25519 over those exact bytes>", "keyId": "..."}`:
//! verifiers never need canonical JSON because the signature covers the payload string byte for byte.

use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;

use crate::game::Os;
use crate::net::{Client, NetError};
use crate::runtime::JavaVersion;

/// A key the launcher trusts for the index, identified by the `keyId` the envelope names.
#[derive(Debug, Clone, Copy)]
pub struct TrustedKey {
    pub key_id: &'static str,
    pub public_key: [u8; 32],
}

/// The keys compiled into the launcher. Empty until the owner generates the signing key pair (the backend's
/// `scripts/launcher_index.py keygen` prints the public key to paste here); with no keys every index is refused.
pub const PINNED_KEYS: &[TrustedKey] = &[];

const SCHEMA: u32 = 1;
const MAX_ENVELOPE_BYTES: u64 = 1 << 20;
/// How long a signature may claim to be valid. Bounds how long a stolen old index could be replayed.
const MAX_VALIDITY_SECS: u64 = 120 * 24 * 3600;
/// Tolerated difference between the signer's clock and this machine's.
const CLOCK_SKEW_SECS: u64 = 24 * 3600;
const MAX_RUNTIME_BYTES: u64 = 1 << 30;
const MAX_MODULE_BYTES: u64 = 256 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub fn current() -> Arch {
        if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else {
            Arch::X64
        }
    }
}

#[derive(Debug)]
pub enum IndexError {
    Net(NetError),
    /// The envelope or the payload is not the JSON we expect.
    Malformed(String),
    /// The envelope names a key the launcher does not trust (or the launcher pins none).
    UnknownKey(String),
    /// The signature does not match the payload.
    BadSignature,
    UnsupportedSchema(u32),
    /// `expiresAt` has passed.
    Expired,
    /// The index is older than one already accepted: a stale but still-valid signature being replayed.
    RolledBack {
        newest_seen: u64,
        got: u64,
    },
    /// The index was generated in the future, or claims a validity longer than allowed.
    BadTimes,
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::Net(e) => write!(f, "{e}"),
            IndexError::Malformed(why) => write!(f, "the index is malformed: {why}"),
            IndexError::UnknownKey(id) => {
                write!(f, "the index is signed with an unknown key ({id})")
            }
            IndexError::BadSignature => write!(f, "the index signature is invalid"),
            IndexError::UnsupportedSchema(v) => write!(f, "unsupported index schema {v}"),
            IndexError::Expired => write!(f, "the index has expired"),
            IndexError::RolledBack { .. } => write!(f, "the index is older than one already seen"),
            IndexError::BadTimes => write!(f, "the index has implausible dates"),
        }
    }
}

impl std::error::Error for IndexError {}

impl From<NetError> for IndexError {
    fn from(e: NetError) -> Self {
        IndexError::Net(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEntry {
    pub id: String,
    pub vendor: String,
    pub java_version: JavaVersion,
    pub os: Os,
    pub arch: Arch,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// The folder inside the archive that is the Java home (for example `jdk-25+36-jre/Contents/Home`).
    pub java_home: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleEntry {
    pub id: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub min_java: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub generated_at: u64,
    pub expires_at: u64,
    pub runtimes: Vec<RuntimeEntry>,
    pub modules: Vec<ModuleEntry>,
    /// Entries that were ignored, with the reason, so callers can log them.
    pub skipped: Vec<String>,
}

impl Index {
    /// The newest runtime for this platform that is at least `min_major` and, if `agent` is set, new enough for the
    /// graphics agent.
    pub fn runtime_for(&self, os: Os, arch: Arch, min_major: u32) -> Option<&RuntimeEntry> {
        self.runtimes
            .iter()
            .filter(|r| r.os == os && r.arch == arch && r.java_version.major >= min_major)
            .max_by_key(|r| r.java_version)
    }

    pub fn module(&self, id: &str) -> Option<&ModuleEntry> {
        self.modules.iter().find(|m| m.id == id)
    }
}

#[derive(Deserialize)]
struct Envelope {
    payload: String,
    signature: String,
    #[serde(rename = "keyId")]
    key_id: String,
}

#[derive(Deserialize)]
struct RawIndex {
    schema: u32,
    #[serde(rename = "generatedAt")]
    generated_at: String,
    #[serde(rename = "expiresAt")]
    expires_at: String,
    #[serde(default)]
    runtimes: Vec<serde_json::Value>,
    #[serde(default)]
    modules: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawRuntime {
    id: String,
    vendor: String,
    #[serde(rename = "javaVersion")]
    java_version: String,
    os: String,
    arch: String,
    url: String,
    sha256: String,
    size: u64,
    #[serde(rename = "javaHome", default)]
    java_home: Option<String>,
}

#[derive(Deserialize)]
struct RawModule {
    id: String,
    version: String,
    url: String,
    sha256: String,
    size: u64,
    #[serde(rename = "minJava", default)]
    min_java: Option<u32>,
}

/// Fetches the index from `url` and verifies it against the pinned keys.
pub fn fetch(client: &Client, url: &str) -> Result<Index, IndexError> {
    let text = client.get_text(url, MAX_ENVELOPE_BYTES)?;
    verify(&text, PINNED_KEYS, unix_now())
}

/// Refuses an index older than the newest one this installation has accepted (`newest_seen` is the `generated_at`
/// the application stored after its last good check). The caller stores `index.generated_at` when this passes.
pub fn reject_rollback(index: &Index, newest_seen: Option<u64>) -> Result<(), IndexError> {
    match newest_seen {
        Some(seen) if index.generated_at < seen => Err(IndexError::RolledBack {
            newest_seen: seen,
            got: index.generated_at,
        }),
        _ => Ok(()),
    }
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Verifies an envelope: known key, valid signature over the payload bytes, supported schema, plausible and
/// unexpired dates. Entries that fail validation are skipped (and listed in `skipped`), they do not fail the index.
pub fn verify(envelope: &str, trusted: &[TrustedKey], now: u64) -> Result<Index, IndexError> {
    if envelope.len() as u64 > MAX_ENVELOPE_BYTES {
        return Err(IndexError::Malformed("envelope too large".into()));
    }
    let envelope: Envelope =
        serde_json::from_str(envelope).map_err(|e| IndexError::Malformed(e.to_string()))?;
    let key = trusted
        .iter()
        .find(|k| k.key_id == envelope.key_id)
        .ok_or_else(|| IndexError::UnknownKey(envelope.key_id.clone()))?;
    let verifying =
        VerifyingKey::from_bytes(&key.public_key).map_err(|_| IndexError::BadSignature)?;
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(envelope.signature.trim())
        .map_err(|_| IndexError::Malformed("signature is not base64".into()))?;
    let signature_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| IndexError::Malformed("signature has the wrong length".into()))?;
    verifying
        .verify_strict(
            envelope.payload.as_bytes(),
            &Signature::from_bytes(&signature_bytes),
        )
        .map_err(|_| IndexError::BadSignature)?;

    let raw: RawIndex = serde_json::from_str(&envelope.payload)
        .map_err(|e| IndexError::Malformed(e.to_string()))?;
    if raw.schema != SCHEMA {
        return Err(IndexError::UnsupportedSchema(raw.schema));
    }
    let generated_at = parse_rfc3339(&raw.generated_at)
        .ok_or_else(|| IndexError::Malformed("generatedAt".into()))?;
    let expires_at =
        parse_rfc3339(&raw.expires_at).ok_or_else(|| IndexError::Malformed("expiresAt".into()))?;
    if generated_at > now.saturating_add(CLOCK_SKEW_SECS)
        || expires_at <= generated_at
        || expires_at - generated_at > MAX_VALIDITY_SECS
    {
        return Err(IndexError::BadTimes);
    }
    if expires_at <= now {
        return Err(IndexError::Expired);
    }

    let mut index = Index {
        generated_at,
        expires_at,
        runtimes: vec![],
        modules: vec![],
        skipped: vec![],
    };
    for value in raw.runtimes {
        match serde_json::from_value::<RawRuntime>(value)
            .map_err(|e| e.to_string())
            .and_then(runtime_entry)
        {
            Ok(entry) => index.runtimes.push(entry),
            Err(why) => index.skipped.push(format!("runtime: {why}")),
        }
    }
    for value in raw.modules {
        match serde_json::from_value::<RawModule>(value)
            .map_err(|e| e.to_string())
            .and_then(module_entry)
        {
            Ok(entry) => index.modules.push(entry),
            Err(why) => index.skipped.push(format!("module: {why}")),
        }
    }
    Ok(index)
}

fn runtime_entry(raw: RawRuntime) -> Result<RuntimeEntry, String> {
    let os = match raw.os.as_str() {
        "windows" => Os::Windows,
        "macos" => Os::MacOs,
        "linux" => Os::Linux,
        other => return Err(format!("{}: unknown os {other:?}", raw.id)),
    };
    let arch = match raw.arch.as_str() {
        "x64" => Arch::X64,
        "arm64" => Arch::Arm64,
        other => return Err(format!("{}: unknown arch {other:?}", raw.id)),
    };
    let java_version = JavaVersion::parse(&raw.java_version)
        .ok_or_else(|| format!("{}: bad javaVersion", raw.id))?;
    check_download(&raw.id, &raw.url, &raw.sha256, raw.size, MAX_RUNTIME_BYTES)?;
    if let Some(home) = &raw.java_home {
        if !is_plain_relative_path(home) {
            return Err(format!(
                "{}: javaHome must be a plain relative path",
                raw.id
            ));
        }
    }
    Ok(RuntimeEntry {
        id: raw.id,
        vendor: raw.vendor,
        java_version,
        os,
        arch,
        url: raw.url,
        sha256: raw.sha256.to_ascii_lowercase(),
        size: raw.size,
        java_home: raw.java_home,
    })
}

fn module_entry(raw: RawModule) -> Result<ModuleEntry, String> {
    check_download(&raw.id, &raw.url, &raw.sha256, raw.size, MAX_MODULE_BYTES)?;
    if raw.id.is_empty()
        || !raw
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(format!("{}: bad module id", raw.id));
    }
    Ok(ModuleEntry {
        id: raw.id,
        version: raw.version,
        url: raw.url,
        sha256: raw.sha256.to_ascii_lowercase(),
        size: raw.size,
        min_java: raw.min_java,
    })
}

fn check_download(id: &str, url: &str, sha256: &str, size: u64, max: u64) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| format!("{id}: the URL must be https"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty()
        || host.contains(['@', '\\', ' '])
        || url.chars().any(|c| c.is_control() || c == ' ')
    {
        return Err(format!("{id}: bad URL"));
    }
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{id}: sha256 must be 64 hex digits"));
    }
    if size == 0 || size > max {
        return Err(format!("{id}: implausible size {size}"));
    }
    Ok(())
}

fn is_plain_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.contains([':', '\0', '\\'])
        && path.split('/').all(|part| part != ".." && part != ".")
}

/// (Public so tests and tools can turn the index's dates into Unix seconds.)
/// Parses `YYYY-MM-DDTHH:MM:SS` with an optional fraction and `Z` or `+HH:MM`/`-HH:MM`; returns Unix seconds.
pub fn parse_rfc3339(text: &str) -> Option<u64> {
    // Digits only: `i64::from_str` would also take "+5" and "-5".
    fn number(part: &str) -> Option<i64> {
        (!part.is_empty() && part.len() <= 9 && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse().ok())?
    }
    let (date, time) = text.trim().split_once(['T', 't'])?;
    let mut d = date.split('-');
    let (year, month, day) = (number(d.next()?)?, number(d.next()?)?, number(d.next()?)?);
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if d.next().is_some() || !(1..=month_days).contains(&day) || !(1970..=9999).contains(&year) {
        return None;
    }
    let (clock, offset_secs) = if let Some(clock) = time.strip_suffix(['Z', 'z']) {
        (clock, 0i64)
    } else {
        let at = time.rfind(['+', '-'])?;
        let (clock, off) = time.split_at(at);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let (oh, om) = off[1..].split_once(':')?;
        let (oh, om) = (number(oh)?, number(om)?);
        if oh > 23 || om > 59 {
            return None;
        }
        (clock, sign * (oh * 3600 + om * 60))
    };
    let clock = clock.split('.').next()?;
    let mut t = clock.split(':');
    let (h, m, s) = (number(t.next()?)?, number(t.next()?)?, number(t.next()?)?);
    if t.next().is_some() || h > 23 || m > 59 || s > 60 {
        return None;
    }
    // Days since 1970-01-01 (Howard Hinnant's civil-from-days inverse).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + m * 60 + s - offset_secs;
    u64::try_from(secs).ok()
}

#[cfg(test)]
mod time_tests {
    use super::*;

    #[test]
    fn impossible_dates_and_times_are_refused() {
        for bad in [
            "2026-02-30T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2025-02-29T00:00:00Z",
            "2026-11-18T-1:00:00Z",
            "2026-11-18T00:-5:00Z",
            "2026-11-18T00:00:+5Z",
            "2026-11-18T00:00:00+24:00",
            "2026-11-18T00:00:00+01:60",
            "2026-11-18T00:00:00+-1:00",
            "+2026-11-18T00:00:00Z",
            "2026-11-18T24:00:00Z",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
        assert!(
            parse_rfc3339("2024-02-29T12:00:00Z").is_some(),
            "a leap day"
        );
        assert!(parse_rfc3339("2026-11-18T00:00:00-05:30").is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const SEED: [u8; 32] = [7; 32];
    const NOW: u64 = 1_795_000_000; // 2026-11-18
    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&SEED)
    }

    fn trusted() -> Vec<TrustedKey> {
        vec![TrustedKey {
            key_id: "k1",
            public_key: signing_key().verifying_key().to_bytes(),
        }]
    }

    fn payload(extra_runtime: &str) -> String {
        format!(
            r#"{{"schema":1,"generatedAt":"2026-11-10T00:00:00Z","expiresAt":"2027-01-10T00:00:00Z",
            "runtimes":[
              {{"id":"t25-mac-arm","vendor":"Temurin","javaVersion":"25.0.3","os":"macos","arch":"arm64","url":"https://example.com/a.tar.gz","sha256":"{SHA}","size":5000,"javaHome":"jdk/Contents/Home"}},
              {{"id":"t24-mac-arm","vendor":"Temurin","javaVersion":"24.0.2","os":"macos","arch":"arm64","url":"https://example.com/b.tar.gz","sha256":"{SHA}","size":4000}},
              {{"id":"t25-win","vendor":"Temurin","javaVersion":"25.0.3","os":"windows","arch":"x64","url":"https://example.com/c.zip","sha256":"{SHA}","size":6000}}
              {extra_runtime}],
            "modules":[{{"id":"skgfx-agent","version":"1.0.0","url":"https://example.com/skgfx-agent.jar","sha256":"{SHA}","size":48000,"minJava":24}}]}}"#
        )
    }

    fn envelope_with(payload: &str, key: &SigningKey, key_id: &str) -> String {
        let signature = base64::engine::general_purpose::STANDARD
            .encode(key.sign(payload.as_bytes()).to_bytes());
        serde_json::json!({ "payload": payload, "signature": signature, "keyId": key_id })
            .to_string()
    }

    fn good() -> String {
        envelope_with(&payload(""), &signing_key(), "k1")
    }

    #[test]
    fn a_valid_index_is_accepted_and_queried() {
        let index = verify(&good(), &trusted(), NOW).unwrap();
        assert_eq!(index.runtimes.len(), 3);
        assert!(index.skipped.is_empty());
        let r = index.runtime_for(Os::MacOs, Arch::Arm64, 24).unwrap();
        assert_eq!(r.id, "t25-mac-arm", "the newest matching runtime wins");
        assert_eq!(r.java_home.as_deref(), Some("jdk/Contents/Home"));
        assert_eq!(index.runtime_for(Os::MacOs, Arch::Arm64, 26), None);
        assert_eq!(index.runtime_for(Os::Linux, Arch::X64, 24), None);
        assert_eq!(index.module("skgfx-agent").unwrap().min_java, Some(24));
        assert_eq!(index.module("nope"), None);
    }

    #[test]
    fn a_tampered_payload_is_rejected() {
        let tampered = good().replace("5000", "5001");
        assert!(matches!(
            verify(&tampered, &trusted(), NOW),
            Err(IndexError::BadSignature)
        ));
    }

    #[test]
    fn an_unknown_key_or_the_wrong_key_is_rejected() {
        assert!(matches!(
            verify(
                &envelope_with(&payload(""), &signing_key(), "other"),
                &trusted(),
                NOW
            ),
            Err(IndexError::UnknownKey(_))
        ));
        let attacker = SigningKey::from_bytes(&[9; 32]);
        assert!(matches!(
            verify(
                &envelope_with(&payload(""), &attacker, "k1"),
                &trusted(),
                NOW
            ),
            Err(IndexError::BadSignature)
        ));
        assert!(
            matches!(verify(&good(), &[], NOW), Err(IndexError::UnknownKey(_))),
            "no pinned key, nothing is trusted"
        );
    }

    #[test]
    fn expired_future_and_overlong_indexes_are_rejected() {
        assert!(matches!(
            verify(
                &good(),
                &trusted(),
                parse_rfc3339("2027-02-01T00:00:00Z").unwrap()
            ),
            Err(IndexError::Expired)
        ));
        assert!(matches!(
            verify(
                &good(),
                &trusted(),
                parse_rfc3339("2026-01-01T00:00:00Z").unwrap()
            ),
            Err(IndexError::BadTimes)
        ));
        let long = payload("").replace("2027-01-10", "2030-01-10");
        assert!(matches!(
            verify(&envelope_with(&long, &signing_key(), "k1"), &trusted(), NOW),
            Err(IndexError::BadTimes)
        ));
    }

    #[test]
    fn unsupported_schema_and_garbage_are_rejected() {
        let v2 = payload("").replace("\"schema\":1", "\"schema\":2");
        assert!(matches!(
            verify(&envelope_with(&v2, &signing_key(), "k1"), &trusted(), NOW),
            Err(IndexError::UnsupportedSchema(2))
        ));
        for garbage in [
            "",
            "{}",
            "not json",
            r#"{"payload":"x","signature":"!!","keyId":"k1"}"#,
            r#"{"payload":"x","signature":"AAAA","keyId":"k1"}"#,
        ] {
            assert!(verify(garbage, &trusted(), NOW).is_err(), "{garbage:?}");
        }
        let signed_nonsense = envelope_with("not json", &signing_key(), "k1");
        assert!(matches!(
            verify(&signed_nonsense, &trusted(), NOW),
            Err(IndexError::Malformed(_))
        ));
    }

    #[test]
    fn bad_entries_are_skipped_not_fatal() {
        let bad = format!(
            r#",{{"id":"http","vendor":"x","javaVersion":"25","os":"linux","arch":"x64","url":"http://example.com/a","sha256":"{SHA}","size":1}},
            {{"id":"sha","vendor":"x","javaVersion":"25","os":"linux","arch":"x64","url":"https://example.com/a","sha256":"zz","size":1}},
            {{"id":"os","vendor":"x","javaVersion":"25","os":"plan9","arch":"x64","url":"https://example.com/a","sha256":"{SHA}","size":1}},
            {{"id":"home","vendor":"x","javaVersion":"25","os":"linux","arch":"x64","url":"https://example.com/a","sha256":"{SHA}","size":1,"javaHome":"../../etc"}},
            {{"id":"user","vendor":"x","javaVersion":"25","os":"linux","arch":"x64","url":"https://example.com@evil.example/a","sha256":"{SHA}","size":1}},
            {{"id":"huge","vendor":"x","javaVersion":"25","os":"linux","arch":"x64","url":"https://example.com/a","sha256":"{SHA}","size":99999999999}},
            {{"nonsense":true}}"#
        );
        let index = verify(
            &envelope_with(&payload(&bad), &signing_key(), "k1"),
            &trusted(),
            NOW,
        )
        .unwrap();
        assert_eq!(index.runtimes.len(), 3, "only the good entries remain");
        assert_eq!(index.skipped.len(), 7);
    }

    #[test]
    fn an_older_index_than_one_already_seen_is_refused() {
        let index = verify(&good(), &trusted(), NOW).unwrap();
        assert!(
            reject_rollback(&index, None).is_ok(),
            "first run: nothing seen yet"
        );
        assert!(
            reject_rollback(&index, Some(index.generated_at)).is_ok(),
            "the same index again is fine"
        );
        assert!(
            reject_rollback(&index, Some(index.generated_at - 1)).is_ok(),
            "a newer one than before"
        );
        assert!(matches!(
            reject_rollback(&index, Some(index.generated_at + 1)),
            Err(IndexError::RolledBack { .. })
        ));
    }

    #[test]
    fn dates_parse_in_all_forms() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_rfc3339("2026-11-18T00:00:00Z"),
            Some(1_795_000_000 - 1_795_000_000 % 86_400)
        );
        assert_eq!(
            parse_rfc3339("2026-11-18T01:00:00+01:00"),
            parse_rfc3339("2026-11-18T00:00:00Z")
        );
        assert_eq!(
            parse_rfc3339("2026-11-18T00:00:00.123Z"),
            parse_rfc3339("2026-11-18T00:00:00Z")
        );
        assert_eq!(
            parse_rfc3339("2024-02-29T12:00:00Z").unwrap() % 86_400,
            12 * 3600
        );
        for bad in [
            "",
            "2026-13-01T00:00:00Z",
            "2026-11-18",
            "yesterday",
            "2026-11-18T25:00:00Z",
            "1969-12-31T23:59:59Z",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad:?}");
        }
    }
}
