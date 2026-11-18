//! The part of the engine's console protocol the launcher needs: telling its own lines apart from everything else
//! the process prints, and answering the prompts of the few commands it sends (the confirmed shutdown and the
//! WebUI account commands). Account, knight and item administration is the WebUI's job, not the launcher's.
//!
//! **Protocol facts** (read from the engine's console source, v0.40.x; the transcripts in the tests are written
//! from that source, not recorded from a run):
//!
//! * With the child's stdin and stdout redirected (and a Java where `System.console()` is null for a pipe, as in
//!   Java 25) the console reads plain lines, and every message it prints is a log line
//!   `[HH:mm:ss] [<thread>/<LEVEL>]: <text>`. Commands print from the thread named `ServerConsole`; refusals are
//!   `WARN`.
//! * A password or confirmation prompt is printed as a log line ending in `:` (`Password:`-style text), then the
//!   console reads the next stdin line as the answer. The answer is never echoed or logged.
//! * `shutdown` prints `Type SHUT DOWN PROCESS to confirm full process shutdown:` and reads one line.
//! * `webui user <name> <ROLE>` and `webui password <name>` ask for the password and its confirmation, then print
//!   `WebUI account saved: <name>; existing sessions revoked.` or `WebUI password changed: <name>; ...`.
//!
//! A prompt is recognised by shape (a `ServerConsole` INFO line ending in `:`), not by its wording, so a reworded
//! prompt still works; a prompt that never arrives ends in a timeout in the caller.

use std::fmt;

/// A line the console printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleLine<'a> {
    pub thread: &'a str,
    pub level: &'a str,
    pub text: &'a str,
}

/// Splits `[HH:mm:ss] [thread/LEVEL]: text`, or `None` for any other line (stack traces, banners, ...).
pub fn parse_log_line(line: &str) -> Option<ConsoleLine<'_>> {
    let rest = line.strip_prefix('[')?;
    let (time, rest) = rest.split_once("] [")?;
    if time.len() != 8
        || !time.bytes().enumerate().all(|(i, b)| {
            if i == 2 || i == 5 {
                b == b':'
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return None;
    }
    let (tag, text) = rest.split_once("]: ")?;
    let (thread, level) = tag.rsplit_once('/')?;
    (!thread.is_empty() && matches!(level, "INFO" | "WARN" | "ERROR" | "DEBUG")).then_some(
        ConsoleLine {
            thread,
            level,
            text,
        },
    )
}

/// The thread the console prints command output from.
pub const CONSOLE_THREAD: &str = "ServerConsole";

/// A string that never shows its content in `Debug`, so a password cannot reach a log through `{:?}`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The text, for writing it to the child's stdin and nowhere else.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// How a script ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// All prompts were answered and (when the script has a success line) it appeared.
    Done,
    /// The console refused: the text of its `WARN` line.
    Refused(String),
}

/// What to do after a line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Step {
    /// A line to send to the child's stdin, if any.
    pub reply: Option<Secret>,
    pub outcome: Option<Outcome>,
}

/// A command, the answers to its prompts in order, and the line that confirms success. A pure state machine:
/// the caller sends [`PromptScript::command`], feeds every line the process prints to
/// [`PromptScript::on_line`] and writes any reply it returns. Timeouts are the caller's.
#[derive(Debug, Clone)]
pub struct PromptScript {
    command: String,
    answers: Vec<Secret>,
    next: usize,
    /// The text a success line starts with; `None` when the command has no success line (the process ending is
    /// the success) and the script is done once the last answer is sent.
    success_prefix: Option<String>,
    finished: bool,
}

impl PromptScript {
    fn new(command: String, answers: Vec<Secret>, success_prefix: Option<String>) -> Self {
        Self {
            command,
            answers,
            next: 0,
            success_prefix,
            finished: false,
        }
    }

    /// The confirmed process shutdown.
    pub fn shutdown() -> Self {
        Self::new(
            "shutdown".to_owned(),
            vec![Secret::new("SHUT DOWN PROCESS")],
            None,
        )
    }

    /// Creates the WebUI account or replaces its password and role.
    pub fn webui_user(username: &str, password: &Secret) -> Self {
        Self::new(
            format!("webui user {username} ADMINISTRATOR"),
            vec![password.clone(), password.clone()],
            Some(format!("WebUI account saved: {username}")),
        )
    }

    /// Resets the password of an existing WebUI account (refused for one that does not exist).
    pub fn webui_password(username: &str, password: &Secret) -> Self {
        Self::new(
            format!("webui password {username}"),
            vec![password.clone(), password.clone()],
            Some(format!("WebUI password changed: {username}")),
        )
    }

