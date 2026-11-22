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
use std::time::{Duration, Instant};

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

/// True when `tasklist` output (plain or `/FO CSV`, quotes included) lists `steam.exe`.
pub fn tasklist_has_client(output: &str) -> bool {
    output.lines().any(|line| {
        line.trim_start()
            .trim_start_matches('"')
            .to_lowercase()
            .starts_with("steam.exe")
    })
}

/// What `HKCU\Software\Valve\Steam\ActiveProcess` says: the running client's process id and the account signed in
/// (`ActiveUser` is 0 while nobody is signed in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActiveProcess {
    pub pid: Option<u32>,
    pub active_user: Option<u32>,
}

fn reg_dword(rest: &str) -> Option<u32> {
    let value = rest.trim_start().strip_prefix("REG_DWORD")?.trim();
    match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => value.parse().ok(),
    }
}

/// Reads `reg query HKCU\Software\Valve\Steam\ActiveProcess` output: lines like `    ActiveUser    REG_DWORD    0x1a2b3c`.
pub fn parse_reg_active_process(output: &str) -> ActiveProcess {
    let mut found = ActiveProcess::default();
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("ActiveUser") {
            found.active_user = reg_dword(rest);
        } else if let Some(rest) = line.strip_prefix("pid") {
            found.pid = reg_dword(rest);
        }
    }
    found
}

/// Reads `ActiveUser` from the text of Steam's `registry.vdf` (macOS and Linux): `Registry/HKCU/Software/Valve/Steam/
/// ActiveProcess/ActiveUser`.
pub fn parse_vdf_active_user(text: &str) -> Option<u32> {
    let doc = crate::vdf::parse(text).ok()?;
    let mut node = doc.get("Registry")?;
    for key in ["HKCU", "Software", "Valve", "Steam", "ActiveProcess"] {
        node = node.get(key)?;
    }
    node.get("ActiveUser")?.as_str()?.trim().parse().ok()
}

/// What a launch needs to know about the Steam client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    NotRunning,
    /// Running, but nobody is signed in yet (its login window is showing).
    NotSignedIn,
    /// Running and signed in, or running with no way to tell the sign-in apart (then the game finds out itself).
    Ready,
}

impl ClientState {
    pub fn describe(self) -> &'static str {
        match self {
            ClientState::NotRunning => "not running",
            ClientState::NotSignedIn => "running, not signed in",
            ClientState::Ready => "running, signed in",
        }
    }
}

/// The decision from the two facts: is the process there, and which account is signed in (`None`: unknown).
pub fn decide_state(running: bool, active_user: Option<u32>) -> ClientState {
    match (running, active_user) {
        (false, _) => ClientState::NotRunning,
        (true, Some(0)) => ClientState::NotSignedIn,
        (true, _) => ClientState::Ready,
    }
}

#[cfg(windows)]
fn hidden(command: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000) // CREATE_NO_WINDOW: no console flash from a GUI program
}

#[cfg(not(windows))]
fn hidden(command: &mut Command) -> &mut Command {
    command
}

