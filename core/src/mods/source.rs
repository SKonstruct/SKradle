//! Where an original code jar comes from when no verified copy is on disk.
//!
//! The launcher keeps a verified copy of each code jar the first time it sees the real one (see the apply module),
//! so this is only needed when the game folder was already modified by something else before the launcher first
//! ran. The fetch goes behind a trait so tests never touch the network, and whatever it returns is checked against
//! `digest2.txt` before it is used, so the server is not trusted.

use crate::getdown_txt::GetdownConfig;
use crate::install::expand_appbase;
use crate::net::Client;

/// Something that can produce the game's original copy of a file.
pub trait PristineSource {
    /// The bytes of `rel` (a path such as `code/config.jar`, as listed in `digest2.txt`), at most `max_bytes`.
    fn fetch(&self, rel: &str, max_bytes: u64) -> Result<Vec<u8>, String>;
}

/// Downloads from the game's own servers over HTTPS, for the installed version.
pub struct HttpSource {
    client: Client,
    appbase: String,
}

impl HttpSource {
    /// `None` when the install's `getdown.txt` does not point at a trustworthy game host (see
    /// [`crate::install::expand_appbase`]).
    pub fn from_getdown(config: &GetdownConfig) -> Option<Self> {
        Some(Self {
            client: Client::new(),
            appbase: expand_appbase(config)?,
        })
    }
}

impl PristineSource for HttpSource {
    fn fetch(&self, rel: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
        // The path comes from digest2.txt, which already limits it; refuse anything odd regardless.
        if rel.is_empty()
            || rel.starts_with('/')
            || rel
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || rel.contains(['\\', '?', '#'])
        {
            return Err(format!("refusing to fetch {rel:?}"));
        }
        self.client
            .get_bytes(&format!("{}/{rel}", self.appbase), max_bytes)
            .map_err(|e| e.to_string())
    }
}

/// A source that has nothing: the apply fails when it would be needed.
pub struct NoSource(pub String);

impl PristineSource for NoSource {
    fn fetch(&self, _rel: &str, _max_bytes: u64) -> Result<Vec<u8>, String> {
        Err(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_trusted_hosts_become_a_source() {
        let good = GetdownConfig {
            version: Some("20261110044614".into()),
            appbase: Some("http://gamemedia.spiralknights.com/spiral/%VERSION%".into()),
            ..GetdownConfig::default()
        };
        assert!(HttpSource::from_getdown(&good).is_some());
        let evil = GetdownConfig {
            appbase: Some("https://evil.example/spiral/%VERSION%".into()),
            ..good.clone()
        };
        assert!(HttpSource::from_getdown(&evil).is_none());
        assert!(HttpSource::from_getdown(&GetdownConfig::default()).is_none());
    }

    #[test]
    fn odd_paths_are_refused_before_any_request() {
        let good = GetdownConfig {
            version: Some("1".into()),
            appbase: Some("http://gamemedia.spiralknights.com/spiral/%VERSION%".into()),
            ..GetdownConfig::default()
        };
        let source = HttpSource::from_getdown(&good).unwrap();
        for bad in ["", "/etc/passwd", "../x", "a//b", "a\\b", "x?y", "a/./b"] {
            assert!(
                source.fetch(bad, 10).unwrap_err().contains("refusing"),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn no_source_fails() {
        assert_eq!(
            NoSource("offline".into()).fetch("code/config.jar", 1),
            Err("offline".into())
        );
    }
}
