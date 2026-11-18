//! `SettingsModel` (QML singleton `Settings`): the persisted launcher settings. Properties are two-way: QML
//! assigns them, every change is validated, written back to the shared [`crate::settings::Store`] and saved in
//! the background.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// "system", "light" or "dark".
        #[qproperty(QString, theme)]
        #[qproperty(bool, reduce_motion, cxx_name = "reduceMotion")]
        /// "system" or a locale such as "en".
        #[qproperty(QString, language)]
        /// The game's heap limit in MB; 0 means "leave the game's own setting".
        #[qproperty(i32, memory_mb, cxx_name = "memoryMb")]
        #[qproperty(QString, java_path, cxx_name = "javaPath")]
        #[qproperty(QString, extra_jvm_args, cxx_name = "extraJvmArgs")]
        #[qproperty(bool, minimize_while_playing, cxx_name = "minimizeWhilePlaying")]
        #[qproperty(bool, crash_reports, cxx_name = "crashReports")]
        #[qproperty(i32, render_scale, cxx_name = "renderScale")]
        #[qproperty(i32, sharpening)]
        #[qproperty(i32, anti_aliasing, cxx_name = "antiAliasing")]
        #[qproperty(bool, sharp_hud, cxx_name = "sharpHud")]
        #[qproperty(bool, show_fps, cxx_name = "showFps")]
        #[qproperty(i32, frame_gen_mode, cxx_name = "frameGenMode")]
        #[qproperty(i32, target_fps, cxx_name = "targetFps")]
        /// "" when the settings were read cleanly; otherwise a sentence for the player.
        #[qproperty(QString, load_warning, cxx_name = "loadWarning")]
        type Settings = super::SettingsRust;
    }

    impl cxx_qt::Initialize for Settings {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Initialize};
use cxx_qt_lib::QString;

use crate::settings::{self, LoadOutcome, Settings as Stored, Theme};

pub struct SettingsRust {
    theme: QString,
    reduce_motion: bool,
    language: QString,
    memory_mb: i32,
    java_path: QString,
    extra_jvm_args: QString,
    minimize_while_playing: bool,
    crash_reports: bool,
    render_scale: i32,
    sharpening: i32,
    anti_aliasing: i32,
    sharp_hud: bool,
    show_fps: bool,
    frame_gen_mode: i32,
    target_fps: i32,
    load_warning: QString,
    /// The translation catalogue last installed.
    applied_language: String,
}

impl Default for SettingsRust {
    fn default() -> Self {
        let d = Stored::default();
        Self {
            theme: QString::from(d.theme.as_str()),
            reduce_motion: d.reduce_motion,
            language: QString::from(d.language.as_str()),
            memory_mb: 0,
            java_path: QString::default(),
            extra_jvm_args: QString::default(),
            minimize_while_playing: d.minimize_while_playing,
            crash_reports: d.crash_reports,
            render_scale: d.render_scale as i32,
            sharpening: d.sharpening as i32,
            anti_aliasing: d.anti_aliasing as i32,
            sharp_hud: d.sharp_hud,
            show_fps: d.show_fps,
            frame_gen_mode: d.frame_gen_mode as i32,
            target_fps: d.target_fps as i32,
            load_warning: QString::default(),
            applied_language: String::new(),
        }
    }
}

/// The catalogue to install for a language setting: the `--pseudo` test mode overrides it.
pub fn language_for(setting: &str) -> String {
    if crate::options::get().pseudo {
        "pseudo".to_owned()
    } else {
        setting.to_owned()
    }
}

fn unsigned(value: i32) -> u32 {
    value.max(0) as u32
}

impl qobject::Settings {
    /// Copies the stored settings into the properties.
    fn pull(mut self: Pin<&mut Self>, s: &Stored) {
        self.as_mut().set_theme(QString::from(s.theme.as_str()));
        self.as_mut().set_reduce_motion(s.reduce_motion);
        self.as_mut()
            .set_language(QString::from(s.language.as_str()));
        self.as_mut()
            .set_memory_mb(s.memory_mb.map_or(0, |m| m as i32));
        self.as_mut()
            .set_java_path(QString::from(s.java_path.as_str()));
        self.as_mut()
            .set_extra_jvm_args(QString::from(s.extra_jvm_args.as_str()));
        self.as_mut()
            .set_minimize_while_playing(s.minimize_while_playing);
        self.as_mut().set_crash_reports(s.crash_reports);
        self.as_mut().set_render_scale(s.render_scale as i32);
        self.as_mut().set_sharpening(s.sharpening as i32);
        self.as_mut().set_anti_aliasing(s.anti_aliasing as i32);
        self.as_mut().set_sharp_hud(s.sharp_hud);
        self.as_mut().set_show_fps(s.show_fps);
        self.as_mut().set_frame_gen_mode(s.frame_gen_mode as i32);
        self.as_mut().set_target_fps(s.target_fps as i32);
    }

