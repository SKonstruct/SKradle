//! Backups of the server's state: a copy of `data/` (database with its WAL files, keys, logs) and `config/`,
//! made while the engine is stopped (a SQLite database copied under a running server can be torn).
//!
//! Every operation here takes a [`Stopped`] token. Only the process manager can make one, and only while it holds
//! the lifecycle lock with no child running, so a backup, restore or reset can never overlap a start.
//!
//! Layout: `backups/<yyyymmdd_hhmmss>[-label][_n]/{data,config,backup.json}`. A backup is built in a hidden
//! `.tmp-` folder and renamed into place when complete, so a crash or error never leaves a half backup that looks
//! whole. The newest [`KEEP`] are kept. `data/server.lock` (the engine's lock file) and `*.tmp` files are not copied.
//! Symbolic links anywhere in `data/` or `config/` make a copy fail rather than be followed.
//!
//! **Restore** first backs the current state up as `before-restore`, replaces `data/` and `config/` through a
//! staging copy and puts the old ones back if the swap fails. **Reset** moves `data/` into a backup (instant, same
//! filesystem) and leaves no `data/`, so the engine starts from nothing; with `keep_keys` the server's key pair is
//! copied back, so players keep the public key they already configured.

use std::fs;
use std::io;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::fsx::{copy_tree, remove_all, sibling, write_atomic};
use super::layout::{is_valid_backup_name, ServerLayout};

/// How many backups are kept; older ones are deleted after a new one is made.
pub const KEEP: usize = 10;
const META: &str = "backup.json";

/// Proof that the engine is stopped and the lifecycle lock is held. See the module notes.
pub struct Stopped<'a> {
    _lock: PhantomData<&'a ()>,
}

impl Stopped<'_> {
    pub(super) fn assume_stopped() -> Self {
        Stopped { _lock: PhantomData }
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Stopped<'static> {
        Stopped { _lock: PhantomData }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupInfo {
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    pub label: Option<String>,
    pub bytes: u64,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.to_owned())
}

fn skip_runtime_files(name: &str) -> bool {
    name == "server.lock" || name.ends_with(".tmp")
}

/// `yyyymmdd_hhmmss` for seconds since the epoch (UTC).
pub fn timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}{month:02}{day:02}_{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn clean_label(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(40)
        .collect()
}

/// A backup folder name that does not exist yet.
fn fresh_name(layout: &ServerLayout, now: u64, label: Option<&str>) -> String {
    let mut base = timestamp(now);
    if let Some(label) = label.map(clean_label).filter(|l| !l.is_empty()) {
        base.push('-');
        base.push_str(&label);
    }
    let mut name = base.clone();
    let mut n = 2;
    while layout.backups_dir().join(&name).exists() {
        name = format!("{base}_{n}");
        n += 1;
    }
    name
}

fn tmp_dir(layout: &ServerLayout, name: &str) -> PathBuf {
    layout.backups_dir().join(format!(".tmp-{name}"))
}

fn write_meta(dir: &Path, info: &BackupInfo) -> io::Result<()> {
    let json = serde_json::to_string_pretty(info).map_err(io::Error::other)?;
    write_atomic(&dir.join(META), json.as_bytes())
}

fn finish(layout: &ServerLayout, tmp: &Path, info: &BackupInfo) -> io::Result<()> {
    write_meta(tmp, info)?;
    fs::rename(tmp, layout.backups_dir().join(&info.name))
}

/// Copies `data/` and `config/` into a new backup and prunes the oldest beyond [`KEEP`].
pub fn create(
    layout: &ServerLayout,
    stopped: &Stopped<'_>,
    label: Option<&str>,
    now: u64,
) -> io::Result<BackupInfo> {
    let info = create_unpruned(layout, stopped, label, now)?;
    prune(layout, KEEP, None)?;
    Ok(info)
}

fn create_unpruned(
    layout: &ServerLayout,
    _stopped: &Stopped<'_>,
    label: Option<&str>,
    now: u64,
) -> io::Result<BackupInfo> {
    fs::create_dir_all(layout.backups_dir())?;
    let name = fresh_name(layout, now, label);
    let tmp = tmp_dir(layout, &name);
    remove_all(&tmp)?;
    let result = (|| {
        let mut bytes = copy_tree(&layout.data_dir(), &tmp.join("data"), &skip_runtime_files)?;
        bytes += copy_tree(
            &layout.config_dir(),
            &tmp.join("config"),
            &skip_runtime_files,
        )?;
        fs::create_dir_all(&tmp)?;
        let info = BackupInfo {
            name: name.clone(),
            created_at: now,
            label: label.map(str::to_owned),
            bytes,
        };
        finish(layout, &tmp, &info)?;
        Ok(info)
    })();
    if result.is_err() {
        let _ = remove_all(&tmp);
    }
    result
}

/// Backups, newest first. Folders without a readable `backup.json` (and hidden temporary ones) are not listed.
pub fn list(layout: &ServerLayout) -> io::Result<Vec<BackupInfo>> {
    let mut found = Vec::new();
    let entries = match fs::read_dir(layout.backups_dir()) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(e),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_valid_backup_name(&name) || !entry.path().is_dir() {
            continue;
        }
        let Some(mut info) = fs::read_to_string(entry.path().join(META))
            .ok()
            .and_then(|t| serde_json::from_str::<BackupInfo>(&t).ok())
        else {
            continue;
        };
        info.name = name;
        found.push(info);
    }
    found.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.name.cmp(&a.name))
    });
    Ok(found)
}

