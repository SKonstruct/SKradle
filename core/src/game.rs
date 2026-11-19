//! Finding an installed copy of the game: through Steam's library files, or in the places the
//! standalone installer uses. Nothing here modifies anything.
//!
//! Known limits: on Windows the Steam folder is taken from the default location (the registry is
//! not read yet), and the standalone install locations are from forum posts and are unverified.

use std::path::{Path, PathBuf};

use crate::vdf;

/// Steam's id for Spiral Knights.
pub const STEAM_APP_ID: u32 = 99900;
const GAME_DIR_NAME: &str = "Spiral Knights";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// The parts of the machine's environment discovery depends on, so tests can fake them.
#[derive(Debug, Clone)]
pub struct Env {
    pub os: Os,
    pub home: Option<PathBuf>,
    /// `%ProgramFiles(x86)%` on Windows.
    pub program_files_x86: Option<PathBuf>,
    /// `%LOCALAPPDATA%` on Windows.
    pub local_app_data: Option<PathBuf>,
    /// Steam's own install folder as Windows' registry records it (`HKCU\Software\Valve\Steam\SteamPath`), which is
    /// where Steam really is when it was installed to another drive.
    pub steam_path: Option<PathBuf>,
}

/// The folder in the output of `reg query HKCU\Software\Valve\Steam /v SteamPath`:
/// a line like `    SteamPath    REG_SZ    D:/Steam`.
pub fn parse_reg_steam_path(output: &str) -> Option<PathBuf> {
    output.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("SteamPath")?;
        let value = rest.trim_start().strip_prefix("REG_SZ")?.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

#[cfg(windows)]
fn steam_path_from_registry() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .ok()?;
    parse_reg_steam_path(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
fn steam_path_from_registry() -> Option<PathBuf> {
    None
}

impl Env {
    pub fn current() -> Env {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Env {
            os: Os::current(),
            home: var("HOME").or_else(|| var("USERPROFILE")),
            program_files_x86: var("ProgramFiles(x86)"),
            local_app_data: var("LOCALAPPDATA"),
            steam_path: steam_path_from_registry(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    Steam,
    Standalone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameInstall {
    pub root: PathBuf,
    pub kind: InstallKind,
}

/// A game folder holds the game's own updater and its settings.
pub fn is_game_dir(dir: &Path) -> bool {
    dir.join("getdown.txt").is_file() && dir.join("getdown-pro.jar").is_file()
}

/// All installs found, Steam first, without duplicates.
pub fn locate(env: &Env) -> Vec<GameInstall> {
    let mut found: Vec<GameInstall> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut add = |root: PathBuf, kind: InstallKind| {
        if !is_game_dir(&root) {
            return;
        }
        let key = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
        if !seen.contains(&key) {
            seen.push(key);
            found.push(GameInstall { root, kind });
        }
    };
    for steam_root in steam_roots(env) {
        for library in steam_libraries(&steam_root) {
            add(steam_game_dir(&library), InstallKind::Steam);
        }
    }
    for dir in standalone_candidates(env) {
        add(dir, InstallKind::Standalone);
    }
    found
}

/// Where Steam itself may be installed.
pub fn steam_roots(env: &Env) -> Vec<PathBuf> {
    let home = env.home.as_deref();
    match env.os {
        Os::MacOs => home
            .map(|h| vec![h.join("Library/Application Support/Steam")])
            .unwrap_or_default(),
        Os::Linux => home
            .map(|h| {
                vec![
                    h.join(".local/share/Steam"),
                    h.join(".steam/steam"),
                    h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
                    h.join("snap/steam/common/.local/share/Steam"),
                ]
            })
            .unwrap_or_default(),
        Os::Windows => {
            let base = env
                .program_files_x86
                .clone()
                .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
            // Where the registry says Steam is comes first (it may be on another drive); the usual place stays as a
            // fallback.
            let mut roots: Vec<PathBuf> = env.steam_path.iter().cloned().collect();
            let usual = base.join("Steam");
            if !roots.contains(&usual) {
                roots.push(usual);
            }
            roots
        }
    }
}

/// The Steam library folders under a Steam install: the install itself plus those listed in
/// `libraryfolders.vdf`.
pub fn steam_libraries(steam_root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![steam_root.to_path_buf()];
    for relative in ["steamapps/libraryfolders.vdf", "config/libraryfolders.vdf"] {
        let Ok(text) = std::fs::read_to_string(steam_root.join(relative)) else {
            continue;
        };
        let Ok(doc) = vdf::parse(&text) else { continue };
        let Some(folders) = doc.get("libraryfolders") else {
            continue;
        };
        for (_, entry) in folders.entries() {
            let path = entry
                .get("path")
                .and_then(vdf::Value::as_str)
                .or_else(|| entry.as_str());
            if let Some(path) = path.filter(|p| !p.is_empty()) {
                let path = PathBuf::from(path);
                if !libraries.contains(&path) {
                    libraries.push(path);
                }
            }
        }
        break;
    }
    libraries
}

/// The game's folder inside one Steam library, taken from its app manifest when there is one.
pub fn steam_game_dir(library: &Path) -> PathBuf {
    let steamapps = library.join("steamapps");
    let manifest = steamapps.join(format!("appmanifest_{STEAM_APP_ID}.acf"));
    let install_dir = std::fs::read_to_string(manifest)
        .ok()
        .and_then(|text| vdf::parse(&text).ok())
        .and_then(|doc| {
            doc.get("AppState")?
                .get("installdir")?
                .as_str()
                .filter(|name| is_plain_folder_name(name))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| GAME_DIR_NAME.to_owned());
    steamapps.join("common").join(install_dir)
}

/// A folder name from a manifest must not be able to point outside `steamapps/common`. A `:` is refused too: on
/// Windows `D:` or `C:foo` joins as a drive-relative path and leaves the folder.
fn is_plain_folder_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0', ':'])
}

/// Places the standalone installer may have used. Unverified; a folder only counts if it holds a game.
pub fn standalone_candidates(env: &Env) -> Vec<PathBuf> {
    match env.os {
        Os::Windows => env
            .local_app_data
            .as_deref()
            .map(|base| {
                vec![
                    base.join("Spiral Knights"),
                    base.join("Spiral Knights").join("app"),
                ]
            })
            .unwrap_or_default(),
        Os::MacOs => env
            .home
            .as_deref()
            .map(|h| {
                vec![
                    h.join("Library/Application Support/Spiral Knights"),
                    h.join("Library/Application Support/spiral"),
                ]
            })
            .unwrap_or_default(),
        Os::Linux => env
            .home
            .as_deref()
            .map(|h| {
                vec![
                    h.join("Spiral Knights"),
                    h.join("spiral"),
                    h.join(".local/share/Spiral Knights"),
                ]
            })
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steam_folder_is_read_from_registry_output_and_comes_first() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\r\n    SteamPath    REG_SZ    D:/Games/Steam\r\n\r\n";
        assert_eq!(
            parse_reg_steam_path(out),
            Some(PathBuf::from("D:/Games/Steam"))
        );
        assert_eq!(
            parse_reg_steam_path("ERROR: The system was unable to find the specified registry key"),
            None
        );
        let env = Env {
            os: Os::Windows,
            home: None,
            program_files_x86: Some(PathBuf::from(r"C:\PF86")),
            local_app_data: None,
            steam_path: Some(PathBuf::from("D:/Games/Steam")),
        };
        let roots = steam_roots(&env);
        assert_eq!(roots[0], PathBuf::from("D:/Games/Steam"));
        assert!(roots.contains(&PathBuf::from(r"C:\PF86").join("Steam")));
    }

    #[test]
    fn a_manifest_folder_name_with_a_drive_colon_is_refused() {
        assert!(is_plain_folder_name("Spiral Knights"));
        assert!(!is_plain_folder_name("D:"));
        assert!(!is_plain_folder_name("C:evil"));
    }
    use std::fs;

    fn make_game(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("getdown.txt"), "version = 1\n").unwrap();
        fs::write(dir.join("getdown-pro.jar"), b"jar").unwrap();
    }

    /// A path as it is written inside a quoted Steam text value (backslashes doubled).
    fn vdf_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "\\\\")
    }

    fn mac_env(home: &Path) -> Env {
        Env {
            os: Os::MacOs,
            home: Some(home.to_path_buf()),
            program_files_x86: None,
            steam_path: None,
            local_app_data: None,
        }
    }

    #[test]
    fn finds_a_steam_install_in_the_default_library() {
        let home = tempfile::tempdir().unwrap();
        let steam = home.path().join("Library/Application Support/Steam");
        make_game(&steam.join("steamapps/common/Spiral Knights"));
        let found = locate(&mac_env(home.path()));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, InstallKind::Steam);
        assert_eq!(found[0].root, steam.join("steamapps/common/Spiral Knights"));
    }

    #[test]
    fn follows_libraryfolders_and_the_app_manifest() {
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let steam = home.path().join("Library/Application Support/Steam");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let vdf = format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
            vdf_path(&steam),
            vdf_path(other.path())
        );
        fs::write(steam.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        fs::create_dir_all(other.path().join("steamapps")).unwrap();
        fs::write(
            other.path().join("steamapps/appmanifest_99900.acf"),
            "\"AppState\" { \"appid\" \"99900\" \"installdir\" \"SK Custom Folder\" }",
        )
        .unwrap();
        make_game(&other.path().join("steamapps/common/SK Custom Folder"));
        let found = locate(&mac_env(home.path()));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].root,
            other.path().join("steamapps/common/SK Custom Folder")
        );
    }

    #[test]
    fn a_manifest_cannot_point_outside_the_library() {
        let library = tempfile::tempdir().unwrap();
        fs::create_dir_all(library.path().join("steamapps")).unwrap();
        for evil in ["..", "../../etc", "a/b", r"a\b", ""] {
            fs::write(
                library.path().join("steamapps/appmanifest_99900.acf"),
                format!("\"AppState\" {{ \"installdir\" \"{evil}\" }}"),
            )
            .unwrap();
            assert_eq!(
                steam_game_dir(library.path()),
                library.path().join("steamapps/common/Spiral Knights"),
                "installdir {evil:?} must fall back to the default folder"
            );
        }
    }

    #[test]
    fn finds_a_standalone_install_and_skips_folders_without_a_game() {
        let home = tempfile::tempdir().unwrap();
        make_game(
            &home
                .path()
                .join("Library/Application Support/Spiral Knights"),
        );
        fs::create_dir_all(home.path().join("Library/Application Support/spiral")).unwrap();
        let found = locate(&mac_env(home.path()));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, InstallKind::Standalone);
    }

    #[test]
    fn lists_each_install_once() {
        let home = tempfile::tempdir().unwrap();
        let game = home
            .path()
            .join("Library/Application Support/Steam/steamapps/common/Spiral Knights");
        make_game(&game);
        // Reachable both as a Steam library and, through a symlink-free duplicate, as a library entry.
        let steam = home.path().join("Library/Application Support/Steam");
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
                vdf_path(&steam)
            ),
        )
        .unwrap();
        assert_eq!(locate(&mac_env(home.path())).len(), 1);
    }

    #[test]
    fn nothing_found_without_a_home() {
        let env = Env {
            os: Os::Linux,
            home: None,
            program_files_x86: None,
            steam_path: None,
            local_app_data: None,
        };
        assert!(locate(&env).is_empty());
    }

    #[test]
    fn windows_uses_program_files_and_local_app_data() {
        let env = Env {
            os: Os::Windows,
            home: None,
            program_files_x86: Some(PathBuf::from(r"D:\PF86")),
            steam_path: None,
            local_app_data: Some(PathBuf::from(r"C:\Users\me\AppData\Local")),
        };
        assert_eq!(
            steam_roots(&env),
            vec![PathBuf::from(r"D:\PF86").join("Steam")]
        );
        assert_eq!(standalone_candidates(&env).len(), 2);
    }

    /// Run with `cargo test -p skradle-core -- --ignored --nocapture` on a machine with the game.
    #[test]
    #[ignore = "needs a real game install"]
    fn locates_the_game_on_this_machine() {
        let found = locate(&Env::current());
        println!("found installs: {found:#?}");
        assert!(!found.is_empty());
    }
}