fn run_text(command: &mut Command) -> Option<String> {
    hidden(command)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn windows_state() -> ClientState {
    let active =
        run_text(Command::new("reg").args(["query", r"HKCU\Software\Valve\Steam\ActiveProcess"]))
            .map(|t| parse_reg_active_process(&t))
            .unwrap_or_default();
    let by_pid = active.pid.filter(|p| *p != 0).is_some_and(|pid| {
        run_text(Command::new("tasklist").args([
            "/FI",
            &format!("PID eq {pid}"),
            "/FI",
            "IMAGENAME eq steam.exe",
            "/NH",
            "/FO",
            "CSV",
        ]))
        .is_some_and(|t| tasklist_has_client(&t))
    });
    let running = by_pid
        || run_text(Command::new("tasklist").args([
            "/FI",
            "IMAGENAME eq steam.exe",
            "/NH",
            "/FO",
            "CSV",
        ]))
        .is_some_and(|t| tasklist_has_client(&t));
    decide_state(running, active.active_user)
}

fn registry_vdf_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let candidates = [
        home.join("Library/Application Support/Steam/registry.vdf"),
        home.join(".steam/registry.vdf"),
        home.join(".steam/steam/registry.vdf"),
        home.join(".local/share/Steam/registry.vdf"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Asks the system about the Steam client: running, and signed in.
pub fn client_state(os: Os) -> ClientState {
    match os {
        Os::Windows => windows_state(),
        Os::MacOs | Os::Linux => {
            let running = Command::new("pgrep")
                .args(["-x", client_process(os)])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            let user = registry_vdf_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|t| parse_vdf_active_user(&t));
            decide_state(running, user)
        }
    }
}

/// Asks the system whether the Steam client is running.
pub fn client_running(os: Os) -> bool {
    client_state(os) != ClientState::NotRunning
}

/// The command that opens the Steam client (and starts it when it is not running).
pub fn start_client_command(os: Os) -> (&'static str, Vec<&'static str>) {
    match os {
        Os::Windows => ("cmd", vec!["/C", "start", "", "steam://open/main"]),
        Os::MacOs => ("open", vec!["steam://open/main"]),
        Os::Linux => ("xdg-open", vec!["steam://open/main"]),
    }
}

/// Starts the Steam client without waiting for it. Returns false when it could not be started.
pub fn start_client(os: Os) -> bool {
    let (program, args) = start_client_command(os);
    let mut command = Command::new(program);
    command.args(args);
    if hidden(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
    {
        return true;
    }
    // Linux without a URL handler: the client itself.
    os == Os::Linux
        && Command::new("steam")
            .arg("steam://open/main")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
}

// ---- Waiting for Steam ---------------------------------------------------------------------------------------------

/// How long a launch waits for Steam to be up and signed in.
pub const WAIT_TIMEOUT: Duration = Duration::from_secs(60);
/// How often the wait looks.
pub const WAIT_POLL: Duration = Duration::from_secs(1);
/// After Steam shows up it gets a moment to open its pipes before the game asks it.
pub const WAIT_SETTLE: Duration = Duration::from_secs(2);

/// What the wait needs from the world: the client's state, starting it, and time. The real one is [`System`]; tests
/// use a fake clock.
pub trait Waiter {
    fn state(&mut self) -> ClientState;
    fn start(&mut self) -> bool;
    /// Time since the wait began.
    fn elapsed(&self) -> Duration;
    fn sleep(&mut self, d: Duration);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// Already up; nothing was started.
    AlreadyReady,
    /// Steam was started or brought up and is now ready.
    BecameReady {
        started: bool,
    },
    CouldNotStart,
    TimedOut {
        last: ClientState,
    },
    Cancelled,
}

/// Makes sure Steam is running and signed in. When it is not, starts it (once) and looks every `poll` until it is, for
/// at most `timeout`. `cancelled` is asked every round; `on_wait` is told the state before each round of waiting.
pub fn ensure_ready(
    waiter: &mut dyn Waiter,
    cancelled: &dyn Fn() -> bool,
    on_wait: &mut dyn FnMut(ClientState),
    timeout: Duration,
    poll: Duration,
    settle: Duration,
) -> Waited {
    let mut state = waiter.state();
    if state == ClientState::Ready {
        return Waited::AlreadyReady;
    }
    let started = waiter.start();
    if !started && state == ClientState::NotRunning {
        return Waited::CouldNotStart;
    }
    loop {
        if cancelled() {
            return Waited::Cancelled;
        }
        if waiter.elapsed() >= timeout {
            return Waited::TimedOut { last: state };
        }
        on_wait(state);
        waiter.sleep(poll);
        state = waiter.state();
        if state == ClientState::Ready {
            if started {
                waiter.sleep(settle);
            }
            return Waited::BecameReady { started };
        }
    }
}

/// The real world: process probes, `open steam://...`, the clock.
pub struct System {
    os: Os,
    begun: Instant,
}

impl System {
    pub fn new(os: Os) -> Self {
        Self {
            os,
            begun: Instant::now(),
        }
    }
}

impl Waiter for System {
    fn state(&mut self) -> ClientState {
        client_state(self.os)
    }
    fn start(&mut self) -> bool {
        start_client(self.os)
    }
    fn elapsed(&self) -> Duration {
        self.begun.elapsed()
    }
    fn sleep(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// The message for a wait that ended without Steam being ready.
pub fn failure_message(outcome: Waited) -> Option<String> {
    match outcome {
        Waited::AlreadyReady | Waited::BecameReady { .. } | Waited::Cancelled => None,
        Waited::CouldNotStart => Some(
            "Steam is not running and SKradle could not start it. Start Steam yourself, sign in, then launch again."
                .to_owned(),
        ),
        Waited::TimedOut { last: ClientState::NotSignedIn } => Some(
            "Steam started but is not signed in. Sign in to Steam (its window may be behind others), then launch again."
                .to_owned(),
        ),
        Waited::TimedOut { .. } => Some(
            "Steam did not start within a minute. Start Steam yourself, sign in, then launch again."
                .to_owned(),
        ),
    }
}

/// True when the sign-in file `steam_appid.txt` should be written into the game folder: Steam installs on Official,
/// on Windows and Linux. (macOS reads the variables.)
pub fn wants_appid_file(os: Os, kind: InstallKind, private_server: bool) -> bool {
    needs_client(kind, private_server) && os != Os::MacOs
}

/// Writes `steam_appid.txt` (the app id) into `game_root` when it is missing or different. Best effort: the result
/// says what happened and a failure never fails the launch.
pub fn ensure_appid_file(game_root: &std::path::Path) -> Result<bool, String> {
    let path = game_root.join("steam_appid.txt");
    let want = STEAM_APP_ID.to_string();
    if std::fs::read_to_string(&path).is_ok_and(|t| t.trim() == want) {
        return Ok(false);
    }
    std::fs::write(&path, format!("{want}\n")).map_err(|e| e.to_string())?;
    Ok(true)
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

    #[test]
    fn registry_output_gives_pid_and_signed_in_user() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\\ActiveProcess\r\n    pid    REG_DWORD    0x1a2b\r\n    ActiveUser    REG_DWORD    0x1e240\r\n    SteamClientDll64    REG_SZ    C:\\Steam\\steamclient64.dll\r\n";
        assert_eq!(
            parse_reg_active_process(out),
            ActiveProcess {
                pid: Some(0x1a2b),
                active_user: Some(123_456)
            }
        );
        let signed_out = "    pid    REG_DWORD    0x10\r\n    ActiveUser    REG_DWORD    0x0\r\n";
        assert_eq!(parse_reg_active_process(signed_out).active_user, Some(0));
        assert_eq!(
            parse_reg_active_process("ERROR: not found"),
            ActiveProcess::default()
        );
        assert_eq!(
            parse_reg_active_process("    ActiveUser    REG_DWORD    77").active_user,
            Some(77)
        );
    }

    #[test]
    fn registry_vdf_gives_the_active_user() {
        let text = r#""Registry" { "HKCU" { "Software" { "Valve" { "Steam" { "ActiveProcess" { "pid" "4242" "ActiveUser" "9001" } } } } } }"#;
        assert_eq!(parse_vdf_active_user(text), Some(9001));
        assert_eq!(parse_vdf_active_user(r#""Registry" { }"#), None);
        assert_eq!(parse_vdf_active_user("garbage {"), None);
    }

    #[test]
    fn the_state_follows_process_and_sign_in() {
        assert_eq!(decide_state(false, Some(5)), ClientState::NotRunning);
        assert_eq!(decide_state(true, Some(0)), ClientState::NotSignedIn);
        assert_eq!(decide_state(true, Some(5)), ClientState::Ready);
        assert_eq!(decide_state(true, None), ClientState::Ready);
    }

    #[test]
    fn tasklist_csv_with_quotes_is_understood() {
        assert!(tasklist_has_client(
            "\"steam.exe\",\"1234\",\"Console\",\"1\",\"80,000 K\"\r\n"
        ));
        assert!(!tasklist_has_client(
            "\"steamwebhelper.exe\",\"1\",\"Console\""
        ));
    }

    #[test]
    fn linux_opens_steam_through_the_url_handler() {
        assert_eq!(start_client_command(Os::Linux).0, "xdg-open");
    }

    #[test]
    fn the_appid_file_is_for_windows_and_linux_steam_installs_on_official() {
        assert!(wants_appid_file(Os::Windows, InstallKind::Steam, false));
        assert!(wants_appid_file(Os::Linux, InstallKind::Steam, false));
        assert!(!wants_appid_file(Os::MacOs, InstallKind::Steam, false));
        assert!(!wants_appid_file(Os::Windows, InstallKind::Steam, true));
        assert!(!wants_appid_file(
            Os::Windows,
            InstallKind::Standalone,
            false
        ));
    }

    #[test]
    fn the_appid_file_is_written_once_and_a_failure_is_an_error_value() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(ensure_appid_file(dir.path()), Ok(true));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("steam_appid.txt"))
                .unwrap()
                .trim(),
            "99900"
        );
        assert_eq!(ensure_appid_file(dir.path()), Ok(false));
        assert!(ensure_appid_file(&dir.path().join("missing")).is_err());
    }

    /// A clock that only moves when told; the client becomes ready after `ready_after` of waiting.
    struct Fake {
        now: Duration,
        ready_after: Option<Duration>,
        start_ok: bool,
        starts: u32,
        running_not_signed_in: bool,
    }

    impl Fake {
        fn new(ready_after: Option<Duration>) -> Self {
            Fake {
                now: Duration::ZERO,
                ready_after,
                start_ok: true,
                starts: 0,
                running_not_signed_in: false,
            }
        }
    }

    impl Waiter for Fake {
        fn state(&mut self) -> ClientState {
            match self.ready_after {
                Some(t) if self.now >= t => ClientState::Ready,
                _ if self.running_not_signed_in => ClientState::NotSignedIn,
                _ => ClientState::NotRunning,
            }
        }
        fn start(&mut self) -> bool {
            self.starts += 1;
            self.start_ok
        }
        fn elapsed(&self) -> Duration {
            self.now
        }
        fn sleep(&mut self, d: Duration) {
            self.now += d;
        }
    }

    fn wait(fake: &mut Fake, cancel_after: Option<Duration>) -> (Waited, usize) {
        let mut rounds = 0;
        let now = std::cell::Cell::new(Duration::ZERO);
        let outcome = {
            let cancelled = || cancel_after.is_some_and(|t| now.get() >= t);
            let mut seen = |_| rounds += 1;
            // The probe runs against `fake`; mirror its clock for the cancel check.
            struct Mirror<'a>(&'a mut Fake, &'a std::cell::Cell<Duration>);
            impl Waiter for Mirror<'_> {
                fn state(&mut self) -> ClientState {
                    self.0.state()
                }
                fn start(&mut self) -> bool {
                    self.0.start()
                }
                fn elapsed(&self) -> Duration {
                    self.0.elapsed()
                }
                fn sleep(&mut self, d: Duration) {
                    self.0.sleep(d);
                    self.1.set(self.0.now);
                }
            }
            ensure_ready(
                &mut Mirror(fake, &now),
                &cancelled,
                &mut seen,
                WAIT_TIMEOUT,
                WAIT_POLL,
                WAIT_SETTLE,
            )
        };
        (outcome, rounds)
    }

    #[test]
    fn a_ready_client_is_not_touched() {
        let mut fake = Fake::new(Some(Duration::ZERO));
        assert_eq!(wait(&mut fake, None).0, Waited::AlreadyReady);
        assert_eq!(fake.starts, 0);
        assert_eq!(fake.now, Duration::ZERO);
    }

    #[test]
    fn a_missing_client_is_started_once_and_waited_for() {
        let mut fake = Fake::new(Some(Duration::from_secs(7)));
        let (outcome, rounds) = wait(&mut fake, None);
        assert_eq!(outcome, Waited::BecameReady { started: true });
        assert_eq!(fake.starts, 1);
        assert_eq!(rounds, 7);
        // Seven polls plus the settle time.
        assert_eq!(fake.now, Duration::from_secs(9));
    }

    #[test]
    fn a_running_client_that_is_not_signed_in_is_brought_up_and_waited_for() {
        let mut fake = Fake::new(Some(Duration::from_secs(3)));
        fake.running_not_signed_in = true;
        assert_eq!(
            wait(&mut fake, None).0,
            Waited::BecameReady { started: true }
        );
        assert_eq!(fake.starts, 1);
    }

    #[test]
    fn it_gives_up_after_sixty_seconds_with_the_last_state() {
        let mut fake = Fake::new(None);
        fake.running_not_signed_in = true;
        let (outcome, rounds) = wait(&mut fake, None);
        assert_eq!(
            outcome,
            Waited::TimedOut {
                last: ClientState::NotSignedIn
            }
        );
        assert_eq!(rounds, 60);
        assert!(failure_message(outcome).unwrap().contains("not signed in"));
        let mut gone = Fake::new(None);
        let outcome = wait(&mut gone, None).0;
        assert!(failure_message(outcome).unwrap().contains("did not start"));
    }

    #[test]
    fn it_can_be_cancelled_while_waiting() {
        let mut fake = Fake::new(None);
        let (outcome, rounds) = wait(&mut fake, Some(Duration::from_secs(4)));
        assert_eq!(outcome, Waited::Cancelled);
        assert_eq!(rounds, 4);
        assert!(failure_message(outcome).is_none());
    }

    #[test]
    fn a_client_that_cannot_be_started_fails_at_once() {
        let mut fake = Fake::new(None);
        fake.start_ok = false;
        let outcome = wait(&mut fake, None).0;
        assert_eq!(outcome, Waited::CouldNotStart);
        assert_eq!(fake.now, Duration::ZERO);
        assert!(failure_message(outcome)
            .unwrap()
            .contains("could not start"));
    }
}
