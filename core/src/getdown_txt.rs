//! The few settings of the game's `getdown.txt` the launcher needs. The file is `key = value` lines; a value may start
//! with a platform qualifier (`java_location = [windows] ...`). Parsed by the same code as the launch settings.

use crate::getdown_launch::{LaunchConfig, Target};

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
    /// Reads the settings for the machine this runs on.
    pub fn parse(text: &str) -> Self {
        Self::parse_for(text, &Target::current())
    }

    /// Reads the settings for `target`, with the updater's own rules (shared with the launch code): a `#` inside
    /// a line starts a comment unless the file sets `strict_comments`, and a value that starts with `[os-arch]`
    /// applies to that platform only. The first non-empty value of a key wins.
    pub fn parse_for(text: &str, target: &Target) -> Self {
        let file = LaunchConfig::parse(text, target);
        let value = |key: &str| {
            file.values(key)
                .into_iter()
                .find(|v| !v.is_empty())
                .map(str::to_owned)
        };
        Self {
            version: value("version"),
            appbase: value("appbase"),
            latest: value("latest"),
            java_version: value("java_version").and_then(|v| v.parse().ok()),
        }
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
    fn inline_comments_and_platform_qualified_values_follow_the_updaters_rules() {
        let text = "version = 7 # the build\njava_version = [windows] 25000000\njava_version = [linux] 21000000\n\
appbase = http://a.invalid/%VERSION% #x\n";
        let windows = GetdownConfig::parse_for(text, &Target::new("Windows 11", "amd64"));
        assert_eq!(windows.version.as_deref(), Some("7"));
        assert_eq!(windows.java_version, Some(25_000_000));
        assert_eq!(
            windows.appbase.as_deref(),
            Some("http://a.invalid/%VERSION%")
        );
        let linux = GetdownConfig::parse_for(text, &Target::new("Linux", "amd64"));
        assert_eq!(linux.java_version, Some(21_000_000));
        // A file that asks for strict comments keeps a `#` that is not in column 0.
        let strict = "strict_comments = true\nappbase = http://a.invalid/x#y\n";
        assert_eq!(
            GetdownConfig::parse_for(strict, &Target::new("Linux", "amd64"))
                .appbase
                .as_deref(),
            Some("http://a.invalid/x#y")
        );
    }

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