    /// A property changed: validate, store, save, and write back any value that had to be corrected.
    fn push(mut self: Pin<&mut Self>) {
        let theme = Theme::parse(&self.theme().to_string());
        let reduce_motion = *self.reduce_motion();
        let language = self.language().to_string();
        let memory = *self.memory_mb();
        let java_path = self.java_path().to_string();
        let extra_jvm_args = self.extra_jvm_args().to_string();
        let minimize_while_playing = *self.minimize_while_playing();
        let crash_reports = *self.crash_reports();
        let render_scale = unsigned(*self.render_scale());
        let sharpening = unsigned(*self.sharpening());
        let anti_aliasing = unsigned(*self.anti_aliasing());
        let sharp_hud = *self.sharp_hud();
        let show_fps = *self.show_fps();
        let frame_gen_mode = unsigned(*self.frame_gen_mode());
        let target_fps = unsigned(*self.target_fps());
        let after = settings::global().update(|s| {
            if let Some(theme) = theme {
                s.theme = theme;
            }
            s.reduce_motion = reduce_motion;
            if !language.is_empty() {
                s.language = language;
            }
            s.memory_mb = (memory > 0).then_some(memory as u32);
            s.java_path = java_path;
            s.extra_jvm_args = extra_jvm_args;
            s.minimize_while_playing = minimize_while_playing;
            s.crash_reports = crash_reports;
            s.render_scale = render_scale;
            s.sharpening = sharpening;
            s.anti_aliasing = anti_aliasing;
            s.sharp_hud = sharp_hud;
            s.show_fps = show_fps;
            s.frame_gen_mode = frame_gen_mode;
            s.target_fps = target_fps;
            s.clone()
        });
        crate::reporter::set_enabled(after.crash_reports);
        let language = language_for(&after.language);
        if language != self.applied_language {
            super::glue::set_language(&language);
            self.as_mut().rust_mut().applied_language = language;
        }
        self.pull(&after);
    }
}

impl Initialize for qobject::Settings {
    fn initialize(mut self: Pin<&mut Self>) {
        let store = settings::global();
        let loaded = store.get();
        self.as_mut().rust_mut().applied_language = language_for(&loaded.language);
        self.as_mut().pull(&loaded);
        if let LoadOutcome::Corrupt(_) = store.load_outcome() {
            self.as_mut().set_load_warning(QString::from(
                "The settings file could not be read, so defaults are in use. The old file was kept next to it.",
            ));
        }
        self.as_mut().on_theme_changed(|q| q.push()).release();
        self.as_mut()
            .on_reduce_motion_changed(|q| q.push())
            .release();
        self.as_mut().on_language_changed(|q| q.push()).release();
        self.as_mut().on_memory_mb_changed(|q| q.push()).release();
        self.as_mut().on_java_path_changed(|q| q.push()).release();
        self.as_mut()
            .on_extra_jvm_args_changed(|q| q.push())
            .release();
        self.as_mut()
            .on_minimize_while_playing_changed(|q| q.push())
            .release();
        self.as_mut()
            .on_crash_reports_changed(|q| q.push())
            .release();
        self.as_mut()
            .on_render_scale_changed(|q| q.push())
            .release();
        self.as_mut().on_sharpening_changed(|q| q.push()).release();
        self.as_mut()
            .on_anti_aliasing_changed(|q| q.push())
            .release();
        self.as_mut().on_sharp_hud_changed(|q| q.push()).release();
        self.as_mut().on_show_fps_changed(|q| q.push()).release();
        self.as_mut()
            .on_frame_gen_mode_changed(|q| q.push())
            .release();
        self.as_mut().on_target_fps_changed(|q| q.push()).release();
    }
}
