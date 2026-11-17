//! The few settings of the game's `getdown.txt` the launcher needs. The file is `key = value` lines;
//! lines starting with `[platform]` apply to one platform only and are ignored here.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetdownConfig {
    /// The installed game version, for example `20261110044614`.
    pub version: Option<String>,
    /// Where this version's files are downloaded from (`%VERSION%` is not expanded here).
    pub appbase: Option<String>,
    /// Where the newest `getdown.txt` is published.
    pub latest: Option<String>,
    /// The minimum Java version, in Getdown's encoding (`25000000` means 25.0.0).
    pub java_version: Option<u64>,
}

impl GetdownConfig {
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if value.is_empty() {
                continue;
            }
            match key {
                "version" if config.version.is_none() => config.version = Some(value.to_owned()),
                "appbase" if config.appbase.is_none() => config.appbase = Some(value.to_owned()),
                "latest" if config.latest.is_none() => config.latest = Some(value.to_owned()),
                "java_version" if config.java_version.is_none() => {
                    config.java_version = value.parse().ok()
                }
                _ => {}
            }
        }
        config
    }

    /// The minimum Java major version (`25000000` gives 25).
    pub fn java_major(&self) -> Option<u64> {
        self.java_version.map(|v| v / 1_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# comment\r\nversion = 20261110044614\r\nappbase = http://example.invalid/spiral/%VERSION%\r\n\
latest = http://example.invalid/spiral/latest/getdown.txt\r\njava_version = 25000000\r\n\
[windows] java_location = /spiral/client/java_windows.jar\r\ncode = code/a.jar\r\n";

    #[test]
    fn reads_the_settings_the_launcher_needs() {
        let config = GetdownConfig::parse(SAMPLE);
        assert_eq!(config.version.as_deref(), Some("20261110044614"));
        assert_eq!(
            config.appbase.as_deref(),
            Some("http://example.invalid/spiral/%VERSION%")
        );
        assert_eq!(
            config.latest.as_deref(),
            Some("http://example.invalid/spiral/latest/getdown.txt")
        );
        assert_eq!(config.java_version, Some(25_000_000));
        assert_eq!(config.java_major(), Some(25));
    }

    #[test]
    fn first_value_wins_and_platform_lines_are_ignored() {
        let config = GetdownConfig::parse("version = 1\n[linux] version = 2\nversion = 3\n");
        assert_eq!(config.version.as_deref(), Some("1"));
    }

    #[test]
    fn tolerates_junk() {
        let config =
            GetdownConfig::parse("no equals sign\n= value\nversion =\njava_version = abc\n");
        assert_eq!(config, GetdownConfig::default());
    }
}
