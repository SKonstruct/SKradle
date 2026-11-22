//! Running the updater and following it, and then the game it starts.
//!
//! A thread starts the updater, reads its `launcher.log` for phase hints, notices when it exits, and then watches
//! the game until it is gone. Everything is reported through an event callback; nothing here touches Qt.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use skradle_core::getdown::CommandSpec;
use skradle_core::launch::plan::LaunchKind;

use super::log::{classify, LogTail};
use super::{Event, LaunchError, Phase, Running, Sink, Step};

/// How often things are checked.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// While the updater runs: its exit and its log.
    pub poll: Duration,
    /// While the game runs: whether it is still there.
    pub watch: Duration,
    /// After asking politely to stop, how long to wait before forcing.
    pub grace: Duration,
    /// A game started directly counts as running once it has lived this long.
    pub settle: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(200),
            watch: Duration::from_millis(1000),
            grace: Duration::from_secs(5),
            settle: Duration::from_secs(3),
        }
    }
}

/// A launch in progress.
pub struct Handle {
    stop: Arc<AtomicBool>,
}

impl Running for Handle {
    /// Asks the launch to stop: the updater and the game are terminated. The supervisor reports `Stopped`.
    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// What the preparation step decided: the command to run and whether it is the updater or the game itself.
pub type Prepared = (CommandSpec, LaunchKind);

/// What the preparation step can use: whether the player cancelled, and a way to report where it is (for example
/// "waiting for Steam").
pub struct Context<'a> {
    pub stop: &'a AtomicBool,
    pub sink: &'a dyn Fn(Event),
}

impl Context<'_> {
    pub fn cancelled(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

/// The work before a process starts (starting Steam, applying mods, pointing the game at a server). It runs on the
/// supervisor's thread, so the window never waits for it, and a failure is reported like any other.
pub type Prepare = Box<dyn FnOnce(&Context<'_>) -> Result<Prepared, LaunchError> + Send>;

/// Starts the supervisor thread; `prepare` decides what to run (and may do work first). Errors (including failing
/// to start the process) come through `sink`.
pub fn start_prepared(prepare: Prepare, game_root: PathBuf, timing: Timing, sink: Sink) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let handle = Handle { stop: stop.clone() };
    let spawned = std::thread::Builder::new()
        .name("skradle-launch".into())
        .spawn({
            let sink = sink.clone();
            move || run(prepare, game_root, timing, &stop, &*sink)
        });
    if let Err(e) = spawned {
        sink(Event::Failed(LaunchError::Spawn(e.to_string())));
    }
    handle
}

const TAIL_LINES: usize = 12;

type Tail = Arc<Mutex<VecDeque<String>>>;

/// Reads a stream into `tail` on its own thread. The receiver gets a message when the stream ends, so the caller can
/// wait for the last lines of an updater that failed (but not forever: a game the updater started keeps the pipe).
fn capture(stream: impl std::io::Read + Send + 'static, tail: Tail) -> mpsc::Receiver<()> {
    let (done, finished) = mpsc::channel();
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
            let _ = done.send(());
        });
    finished
}

/// Waits (briefly) until the captured streams have ended, so the failure message is complete.
fn drain(finished: &[mpsc::Receiver<()>]) {
    let deadline = Instant::now() + Duration::from_millis(750);
    for rx in finished {
        let left = deadline.saturating_duration_since(Instant::now());
        let _ = rx.recv_timeout(left);
    }
}

fn exit_text(status: &std::process::ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| "no exit code".to_owned(), |c| format!("exit code {c}"))
}

