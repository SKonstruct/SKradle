//! Which game install the launcher uses: the ones `skradle_core::game::locate` finds, a folder the player
//! pointed to, and the player's saved choice among them. Plain Rust, no Qt.

use std::path::{Path, PathBuf};

use skradle_core::game::{self, GameInstall, InstallKind};
use skradle_core::getdown_txt::GetdownConfig;

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
    pub fn message(&self) -> &'static str {
        match self {
            FolderError::NotAFolder => "That is not a folder.",
            FolderError::NotAGame => "That folder does not hold Spiral Knights (getdown.txt and getdown-pro.jar are missing).",
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
/// path.
pub fn path_from_url_text(text: &str) -> PathBuf {
    let Some(rest) = text.strip_prefix("file://") else {
        return PathBuf::from(text);
    };
    let decoded = percent_decode(rest);
    // `file:///C:/x` carries a slash before the drive letter.
    let bytes = decoded.as_bytes();
    let path = if bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[2] == b':'
        && bytes[1].is_ascii_alphabetic()
    {
        &decoded[1..]
    } else {
        &decoded[..]
    };
    PathBuf::from(path)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(v);
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
            path_from_url_text("file:///C:/Games/SK"),
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
}
