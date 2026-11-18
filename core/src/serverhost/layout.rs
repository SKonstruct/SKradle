//! The server folder: where the engine, its configuration, its runtime data and the launcher's own records live,
//! and the path checks that keep names from archives and from the player inside it.
//!
//! ```text
//! <root>/
//!   tortodrone.jar            the engine (its `data/` is found beside the jar, not in the working directory)
//!   tortodrone.jar.previous   the engine before the last update, for rollback
//!   content/  content.previous/   the content pack (maps, items), replaced as a whole on update
//!   webui/  webui.previous/       the WebUI pages the engine serves under /admin/
//!   config/server.yml         the engine's settings; config/combat.yml
//!   plugins/                  empty; the engine creates and reads it
//!   data/                     the engine's runtime state: tortodrone.db, keys/, logs/ (the engine is its only writer)
//!   backups/<name>/{data,config}
//!   LICENSE NOTICE THIRD-PARTY.md licenses/   the engine's licence texts, kept next to the jar
//!   engine.json               the launcher's record of the installed engine (version, hashes)
//!   terms-accepted.json       the player's recorded acceptance of the engine's licence
//! ```

use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Why a name or path was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    TooLong,
    /// A NUL or control character, a backslash or a colon.
    BadCharacter,
    Absolute,
    /// `.`, `..` or an empty component.
    Traversal,
    /// A component Windows would silently alter (trailing dot or space).
    WindowsUnsafe,
    /// An existing path component is a symbolic link.
    Symlink(PathBuf),
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathError::Empty => write!(f, "empty path"),
            PathError::TooLong => write!(f, "path too long"),
            PathError::BadCharacter => {
                write!(f, "path contains a control character, backslash or colon")
            }
            PathError::Absolute => write!(f, "absolute path"),
            PathError::Traversal => write!(f, "path leaves its folder"),
            PathError::WindowsUnsafe => write!(f, "path component ends with a dot or a space"),
            PathError::Symlink(p) => write!(f, "{} is a symbolic link", p.display()),
        }
    }
}

impl std::error::Error for PathError {}

const MAX_PATH_BYTES: usize = 512;
const MAX_COMPONENT_BYTES: usize = 255;

/// Checks a `/`-separated relative name (a zip entry, say) and returns it as a relative path. Refused: empty or
/// overlong names, control characters, backslashes (zip tools disagree on them), colons (drive letters and
/// alternate streams), absolute names, `.`/`..`/empty components and components Windows would rewrite. A single
/// trailing `/` (a directory entry) is accepted.
pub fn safe_relative(name: &str) -> Result<PathBuf, PathError> {
    if name.is_empty() {
        return Err(PathError::Empty);
    }
    if name.len() > MAX_PATH_BYTES {
        return Err(PathError::TooLong);
    }
    if name
        .chars()
        .any(|c| c.is_control() || c == '\\' || c == ':')
    {
        return Err(PathError::BadCharacter);
    }
    if name.starts_with('/') {
        return Err(PathError::Absolute);
    }
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    if trimmed.is_empty() {
        return Err(PathError::Empty);
    }
    let mut out = PathBuf::new();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(PathError::Traversal);
        }
        if part.len() > MAX_COMPONENT_BYTES {
            return Err(PathError::TooLong);
        }
        if part.ends_with('.') || part.ends_with(' ') {
            return Err(PathError::WindowsUnsafe);
        }
        out.push(part);
    }
    // PathBuf::push would honour a prefix on Windows; the checks above make that impossible, but be sure.
    if out.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(PathError::Traversal);
    }
    Ok(out)
}