/// Writes the end of the game folder's own `launcher.log` to SKradle's log, for a launch that failed early.
fn log_game_log_tail(game_root: &Path) {
    let lines = skradle_core::log::tail_of(&game_root.join("launcher.log"), 15);
    if !lines.is_empty() {
        skradle_core::log::warn(
            "launch",
            format!("end of the game's launcher.log:\n{}", lines.join("\n")),
        );
    }
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
    prepare: Prepare,
    game_root: PathBuf,
    timing: Timing,
    stop: &AtomicBool,
    sink: &dyn Fn(Event),
) {
    // The window shows work from the first moment, also while mods are applied.
    sink(Event::Phase(Phase::Launching, Step::Preparing));
    let (spec, kind) = match prepare(&Context { stop, sink }) {
        Ok(prepared) => prepared,
        Err(error) => {
            if stop.load(Ordering::SeqCst) {
                sink(Event::Stopped);
            } else {
                skradle_core::log::error(
                    "launch",
                    format!("launch failed before the game started: {}", error.message()),
                );
                sink(Event::Failed(error));
            }
            return;
        }
    };
    if stop.load(Ordering::SeqCst) {
        sink(Event::Stopped);
        return;
    }
    let log_path = game_root.join("launcher.log");
    let mut log = LogTail::at_end(&log_path);
    let mut command = spec.to_command();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_group(&mut command);
    skradle_core::log::info(
        "launch",
        format!(
            "starting {} ({}): program {}, {} arguments, {} variables set ({}), working folder {}",
            if kind == LaunchKind::Direct {
                "the game"
            } else {
                "the updater"
            },
            if kind == LaunchKind::Direct {
                "direct"
            } else {
                "updater"
            },
            spec.program.display(),
            spec.args.len(),
            spec.env.len(),
            spec.env
                .iter()
                .map(|(k, _)| k.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(", "),
            spec.cwd.display()
        ),
    );
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            skradle_core::log::error("launch", format!("could not start the process: {e}"));
            sink(Event::Failed(LaunchError::Spawn(e.to_string())));
            return;
        }
    };
    let output: Tail = Arc::default();
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(capture(out, output.clone()));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(capture(err, output.clone()));
    }
    let mut probe = GameProbe::new(&child, &game_root);
    if kind == LaunchKind::Direct {
        follow_game(child, probe, &output, &readers, timing, stop, sink);
        return;
    }

    let mut phase = (Phase::Updating, Step::Downloading);
    sink(Event::Phase(phase.0, phase.1));
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
                // Cannot tell what the updater is doing: end it rather than leave it running.
                probe.terminate(&mut child, timing.grace);
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

    skradle_core::log::info(
        "launch",
        format!("the updater exited ({})", exit_text(&status)),
    );
    if !status.success() {
        drain(&readers);
        log_game_log_tail(&game_root);
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
    if stop.load(Ordering::SeqCst) {
        // Stop was asked for while the updater handed over: the game may be just starting, so end the group
        // whether or not it has shown up yet.
        probe.terminate(&mut child, timing.grace);
        sink(Event::Stopped);
        return;
    }
    if !alive {
        if phase.0 == Phase::Launching {
            // The game started and ended again within moments: it crashed or was closed at once.
            sink(Event::Finished);
        } else {
            sink(Event::Failed(LaunchError::NothingStarted));
        }
        return;
    }
    sink(Event::Phase(Phase::Running, Step::Preparing));
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

/// A game that was started directly is the process itself: it is "launching" until it has lived a moment, then
/// "running", and its end is the end of the launch. A non-zero exit shortly after the start is a failed start and
/// carries the end of the game's output.
fn follow_game(
    mut child: Child,
    mut probe: GameProbe,
    output: &Tail,
    readers: &[mpsc::Receiver<()>],
    timing: Timing,
    stop: &AtomicBool,
    sink: &dyn Fn(Event),
) {
    const EARLY: Duration = Duration::from_secs(20);
    sink(Event::Phase(Phase::Launching, Step::Preparing));
    let started = Instant::now();
    let mut running = false;
    loop {
        if stop.load(Ordering::SeqCst) {
            probe.terminate(&mut child, timing.grace);
            sink(Event::Stopped);
            return;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                skradle_core::log::info(
                    "launch",
                    format!(
                        "the game exited ({}) after {} s",
                        exit_text(&status),
                        started.elapsed().as_secs()
                    ),
                );
                if status.success() || started.elapsed() > EARLY {
                    sink(Event::Finished);
                } else {
                    drain(readers);
                    log_game_log_tail(probe.root());
                    skradle_core::log::error(
                        "launch",
                        format!("the game's output ended with: {}", tail_text(output)),
                    );
                    sink(Event::Failed(LaunchError::GameFailed {
                        code: status.code(),
                        detail: tail_text(output),
                    }));
                }
                return;
            }
            Ok(None) => {}
            Err(e) => {
                probe.terminate(&mut child, timing.grace);
                sink(Event::Failed(LaunchError::Spawn(e.to_string())));
                return;
            }
        }
        if !running && started.elapsed() >= timing.settle {
            running = true;
            sink(Event::Phase(Phase::Running, Step::Preparing));
        }
        std::thread::sleep(timing.poll);
    }
}

