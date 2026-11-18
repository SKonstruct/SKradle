//! Running the updater and following it, and then the game it starts.
//!
//! A thread starts the updater, reads its `launcher.log` for phase hints, notices when it exits, and then watches
//! the game until it is gone. Everything is reported through an event callback; nothing here touches Qt.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use skradle_core::getdown::CommandSpec;

use super::log::{classify, LogTail};
use super::{Event, LaunchError, Phase, Step};

/// How often things are checked.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// While the updater runs: its exit and its log.
    pub poll: Duration,
    /// While the game runs: whether it is still there.
    pub watch: Duration,
    /// After asking politely to stop, how long to wait before forcing.
    pub grace: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(200),
            watch: Duration::from_millis(1000),
            grace: Duration::from_secs(5),
        }
    }
}

pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

/// A launch in progress.
pub struct Handle {
    stop: Arc<AtomicBool>,
}

impl Handle {
    /// Asks the launch to stop: the updater and the game are terminated. The supervisor reports `Stopped`.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Starts the supervisor thread. Errors (including failing to start the process) come through `sink`.
pub fn start(spec: CommandSpec, game_root: PathBuf, timing: Timing, sink: Sink) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let handle = Handle { stop: stop.clone() };
    let spawned = std::thread::Builder::new()
        .name("skradle-launch".into())
        .spawn({
            let sink = sink.clone();
            move || run(spec, game_root, timing, &stop, &*sink)
        });
    if let Err(e) = spawned {
        sink(Event::Failed(LaunchError::Spawn(e.to_string())));
    }
    handle
}

const TAIL_LINES: usize = 12;

type Tail = Arc<Mutex<VecDeque<String>>>;

fn capture(stream: impl std::io::Read + Send + 'static, tail: Tail) {
    let _ = std::thread::Builder::new()
        .name("skradle-output".into())
        .spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let mut tail = tail.lock().unwrap_or_else(|e| e.into_inner());
                if tail.len() == TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });
}

fn tail_text(tail: &Tail) -> String {
    let tail = tail.lock().unwrap_or_else(|e| e.into_inner());
    let text: Vec<&str> = tail
        .iter()
        .map(String::as_str)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let text = text.join(" / ");
    // Long enough to read, short enough for one dialog line.
    text.chars().take(300).collect()
}

fn run(
    spec: CommandSpec,
    game_root: PathBuf,
    timing: Timing,
    stop: &AtomicBool,
    sink: &dyn Fn(Event),
) {
    let log_path = game_root.join("launcher.log");
    let mut log = LogTail::at_end(&log_path);
    let mut command = spec.to_command();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_group(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            sink(Event::Failed(LaunchError::Spawn(e.to_string())));
            return;
        }
    };
    let output: Tail = Arc::default();
    if let Some(out) = child.stdout.take() {
        capture(out, output.clone());
    }
    if let Some(err) = child.stderr.take() {
        capture(err, output.clone());
    }
    let mut probe = GameProbe::new(&child, &game_root);
    sink(Event::Phase(Phase::Updating, Step::Checking));

    let mut phase = (Phase::Updating, Step::Checking);
    let mut log_lines: VecDeque<String> = VecDeque::new();
    let status = loop {
        if stop.load(Ordering::SeqCst) {
            probe.terminate(&mut child, timing.grace);
            sink(Event::Stopped);
            return;
        }
        for line in log.read_new() {
            if let Some(next) = classify(&line) {
                // The phase only moves forward.
                if rank(next) > rank(phase) {
                    phase = next;
                    sink(Event::Phase(next.0, next.1));
                }
            }
            if log_lines.len() == TAIL_LINES {
                log_lines.pop_front();
            }
            log_lines.push_back(line);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(timing.poll),
            Err(e) => {
                sink(Event::Failed(LaunchError::Spawn(e.to_string())));
                return;
            }
        }
    };
    // Anything the updater wrote just before it exited.
    for line in log.read_new() {
        if let Some(next) = classify(&line) {
            if rank(next) > rank(phase) {
                phase = next;
            }
        }
        if log_lines.len() == TAIL_LINES {
            log_lines.pop_front();
        }
        log_lines.push_back(line);
    }

    if !status.success() {
        let mut detail = tail_text(&output);
        if detail.is_empty() {
            detail = log_lines
                .iter()
                .rev()
                .take(2)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ");
        }
        sink(Event::Failed(LaunchError::UpdaterFailed {
            code: status.code(),
            detail,
        }));
        return;
    }

    // The updater exited cleanly. If it started the game, the game is in the same group and still there.
    // Give the group a moment to show up on platforms where it is found by looking.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut alive = probe.game_alive();
    while !alive
        && phase.0 == Phase::Launching
        && Instant::now() < deadline
        && !stop.load(Ordering::SeqCst)
    {
        std::thread::sleep(timing.poll);
        alive = probe.game_alive();
    }
    if !alive {
        if stop.load(Ordering::SeqCst) {
            sink(Event::Stopped);
        } else if phase.0 == Phase::Launching {
            // The game started and ended again within moments: it crashed or was closed at once.
            sink(Event::Finished);
        } else {
            sink(Event::Failed(LaunchError::NothingStarted));
        }
        return;
    }
    sink(Event::Phase(Phase::Running, Step::Checking));
    loop {
        if stop.load(Ordering::SeqCst) {
            probe.terminate(&mut child, timing.grace);
            sink(Event::Stopped);
            return;
        }
        // Short sleeps keep Stop responsive; the game is only looked for every `watch`.
        std::thread::sleep(timing.watch.min(Duration::from_millis(100)));
        if probe.due(timing.watch) && !probe.game_alive() {
            sink(Event::Finished);
            return;
        }
    }
}

