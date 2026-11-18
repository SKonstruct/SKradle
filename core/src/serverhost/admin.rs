//! The WebUI administrator the launcher manages: a generated password, kept in the server folder, applied to the
//! engine with the `webui user` console command so the player can sign in to the embedded WebUI without ever
//! choosing one.
//!
//! **Why the launcher creates the account**: the engine has separate WebUI accounts and, on a fresh deployment,
//! shows a first-run page that anyone who can reach the port and knows the eight-digit setup code (printed on the
//! console only) can use to become administrator. A console-created account permanently closes that page (the
//! engine's identity store treats any `identity.*` event as completed setup), so creating it at the first start
//! also removes the open window. The setup code line is hidden from the log buffer by [`crate::serverhost::process`].
//!
//! **Credential file**: `admin-credentials.json` in the server folder, created with mode `0600` on unix (best
//! effort `icacls` on Windows). It is written *before* the password is sent to the engine, so a crash in between
//! leaves a file the next start can apply. The launcher owns the account named [`ADMIN_USER`] and re-applies its
//! stored password at every start (a reset or a restored backup replaces the engine's account database, so the two
//! would drift otherwise). A password the player changed for this account in the WebUI is therefore overwritten;
//! additional administrators created in the WebUI's own access page are not touched.

use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::fsx::sibling;
use super::layout::ServerLayout;
use super::prompts::Secret;

/// The WebUI account the launcher manages. The engine accepts 3-64 letters, digits, `_` and `-`.
pub const ADMIN_USER: &str = "skradle";
/// Generated password length; the engine accepts 8-128 characters.
pub const PASSWORD_LENGTH: usize = 24;

const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

/// The administrator's credentials. `Debug` hides the password.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    password: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

impl Credentials {
    pub fn new(username: &str, password: String) -> Self {
        Self {
            username: username.to_owned(),
            password,
        }
    }

    /// The password as a [`Secret`], for answering the engine's prompt.
    pub fn secret(&self) -> Secret {
        Secret::new(self.password.clone())
    }

    /// The password in the clear, for showing to the player. Do not log it.
    pub fn reveal_password(&self) -> &str {
        &self.password
    }
}

/// A random password of [`PASSWORD_LENGTH`] characters from a 56-letter alphabet without look-alikes (about 139
/// bits), drawn from the operating system's random source without modulo bias.
pub fn generate_password() -> io::Result<String> {
    let limit = 256 - (256 % ALPHABET.len());
    let mut out = String::with_capacity(PASSWORD_LENGTH);
    let mut buf = [0u8; 64];
    while out.len() < PASSWORD_LENGTH {
        getrandom::fill(&mut buf).map_err(|e| io::Error::other(e.to_string()))?;
        for &byte in &buf {
            if (byte as usize) < limit && out.len() < PASSWORD_LENGTH {
                out.push(ALPHABET[byte as usize % ALPHABET.len()] as char);
            }
        }
    }
    Ok(out)
}

pub fn credentials_path(layout: &ServerLayout) -> PathBuf {
    layout.root().join("admin-credentials.json")
}

/// The stored credentials, if there is a readable file whose password the engine would accept.
pub fn load(layout: &ServerLayout) -> Option<Credentials> {
    let text = std::fs::read_to_string(credentials_path(layout)).ok()?;
    let credentials: Credentials = serde_json::from_str(&text).ok()?;
    let ok = (3..=64).contains(&credentials.username.len())
        && credentials
            .username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && (8..=128).contains(&credentials.password.chars().count())
        && !credentials.password.chars().any(char::is_control);
    ok.then_some(credentials)
}

/// Writes the credentials so that they are never readable by other users: the file is created private and then
/// renamed into place.
pub fn store(layout: &ServerLayout, credentials: &Credentials) -> io::Result<()> {
    let path = credentials_path(layout);
    let tmp = sibling(&path, "tmp");
    let json = serde_json::to_string_pretty(credentials).map_err(io::Error::other)?;
    let result = (|| {
        let _ = std::fs::remove_file(&tmp);
        let mut file = create_private(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        restrict_windows_acl(&tmp);
        std::fs::rename(&tmp, &path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(unix)]
fn create_private(path: &std::path::Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private(path: &std::path::Path) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Best effort: remove inherited permissions and grant only the current user. Failures are ignored; not verified.
#[cfg(windows)]
fn restrict_windows_acl(path: &std::path::Path) {
    if let Some(user) = std::env::var_os("USERNAME") {
        let mut grant = user;
        grant.push(":F");
        let _ = std::process::Command::new("icacls")
            .arg(path)
            .args(["/inheritance:r", "/grant:r"])
            .arg(grant)
            .output();
    }
}

#[cfg(not(windows))]
fn restrict_windows_acl(_path: &std::path::Path) {}

/// The credentials to apply at a start: the stored ones, or a freshly generated pair that is stored first.
pub fn ensure(layout: &ServerLayout) -> io::Result<Credentials> {
    if let Some(existing) = load(layout) {
        return Ok(existing);
    }
    let fresh = Credentials::new(ADMIN_USER, generate_password()?);
    store(layout, &fresh)?;
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_passwords_are_long_distinct_and_acceptable_to_the_engine() {
        let a = generate_password().unwrap();
        let b = generate_password().unwrap();
        assert_eq!(a.len(), PASSWORD_LENGTH);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| ALPHABET.contains(&c)));
    }

    #[test]
    fn credentials_round_trip_and_never_print() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        assert!(load(&layout).is_none());
        let made = ensure(&layout).unwrap();
        assert_eq!(made.username, ADMIN_USER);
        assert_eq!(ensure(&layout).unwrap(), made, "kept, not regenerated");
        assert_eq!(load(&layout), Some(made.clone()));
        assert!(!format!("{made:?}").contains(made.reveal_password()));
        assert_eq!(made.secret().expose(), made.reveal_password());
    }

    #[cfg(unix)]
    #[test]
    fn the_credential_file_is_private_from_the_moment_it_exists() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        ensure(&layout).unwrap();
        let mode = std::fs::metadata(credentials_path(&layout))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!sibling(&credentials_path(&layout), "tmp").exists());
        // Replacing keeps it private.
        store(
            &layout,
            &Credentials::new("skradle", "another-password-1".into()),
        )
        .unwrap();
        let mode = std::fs::metadata(credentials_path(&layout))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_damaged_or_unacceptable_file_is_not_used() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        for bad in [
            "not json",
            r#"{"username":"ab","password":"long-enough-password"}"#,
            r#"{"username":"skradle","password":"short"}"#,
            r#"{"username":"sk radle","password":"long-enough-password"}"#,
        ] {
            std::fs::write(credentials_path(&layout), bad).unwrap();
            assert!(load(&layout).is_none(), "{bad}");
        }
        // And a fresh pair replaces a damaged file.
        let fresh = ensure(&layout).unwrap();
        assert_eq!(load(&layout), Some(fresh));
    }
}
