//! The launcher's persisted settings: one JSON file in the OS config folder.
//!
//! - Unknown keys are kept and written back, so a newer launcher's settings survive an older one.
//! - A value of the wrong type falls back to its default; a file that is not JSON (or not an object) is moved
//!   aside as `settings.json.corrupt` and the defaults are used. Loading never fails.
//! - Writes go to a temporary file in the same folder and are renamed over the old one, so a crash cannot leave
//!   half a file. [`Store`] coalesces bursts of changes (a slider drag) into one write on a background thread.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{Map, Value};

/// Bounds for the game's memory limit (megabytes).
pub const MEMORY_MIN: u32 = 512;
pub const MEMORY_MAX: u32 = 16384;
pub const FPS_MIN: u32 = 30;
pub const FPS_MAX: u32 = 360;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn parse(text: &str) -> Option<Theme> {
        match text {
            "system" => Some(Theme::System),
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: Theme,
    pub reduce_motion: bool,
    /// `system` or a locale such as `en`.
    pub language: String,
    /// The install the player picked (a discovered one or the custom folder).
    pub game_path: Option<String>,
    /// A folder the player pointed to, kept even when it is not the selected one.
    pub custom_game_path: Option<String>,
    /// A Java the player chose; empty means pick one automatically.
    pub java_path: String,
    /// Extra arguments for the updater's JVM, split on spaces.
    pub extra_jvm_args: String,
    /// The game's heap limit. `None` leaves the game's own `extra.txt` setting alone.
    pub memory_mb: Option<u32>,
    pub minimize_while_playing: bool,
    /// Crash reports (the telemetry switch).
    pub crash_reports: bool,
    /// A launcher version the player chose not to be offered again.
    pub skipped_update: Option<String>,
    // The Graphics page. Stored for the page; the game does not read them yet (TODO(graphics)).
    pub render_scale: u32,
    pub sharpening: u32,
    pub anti_aliasing: u32,
    pub sharp_hud: bool,
    pub show_fps: bool,
    pub frame_gen_mode: u32,
    pub target_fps: u32,
    /// Keys this version does not know, written back unchanged.
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            reduce_motion: false,
            language: "system".to_owned(),
            game_path: None,
            custom_game_path: None,
            java_path: String::new(),
            extra_jvm_args: String::new(),
            memory_mb: None,
            minimize_while_playing: true,
            crash_reports: true,
            skipped_update: None,
            render_scale: 100,
            sharpening: 40,
            anti_aliasing: 1,
            sharp_hud: true,
            show_fps: false,
            frame_gen_mode: 0,
            target_fps: 120,
            extra: Map::new(),
        }
    }
}

const KNOWN_KEYS: &[&str] = &[
    "version",
    "theme",
    "reduceMotion",
    "language",
    "gamePath",
    "customGamePath",
    "javaPath",
    "extraJvmArgs",
    "memoryMb",
    "minimizeWhilePlaying",
    "crashReports",
    "skippedUpdate",
    "renderScale",
    "sharpening",
    "antiAliasing",
    "sharpHud",
    "showFps",
    "frameGenMode",
    "targetFps",
];

fn get<T: serde::de::DeserializeOwned>(map: &Map<String, Value>, key: &str) -> Option<T> {
    serde_json::from_value(map.get(key)?.clone()).ok()
}

fn path_value(map: &Map<String, Value>, key: &str) -> Option<String> {
    get::<String>(map, key).filter(|s| !s.trim().is_empty())
}

