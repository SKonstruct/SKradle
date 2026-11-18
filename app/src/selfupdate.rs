//! Updating the launcher itself: how the downloaded installer is started. Checking and downloading are the core's
//! `skradle_core::update`; this is the part that is the application's job.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use skradle_core::game::Os;

/// The program and arguments that open an installer: the disk image on macOS (Finder mounts it), the installer
/// itself on Windows, the AppImage itself on Linux (it is made executable first).
pub fn installer_command(path: &Path, os: Os) -> (PathBuf, Vec<OsString>) {
    match os {
        Os::MacOs => (PathBuf::from("open"), vec![path.into()]),
        Os::Windows | Os::Linux => (path.to_path_buf(), Vec::new()),
    }
}

/// Starts the installer without waiting for it.
pub fn start_installer(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    if Os::current() == Os::Linux {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    let (program, args) = installer_command(path, Os::current());
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Where installers are downloaded to.
pub fn download_dir() -> PathBuf {
    crate::settings::config_dir().join("updates")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_each_os_opens_its_installer() {
        let path = Path::new("/tmp/SKradle-1.0.0-macos-arm64.dmg");
        assert_eq!(
            installer_command(path, Os::MacOs),
            (PathBuf::from("open"), vec![OsString::from(path)])
        );
        assert_eq!(
            installer_command(path, Os::Windows),
            (path.to_path_buf(), vec![])
        );
        assert_eq!(
            installer_command(path, Os::Linux),
            (path.to_path_buf(), vec![])
        );
    }
}