/// A backup name as the backups module creates it: letters, digits, `.`, `_` and `-`, not starting with a dot.
pub fn is_valid_backup_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The paths of one server folder. Only computes paths; nothing is created except by [`ServerLayout::ensure`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerLayout {
    root: PathBuf,
}

impl ServerLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn jar(&self) -> PathBuf {
        self.root.join("tortodrone.jar")
    }

    pub fn previous_jar(&self) -> PathBuf {
        self.root.join("tortodrone.jar.previous")
    }

    pub fn content_dir(&self) -> PathBuf {
        self.root.join("content")
    }

    pub fn previous_content(&self) -> PathBuf {
        self.root.join("content.previous")
    }

    /// The WebUI pages the engine serves under `/admin/` from `<root>/webui`.
    pub fn webui_dir(&self) -> PathBuf {
        self.root.join("webui")
    }

    pub fn previous_webui(&self) -> PathBuf {
        self.root.join("webui.previous")
    }

    pub fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }

    /// The file passed to the engine with `--config`.
    pub fn server_yml(&self) -> PathBuf {
        self.root.join("config").join("server.yml")
    }

    pub fn combat_yml(&self) -> PathBuf {
        self.root.join("config").join("combat.yml")
    }

    pub fn plugins_dir(&self) -> PathBuf {
        self.root.join("plugins")
    }

    pub fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }

    pub fn database(&self) -> PathBuf {
        self.root.join("data").join("tortodrone.db")
    }

    pub fn keys_dir(&self) -> PathBuf {
        self.root.join("data").join("keys")
    }

    /// The public key the engine writes when it generates its key pair: `modulus#exponent` in hex.
    pub fn public_key(&self) -> PathBuf {
        self.keys_dir().join("public.txt")
    }

    pub fn engine_log(&self) -> PathBuf {
        self.root.join("data").join("logs").join("latest.log")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    /// The launcher's record of the installed engine.
    pub fn engine_record(&self) -> PathBuf {
        self.root.join("engine.json")
    }

    /// The recorded licence acceptance.
    pub fn terms_record(&self) -> PathBuf {
        self.root.join("terms-accepted.json")
    }

    pub(crate) fn download_path(&self) -> PathBuf {
        self.root.join(".engine-download.zip")
    }

    pub(crate) fn staging_dir(&self) -> PathBuf {
        self.root.join(".engine-staging")
    }

    /// Joins a checked relative name onto the root and refuses it when any existing component on the way is a
    /// symbolic link (which could point the write outside the folder).
    pub fn resolve(&self, name: &str) -> Result<PathBuf, PathError> {
        let relative = safe_relative(name)?;
        let mut current = self.root.clone();
        for part in relative.components() {
            current.push(part);
            if std::fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(PathError::Symlink(current));
            }
        }
        Ok(current)
    }

    /// The path of a named backup, if `name` is a valid backup name.
    pub fn backup_dir(&self, name: &str) -> Option<PathBuf> {
        is_valid_backup_name(name).then(|| self.backups_dir().join(name))
    }

    /// Creates the root and the folders the launcher owns (`config/`, `plugins/`, `backups/`). `data/` is the
    /// engine's and is left for it to create.
    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [
            self.root.clone(),
            self.config_dir(),
            self.plugins_dir(),
            self.backups_dir(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_relative_names() {
        assert_eq!(
            safe_relative("content/maps/haven/map.yml").unwrap(),
            Path::new("content/maps/haven/map.yml")
        );
        assert_eq!(safe_relative("content/").unwrap(), Path::new("content"));
        assert_eq!(safe_relative("a b/c.txt").unwrap(), Path::new("a b/c.txt"));
    }

    #[test]
    fn refuses_hostile_names() {
        for (name, why) in [
            ("", PathError::Empty),
            ("/", PathError::Absolute),
            ("/etc/passwd", PathError::Absolute),
            ("../x", PathError::Traversal),
            ("a/../../x", PathError::Traversal),
            ("a//b", PathError::Traversal),
            ("./a", PathError::Traversal),
            ("a\\..\\b", PathError::BadCharacter),
            ("C:/x", PathError::BadCharacter),
            ("a:stream", PathError::BadCharacter),
            ("a\0b", PathError::BadCharacter),
            ("a\nb", PathError::BadCharacter),
            ("dir./x", PathError::WindowsUnsafe),
            ("x ", PathError::WindowsUnsafe),
        ] {
            assert_eq!(safe_relative(name).unwrap_err(), why, "{name:?}");
        }
        assert_eq!(
            safe_relative(&"a/".repeat(300)).unwrap_err(),
            PathError::TooLong
        );
        assert_eq!(
            safe_relative(&"x".repeat(300)).unwrap_err(),
            PathError::TooLong
        );
    }

    #[test]
    fn paths_are_where_the_engine_expects_them() {
        let layout = ServerLayout::new("/srv/sk");
        assert_eq!(layout.jar(), Path::new("/srv/sk/tortodrone.jar"));
        assert_eq!(layout.server_yml(), Path::new("/srv/sk/config/server.yml"));
        assert_eq!(layout.database(), Path::new("/srv/sk/data/tortodrone.db"));
        assert_eq!(
            layout.public_key(),
            Path::new("/srv/sk/data/keys/public.txt")
        );
        assert_eq!(
            layout.engine_log(),
            Path::new("/srv/sk/data/logs/latest.log")
        );
    }

    #[test]
    fn backup_names_are_restricted() {
        assert!(is_valid_backup_name("20260101_120000-reset"));
        assert!(is_valid_backup_name("x_2"));
        for bad in ["", ".hidden", "a/b", "..", "a b", "a\\b", &"x".repeat(101)] {
            assert!(!is_valid_backup_name(bad), "{bad:?}");
        }
        let layout = ServerLayout::new("/srv/sk");
        assert_eq!(
            layout.backup_dir("b1"),
            Some(PathBuf::from("/srv/sk/backups/b1"))
        );
        assert_eq!(layout.backup_dir("../x"), None);
    }

    #[test]
    fn ensure_creates_the_launcher_owned_folders_only() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path().join("server"));
        layout.ensure().unwrap();
        assert!(layout.config_dir().is_dir());
        assert!(layout.plugins_dir().is_dir());
        assert!(layout.backups_dir().is_dir());
        assert!(!layout.data_dir().exists());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_refuses_a_symlinked_component() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        std::os::unix::fs::symlink(outside.path(), dir.path().join("content")).unwrap();
        assert!(matches!(
            layout.resolve("content/x.yml"),
            Err(PathError::Symlink(_))
        ));
        assert_eq!(
            layout.resolve("config/server.yml").unwrap(),
            dir.path().join("config/server.yml")
        );
        assert_eq!(layout.resolve("../x").unwrap_err(), PathError::Traversal);
    }
}