impl Settings {
    /// Builds settings from a parsed object, field by field.
    pub fn from_map(map: &Map<String, Value>) -> Settings {
        let d = Settings::default();
        let mut s = Settings {
            theme: get::<String>(map, "theme")
                .and_then(|t| Theme::parse(&t))
                .unwrap_or(d.theme),
            reduce_motion: get(map, "reduceMotion").unwrap_or(d.reduce_motion),
            language: get::<String>(map, "language")
                .filter(|l| !l.is_empty() && l.len() <= 16)
                .unwrap_or(d.language),
            game_path: path_value(map, "gamePath"),
            custom_game_path: path_value(map, "customGamePath"),
            java_path: get(map, "javaPath").unwrap_or(d.java_path),
            extra_jvm_args: get(map, "extraJvmArgs").unwrap_or(d.extra_jvm_args),
            memory_mb: get(map, "memoryMb"),
            minimize_while_playing: get(map, "minimizeWhilePlaying")
                .unwrap_or(d.minimize_while_playing),
            crash_reports: get(map, "crashReports").unwrap_or(d.crash_reports),
            skipped_update: path_value(map, "skippedUpdate"),
            render_scale: get(map, "renderScale").unwrap_or(d.render_scale),
            sharpening: get(map, "sharpening").unwrap_or(d.sharpening),
            anti_aliasing: get(map, "antiAliasing").unwrap_or(d.anti_aliasing),
            sharp_hud: get(map, "sharpHud").unwrap_or(d.sharp_hud),
            show_fps: get(map, "showFps").unwrap_or(d.show_fps),
            frame_gen_mode: get(map, "frameGenMode").unwrap_or(d.frame_gen_mode),
            target_fps: get(map, "targetFps").unwrap_or(d.target_fps),
            extra: map
                .iter()
                .filter(|(k, _)| !KNOWN_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        s.sanitize();
        s
    }

    /// Brings every value into its allowed range.
    pub fn sanitize(&mut self) {
        self.memory_mb = self.memory_mb.map(|m| m.clamp(MEMORY_MIN, MEMORY_MAX));
        self.render_scale = self.render_scale.clamp(50, 200);
        self.sharpening = self.sharpening.min(100);
        self.anti_aliasing = self.anti_aliasing.min(2);
        self.frame_gen_mode = self.frame_gen_mode.min(1);
        self.target_fps = self.target_fps.clamp(FPS_MIN, FPS_MAX);
    }

    pub fn to_map(&self) -> Map<String, Value> {
        let mut map = self.extra.clone();
        let mut put = |key: &str, value: Value| {
            map.insert(key.to_owned(), value);
        };
        put("version", Value::from(1));
        put("theme", Value::from(self.theme.as_str()));
        put("reduceMotion", Value::from(self.reduce_motion));
        put("language", Value::from(self.language.as_str()));
        put("gamePath", self.game_path.clone().into());
        put("customGamePath", self.custom_game_path.clone().into());
        put("javaPath", Value::from(self.java_path.as_str()));
        put("extraJvmArgs", Value::from(self.extra_jvm_args.as_str()));
        put("memoryMb", self.memory_mb.into());
        put(
            "minimizeWhilePlaying",
            Value::from(self.minimize_while_playing),
        );
        put("crashReports", Value::from(self.crash_reports));
        put("skippedUpdate", self.skipped_update.clone().into());
        put("renderScale", Value::from(self.render_scale));
        put("sharpening", Value::from(self.sharpening));
        put("antiAliasing", Value::from(self.anti_aliasing));
        put("sharpHud", Value::from(self.sharp_hud));
        put("showFps", Value::from(self.show_fps));
        put("frameGenMode", Value::from(self.frame_gen_mode));
        put("targetFps", Value::from(self.target_fps));
        map
    }
}

/// What reading the file found.
#[derive(Debug, PartialEq)]
pub enum LoadOutcome {
    Loaded,
    /// No file yet.
    Missing,
    /// The file was unusable; it was moved to the given path (or removal failed: `None`).
    Corrupt(Option<PathBuf>),
}

/// Reads `path`. Never fails: problems give defaults.
pub fn load(path: &Path) -> (Settings, LoadOutcome) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Settings::default(), LoadOutcome::Missing)
        }
        Err(_) => return (Settings::default(), set_aside(path)),
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => (Settings::from_map(&map), LoadOutcome::Loaded),
        _ => (Settings::default(), set_aside(path)),
    }
}