    /// The command line to send first (without a newline).
    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Feeds one line the process printed.
    pub fn on_line(&mut self, line: &str) -> Step {
        let mut step = Step::default();
        if self.finished {
            return step;
        }
        let Some(parsed) = parse_log_line(line) else {
            return step;
        };
        if parsed.thread != CONSOLE_THREAD {
            return step;
        }
        if parsed.level == "WARN" || parsed.level == "ERROR" {
            self.finished = true;
            step.outcome = Some(Outcome::Refused(parsed.text.to_owned()));
            return step;
        }
        if self.next < self.answers.len() {
            if parsed.text.ends_with(':') {
                step.reply = Some(self.answers[self.next].clone());
                self.next += 1;
                if self.next == self.answers.len() && self.success_prefix.is_none() {
                    self.finished = true;
                    step.outcome = Some(Outcome::Done);
                }
            }
            return step;
        }
        if self
            .success_prefix
            .as_deref()
            .is_some_and(|p| parsed.text.starts_with(p))
        {
            self.finished = true;
            step.outcome = Some(Outcome::Done);
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(script: &mut PromptScript, lines: &[&str]) -> Vec<Step> {
        lines.iter().map(|l| script.on_line(l)).collect()
    }

    #[test]
    fn parses_log_lines_and_ignores_the_rest() {
        let line =
            parse_log_line("[12:01:59] [ServerConsole/INFO]: Account created: Alice").unwrap();
        assert_eq!(
            line,
            ConsoleLine {
                thread: "ServerConsole",
                level: "INFO",
                text: "Account created: Alice"
            }
        );
        assert_eq!(
            parse_log_line("[00:00:01] [Client-3/WARN]: x")
                .unwrap()
                .thread,
            "Client-3"
        );
        for other in [
            "",
            "Exception in thread \"main\"",
            "\tat org.tortodrone.server.Server.main(Server.java:1)",
            "[12:01] [ServerConsole/INFO]: short time",
            "[12:01:59] [ServerConsole/NOTICE]: odd level",
            "[ab:cd:ef] [ServerConsole/INFO]: not digits",
            "[12:01:59] ServerConsole/INFO: no brackets",
        ] {
            assert_eq!(parse_log_line(other), None, "{other:?}");
        }
    }

    #[test]
    fn secrets_do_not_print() {
        let s = Secret::new("hunter2hunter2");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        let script = PromptScript::webui_user("skradle", &s);
        assert!(!format!("{script:?}").contains("hunter2"));
    }

    #[test]
    fn webui_user_answers_both_prompts_then_waits_for_the_saved_line() {
        let secret = Secret::new("a-long-generated-pass");
        let mut script = PromptScript::webui_user("skradle", &secret);
        assert_eq!(script.command(), "webui user skradle ADMINISTRATOR");
        let steps = feed(
            &mut script,
            &[
                "[10:00:00] [Main/INFO]: Server ready (1.234s)",
                "[10:00:01] [Client-1/INFO]: Something: with a colon:",
                "[10:00:02] [ServerConsole/INFO]: WebUI password (8–128 characters):",
                "[10:00:02] [ServerConsole/INFO]: Confirm WebUI password:",
                "[10:00:03] [Client-1/INFO]: unrelated",
                "[10:00:03] [ServerConsole/INFO]: WebUI account saved: skradle; existing sessions revoked.",
            ],
        );
        assert_eq!(steps[0], Step::default());
        assert_eq!(steps[1], Step::default(), "other threads never count");
        assert_eq!(steps[2].reply, Some(secret.clone()));
        assert_eq!(steps[3].reply, Some(secret));
        assert_eq!(steps[3].outcome, None);
        assert_eq!(steps[4], Step::default());
        assert_eq!(steps[5].outcome, Some(Outcome::Done));
        assert!(script.is_finished());
        assert_eq!(
            script.on_line("[10:00:04] [ServerConsole/WARN]: late"),
            Step::default()
        );
    }

    #[test]
    fn a_refusal_ends_the_script_at_once() {
        let mut script = PromptScript::webui_user("ab", &Secret::new("password1"));
        let step = script.on_line(
            "[10:00:00] [ServerConsole/WARN]: WebUI username needs 3–64 letters, digits, underscores or hyphens",
        );
        assert_eq!(
            step.outcome,
            Some(Outcome::Refused(
                "WebUI username needs 3–64 letters, digits, underscores or hyphens".into()
            ))
        );
        assert_eq!(step.reply, None);
    }

    #[test]
    fn a_mismatch_after_the_answers_is_a_refusal() {
        let mut script = PromptScript::webui_password("skradle", &Secret::new("password1"));
        assert_eq!(script.command(), "webui password skradle");
        let steps = feed(
            &mut script,
            &[
                "[10:00:00] [ServerConsole/INFO]: WebUI password (8–128 characters):",
                "[10:00:00] [ServerConsole/INFO]: Confirm WebUI password:",
                "[10:00:00] [ServerConsole/WARN]: Passwords do not match",
            ],
        );
        assert!(steps[0].reply.is_some() && steps[1].reply.is_some());
        assert_eq!(
            steps[2].outcome,
            Some(Outcome::Refused("Passwords do not match".into()))
        );
    }

    #[test]
    fn shutdown_confirms_with_the_exact_phrase() {
        let mut script = PromptScript::shutdown();
        assert_eq!(script.command(), "shutdown");
        let step = script.on_line(
            "[10:00:00] [ServerConsole/INFO]: Type SHUT DOWN PROCESS to confirm full process shutdown:",
        );
        assert_eq!(
            step.reply.as_ref().map(Secret::expose),
            Some("SHUT DOWN PROCESS")
        );
        assert_eq!(step.outcome, Some(Outcome::Done));
    }

    #[test]
    fn the_shutdown_refusal_is_reported() {
        let mut script = PromptScript::shutdown();
        let step = script.on_line("[10:00:00] [ServerConsole/WARN]: Lifecycle control unavailable");
        assert!(matches!(step.outcome, Some(Outcome::Refused(_))));
    }
}
