//! Installing the game from nothing: the game's own updater does the work, so a fresh install is
//! `getdown.txt` in an empty folder plus the updater jar, then running it. This module builds the URLs and the
//! command; downloading is up to the caller.
//!
//! The launcher downloads a jar and runs it, so nothing from a downloaded config is trusted blindly: the app base
//! must be on one of the game's own hosts, the version must be plain digits, and the launcher's own downloads use
//! HTTPS (the game's config says `http://`, but its hosts also answer HTTPS).

use std::ffi::OsString;
use std::path::Path;

use crate::getdown::CommandSpec;
use crate::getdown_txt::GetdownConfig;

/// Where the newest `getdown.txt` is. The older `.../spiral/client/getdown.txt` is stale: do not use it.
pub const LATEST_GETDOWN_TXT_URL: &str =
    "https://gamemedia2.spiralknights.com/spiral/latest/getdown.txt";

/// The only hosts a downloaded config may point the launcher at.
const GAME_HOSTS: [&str; 2] = [
    "gamemedia.spiralknights.com",
    "gamemedia2.spiralknights.com",
];

/// The app base of `config` with `%VERSION%` filled in, upgraded to HTTPS, or `None` if the config is not
/// trustworthy (no version, a version that is not digits, another host, user info, a port, another scheme).
pub fn expand_appbase(config: &GetdownConfig) -> Option<String> {
    let version = config
        .version
        .as_deref()
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))?;
    let appbase = config.appbase.as_deref()?.replace("%VERSION%", version);
    let rest = appbase
        .strip_prefix("https://")
        .or_else(|| appbase.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if !GAME_HOSTS.contains(&host)
        || path.contains(['\\', '?', '#'])
        || path.split('/').any(|part| part == "..")
    {
        return None;
    }
    let path = path.trim_end_matches('/');
    // No path: no trailing slash either, or `updater_jar_url` would produce `//code/...`.
    Some(if path.is_empty() {
        format!("https://{host}")
    } else {
        format!("https://{host}/{path}")
    })
}

/// The updater jar a fresh folder needs (`code/getdown-pro-new.jar`, the same file the Steam copy has).
pub fn updater_jar_url(config: &GetdownConfig) -> Option<String> {
    Some(format!(
        "{}/code/getdown-pro-new.jar",
        expand_appbase(config)?
    ))
}

/// Runs the downloaded updater in `dir` (which holds `getdown.txt` and `getdown-pro-new.jar`): it downloads the
/// game, verifies it and finishes. `java` must satisfy the game's minimum version.
pub fn fresh_install_command(dir: &Path, java: &Path) -> CommandSpec {
    CommandSpec {
        program: java.to_path_buf(),
        args: [
            "-Dsilent=true",
            "-jar",
            "getdown-pro-new.jar",
            ".",
            "client",
        ]
        .map(OsString::from)
        .to_vec(),
        cwd: dir.to_path_buf(),
        env: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_appbase_without_a_path_has_no_trailing_slash() {
        let c = config(Some("7"), Some("http://gamemedia.spiralknights.com/"));
        assert_eq!(
            expand_appbase(&c).unwrap(),
            "https://gamemedia.spiralknights.com"
        );
        assert_eq!(
            updater_jar_url(&c).unwrap(),
            "https://gamemedia.spiralknights.com/code/getdown-pro-new.jar"
        );
    }

    fn config(version: Option<&str>, appbase: Option<&str>) -> GetdownConfig {
        GetdownConfig {
            version: version.map(str::to_owned),
            appbase: appbase.map(str::to_owned),
            ..GetdownConfig::default()
        }
    }

    const LIVE_APPBASE: &str = "http://gamemedia.spiralknights.com/spiral/%VERSION%";

    #[test]
    fn expands_the_live_config_to_an_https_url() {
        let c = config(Some("20261110044614"), Some(LIVE_APPBASE));
        assert_eq!(
            expand_appbase(&c).unwrap(),
            "https://gamemedia.spiralknights.com/spiral/20261110044614"
        );
        assert_eq!(
            updater_jar_url(&c).unwrap(),
            "https://gamemedia.spiralknights.com/spiral/20261110044614/code/getdown-pro-new.jar"
        );
    }

    #[test]
    fn accepts_the_second_game_host_and_https_input() {
        let c = config(
            Some("1"),
            Some("https://gamemedia2.spiralknights.com/spiral/%VERSION%/"),
        );
        assert_eq!(
            expand_appbase(&c).unwrap(),
            "https://gamemedia2.spiralknights.com/spiral/1"
        );
    }

    #[test]
    fn rejects_untrustworthy_configs() {
        let bad = [
            config(
                Some("20261110044614"),
                Some("http://evil.example/spiral/%VERSION%"),
            ),
            config(
                Some("20261110044614"),
                Some("http://gamemedia.spiralknights.com@evil.example/spiral/%VERSION%"),
            ),
            config(
                Some("20261110044614"),
                Some("http://gamemedia.spiralknights.com:8080/spiral/%VERSION%"),
            ),
            config(
                Some("20261110044614"),
                Some("http://gamemedia.spiralknights.com.evil.example/spiral/%VERSION%"),
            ),
            config(
                Some("20261110044614"),
                Some("ftp://gamemedia.spiralknights.com/spiral/%VERSION%"),
            ),
            config(Some("20261110044614"), Some("file:///etc/passwd")),
            config(
                Some("20261110044614"),
                Some("http://gamemedia.spiralknights.com/spiral/../x/%VERSION%"),
            ),
            config(
                Some("20261110044614"),
                Some("http://gamemedia.spiralknights.com/spiral/%VERSION%?a=b"),
            ),
            config(Some("../../x"), Some(LIVE_APPBASE)),
            config(Some(""), Some(LIVE_APPBASE)),
            config(None, Some(LIVE_APPBASE)),
            config(Some("20261110044614"), None),
        ];
        for c in bad {
            assert_eq!(expand_appbase(&c), None, "should reject {c:?}");
            assert_eq!(updater_jar_url(&c), None);
        }
    }

    #[test]
    fn the_install_command_runs_the_new_updater_in_the_target_folder() {
        let spec = fresh_install_command(Path::new("/games/new"), Path::new("/jre/bin/java"));
        assert_eq!(spec.program, Path::new("/jre/bin/java"));
        assert_eq!(spec.cwd, Path::new("/games/new"));
        let args: Vec<_> = spec.args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            [
                "-Dsilent=true",
                "-jar",
                "getdown-pro-new.jar",
                ".",
                "client"
            ]
        );
    }

    #[test]
    fn latest_config_url_is_https_on_a_game_host() {
        assert!(LATEST_GETDOWN_TXT_URL.starts_with("https://"));
        assert!(GAME_HOSTS
            .iter()
            .any(|h| LATEST_GETDOWN_TXT_URL.contains(h)));
    }
}