fn set_aside(path: &Path) -> LoadOutcome {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".corrupt");
    let target = path.with_file_name(name);
    match std::fs::rename(path, &target) {
        Ok(()) => LoadOutcome::Corrupt(Some(target)),
        Err(_) => LoadOutcome::Corrupt(None),
    }
}

/// Writes `settings` to `path` atomically.
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut text = serde_json::to_string_pretty(&Value::Object(settings.to_map()))
        .map_err(std::io::Error::other)?;
    text.push('\n');
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// The folder for the launcher's own files: `SKRADLE_CONFIG_DIR` if set, else the OS config folder.
pub fn config_dir() -> PathBuf {
    std::env::var_os("SKRADLE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("SKradle")))
        .unwrap_or_else(|| std::env::temp_dir().join("SKradle"))
}

struct Inner {
    settings: Settings,
    dirty: bool,
}

/// The settings in memory, shared by the models, saved in the background.
pub struct Store {
    path: PathBuf,
    inner: Mutex<Inner>,
    wake: Mutex<Option<mpsc::Sender<()>>>,
    outcome: LoadOutcome,
}

impl Store {
    /// Loads `path` and starts the saving thread.
    pub fn open(path: PathBuf) -> Arc<Store> {
        let (settings, outcome) = load(&path);
        let (tx, rx) = mpsc::channel::<()>();
        let store = Arc::new(Store {
            path,
            inner: Mutex::new(Inner {
                settings,
                dirty: false,
            }),
            wake: Mutex::new(Some(tx)),
            outcome,
        });
        let weak = Arc::downgrade(&store);
        let _ = std::thread::Builder::new()
            .name("skradle-settings".into())
            .spawn(move || {
                // One write for a burst of changes.
                while rx.recv().is_ok() {
                    loop {
                        match rx.recv_timeout(Duration::from_millis(300)) {
                            Ok(()) => continue,
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => break,
                        }
                    }
                    match weak.upgrade() {
                        Some(store) => store.flush(),
                        None => break,
                    }
                }
            });
        store
    }

    pub fn load_outcome(&self) -> &LoadOutcome {
        &self.outcome
    }

    pub fn get(&self) -> Settings {
        self.lock().settings.clone()
    }

    /// Changes the settings and schedules a save.
    pub fn update<R>(&self, change: impl FnOnce(&mut Settings) -> R) -> R {
        let result = {
            let mut inner = self.lock();
            let result = change(&mut inner.settings);
            inner.settings.sanitize();
            inner.dirty = true;
            result
        };
        if let Some(tx) = self.wake.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(());
        }
        result
    }

    /// Writes pending changes now. Errors are dropped: losing a setting must not stop the launcher.
    pub fn flush(&self) {
        let snapshot = {
            let mut inner = self.lock();
            if !inner.dirty {
                return;
            }
            inner.dirty = false;
            inner.settings.clone()
        };
        if save(&self.path, &snapshot).is_err() {
            self.lock().dirty = true;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static GLOBAL: OnceLock<Arc<Store>> = OnceLock::new();

/// The process-wide store, opened on first use.
pub fn global() -> &'static Arc<Store> {
    GLOBAL.get_or_init(|| Store::open(config_dir().join("settings.json")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = temp();
        let (s, outcome) = load(&dir.path().join("settings.json"));
        assert_eq!(s, Settings::default());
        assert_eq!(outcome, LoadOutcome::Missing);
    }

    #[test]
    fn round_trips() {
        let dir = temp();
        let path = dir.path().join("sub/settings.json");
        let s = Settings {
            theme: Theme::Dark,
            memory_mb: Some(4096),
            game_path: Some("/games/sk".into()),
            crash_reports: false,
            ..Settings::default()
        };
        save(&path, &s).unwrap();
        let (back, outcome) = load(&path);
        assert_eq!(back, s);
        assert_eq!(outcome, LoadOutcome::Loaded);
        assert!(!dir.path().join("sub/settings.json.tmp").exists());
    }

    #[test]
    fn unknown_keys_survive() {
        let dir = temp();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"theme":"light","futureThing":{"a":[1,2]},"servers":["x"]}"#,
        )
        .unwrap();
        let (mut s, _) = load(&path);
        assert_eq!(s.theme, Theme::Light);
        s.reduce_motion = true;
        save(&path, &s).unwrap();
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["futureThing"], json!({"a": [1, 2]}));
        assert_eq!(written["servers"], json!(["x"]));
        assert_eq!(written["reduceMotion"], json!(true));
    }

    #[test]
    fn corrupt_file_falls_back_and_is_kept_aside() {
        let dir = temp();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        let (s, outcome) = load(&path);
        assert_eq!(s, Settings::default());
        let aside = dir.path().join("settings.json.corrupt");
        assert_eq!(outcome, LoadOutcome::Corrupt(Some(aside.clone())));
        assert_eq!(std::fs::read_to_string(aside).unwrap(), "{ not json");
        assert!(!path.exists());
    }

    #[test]
    fn a_json_array_counts_as_corrupt() {
        let dir = temp();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "[1,2,3]").unwrap();
        assert!(matches!(load(&path).1, LoadOutcome::Corrupt(_)));
    }

    #[test]
    fn wrong_types_fall_back_one_field_at_a_time() {
        let dir = temp();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"theme":5,"reduceMotion":"yes","memoryMb":"lots","language":"de","showFps":true}"#,
        )
        .unwrap();
        let (s, outcome) = load(&path);
        assert_eq!(outcome, LoadOutcome::Loaded);
        assert_eq!(s.theme, Theme::System);
        assert!(!s.reduce_motion);
        assert_eq!(s.memory_mb, None);
        assert_eq!(s.language, "de");
        assert!(s.show_fps);
    }

    #[test]
    fn values_are_clamped() {
        let map = json!({"memoryMb": 99999999, "renderScale": 5, "targetFps": 9000, "sharpening": 400, "theme": "neon"});
        let s = Settings::from_map(map.as_object().unwrap());
        assert_eq!(s.memory_mb, Some(MEMORY_MAX));
        assert_eq!(s.render_scale, 50);
        assert_eq!(s.target_fps, FPS_MAX);
        assert_eq!(s.sharpening, 100);
        assert_eq!(s.theme, Theme::System);
        let low = Settings::from_map(json!({"memoryMb": 1}).as_object().unwrap());
        assert_eq!(low.memory_mb, Some(MEMORY_MIN));
    }

    #[test]
    fn empty_paths_count_as_unset() {
        let s = Settings::from_map(
            json!({"gamePath": "  ", "customGamePath": ""})
                .as_object()
                .unwrap(),
        );
        assert_eq!((s.game_path, s.custom_game_path), (None, None));
    }

    #[test]
    fn store_saves_in_the_background_and_on_flush() {
        let dir = temp();
        let path = dir.path().join("settings.json");
        let store = Store::open(path.clone());
        assert_eq!(*store.load_outcome(), LoadOutcome::Missing);
        store.update(|s| s.theme = Theme::Dark);
        store.update(|s| s.memory_mb = Some(100)); // clamped on the way in
        assert_eq!(store.get().memory_mb, Some(MEMORY_MIN));
        store.flush();
        let (saved, _) = load(&path);
        assert_eq!(saved.theme, Theme::Dark);
        assert_eq!(saved.memory_mb, Some(MEMORY_MIN));
        // The background thread writes without a flush.
        store.update(|s| s.theme = Theme::Light);
        for _ in 0..40 {
            if load(&path).0.theme == Theme::Light {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the background save never happened");
    }
}