/// Deletes the backups beyond the newest `keep`, except `protect` (a backup that is being restored from).
fn prune(layout: &ServerLayout, keep: usize, protect: Option<&str>) -> io::Result<()> {
    for old in list(layout)?.into_iter().skip(keep) {
        if protect != Some(old.name.as_str()) {
            remove_all(&layout.backups_dir().join(old.name))?;
        }
    }
    Ok(())
}

/// Deletes one backup by name.
pub fn delete(layout: &ServerLayout, name: &str) -> io::Result<()> {
    let dir = layout
        .backup_dir(name)
        .filter(|d| d.join(META).is_file())
        .ok_or_else(|| invalid("no such backup"))?;
    remove_all(&dir)
}

/// Replaces `target` by a copy of `source`: the copy is made beside it, the old one is kept until the new one is
/// in place, and put back if the last step fails. A failed copy leaves `target` untouched.
fn replace_dir(source: &Path, target: &Path) -> io::Result<()> {
    let staging = sibling(target, "restoring");
    let old = sibling(target, "replaced");
    remove_all(&staging)?;
    remove_all(&old)?;
    if let Err(e) = copy_tree(source, &staging, &skip_runtime_files) {
        let _ = remove_all(&staging);
        return Err(e);
    }
    let had_old = target.exists();
    if had_old {
        fs::rename(target, &old)?;
    }
    if let Err(e) = fs::rename(&staging, target) {
        if had_old {
            let _ = fs::rename(&old, target);
        }
        let _ = remove_all(&staging);
        return Err(e);
    }
    remove_all(&old)
}

/// Puts a backup's `data/` and `config/` back. The current state is backed up first (`before-restore`) and put
/// back if the restore fails halfway.
pub fn restore(
    layout: &ServerLayout,
    stopped: &Stopped<'_>,
    name: &str,
    now: u64,
) -> io::Result<()> {
    let source = layout
        .backup_dir(name)
        .filter(|d| d.join(META).is_file())
        .ok_or_else(|| invalid("no such backup"))?;
    if !source.join("data").is_dir() && !source.join("config").is_dir() {
        return Err(invalid("the backup holds no data or configuration"));
    }
    // Not pruned yet: with the folder full, the oldest backup may be the very one being restored.
    let safety = create_unpruned(layout, stopped, Some("before-restore"), now)?;
    let safety_dir = layout.backups_dir().join(&safety.name);
    let pairs = [
        (source.join("data"), layout.data_dir()),
        (source.join("config"), layout.config_dir()),
    ];
    let mut replaced = 0;
    let result = (|| {
        for (from, to) in &pairs {
            if from.is_dir() {
                replace_dir(from, to)?;
            }
            replaced += 1;
        }
        Ok(())
    })();
    if let Err(e) = result {
        // Put the earlier pairs back from the safety backup.
        for (_, to) in pairs.iter().take(replaced) {
            let name = to.file_name().unwrap_or_default();
            let _ = replace_dir(&safety_dir.join(name), to);
        }
        return Err(e);
    }
    prune(layout, KEEP, Some(name))
}

