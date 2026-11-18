//! The mods engine: reading mods, listing them, applying and removing them in a game folder, and keeping the
//! mods folder in step with the owner's modpack. See `docs/mods.md` for the formats, the pipeline and the safety
//! rules.
//!
//! - [`model`]: a mod file (`.zip`, `.jar`, `.modpack`) as a typed [`model::Mod`], read without trusting it.
//! - [`metadata`]: `mod.json` (tolerating trailing commas) and locale `.properties` edits.
//! - [`protected`]: the game files a mod may not replace.
//! - [`list`]: the mods folder and which mods are enabled.
//! - [`apply`]: [`apply::apply`] and [`apply::restore_pristine`], atomic and fail-closed.
//! - [`source`]: where an original code jar comes from when none is on disk.
//! - [`txn`]: the journaled all-or-nothing file changes `apply` is built on.
//! - [`sync`]: the modpack download.

pub mod apply;
pub mod list;
pub mod metadata;
pub mod model;
pub mod protected;
pub mod source;
pub mod sync;
pub mod txn;

pub use apply::{apply, restore_pristine, ApplyError, ApplyOptions, ApplyReport};
pub use list::{list as list_mods, set_enabled, InstalledMod};
pub use model::{Container, Kind, Mod, ModError};
