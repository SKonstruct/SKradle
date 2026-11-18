//! Java runtimes: reading a runtime's `release` file, checking it against what the game and the graphics module
//! need, and finding runtimes on this machine. Nothing here starts a Java process or modifies anything.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::game::Os;

/// The game's updater refuses a Java older than `java_version` in `getdown.txt`, written as `25000000`.
const AGENT_MIN_MAJOR: u32 = 24;
const AGENT_MODULE: &str = "java.instrument";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct JavaVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl JavaVersion {
    /// Parses `25`, `25.0.3`, `21.0.5+11` and the old `1.8.0_452` style (read as major 8, minor 0, patch 452).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let core = text.split(['+', '-']).next()?;
        if let Some(rest) = core.strip_prefix("1.") {
            let (middle, update) = rest.split_once('_').unwrap_or((rest, "0"));
            let mut parts = middle.split('.');
            let major = parts.next()?.parse().ok()?;
            let minor = parts.next().unwrap_or("0").parse().ok()?;
            return Some(Self {
                major,
                minor,
                patch: update.parse().ok()?,
            });
        }
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// The updater's number for this version: `25.0.3` is `25000300`.
    pub fn getdown_encoded(&self) -> u64 {
        u64::from(self.major) * 1_000_000
            + u64::from(self.minor) * 10_000
            + u64::from(self.patch) * 100
    }
}

/// What a runtime's `release` file says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRelease {
    pub version: JavaVersion,
    pub os_arch: Option<String>,
    /// The module list of a trimmed (jlink) runtime; `None` when the file has no `MODULES` line.
    pub modules: Option<BTreeSet<String>>,
}

impl JavaRelease {
    /// Parses the `KEY="value"` lines of a `release` file. Needs a readable `JAVA_VERSION`.
    pub fn parse(text: &str) -> Option<Self> {
        let value = |key: &str| {
            text.lines().find_map(|line| {
                let (k, v) = line.split_once('=')?;
                (k.trim() == key).then(|| v.trim().trim_matches('"').to_owned())
            })
        };
        Some(Self {
            version: JavaVersion::parse(&value("JAVA_VERSION")?)?,
            os_arch: value("OS_ARCH").filter(|s| !s.is_empty()),
            modules: value("MODULES").map(|m| m.split_whitespace().map(str::to_owned).collect()),
        })
    }

    pub fn read(java_home: &Path) -> Option<Self> {
        Self::parse(&std::fs::read_to_string(java_home.join("release")).ok()?)
    }

    /// True if it satisfies the game's `java_version` (Getdown's encoding, for example `25000000`).
    pub fn meets_game_minimum(&self, minimum: u64) -> bool {
        self.version.getdown_encoded() >= minimum
    }

    /// The graphics agent needs Java 24 or newer (`java.lang.classfile`) and the `java.instrument` module, which the
    /// game's own trimmed runtime does not have. A release file without a module list is taken to be a full runtime.
    pub fn supports_graphics_agent(&self) -> bool {
        self.version.major >= AGENT_MIN_MAJOR
            && self
                .modules
                .as_ref()
                .is_none_or(|m| m.contains(AGENT_MODULE))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The runtime the game ships in `java_vm`.
    GameBundled,
    /// One SKradle downloaded.
    Managed,
    /// `JAVA_HOME`.
    JavaHome,
    /// A `java` found on `PATH`.
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    pub home: PathBuf,
    pub release: JavaRelease,
    pub source: Source,
}

impl Runtime {
    /// The executable that starts a game or the updater with this runtime.
    pub fn java_executable(&self, os: Os) -> PathBuf {
        let name = if os == Os::Windows {
            "java.exe"
        } else {
            "java"
        };
        self.home.join("bin").join(name)
    }
}

/// The machine's Java-related environment, so tests can fake it.
#[derive(Debug, Clone, Default)]
pub struct RuntimeEnv {
    pub java_home: Option<PathBuf>,
    /// The directories of `PATH`.
    pub path: Vec<PathBuf>,
}

impl RuntimeEnv {
    pub fn current() -> Self {
        Self {
            java_home: std::env::var_os("JAVA_HOME").map(PathBuf::from),
            path: std::env::var_os("PATH")
                .map(|p: OsString| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
        }
    }
}

/// Every runtime that can be found, best source first: the game's own, then SKradle's downloaded ones (one folder
/// each under `managed_dir`, newest version first), `JAVA_HOME`, then `PATH`. A folder only counts if it has a
/// readable `release` file, which also drops stubs such as macOS's `/usr/bin/java`.
pub fn find_runtimes(
    env: &RuntimeEnv,
    game_root: Option<&Path>,
    managed_dir: Option<&Path>,
) -> Vec<Runtime> {
    let mut found: Vec<Runtime> = Vec::new();
    let mut add = |home: PathBuf, source: Source| {
        // The same runtime reached through a link counts once.
        let key = std::fs::canonicalize(&home).unwrap_or_else(|_| home.clone());
        if found
            .iter()
            .any(|r| r.home == home || std::fs::canonicalize(&r.home).is_ok_and(|c| c == key))
        {
            return;
        }
        if let Some(release) = JavaRelease::read(&home) {
            found.push(Runtime {
                home,
                release,
                source,
            });
        }
    };
    if let Some(root) = game_root {
        add(root.join("java_vm"), Source::GameBundled);
    }
    if let Some(dir) = managed_dir {
        let mut managed: Vec<(JavaVersion, PathBuf)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let home = entry.path();
                Some((JavaRelease::read(&home)?.version, home))
            })
            .collect();
        managed.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        for (_, home) in managed {
            add(home, Source::Managed);
        }
    }
    if let Some(home) = &env.java_home {
        add(home.clone(), Source::JavaHome);
    }
    for dir in &env.path {
        let Some(java) = ["java", "java.exe"]
            .iter()
            .map(|name| dir.join(name))
            .find(|p| p.is_file())
        else {
            continue;
        };
        // `<home>/bin/java` as found on PATH, and where a link such as /usr/bin/java really leads.
        if let Some(home) = dir.parent() {
            add(home.to_path_buf(), Source::Path);
        }
        if let Some(home) = std::fs::canonicalize(&java)
            .ok()
            .and_then(|real| real.parent()?.parent().map(Path::to_path_buf))
        {
            add(home, Source::Path);
        }
    }
    found
}

