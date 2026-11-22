//! `Logs` (QML singleton `Logs`): where SKradle's log lives, for the Settings page's buttons. The log itself is
//! `skradle_core::log`; see `docs/logging.md`.

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
        /// The folder holding `skradle.log` ("" when logging is not running, as in the self-test).
        #[qproperty(QString, folder, READ, CONSTANT)]
        type Logs = super::LogsRust;

        /// Shows the log folder in the system file manager.
        #[qinvokable]
        #[cxx_name = "openFolder"]
        fn open_folder(self: &Logs);

        /// Copies the path of the log file to the clipboard.
        #[qinvokable]
        #[cxx_name = "copyPath"]
        fn copy_path(self: &Logs);

        /// The last `n` lines of the current log, joined with newlines.
        #[qinvokable]
        fn tail(self: &Logs, n: i32) -> QString;
    }
}

use cxx_qt_lib::QString;
use skradle_core::log;

pub struct LogsRust {
    folder: QString,
}

impl Default for LogsRust {
    fn default() -> Self {
        Self {
            folder: QString::from(
                log::folder()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default()
                    .as_str(),
            ),
        }
    }
}

/// The program that shows a folder in the system file manager.
pub fn file_manager_command() -> &'static str {
    if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

impl qobject::Logs {
    pub fn open_folder(&self) {
        let Some(folder) = log::folder() else {
            return;
        };
        let mut command = std::process::Command::new(file_manager_command());
        command
            .arg(&folder)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Err(e) = command.spawn() {
            log::warn("app", format!("could not open the log folder: {e}"));
        }
    }

    pub fn copy_path(&self) {
        if let Some(path) = log::current_file() {
            super::glue::copy_text(&path.to_string_lossy());
        }
    }

    pub fn tail(&self, n: i32) -> QString {
        let n = usize::try_from(n).unwrap_or(0).min(5000);
        QString::from(log::tail(n).join("\n").as_str())
    }
}

/// Qt's own messages, forwarded from the C++ message handler (`cpp/glue.cpp`): 1 warning, 2 critical or fatal,
/// 0 info/debug. Never unwinds into C++.
#[no_mangle]
pub extern "C" fn skradle_log_qt(level: i32, message: *const std::os::raw::c_char) {
    if message.is_null() {
        return;
    }
    // SAFETY: the C++ caller passes a NUL-terminated string that lives for the duration of the call.
    let text = unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy();
    let _ = std::panic::catch_unwind(|| match level {
        2 => log::error("qt", text.as_ref()),
        1 => log::warn("qt", text.as_ref()),
        _ => log::debug("qt", text.as_ref()),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_manager_program_is_a_known_one() {
        assert!(["explorer", "open", "xdg-open"].contains(&file_manager_command()));
    }
}
