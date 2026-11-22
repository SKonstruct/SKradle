//! The pieces of the built-in server console that need no running engine: the command history the input line
//! walks with Up and Down, and the rule that keeps a secret out of the echo and the history.
//!
//! The engine's own output is already buffered by [`super::process`] (`ServerHost::log_since`) and commands go to
//! its stdin with `ServerHost::send_console`; this module is only what sits between those and the screen.

/// Commands kept for Up/Down.
pub const HISTORY_CAP: usize = 100;
/// Longest command accepted (the engine's console reads one line at a time).
pub const MAX_COMMAND: usize = 1000;

/// True when the engine's last line asks for input (a console `INFO` line ending in `:`, or one that names a
/// password). What the player types next is then a secret: it is neither echoed nor remembered.
pub fn looks_like_prompt(last_line: &str) -> bool {
    let line = last_line.trim_end();
    line.ends_with(':') || line.to_ascii_lowercase().contains("password")
}

/// One command as the console shows it back: `> command`, or a mask when it answers a prompt.
pub fn echo(command: &str, secret: bool) -> String {
    if secret {
        "> \u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}".to_owned()
    } else {
        format!("> {command}")
    }
}

/// Cleans a typed command for sending: no line breaks or control characters, trimmed, bounded. `None` when nothing
/// is left to send.
pub fn clean_command(text: &str) -> Option<String> {
    let cleaned: String = text.chars().filter(|c| !c.is_control()).collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.chars().take(MAX_COMMAND).collect())
}

/// Previously sent commands, walked with Up (older) and Down (newer). The line being typed is kept while walking
/// and comes back at the bottom.
#[derive(Debug, Default)]
pub struct History {
    items: Vec<String>,
    /// Where the walk is: an index into `items`, or `None` when not walking.
    at: Option<usize>,
    draft: String,
}

impl History {
    /// Remembers a sent command (not a repeat of the last one) and ends any walk.
    pub fn push(&mut self, command: &str) {
        self.at = None;
        self.draft.clear();
        if self.items.last().map(String::as_str) == Some(command) {
            return;
        }
        if self.items.len() == HISTORY_CAP {
            self.items.remove(0);
        }
        self.items.push(command.to_owned());
    }

    /// Up: the next older command, given what is typed now. `None` at the oldest (keep what is shown).
    pub fn older(&mut self, current: &str) -> Option<String> {
        let index = match self.at {
            None => {
                if self.items.is_empty() {
                    return None;
                }
                self.draft = current.to_owned();
                self.items.len() - 1
            }
            Some(0) => return None,
            Some(i) => i - 1,
        };
        self.at = Some(index);
        Some(self.items[index].clone())
    }

    /// Down: the next newer command, or the line that was being typed after the newest. `None` when not walking.
    pub fn newer(&mut self) -> Option<String> {
        let i = self.at?;
        if i + 1 < self.items.len() {
            self.at = Some(i + 1);
            Some(self.items[i + 1].clone())
        } else {
            self.at = None;
            Some(std::mem::take(&mut self.draft))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_are_recognised_by_shape() {
        assert!(looks_like_prompt("[10:00:01] [ServerConsole/INFO]: Password for skradle:"));
        assert!(looks_like_prompt("Enter new value: "));
        assert!(looks_like_prompt("Retype PASSWORD please"));
        assert!(!looks_like_prompt("[10:00:02] [ServerConsole/INFO]: WebUI account saved: skradle"));
        assert!(!looks_like_prompt(""));
    }

    #[test]
    fn a_secret_is_masked_in_the_echo() {
        assert_eq!(echo("list", false), "> list");
        let masked = echo("hunter2hunter2", true);
        assert!(!masked.contains("hunter"));
        assert!(masked.starts_with("> "));
    }

    #[test]
    fn commands_are_cleaned_before_sending() {
        assert_eq!(clean_command("  list \r\n"), Some("list".into()));
        assert_eq!(clean_command("a\nshutdown"), Some("ashutdown".into()));
        assert_eq!(clean_command("\u{0}\t "), None);
        assert_eq!(clean_command("").is_none(), true);
        assert_eq!(clean_command(&"x".repeat(5000)).unwrap().len(), MAX_COMMAND);
    }

    #[test]
    fn history_walks_up_and_down_and_returns_the_draft() {
        let mut h = History::default();
        assert_eq!(h.older("x"), None, "nothing to walk");
        h.push("one");
        h.push("two");
        h.push("two");
        assert_eq!(h.older("dra"), Some("two".into()));
        assert_eq!(h.older("two"), Some("one".into()));
        assert_eq!(h.older("one"), None, "the oldest stays");
        assert_eq!(h.newer(), Some("two".into()));
        assert_eq!(h.newer(), Some("dra".into()), "the draft comes back");
        assert_eq!(h.newer(), None, "not walking any more");
    }

    #[test]
    fn history_is_bounded_and_a_send_ends_the_walk() {
        let mut h = History::default();
        for n in 0..(HISTORY_CAP + 5) {
            h.push(&format!("c{n}"));
        }
        assert_eq!(h.items.len(), HISTORY_CAP);
        assert_eq!(h.items[0], "c5");
        assert_eq!(h.older(""), Some(format!("c{}", HISTORY_CAP + 4)));
        h.push("fresh");
        assert_eq!(h.newer(), None);
        assert_eq!(h.older(""), Some("fresh".into()));
    }
}