/// The runtime to start the game with: the first that satisfies the game's minimum, in the order of `find_runtimes`.
pub fn pick_for_game(runtimes: &[Runtime], game_minimum: u64) -> Option<&Runtime> {
    runtimes
        .iter()
        .find(|r| r.release.meets_game_minimum(game_minimum))
}

/// The runtime to load the graphics agent: the first that supports it and also satisfies the game's minimum.
pub fn pick_for_graphics_agent(runtimes: &[Runtime], game_minimum: u64) -> Option<&Runtime> {
    runtimes
        .iter()
        .find(|r| r.release.supports_graphics_agent() && r.release.meets_game_minimum(game_minimum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const GAME_JRE: &str = "IMPLEMENTOR=\"Eclipse Adoptium\"\nJAVA_VERSION=\"25.0.1\"\nOS_ARCH=\"aarch64\"\nMODULES=\"java.base java.desktop java.logging jdk.unsupported\"\n";
    const FULL_JRE: &str = "JAVA_VERSION=\"25.0.3\"\nOS_ARCH=\"aarch64\"\nMODULES=\"java.base java.desktop java.instrument java.logging\"\n";

    fn make_runtime(home: &Path, release: &str) {
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("release"), release).unwrap();
        fs::write(home.join("bin/java"), b"").unwrap();
    }

    #[test]
    fn parses_versions_in_all_styles() {
        assert_eq!(
            JavaVersion::parse("25"),
            Some(JavaVersion {
                major: 25,
                minor: 0,
                patch: 0
            })
        );
        assert_eq!(
            JavaVersion::parse("25.0.3"),
            Some(JavaVersion {
                major: 25,
                minor: 0,
                patch: 3
            })
        );
        assert_eq!(
            JavaVersion::parse("21.0.5+11"),
            Some(JavaVersion {
                major: 21,
                minor: 0,
                patch: 5
            })
        );
        assert_eq!(
            JavaVersion::parse("1.8.0_452"),
            Some(JavaVersion {
                major: 8,
                minor: 0,
                patch: 452
            })
        );
        assert_eq!(JavaVersion::parse("abc"), None);
        assert_eq!(JavaVersion::parse(""), None);
    }

    #[test]
    fn encodes_versions_the_way_the_updater_does() {
        assert_eq!(
            JavaVersion::parse("25.0.3").unwrap().getdown_encoded(),
            25_000_300
        );
        assert_eq!(
            JavaVersion::parse("25").unwrap().getdown_encoded(),
            25_000_000
        );
        assert!(JavaVersion::parse("25.0.3").unwrap() > JavaVersion::parse("25.0.1").unwrap());
    }

    #[test]
    fn reads_a_release_file() {
        let release = JavaRelease::parse(GAME_JRE).unwrap();
        assert_eq!(release.version.major, 25);
        assert_eq!(release.os_arch.as_deref(), Some("aarch64"));
        assert!(release.modules.as_ref().unwrap().contains("java.desktop"));
        assert!(
            JavaRelease::parse("OS_ARCH=\"x86_64\"").is_none(),
            "no version, no release"
        );
    }

    #[test]
    fn the_games_trimmed_runtime_cannot_load_the_agent() {
        let trimmed = JavaRelease::parse(GAME_JRE).unwrap();
        assert!(trimmed.meets_game_minimum(25_000_000));
        assert!(
            !trimmed.supports_graphics_agent(),
            "java.instrument is missing"
        );
        assert!(JavaRelease::parse(FULL_JRE)
            .unwrap()
            .supports_graphics_agent());
    }

    #[test]
    fn agent_needs_java_24_and_a_missing_module_list_means_full() {
        let old =
            JavaRelease::parse("JAVA_VERSION=\"21.0.5\"\nMODULES=\"java.base java.instrument\"\n")
                .unwrap();
        assert!(!old.supports_graphics_agent());
        let no_modules = JavaRelease::parse("JAVA_VERSION=\"25.0.0\"\n").unwrap();
        assert!(no_modules.supports_graphics_agent());
    }

    #[test]
    fn minimum_comparison_uses_the_updater_encoding() {
        let release = JavaRelease::parse(GAME_JRE).unwrap();
        assert!(release.meets_game_minimum(25_000_000));
        assert!(release.meets_game_minimum(25_000_100));
        assert!(!release.meets_game_minimum(25_000_200));
    }

    #[test]
    fn finds_runtimes_in_priority_order_and_picks() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("game");
        let managed = root.path().join("managed");
        make_runtime(&game.join("java_vm"), GAME_JRE);
        make_runtime(
            &managed.join("temurin-24"),
            "JAVA_VERSION=\"24.0.2\"\nMODULES=\"java.base java.instrument\"\n",
        );
        make_runtime(&managed.join("temurin-25"), FULL_JRE);
        let env = RuntimeEnv::default();
        let found = find_runtimes(&env, Some(&game), Some(&managed));
        let order: Vec<_> = found
            .iter()
            .map(|r| (r.source, r.release.version.patch))
            .collect();
        assert_eq!(
            order,
            [
                (Source::GameBundled, 1),
                (Source::Managed, 3),
                (Source::Managed, 2)
            ]
        );
        assert_eq!(
            pick_for_game(&found, 25_000_000).unwrap().source,
            Source::GameBundled
        );
        let agent = pick_for_graphics_agent(&found, 25_000_000).unwrap();
        assert_eq!(
            agent.release.version.patch, 3,
            "the full 25 runtime, not the trimmed one or the 24 that the game rejects"
        );
    }

    #[test]
    fn java_home_and_path_are_used_without_duplicates() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("jdk");
        make_runtime(&home, FULL_JRE);
        let env = RuntimeEnv {
            java_home: Some(home.clone()),
            path: vec![home.join("bin")],
        };
        let found = find_runtimes(&env, None, None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, Source::JavaHome);
    }

    #[cfg(unix)]
    #[test]
    fn a_java_on_path_that_is_a_link_into_a_runtime_is_found() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("lib/jvm/jdk-25");
        make_runtime(&home, FULL_JRE);
        // Like /usr/bin/java -> /usr/lib/jvm/jdk-25/bin/java: `/usr` has no `release` file.
        let usr_bin = root.path().join("usr/bin");
        fs::create_dir_all(&usr_bin).unwrap();
        std::os::unix::fs::symlink(home.join("bin/java"), usr_bin.join("java")).unwrap();
        let env = RuntimeEnv {
            java_home: None,
            path: vec![usr_bin],
        };
        let found = find_runtimes(&env, None, None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].home, fs::canonicalize(&home).unwrap());
        assert_eq!(found[0].source, Source::Path);

        // The same runtime also given as JAVA_HOME (by its unresolved name) is not listed twice.
        let env = RuntimeEnv {
            java_home: Some(home),
            path: env.path,
        };
        assert_eq!(find_runtimes(&env, None, None).len(), 1);
    }

    #[test]
    fn stubs_without_a_release_file_are_skipped() {
        let root = tempfile::tempdir().unwrap();
        let stub = root.path().join("usr");
        fs::create_dir_all(stub.join("bin")).unwrap();
        fs::write(stub.join("bin/java"), b"").unwrap();
        let env = RuntimeEnv {
            java_home: None,
            path: vec![stub.join("bin")],
        };
        assert!(find_runtimes(&env, None, None).is_empty());
    }

    #[test]
    fn nothing_acceptable_gives_none() {
        let release = JavaRelease::parse("JAVA_VERSION=\"17.0.1\"\n").unwrap();
        let runtime = Runtime {
            home: PathBuf::from("/j"),
            release,
            source: Source::Path,
        };
        assert!(pick_for_game(&[runtime], 25_000_000).is_none());
    }

    #[test]
    fn executable_name_follows_the_os() {
        let release = JavaRelease::parse(FULL_JRE).unwrap();
        let runtime = Runtime {
            home: PathBuf::from("/j"),
            release,
            source: Source::Managed,
        };
        assert_eq!(runtime.java_executable(Os::Linux), Path::new("/j/bin/java"));
        assert_eq!(
            runtime.java_executable(Os::Windows),
            Path::new("/j/bin/java.exe")
        );
    }
}