fn rank(phase: (Phase, Step)) -> u8 {
    match phase {
        (Phase::Updating, Step::Checking) => 0,
        (Phase::Updating, Step::Downloading) => 1,
        (Phase::Launching, _) => 2,
        _ => 3,
    }
}

#[cfg(unix)]
fn process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // The updater leads a new group; the game it starts stays in it.
    command.process_group(0);
}

#[cfg(not(unix))]
fn process_group(_command: &mut Command) {}

/// Finds out whether the game is still running and ends it.
struct GameProbe {
    #[cfg(unix)]
    group: u32,
    #[cfg(not(unix))]
    root: PathBuf,
    last_check: Instant,
}

impl GameProbe {
    fn due(&mut self, interval: Duration) -> bool {
        if self.last_check.elapsed() >= interval {
            self.last_check = Instant::now();
            true
        } else {
            false
        }
    }
}

#[cfg(unix)]
impl GameProbe {
    fn new(child: &Child, _game_root: &Path) -> Self {
        Self {
            group: child.id(),
            last_check: Instant::now(),
        }
    }

    /// `kill -0 -PGID` succeeds while any process of the group exists.
    fn game_alive(&mut self) -> bool {
        signal_group(self.group, "-0")
    }

    /// TERM, then KILL after `grace`.
    fn terminate(&mut self, child: &mut Child, grace: Duration) {
        signal_group(self.group, "-TERM");
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            let _ = child.try_wait();
            if !signal_group(self.group, "-0") {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        signal_group(self.group, "-KILL");
        let _ = child.wait();
    }
}

#[cfg(unix)]
fn signal_group(group: u32, signal: &str) -> bool {
    Command::new("kill")
        .arg(signal)
        .arg("--")
        .arg(format!("-{group}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// Windows: the game is found by its command line (it holds the game folder's path) with PowerShell, since no
// group is available without unsafe code. NOT TESTED on Windows.
#[cfg(not(unix))]
impl GameProbe {
    fn new(_child: &Child, game_root: &Path) -> Self {
        Self {
            root: game_root.to_path_buf(),
            last_check: Instant::now(),
        }
    }

    fn pids(&self) -> Vec<u32> {
        let root = self.root.display().to_string().replace('\'', "''");
        let script = format!(
            "Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -like '*{root}*' -and $_.ProcessId -ne $PID -and $_.Name -match '^javaw?\\.exe$' }} | ForEach-Object {{ $_.ProcessId }}"
        );
        Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| l.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn game_alive(&mut self) -> bool {
        !self.pids().is_empty()
    }

    fn terminate(&mut self, child: &mut Child, _grace: Duration) {
        let _ = child.kill();
        for pid in self.pids() {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Condvar;

    /// Collects events and lets a test wait for one.
    #[derive(Default)]
    struct Events {
        seen: Mutex<Vec<Event>>,
        changed: Condvar,
    }

    impl Events {
        fn sink(self: &Arc<Self>) -> Sink {
            let this = self.clone();
            Arc::new(move |event| {
                this.seen.lock().unwrap().push(event);
                this.changed.notify_all();
            })
        }

        fn wait_for(&self, what: impl Fn(&Event) -> bool) -> Vec<Event> {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut seen = self.seen.lock().unwrap();
            loop {
                if seen.iter().any(&what) {
                    return seen.clone();
                }
                let left = deadline.saturating_duration_since(Instant::now());
                assert!(!left.is_zero(), "timed out; saw {seen:?}");
                seen = self.changed.wait_timeout(seen, left).unwrap().0;
            }
        }
    }

    fn fast() -> Timing {
        Timing {
            poll: Duration::from_millis(30),
            watch: Duration::from_millis(100),
            grace: Duration::from_secs(2),
        }
    }

    /// A game folder with a fake `java` that follows `script` (the body of a shell script run in the folder).
    fn fake(script: &str) -> (tempfile::TempDir, CommandSpec) {
        let dir = tempfile::tempdir().unwrap();
        let java = dir.path().join("java");
        std::fs::write(&java, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        let spec = CommandSpec {
            program: java,
            args: vec![],
            cwd: dir.path().to_path_buf(),
        };
        (dir, spec)
    }

    const LOG: &str = "2026/11/17 12:00:00:000 INFO";

    /// An updater that downloads, starts a "game" (a long sleep, quiet like the real one) and exits.
    fn updater_that_starts_a_game(extra: &str) -> String {
        format!(
            "echo '{LOG} Application requires update.' >> launcher.log\nsleep 0.2\n\
echo '{LOG} Running /g/java_vm/bin/java' >> launcher.log\n{extra}\nsleep 600 >/dev/null 2>&1 &\nsleep 0.2\nexit 0"
        )
    }

    #[test]
    fn follows_the_updater_into_the_running_game_and_stops_it() {
        let (dir, spec) = fake(&updater_that_starts_a_game(""));
        let events = Arc::new(Events::default());
        let handle = start(spec, dir.path().to_path_buf(), fast(), events.sink());
        let seen = events.wait_for(|e| *e == Event::Phase(Phase::Running, Step::Checking));
        let phases: Vec<_> = seen
            .iter()
            .filter_map(|e| match e {
                Event::Phase(p, s) => Some((*p, *s)),
                _ => None,
            })
            .collect();
        assert_eq!(
            phases,
            [
                (Phase::Updating, Step::Checking),
                (Phase::Updating, Step::Downloading),
                (Phase::Launching, Step::Checking),
                (Phase::Running, Step::Checking)
            ]
        );
        handle.stop();
        events.wait_for(|e| *e == Event::Stopped);
    }

    #[test]
    fn reports_when_the_game_exits_by_itself() {
        let script = format!(
            "echo '{LOG} Running x' >> launcher.log\nsleep 0.6 >/dev/null 2>&1 &\nsleep 0.1\nexit 0"
        );
        let (dir, spec) = fake(&script);
        let events = Arc::new(Events::default());
        let _handle = start(spec, dir.path().to_path_buf(), fast(), events.sink());
        let seen = events.wait_for(|e| *e == Event::Finished);
        assert!(seen.contains(&Event::Phase(Phase::Running, Step::Checking)));
    }

    #[test]
    fn a_failing_updater_is_an_error_with_its_output() {
        let (dir, spec) = fake("echo 'Unsupported class file version' >&2\nexit 3");
        let events = Arc::new(Events::default());
        let _handle = start(spec, dir.path().to_path_buf(), fast(), events.sink());
        let seen = events.wait_for(|e| matches!(e, Event::Failed(_)));
        let Some(Event::Failed(LaunchError::UpdaterFailed { code, detail })) = seen.last().cloned()
        else {
            panic!("{seen:?}")
        };
        assert_eq!(code, Some(3));
        assert!(
            detail.contains("Unsupported class file version"),
            "{detail}"
        );
    }

    #[test]
    fn an_updater_that_exits_cleanly_without_a_game_is_an_error() {
        let (dir, spec) = fake("exit 0");
        let events = Arc::new(Events::default());
        let _handle = start(spec, dir.path().to_path_buf(), fast(), events.sink());
        let seen = events.wait_for(|e| matches!(e, Event::Failed(_)));
        assert_eq!(
            seen.last(),
            Some(&Event::Failed(LaunchError::NothingStarted))
        );
    }

    #[test]
    fn a_program_that_cannot_start_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let spec = CommandSpec {
            program: dir.path().join("no-such-java"),
            args: vec![],
            cwd: dir.path().to_path_buf(),
        };
        let events = Arc::new(Events::default());
        let _handle = start(spec, dir.path().to_path_buf(), fast(), events.sink());
        let seen = events.wait_for(|e| matches!(e, Event::Failed(_)));
        assert!(matches!(
            seen.last(),
            Some(Event::Failed(LaunchError::Spawn(_)))
        ));
    }

    #[test]
    fn cancelling_during_the_update_ends_the_updater() {
        let (dir, spec) = fake(&format!(
            "echo '{LOG} Application requires update.' >> launcher.log\nsleep 600"
        ));
        let events = Arc::new(Events::default());
        let handle = start(spec, dir.path().to_path_buf(), fast(), events.sink());
        events.wait_for(|e| *e == Event::Phase(Phase::Updating, Step::Downloading));
        let before = Instant::now();
        handle.stop();
        events.wait_for(|e| *e == Event::Stopped);
        assert!(before.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn stopping_also_ends_a_game_that_ignores_the_polite_request() {
        // The "game" traps TERM, so it needs the forced kill after the grace period.
        let script = format!(
            "echo '{LOG} Running x' >> launcher.log\n(trap '' TERM; while :; do sleep 1; done) >/dev/null 2>&1 &\nsleep 0.2\nexit 0"
        );
        let (dir, spec) = fake(&script);
        let events = Arc::new(Events::default());
        let mut timing = fast();
        timing.grace = Duration::from_millis(400);
        let handle = start(spec, dir.path().to_path_buf(), timing, events.sink());
        events.wait_for(|e| *e == Event::Phase(Phase::Running, Step::Checking));
        handle.stop();
        events.wait_for(|e| *e == Event::Stopped);
    }
}
