//! Which game install the launcher uses: the ones `skradle_core::game::locate` finds, a folder the player
//! pointed to, and the player's saved choice among them. Plain Rust, no Qt.

use std::path::{Path, PathBuf};

use skradle_core::game::{self, GameInstall, InstallKind};
use skradle_core::getdown_txt::GetdownConfig;

use crate::i18n::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Steam,
    Standalone,
    /// A folder the player chose.
    Custom,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Steam => "steam",
            Kind::Standalone => "standalone",
            Kind::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    pub path: PathBuf,
    pub kind: Kind,
}

impl Install {
    /// The `GameInstall` the core's updater driver takes.
    pub fn to_core(&self) -> GameInstall {
        GameInstall {
            root: self.path.clone(),
            kind: match self.kind {
                Kind::Steam => InstallKind::Steam,
                Kind::Standalone | Kind::Custom => InstallKind::Standalone,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resolved {
    pub installs: Vec<Install>,
    /// Index into `installs`.
    pub selected: Option<usize>,
}

impl Resolved {
    pub fn selected_install(&self) -> Option<&Install> {
        self.selected.and_then(|i| self.installs.get(i))
    }
}

/// True when `path` sits in a Steam library: `...\steamapps\common\<game>` (any case, either slash), the way a Steam
/// install on another drive or under a path with spaces looks, or next to Steam's own manifest for the game.
pub fn looks_like_steam_library(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let parts: Vec<&str> = text.split('/').filter(|p| !p.is_empty()).collect();
    if parts
        .windows(2)
        .any(|w| w[0] == "steamapps" && w[1] == "common")
    {
        return true;
    }
    path.parent()
        .and_then(Path::parent)
        .is_some_and(|steamapps| {
            steamapps
                .join(format!("appmanifest_{}.acf", game::STEAM_APP_ID))
                .is_file()
        })
}

/// What kind of install the folder `path` is: the kind discovery gave it when it found it, otherwise Steam when it
/// lies in a Steam library, otherwise `Custom`. A launch must know this: only a Steam install signs in through Steam.
pub fn detect_kind(path: &Path, found: &[GameInstall]) -> Kind {
    if let Some(known) = found.iter().find(|g| same_folder(&g.root, path)) {
        return match known.kind {
            InstallKind::Steam => Kind::Steam,
            InstallKind::Standalone => Kind::Standalone,
        };
    }
    if looks_like_steam_library(path) {
        Kind::Steam
    } else {
        Kind::Custom
    }
}

/// Combines the discovered installs with the custom folder and the saved choice. A custom folder only counts while
/// it still holds a game. The saved choice is kept if it is still in the list; otherwise the first install is used.
pub fn resolve(found: Vec<GameInstall>, custom: Option<&Path>, saved: Option<&Path>) -> Resolved {
    let mut installs: Vec<Install> = found
        .into_iter()
        .map(|g| Install {
            path: g.root,
            kind: match g.kind {
                InstallKind::Steam => Kind::Steam,
                InstallKind::Standalone => Kind::Standalone,
            },
        })
        .collect();
    if let Some(custom) = custom.filter(|c| game::is_game_dir(c)) {
        if !installs.iter().any(|i| same_folder(&i.path, custom)) {
            installs.push(Install {
                path: custom.to_path_buf(),
                kind: Kind::Custom,
            });
        }
    }
    let selected = saved
        .and_then(|s| installs.iter().position(|i| same_folder(&i.path, s)))
        .or(if installs.is_empty() { None } else { Some(0) });
    Resolved { installs, selected }
}

fn same_folder(a: &Path, b: &Path) -> bool {
    a == b
        || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderError {
    NotAFolder,
    NotAGame,
}

impl FolderError {
    pub fn message(&self) -> String {
        match self {
            FolderError::NotAFolder => tr("That is not a folder."),
            FolderError::NotAGame => tr("That folder does not hold Spiral Knights (getdown.txt and getdown-pro.jar are missing)."),
        }
    }
}

/// Checks a folder the player chose. The folder itself, or a `Spiral Knights` folder inside it (the player picked
/// the parent), is accepted. Returns the game folder.
pub fn validate_folder(chosen: &Path) -> Result<PathBuf, FolderError> {
    if !chosen.is_dir() {
        return Err(FolderError::NotAFolder);
    }
    if game::is_game_dir(chosen) {
        return Ok(chosen.to_path_buf());
    }
    let inner = chosen.join("Spiral Knights");
    if game::is_game_dir(&inner) {
        return Ok(inner);
    }
    Err(FolderError::NotAGame)
}

/// The installed game version from the folder's `getdown.txt`.
pub fn game_version(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("getdown.txt")).ok()?;
    GetdownConfig::parse(&text).version
}

/// Turns the file URL a folder dialog gives (`file:///C:/Games/SK`, `file:///home/me/SK`) or a plain path into a
/// path. On Windows a drive path gets backslashes: the game's updater reports its folder that way, which is how the
/// running game is found again.
pub fn path_from_url_text(text: &str) -> PathBuf {
    path_from_url_text_for(text, cfg!(windows))
}

/// The local path a URL from a file dialog names: a `file://` URL or a plain path. Any other scheme (`https://`,
/// `content://`) names nothing on this machine, so the answer is empty.
pub fn local_path_from_url_text(text: &str) -> PathBuf {
    if text.contains("://") && !text.starts_with("file://") {
        return PathBuf::new();
    }
    path_from_url_text(text)
}

fn path_from_url_text_for(text: &str, windows: bool) -> PathBuf {
    let Some(rest) = text.strip_prefix("file://") else {
        return PathBuf::from(text);
    };
    // `file://localhost/x` is the same as `file:///x`.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest);
    // `file:///C:/x` carries a slash before the drive letter.
    let bytes = decoded.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic();
    if drive {
        let path = &decoded[1..];
        return if windows {
            PathBuf::from(path.replace('/', "\\"))
        } else {
            PathBuf::from(path)
        };
    }
    PathBuf::from(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Decodes `%XX` escapes byte by byte, never slicing the text, so a multi-byte character cannot be split. Anything
/// that is not a valid escape stays as it is; bytes that are not UTF-8 after decoding are replaced.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push(high << 4 | low);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_file_url_or_a_plain_path_names_a_local_file() {
        assert_eq!(
            local_path_from_url_text("file:///usr/bin/java"),
            Path::new("/usr/bin/java")
        );
        assert_eq!(
            local_path_from_url_text("/usr/bin/java"),
            Path::new("/usr/bin/java")
        );
        assert!(local_path_from_url_text("https://example.invalid/java")
            .as_os_str()
            .is_empty());
        assert!(local_path_from_url_text("content://media/x")
            .as_os_str()
            .is_empty());
    }
    use std::fs;

    fn make_game(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("getdown.txt"), "version = 20260101000000\n").unwrap();
        fs::write(dir.join("getdown-pro.jar"), b"jar").unwrap();
    }

    fn found(path: &Path, kind: InstallKind) -> GameInstall {
        GameInstall {
            root: path.to_path_buf(),
            kind,
        }
    }

    #[test]
    fn nothing_found_means_not_found() {
        let r = resolve(vec![], None, None);
        assert!(r.installs.is_empty() && r.selected.is_none());
        assert!(r.selected_install().is_none());
    }

    #[test]
    fn first_install_is_selected_by_default() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let r = resolve(
            vec![
                found(a.path(), InstallKind::Steam),
                found(b.path(), InstallKind::Standalone),
            ],
            None,
            None,
        );
        assert_eq!(r.selected, Some(0));
        assert_eq!(r.installs[1].kind, Kind::Standalone);
    }

    #[test]
    fn saved_choice_wins_when_still_present() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let r = resolve(
            vec![
                found(a.path(), InstallKind::Steam),
                found(b.path(), InstallKind::Standalone),
            ],
            None,
            Some(b.path()),
        );
        assert_eq!(r.selected, Some(1));
        let gone = resolve(
            vec![found(a.path(), InstallKind::Steam)],
            None,
            Some(b.path()),
        );
        assert_eq!(gone.selected, Some(0));
    }

    #[test]
    fn custom_folder_is_added_only_while_it_holds_a_game() {
        let dir = tempfile::tempdir().unwrap();
        make_game(dir.path());
        let r = resolve(vec![], Some(dir.path()), Some(dir.path()));
        assert_eq!(r.installs.len(), 1);
        assert_eq!(r.installs[0].kind, Kind::Custom);
        assert_eq!(r.selected, Some(0));
        fs::remove_file(dir.path().join("getdown.txt")).unwrap();
        assert!(resolve(vec![], Some(dir.path()), None).installs.is_empty());
    }

    #[test]
    fn custom_folder_that_was_also_discovered_is_not_listed_twice() {
        let dir = tempfile::tempdir().unwrap();
        make_game(dir.path());
        let r = resolve(
            vec![found(dir.path(), InstallKind::Steam)],
            Some(dir.path()),
            None,
        );
        assert_eq!(r.installs.len(), 1);
        assert_eq!(r.installs[0].kind, Kind::Steam);
    }

    #[test]
    fn validate_accepts_the_folder_or_a_game_folder_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(validate_folder(dir.path()), Err(FolderError::NotAGame));
        assert_eq!(
            validate_folder(&dir.path().join("missing")),
            Err(FolderError::NotAFolder)
        );
        make_game(&dir.path().join("Spiral Knights"));
        assert_eq!(
            validate_folder(dir.path()).unwrap(),
            dir.path().join("Spiral Knights")
        );
        make_game(dir.path());
        assert_eq!(validate_folder(dir.path()).unwrap(), dir.path());
    }