fn rank(phase: (Phase, Step)) -> u8 {
    match phase {
        (Phase::Updating, _) => 1,
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

#[cfg(windows)]
fn process_group(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // The game and the updater are started by a program without a console: without this flag Windows opens a
    // console window for `java.exe`. Their output is captured through pipes, so nothing is lost.
    command.creation_flags(super::winprobe::CREATE_NO_WINDOW);
}

#[cfg(all(not(unix), not(windows)))]
fn process_group(_command: &mut Command) {}

/// Finds out whether the game is still running and ends it.
struct GameProbe {
    root_dir: PathBuf,
    #[cfg(unix)]
    group: u32,
    #[cfg(not(unix))]
    root: PathBuf,
    last_check: Instant,
}

impl GameProbe {
    fn root(&self) -> &Path {
        &self.root_dir
    }

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
    fn new(child: &Child, game_root: &Path) -> Self {
        Self {
            root_dir: game_root.to_path_buf(),
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
// group is available without unsafe code. The commands are built in `winprobe` (tested on every platform); this
// part only runs them, hidden. NOT TESTED on Windows.
#[cfg(not(unix))]
fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(super::winprobe::CREATE_NO_WINDOW);
    }
    command
}

#[cfg(not(unix))]
impl GameProbe {
    fn new(_child: &Child, game_root: &Path) -> Self {
        Self {
            root_dir: game_root.to_path_buf(),
            root: game_root.to_path_buf(),
            last_check: Instant::now(),
        }
    }

    fn pids(&self) -> Vec<u32> {
        hidden(
            Command::new("powershell")
                .args(super::winprobe::probe_args())
                .env(
                    super::winprobe::ROOT_VARIABLE,
                    super::winprobe::root_text(&self.root),
                ),
        )
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| super::winprobe::parse_pids(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
    }

    fn game_alive(&mut self) -> bool {
        !self.pids().is_empty()
    }

    fn terminate(&mut self, child: &mut Child, _grace: Duration) {
        let _ = child.kill();
        for pid in self.pids() {
            let _ = hidden(Command::new("taskkill").args(super::winprobe::taskkill_args(pid)))
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

    /// Runs `spec` as the game's updater.
    fn start(spec: CommandSpec, game_root: PathBuf, timing: Timing, sink: Sink) -> Handle {
        start_prepared(
            Box::new(move |_| Ok((spec, LaunchKind::Updater))),
            game_root,
            timing,
            sink,
        )
    }

    fn fast() -> Timing {
        Timing {
            poll: Duration::from_millis(30),
            watch: Duration::from_millis(100),
            grace: Duration::from_secs(2),
            settle: Duration::from_millis(150),
        }
    }

    /// A game folder with a fake `java` that follows `script` (the body of a shell script run in the folder).
    fn fake(script: &str) -> (tempfile::TempDir, CommandSpec) {
        let dir = tempfile::tempdir().unwrap();
        let java = dir.path().join("java");
        std::fs::write(&java, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Run it through `sh` rather than executing the file: another test forking at the same
        // moment can briefly hold this file's write descriptor, and exec then fails with
        // "Text file busy".
        let spec = CommandSpec {
            program: "/bin/sh".into(),
            args: vec![java.into_os_string()],
            cwd: dir.path().to_path_buf(),
            env: Vec::new(),
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
        let seen = events.wait_for(|e| *e == Event::Phase(Phase::Running, Step::Preparing));
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
                (Phase::Launching, Step::Preparing),
                (Phase::Updating, Step::Downloading),
                (Phase::Launching, Step::Preparing),
                (Phase::Running, Step::Preparing)
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
        assert!(seen.contains(&Event::Phase(Phase::Running, Step::Preparing)));
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
            env: Vec::new(),
        };
        let events = Arc::new(Events::default());
        let _handle = start(spec, dir.path().to_path_buf(), fast(), events.sink());
        let seen = events.wait_for(|e| matches!(e, Event::Failed(_)));
        assert!(matches!(
            seen.last(),
            Some(Event::Failed(LaunchError::Spawn(_)))
        ));
    }

    fn start_direct(spec: CommandSpec, dir: &Path, events: &Arc<Events>) -> Handle {
        start_prepared(
            Box::new(move |_| Ok((spec, LaunchKind::Direct))),
            dir.to_path_buf(),
            fast(),
            events.sink(),
        )
    }

    #[test]
    fn a_directly_started_game_goes_from_launching_to_running_and_can_be_stopped() {
        let (dir, spec) = fake("sleep 600");
        let events = Arc::new(Events::default());
        let handle = start_direct(spec, dir.path(), &events);
        let seen = events.wait_for(|e| matches!(e, Event::Phase(Phase::Running, _)));
        let phases: Vec<_> = seen
            .iter()
            .filter_map(|e| match e {
                Event::Phase(p, _) => Some(*p),
                _ => None,
            })
            .collect();
        assert_eq!(
            phases,
            [Phase::Launching, Phase::Launching, Phase::Running],
            "{seen:?}"
        );
        handle.stop();
        events.wait_for(|e| matches!(e, Event::Stopped));
    }

    #[test]
    fn a_directly_started_game_that_ends_normally_finishes_the_launch() {
        let (dir, spec) = fake("sleep 0.4");
        let events = Arc::new(Events::default());
        let _handle = start_direct(spec, dir.path(), &events);
        let seen = events.wait_for(|e| matches!(e, Event::Finished));
        assert!(seen.contains(&Event::Phase(Phase::Running, Step::Preparing)));
    }

    #[test]
    fn a_directly_started_game_that_fails_at_once_reports_its_output() {
        let (dir, spec) = fake("echo 'Unrecognized option: -javaagent:x' >&2\nexit 1");
        let events = Arc::new(Events::default());
        let _handle = start_direct(spec, dir.path(), &events);
        let seen = events.wait_for(|e| matches!(e, Event::Failed(_) | Event::Finished));
        match seen.last() {
            Some(Event::Failed(LaunchError::GameFailed { code, detail })) => {
                assert_eq!(*code, Some(1));
                assert!(detail.contains("Unrecognized option"), "{detail}");
            }
            other => panic!("expected a failed start, saw {other:?}"),
        }
    }

    #[test]
    fn a_failed_preparation_is_reported_and_nothing_is_started() {
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Events::default());
        let _handle = start_prepared(
            Box::new(|_| Err(LaunchError::Prepare("config.jar is locked".into()))),
            dir.path().to_path_buf(),
            fast(),
            events.sink(),
        );
        let seen = events.wait_for(|e| matches!(e, Event::Failed(_)));
        assert_eq!(
            seen.last(),
            Some(&Event::Failed(LaunchError::Prepare(
                "config.jar is locked".into()
            )))
        );
        assert!(!seen.iter().any(|e| matches!(
            e,
            Event::Phase(Phase::Running, _) | Event::Phase(_, Step::Downloading)
        )));
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
        events.wait_for(|e| *e == Event::Phase(Phase::Running, Step::Preparing));
        handle.stop();
        events.wait_for(|e| *e == Event::Stopped);
    }
}
