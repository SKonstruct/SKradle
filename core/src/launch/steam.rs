//! What the game needs to sign in through Steam, and whether Steam is there to do it.
//!
//! The game decides in `ProjectXApp.init`: it calls the Steam API (`SteamAPI_Init`, through `native/libsteam_api`,
//! found on `java.library.path`, which `getdown.txt` already sets). When that succeeds it logs in with the Steam
//! account of the running Steam client and never shows the password screen; when it fails it shows the password
//! screen. The API needs two things: a running Steam client of the same user, and the app id, which Steam hands to
//! the games it starts as the environment variables `SteamAppId` and `SteamGameId`. A game started any other way
//! has no app id and the API refuses.
//!
//! So a Steam install started by SKradle gets those two variables, and the launch is refused with a clear message
//! when no Steam client is running. Nothing else from Steam's own launch is needed for the sign-in (the overlay is
//! `DYLD_INSERT_LIBRARIES`/`LD_PRELOAD`, which SKradle does not set: no overlay, same sign-in).

use std::ffi::OsString;
use std::process::{Command, Stdio};

use crate::game::{InstallKind, Os, STEAM_APP_ID};

/// The environment variables Steam sets for the game that the game's Steam sign-in depends on.
pub fn environment() -> Vec<(OsString, OsString)> {
    let id = STEAM_APP_ID.to_string();
    vec![
        ("SteamAppId".into(), id.clone().into()),
        ("SteamGameId".into(), id.into()),
    ]
}

/// The variables a launch of this install adds. Only an Official launch of a Steam install signs in through
/// Steam; a private server has its own accounts, and the player's Steam identity is not sent there.
pub fn launch_environment(kind: InstallKind, private_server: bool) -> Vec<(OsString, OsString)> {
    if kind == InstallKind::Steam && !private_server {
        environment()
    } else {
        Vec::new()
    }
}

/// True when a launch of this install needs a running Steam client.
pub fn needs_client(kind: InstallKind, private_server: bool) -> bool {
    kind == InstallKind::Steam && !private_server
}

/// The process name of the Steam client on `os`.
pub fn client_process(os: Os) -> &'static str {
    match os {
        Os::Windows => "steam.exe",
        Os::MacOs => "steam_osx",
        Os::Linux => "steam",
    }
}

/// True when `tasklist /FI "IMAGENAME eq steam.exe" /NH` output lists the client.
pub fn tasklist_has_client(output: &str) -> bool {
    output
        .lines()
        .any(|line| line.trim_start().to_lowercase().starts_with("steam.exe"))
}

/// Asks the system whether the Steam client is running.
pub fn client_running(os: Os) -> bool {
    let name = client_process(os);
    match os {
        Os::Windows => Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {name}"), "/NH"])
            .stderr(Stdio::null())
            .output()
            .map(|out| tasklist_has_client(&String::from_utf8_lossy(&out.stdout)))
            .unwrap_or(false),
        Os::MacOs | Os::Linux => Command::new("pgrep")
            .args(["-x", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
    }
}

/// The command that opens the Steam client (and starts it when it is not running).
pub fn start_client_command(os: Os) -> (&'static str, Vec<&'static str>) {
    match os {
        Os::Windows => ("cmd", vec!["/C", "start", "", "steam://open/main"]),
        Os::MacOs => ("open", vec!["steam://open/main"]),
        Os::Linux => ("steam", vec!["steam://open/main"]),
    }
}

/// Starts the Steam client without waiting for it. Returns false when it could not be started.
pub fn start_client(os: Os) -> bool {
    let (program, args) = start_client_command(os);
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(env: &[(OsString, OsString)]) -> Vec<(String, String)> {
        env.iter()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn a_steam_install_on_official_gets_the_app_id_variables() {
        assert_eq!(
            pairs(&launch_environment(InstallKind::Steam, false)),
            [
                ("SteamAppId".to_owned(), "99900".to_owned()),
                ("SteamGameId".to_owned(), "99900".to_owned())
            ]
        );
    }

    #[test]
    fn a_standalone_install_or_a_private_server_gets_nothing() {
        assert!(launch_environment(InstallKind::Standalone, false).is_empty());
        assert!(launch_environment(InstallKind::Steam, true).is_empty());
        assert!(!needs_client(InstallKind::Standalone, false));
        assert!(!needs_client(InstallKind::Steam, true));
        assert!(needs_client(InstallKind::Steam, false));
    }

    #[test]
    fn the_client_is_found_in_tasklist_output() {
        assert!(tasklist_has_client(
            "steam.exe                     1234 Console                    1    80,000 K\r\n"
        ));
        assert!(!tasklist_has_client(
            "INFO: No tasks are running which match the specified criteria.\r\n"
        ));
        assert!(!tasklist_has_client("steamwebhelper.exe 99 Console 1 1 K"));
    }

    #[test]
    fn the_client_process_and_start_command_per_os() {
        assert_eq!(client_process(Os::MacOs), "steam_osx");
        assert_eq!(client_process(Os::Windows), "steam.exe");
        assert_eq!(client_process(Os::Linux), "steam");
        assert_eq!(start_client_command(Os::MacOs).0, "open");
        assert!(start_client_command(Os::Windows)
            .1
            .contains(&"steam://open/main"));
    }
}