    #[test]
    fn reads_the_game_version() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(game_version(dir.path()), None);
        make_game(dir.path());
        assert_eq!(game_version(dir.path()).as_deref(), Some("20260101000000"));
    }

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            path_from_url_text("file:///home/me/Spiral%20Knights"),
            PathBuf::from("/home/me/Spiral Knights")
        );
        assert_eq!(
            path_from_url_text_for("file:///C:/Games/SK", false),
            PathBuf::from("C:/Games/SK")
        );
        assert_eq!(
            path_from_url_text("/plain/path"),
            PathBuf::from("/plain/path")
        );
        assert_eq!(path_from_url_text("file:///100%25"), PathBuf::from("/100%"));
        assert_eq!(
            path_from_url_text("file:///bad%zz"),
            PathBuf::from("/bad%zz")
        );
    }

    #[test]
    fn a_percent_before_a_multibyte_character_does_not_panic() {
        // The reviewer's case: `%`, one ASCII byte, then a two-byte character.
        assert_eq!(
            path_from_url_text("file:///Users/me/100%a\u{e9}"),
            PathBuf::from("/Users/me/100%a\u{e9}")
        );
        assert_eq!(percent_decode("%\u{e9}"), "%\u{e9}");
        assert_eq!(percent_decode("%e\u{e9}"), "%e\u{e9}");
        assert_eq!(percent_decode("%%41"), "%A");
        assert_eq!(percent_decode("caf%C3%A9"), "caf\u{e9}");
    }

    /// Fuzz-style: random mixes of `%`, hex digits, ASCII and multi-byte characters never panic, and text without a
    /// `%` comes back unchanged.
    #[test]
    fn percent_decode_survives_random_text() {
        let alphabet = [
            "%",
            "%",
            "%",
            "0",
            "a",
            "F",
            "g",
            "2",
            "5",
            " ",
            "/",
            "\u{e9}",
            "\u{20ac}",
            "\u{1f600}",
            "\u{0}",
            "z",
        ];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 12) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
                .collect();
            let decoded = percent_decode(&text);
            let _ = path_from_url_text(&format!("file://{text}"));
            if !text.contains('%') {
                assert_eq!(decoded, text);
            }
        }
    }

    #[test]
    fn windows_drive_paths_use_backslashes() {
        assert_eq!(
            path_from_url_text_for("file:///C:/Games/Spiral%20Knights", true),
            PathBuf::from("C:\\Games\\Spiral Knights")
        );
        assert_eq!(
            path_from_url_text_for("file:///C:/Games/SK", false),
            PathBuf::from("C:/Games/SK")
        );
        assert_eq!(
            path_from_url_text_for("file://localhost/home/me", true),
            PathBuf::from("/home/me")
        );
    }

    #[test]
    fn a_steam_library_path_is_recognised_on_any_drive() {
        for p in [
            r"D:\SteamLibrary\steamapps\common\Spiral Knights",
            r"C:\Program Files (x86)\Steam\SteamApps\Common\Spiral Knights",
            "/home/ann/.local/share/Steam/steamapps/common/Spiral Knights",
        ] {
            assert!(looks_like_steam_library(Path::new(p)), "{p}");
        }
        assert!(!looks_like_steam_library(Path::new(
            r"C:\Games\Spiral Knights"
        )));
        assert!(!looks_like_steam_library(Path::new(
            "/home/ann/steamapps/other/SK"
        )));
    }

    #[test]
    fn the_install_kind_comes_from_discovery_then_from_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let found = vec![GameInstall {
            root: dir.path().to_path_buf(),
            kind: InstallKind::Steam,
        }];
        assert_eq!(detect_kind(dir.path(), &found), Kind::Steam);
        let other = tempfile::tempdir().unwrap();
        assert_eq!(detect_kind(other.path(), &found), Kind::Custom);
        assert_eq!(
            detect_kind(Path::new(r"E:\Lib\steamapps\common\Spiral Knights"), &[]),
            Kind::Steam
        );
        // Next to Steam's manifest for the game, wherever the library is called.
        let lib = tempfile::tempdir().unwrap();
        let game = lib.path().join("steamapps/odd/SK");
        std::fs::create_dir_all(&game).unwrap();
        assert_eq!(detect_kind(&game, &[]), Kind::Custom);
        std::fs::write(lib.path().join("steamapps/appmanifest_99900.acf"), "x").unwrap();
        assert_eq!(detect_kind(&game, &[]), Kind::Steam);
    }
}
