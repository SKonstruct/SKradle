//! Alt-account mode: a second, lighter game client started next to the main one, using the direct launch.
//!
//! What it changes compared with a normal direct launch: the heap is capped (default 256 MiB initial, 1 GiB maximum)
//! replacing the player's `-Xms`/`-Xmx`, and the graphics agent's `extra.txt` lines are left out so the second client
//! does not scale or capture its window. Nothing else is touched; the player's other `extra.txt` lines stay.
//!
//! **What stops a second client from starting** (read-only inspection, nothing was run):
//!
//! * The updater: `Application.lockForUpdates` takes an exclusive `FileLock` on `gettingdown.lock` in the game folder
//!   and the updater throws `MultipleGetdownRunning` ("another getdown running") when it cannot. The lock is held
//!   while the updater process lives. Whether a second updater run is refused therefore depends on how long the
//!   first updater lingers after starting the game; not verified. The direct launch never touches the lock.
//! * The game itself: no class of `projectx-pcode.jar` uses `FileLock`, `ServerSocket` or `ServerSocketChannel`,
//!   apart from the updater's `Application` (embedded in that jar) and a server-side connection manager. So there
//!   is no single-instance guard in the game's code.
//! * Not verifiable without running the game, and not worked around here: Steam (a second launch through Steam
//!   may be refused by Steam itself; direct launch bypasses Steam's launcher but the game still talks to the Steam
//!   client library), the shared `projectx.log` (the game opens it by relative name, so both clients write the
//!   game folder's file), shared client preferences, and the server logging the same account out of one client
//!   when it logs in on the other (the alt must be a different game account).
//!
//! The memory defaults are a guess; the game's real minimum is not known.

use super::direct::{DirectOptions, Memory};
use crate::extra_txt::is_skgfx_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AltSettings {
    pub initial_mb: u32,
    pub max_mb: u32,
}

impl Default for AltSettings {
    fn default() -> Self {
        Self {
            initial_mb: 256,
            max_mb: 1024,
        }
    }
}

/// Turns direct-launch options into the alt client's. An explicit `memory` in `base` is kept.
pub fn alt_options(mut base: DirectOptions, settings: AltSettings) -> DirectOptions {
    base.memory.get_or_insert(Memory {
        initial_mb: Some(settings.initial_mb.min(settings.max_mb)),
        max_mb: Some(settings.max_mb),
    });
    base.skip_extra_line = Some(is_skgfx_line);
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_memory_and_drops_the_graphics_agent() {
        let options = alt_options(DirectOptions::default(), AltSettings::default());
        assert_eq!(
            options.memory,
            Some(Memory {
                initial_mb: Some(256),
                max_mb: Some(1024)
            })
        );
        let skip = options.skip_extra_line.unwrap();
        assert!(skip("-javaagent:/m/skgfx-agent.jar"));
        assert!(skip("-Dskgfx.scale=2"));
        assert!(!skip("-XX:+UseG1GC"));
    }

    #[test]
    fn initial_never_exceeds_max_and_explicit_memory_wins() {
        let settings = AltSettings {
            initial_mb: 2048,
            max_mb: 512,
        };
        let options = alt_options(DirectOptions::default(), settings);
        assert_eq!(options.memory.unwrap().initial_mb, Some(512));
        let explicit = DirectOptions {
            memory: Some(Memory {
                initial_mb: None,
                max_mb: Some(3000),
            }),
            ..DirectOptions::default()
        };
        assert_eq!(
            alt_options(explicit, AltSettings::default())
                .memory
                .unwrap()
                .max_mb,
            Some(3000)
        );
    }
}
