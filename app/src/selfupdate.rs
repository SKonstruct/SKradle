//! Updating the launcher itself: how the downloaded installer is started. Checking and downloading are the core's
//! `skradle_core::update`; this is the part that is the application's job.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use skradle_core::game::Os;

/// The program and arguments that run an installer: the disk image on macOS (Finder mounts it; the player drags the
/// app over the old one), the per-user installer on Windows (no administrator rights, so it is started directly;
/// `/S /UPDATE` runs it silently and starts SKradle again when done), and the AppImage itself on Linux (made executable
/// first).
pub fn installer_command(path: &Path, os: Os) -> (PathBuf, Vec<OsString>) {
    match os {
        Os::MacOs => (PathBuf::from("open"), vec![path.into()]),
        Os::Windows => (path.to_path_buf(), vec!["/S".into(), "/UPDATE".into()]),
        Os::Linux => (path.to_path_buf(), Vec::new()),
    }
}

/// Linux: when SKradle runs from an AppImage (`$APPIMAGE` names the file), the downloaded one takes its place and is
/// what starts, so the launcher keeps its one file. The new file is copied next to the old one first and renamed over
/// it, so a failure half-way leaves the old AppImage whole. `None` when this is not an AppImage run or the folder
/// cannot be written (the downloaded file then runs from where it is).
#[cfg(unix)]
fn replace_running_appimage(downloaded: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let current = PathBuf::from(std::env::var_os("APPIMAGE")?);
    if !current.is_file() {
        return None;
    }
    let staged = current.with_file_name(format!(".{}.new", current.file_name()?.to_string_lossy()));
    let moved = std::fs::copy(downloaded, &staged)
        .and_then(|_| std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)))
        .and_then(|_| std::fs::rename(&staged, &current));
    if moved.is_err() {
        let _ = std::fs::remove_file(&staged);
        return None;
    }
    let _ = std::fs::remove_file(downloaded);
    Some(current)
}

/// The file to run: on Linux made executable, and put in place of a running AppImage; elsewhere as it is.
fn prepared(path: &Path) -> std::io::Result<PathBuf> {
    #[cfg(unix)]
    if Os::current() == Os::Linux {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
        if let Some(replaced) = replace_running_appimage(path) {
            return Ok(replaced);
        }
    }
    Ok(path.to_path_buf())
}

/// Starts the installer without waiting for it (a thread reaps the process, so it never lingers as a zombie).
pub fn start_installer(path: &Path) -> std::io::Result<()> {
    let path = prepared(path)?;
    let (program, args) = installer_command(&path, Os::current());
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let _ = std::thread::Builder::new()
        .name("skradle-installer-wait".into())
        .spawn(move || {
            let _ = child.wait();
        });
    Ok(())
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
            installer_command(Path::new("C:/u/SKradle-1.0.0-windows-x64.exe"), Os::Windows),
            (
                PathBuf::from("C:/u/SKradle-1.0.0-windows-x64.exe"),
                vec![OsString::from("/S"), OsString::from("/UPDATE")]
            )
        );
        assert_eq!(
            installer_command(path, Os::Linux),
            (path.to_path_buf(), vec![])
        );
    }
}