/// Retires the current `data/` into a backup named `...-reset` and leaves no `data/` for a fresh start. With
/// `keep_keys` the server's key pair is put back.
pub fn reset(
    layout: &ServerLayout,
    _stopped: &Stopped<'_>,
    keep_keys: bool,
    now: u64,
) -> io::Result<BackupInfo> {
    fs::create_dir_all(layout.backups_dir())?;
    let name = fresh_name(layout, now, Some("reset"));
    let tmp = tmp_dir(layout, &name);
    remove_all(&tmp)?;
    fs::create_dir_all(&tmp)?;
    let data = layout.data_dir();
    let moved = data.is_dir();
    let result = (|| {
        let mut bytes = 0;
        if moved {
            bytes = super::fsx::tree_size(&data);
            fs::rename(&data, tmp.join("data"))?;
        }
        let info = BackupInfo {
            name: name.clone(),
            created_at: now,
            label: Some("reset".to_owned()),
            bytes,
        };
        finish(layout, &tmp, &info)?;
        Ok(info)
    })();
    let info = match result {
        Ok(info) => info,
        Err(e) => {
            if moved && !data.exists() && tmp.join("data").is_dir() {
                let _ = fs::rename(tmp.join("data"), &data);
            }
            let _ = remove_all(&tmp);
            return Err(e);
        }
    };
    if keep_keys {
        let kept = layout.backups_dir().join(&info.name).join("data/keys");
        if let Err(e) = copy_tree(&kept, &layout.keys_dir(), &|_| false) {
            // Undo the whole reset: the player asked to keep the keys and cannot have them.
            let _ = remove_all(&data);
            let backup = layout.backups_dir().join(&info.name);
            let _ = fs::rename(backup.join("data"), &data);
            let _ = remove_all(&backup);
            return Err(e);
        }
    }
    prune(layout, KEEP, None)?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> (tempfile::TempDir, ServerLayout) {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path().join("server"));
        layout.ensure().unwrap();
        fs::create_dir_all(layout.keys_dir()).unwrap();
        fs::write(layout.database(), b"db v1").unwrap();
        fs::write(layout.data_dir().join("tortodrone.db-wal"), b"wal v1").unwrap();
        fs::write(layout.data_dir().join("server.lock"), b"").unwrap();
        fs::write(layout.keys_dir().join("public.txt"), b"abc#10001").unwrap();
        fs::write(layout.keys_dir().join("private.pk8"), b"secret").unwrap();
        fs::write(layout.server_yml(), b"server-port: 47624\n").unwrap();
        (dir, layout)
    }

    fn stopped() -> Stopped<'static> {
        Stopped::for_tests()
    }

    #[test]
    fn timestamps_are_utc_civil_time() {
        assert_eq!(timestamp(0), "19700101_000000");
        assert_eq!(timestamp(1_700_000_000), "20231114_221320");
        assert_eq!(timestamp(951_782_400 + 86_399), "20000229_235959");
    }

    #[test]
    fn a_backup_copies_data_and_config_but_not_the_lock() {
        let (_d, layout) = server();
        let info = create(&layout, &stopped(), Some("manual"), 1_700_000_000).unwrap();
        assert_eq!(info.name, "20231114_221320-manual");
        let dir = layout.backups_dir().join(&info.name);
        assert_eq!(fs::read(dir.join("data/tortodrone.db")).unwrap(), b"db v1");
        assert_eq!(
            fs::read(dir.join("data/tortodrone.db-wal")).unwrap(),
            b"wal v1"
        );
        assert!(!dir.join("data/server.lock").exists());
        assert!(dir.join("config/server.yml").is_file());
        assert!(info.bytes > 0);
        assert_eq!(list(&layout).unwrap(), vec![info]);
        assert!(fs::read_dir(layout.backups_dir()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tmp")));
    }

    #[test]
    fn two_backups_in_one_second_do_not_collide_and_the_newest_n_are_kept() {
        let (_d, layout) = server();
        let a = create(&layout, &stopped(), None, 1000).unwrap();
        let b = create(&layout, &stopped(), None, 1000).unwrap();
        assert_ne!(a.name, b.name);
        for i in 0..(KEEP as u64 + 3) {
            create(&layout, &stopped(), None, 2000 + i).unwrap();
        }
        let all = list(&layout).unwrap();
        assert_eq!(all.len(), KEEP);
        assert!(all.windows(2).all(|w| w[0].created_at >= w[1].created_at));
        assert_eq!(all[0].created_at, 2000 + KEEP as u64 + 2);
        assert!(all.iter().all(|i| i.created_at >= 2000));
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_backup_leaves_no_trace_and_the_data_alone() {
        let (_d, layout) = server();
        std::os::unix::fs::symlink("/etc/hosts", layout.data_dir().join("evil")).unwrap();
        assert!(create(&layout, &stopped(), None, 5).is_err());
        assert!(list(&layout).unwrap().is_empty());
        assert_eq!(fs::read_dir(layout.backups_dir()).unwrap().count(), 0);
        assert_eq!(fs::read(layout.database()).unwrap(), b"db v1");
    }

    #[test]
    fn restore_round_trip_and_the_state_before_is_kept() {
        let (_d, layout) = server();
        let original = create(&layout, &stopped(), None, 100).unwrap();
        fs::write(layout.database(), b"db v2").unwrap();
        fs::write(layout.server_yml(), b"server-port: 1\n").unwrap();
        fs::write(layout.data_dir().join("extra"), b"x").unwrap();
        restore(&layout, &stopped(), &original.name, 200).unwrap();
        assert_eq!(fs::read(layout.database()).unwrap(), b"db v1");
        assert_eq!(
            fs::read(layout.server_yml()).unwrap(),
            b"server-port: 47624\n"
        );
        assert!(!layout.data_dir().join("extra").exists());
        let safety = list(&layout)
            .unwrap()
            .into_iter()
            .find(|i| i.label.as_deref() == Some("before-restore"))
            .unwrap();
        let kept = layout.backups_dir().join(safety.name);
        assert_eq!(fs::read(kept.join("data/tortodrone.db")).unwrap(), b"db v2");
        for leftover in [
            "data.restoring",
            "data.replaced",
            "config.restoring",
            "config.replaced",
        ] {
            assert!(!layout.root().join(leftover).exists(), "{leftover}");
        }
    }

    #[test]
    fn restoring_the_oldest_backup_of_a_full_folder_keeps_it_and_applies_it() {
        let (_d, layout) = server();
        fs::write(layout.database(), b"oldest state").unwrap();
        let oldest = create(&layout, &stopped(), Some("oldest"), 100).unwrap();
        for i in 0..(KEEP as u64 - 1) {
            fs::write(layout.database(), format!("state {i}")).unwrap();
            create(&layout, &stopped(), None, 200 + i).unwrap();
        }
        assert_eq!(list(&layout).unwrap().len(), KEEP);
        assert_eq!(list(&layout).unwrap().last().unwrap().name, oldest.name);
        restore(&layout, &stopped(), &oldest.name, 1000).unwrap();
        assert_eq!(fs::read(layout.database()).unwrap(), b"oldest state");
        let names: Vec<_> = list(&layout).unwrap().into_iter().map(|i| i.name).collect();
        assert!(
            names.contains(&oldest.name),
            "the source survives the prune"
        );
        assert!(names.len() <= KEEP + 1);
    }

    #[test]
    fn restore_refuses_names_that_are_not_backups() {
        let (_d, layout) = server();
        for name in ["../server", "nope", "", ".tmp-x", "a/b"] {
            assert!(restore(&layout, &stopped(), name, 1).is_err(), "{name:?}");
            assert!(delete(&layout, name).is_err(), "{name:?}");
        }
        assert_eq!(fs::read(layout.database()).unwrap(), b"db v1");
        assert!(
            list(&layout).unwrap().is_empty(),
            "no safety backup for a refused restore"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_restore_that_fails_midway_puts_the_current_state_back() {
        let (_d, layout) = server();
        let good = create(&layout, &stopped(), None, 100).unwrap();
        // Damage the backup's config so the second step fails after data/ was already replaced.
        let cfg = layout.backups_dir().join(&good.name).join("config");
        std::os::unix::fs::symlink("/etc/hosts", cfg.join("evil")).unwrap();
        fs::write(layout.database(), b"db v2").unwrap();
        fs::write(layout.server_yml(), b"server-port: 2\n").unwrap();
        assert!(restore(&layout, &stopped(), &good.name, 200).is_err());
        assert_eq!(
            fs::read(layout.database()).unwrap(),
            b"db v2",
            "data put back"
        );
        assert_eq!(fs::read(layout.server_yml()).unwrap(), b"server-port: 2\n");
    }

    #[test]
    fn reset_moves_data_into_a_backup_and_can_keep_the_keys() {
        let (_d, layout) = server();
        let info = reset(&layout, &stopped(), true, 300).unwrap();
        assert!(info.name.ends_with("-reset"));
        assert!(!layout.database().exists());
        assert_eq!(
            fs::read(layout.keys_dir().join("public.txt")).unwrap(),
            b"abc#10001"
        );
        assert_eq!(
            fs::read(layout.keys_dir().join("private.pk8")).unwrap(),
            b"secret"
        );
        let kept = layout.backups_dir().join(&info.name);
        assert_eq!(fs::read(kept.join("data/tortodrone.db")).unwrap(), b"db v1");
        assert_eq!(
            fs::read(layout.server_yml()).unwrap(),
            b"server-port: 47624\n",
            "config untouched"
        );
    }

    #[test]
    fn reset_without_keeping_keys_leaves_nothing_and_a_missing_data_folder_is_fine() {
        let (_d, layout) = server();
        reset(&layout, &stopped(), false, 300).unwrap();
        assert!(!layout.data_dir().exists());
        let again = reset(&layout, &stopped(), true, 400).unwrap();
        assert_eq!(again.bytes, 0);
        assert!(!layout.data_dir().exists());
    }

    #[test]
    fn a_reset_and_its_backup_restore_each_other() {
        let (_d, layout) = server();
        let info = reset(&layout, &stopped(), false, 300).unwrap();
        restore(&layout, &stopped(), &info.name, 400).unwrap();
        assert_eq!(fs::read(layout.database()).unwrap(), b"db v1");
    }

    #[test]
    fn foreign_folders_and_damaged_metadata_are_not_listed() {
        let (_d, layout) = server();
        fs::create_dir_all(layout.backups_dir().join("random")).unwrap();
        fs::create_dir_all(layout.backups_dir().join("bad")).unwrap();
        fs::write(layout.backups_dir().join("bad").join(META), "{").unwrap();
        fs::create_dir_all(layout.backups_dir().join(".tmp-half")).unwrap();
        assert!(list(&layout).unwrap().is_empty());
        let made = create(&layout, &stopped(), None, 9).unwrap();
        delete(&layout, &made.name).unwrap();
        assert!(list(&layout).unwrap().is_empty());
    }
}
