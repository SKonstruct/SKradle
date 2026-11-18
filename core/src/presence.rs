//! Discord Rich Presence: what the launcher tells Discord it is doing.
//!
//! Presence is optional and can never affect the launcher. [`from_client_id`] gives a no-op ([`NoPresence`]) unless a
//! Discord application id is configured; SKradle does not ship one, the owner creates a Discord application and puts
//! its id in the configuration. Every method returns a [`PresenceResult`] instead of an error, so a caller can log it
//! and carry on; nothing panics and nothing blocks for long (the socket has short timeouts on Unix; on Windows a named
//! pipe cannot be given one, so run presence on its own thread, which the `Send` bound allows).
//!
//! Limits (checked against Discord's current developer documentation, docs.discord.com, "Activity Structure" and
//! "Activity Buttons"): button labels are 1 to 32 characters; the page states no length for `state`, `details` or
//! image text, so this module caps text at 128 characters, the long-standing figure of the Rich Presence documentation,
//! and sends at least 2 characters because Discord has rejected single-character values. Treat both as conservative
//! assumptions, not citations. The same documentation lists timestamps in milliseconds for the gateway activity
//! object, while RPC clients have always sent seconds over the local IPC; this module sends seconds and that is
//! UNVERIFIED against a running Discord.

mod discord;

pub use discord::DiscordPresence;

/// Longest `details`/`state` text sent, in characters.
pub const MAX_TEXT_CHARS: usize = 128;
/// Shortest text sent; shorter values are padded.
pub const MIN_TEXT_CHARS: usize = 2;

/// What the launcher is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Launching,
    Playing,
}

/// The status the launcher reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    /// The game server or world the player is on, when known.
    pub server: Option<String>,
    /// When the state began, in seconds since the Unix epoch; Discord shows an elapsed timer from it.
    pub started_at: Option<u64>,
}

impl Status {
    pub fn new(state: State) -> Self {
        Self {
            state,
            server: None,
            started_at: None,
        }
    }
}

/// The text and timer Discord shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub details: String,
    pub state: Option<String>,
    pub start: Option<u64>,
}

/// Removes control characters, collapses whitespace, cuts to `max` characters (adding `...`) and pads to
/// [`MIN_TEXT_CHARS`]. Never returns an empty string.
pub fn clamp_text(text: &str, max: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = if cleaned.chars().count() > max {
        let keep = max.saturating_sub(3);
        let mut cut: String = cleaned.chars().take(keep).collect();
        cut.push_str("...");
        cut
    } else {
        cleaned
    };
    while out.chars().count() < MIN_TEXT_CHARS {
        out.push('.');
    }
    out
}

/// The activity for a status.
pub fn activity(status: &Status) -> Activity {
    let details = match status.state {
        State::Idle => "In the launcher",
        State::Launching => "Starting Spiral Knights",
        State::Playing => "Playing Spiral Knights",
    };
    let state = match (&status.state, status.server.as_deref().map(str::trim)) {
        (State::Playing, Some(server)) if !server.is_empty() => {
            Some(clamp_text(&format!("on {server}"), MAX_TEXT_CHARS))
        }
        _ => None,
    };
    Activity {
        details: clamp_text(details, MAX_TEXT_CHARS),
        state,
        start: match status.state {
            State::Idle => None,
            _ => status.started_at,
        },
    }
}

/// What an update did. Meant to be logged, never to be acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresenceResult {
    Updated,
    /// Presence is off (no application id) or Discord is not running.
    Disabled,
    /// Discord answered with an error or the connection broke; the next call reconnects.
    Failed(String),
}

pub trait Presence: Send {
    fn update(&mut self, status: &Status) -> PresenceResult;
    fn clear(&mut self) -> PresenceResult;
}

/// Presence that does nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPresence;

impl Presence for NoPresence {
    fn update(&mut self, _: &Status) -> PresenceResult {
        PresenceResult::Disabled
    }
    fn clear(&mut self) -> PresenceResult {
        PresenceResult::Disabled
    }
}

/// A Discord application id is a decimal "snowflake" of 17 to 20 digits.
pub fn is_valid_client_id(id: &str) -> bool {
    (17..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

/// Real presence when `client_id` is a valid application id, [`NoPresence`] when it is absent, empty or malformed.
pub fn from_client_id(client_id: Option<&str>) -> Box<dyn Presence> {
    match client_id.map(str::trim) {
        Some(id) if is_valid_client_id(id) => Box::new(DiscordPresence::new(id)),
        _ => Box::new(NoPresence),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_launching_and_playing_text() {
        let idle = activity(&Status::new(State::Idle));
        assert_eq!(idle.details, "In the launcher");
        assert_eq!((idle.state, idle.start), (None, None));

        let launching = activity(&Status {
            started_at: Some(5),
            ..Status::new(State::Launching)
        });
        assert_eq!(launching.details, "Starting Spiral Knights");
        assert_eq!(launching.start, Some(5));

        let playing = activity(&Status {
            state: State::Playing,
            server: Some("  Emberlight ".into()),
            started_at: Some(9),
        });
        assert_eq!(playing.details, "Playing Spiral Knights");
        assert_eq!(playing.state.as_deref(), Some("on Emberlight"));
        assert_eq!(playing.start, Some(9));
    }

    #[test]
    fn idle_never_shows_a_timer_or_server() {
        let a = activity(&Status {
            state: State::Idle,
            server: Some("x".into()),
            started_at: Some(1),
        });
        assert_eq!((a.state, a.start), (None, None));
    }

    #[test]
    fn blank_server_gives_no_state_line() {
        let a = activity(&Status {
            state: State::Playing,
            server: Some("   ".into()),
            started_at: None,
        });
        assert_eq!(a.state, None);
    }

    #[test]
    fn long_text_is_cut_to_the_limit() {
        let out = clamp_text(&"x".repeat(500), MAX_TEXT_CHARS);
        assert_eq!(out.chars().count(), MAX_TEXT_CHARS);
        assert!(out.ends_with("..."));
        let a = activity(&Status {
            state: State::Playing,
            server: Some("é".repeat(400)),
            started_at: None,
        });
        assert_eq!(a.state.unwrap().chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn text_is_cleaned_and_never_too_short() {
        assert_eq!(clamp_text("a\n\tb   c\u{0007}d", 50), "a b c d");
        assert_eq!(clamp_text("", 50), "..");
        assert_eq!(clamp_text("x", 50), "x.");
        assert_eq!(clamp_text("  \n ", 50), "..");
    }

    #[test]
    fn client_ids_are_validated() {
        assert!(is_valid_client_id("123456789012345678"));
        assert!(!is_valid_client_id(""));
        assert!(!is_valid_client_id("1234"));
        assert!(!is_valid_client_id("12345678901234567x"));
        assert!(!is_valid_client_id(&"1".repeat(21)));
    }

    #[test]
    fn no_application_id_means_no_presence() {
        for id in [None, Some(""), Some("  "), Some("not-an-id")] {
            let mut presence = from_client_id(id);
            assert_eq!(
                presence.update(&Status::new(State::Idle)),
                PresenceResult::Disabled
            );
            assert_eq!(presence.clear(), PresenceResult::Disabled);
        }
    }
}
