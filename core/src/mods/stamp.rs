//! Skipping an apply that would change nothing.
//!
//! An apply rebuilds both code jars from their pristine copies and restores or removes every loose file an earlier
//! apply wrote, so doing it on every launch is slow and pointless when nothing changed. After a successful apply a
//! small stamp is written to `.skradle/apply-stamp.json`: a hash of what the apply was based on (the mods folder, the
//! file cleaning and purging choices, the game version) and a hash of what it left behind (the two code jars and the
//! record of written files). The next apply is skipped when both still match, so it runs again only when a mod was
//! added, removed, edited, enabled or disabled, a switch changed, the game was updated or something replaced one of
//! the jars. Only names, sizes and modification times are hashed, never contents.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::apply::{ApplyOptions, ApplyReport, SkippedMod};
use crate::jar::write_atomic;
use crate::net::sha256_hex;

pub(super) const STAMP: &str = ".skradle/apply-stamp.json";
/// Bump when what the hashes cover changes, so an old stamp never skips a needed apply.
const VERSION: u32 = 1;
const CODE_JARS: [&str; 2] = ["code/config.jar", "code/projectx-config.jar"];
const APPLIED: &str = ".skradle/applied.json";
/// How deep a mods folder is looked through (mods are files, but a folder of them must still count).
const MAX_DEPTH: usize = 4;
const MAX_ENTRIES: usize = 20_000;

#[derive(Debug, Serialize, Deserialize)]
struct Stamp {
    version: u32,
    inputs: String,
    outputs: String,
    warnings: Vec<String>,
    skipped: Vec<(String, String)>,
}

/// `size mtime` of a file (the time in nanoseconds since the Unix epoch), or `missing`: a changed or replaced file
/// shows as a different line.
fn describe(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(m) => {
            let nanos = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{} {nanos}", m.len())
        }
        Err(_) => "missing".to_owned(),
    }
}

fn collect(dir: &Path, base: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_DEPTH || out.len() > MAX_ENTRIES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(&path);
        match entry.file_type() {
            Ok(t) if t.is_dir() => {
                out.push(format!("dir {}", rel.display()));
                collect(&path, base, depth + 1, out);
            }
            Ok(_) => out.push(format!("{} {}", rel.display(), describe(&path))),
            Err(_) => out.push(format!("{} unreadable", rel.display())),
        }
    }
}

fn inputs_hash(game_root: &Path, mods_dir: &Path, options: ApplyOptions) -> String {
    let mut lines = vec![
        format!("v{VERSION}"),
        format!("rebuild_resources {}", options.rebuild_resources),
        format!(
            "keep_modified_originals {}",
            options.keep_modified_originals
        ),
        format!("keep_added_files {}", options.keep_added_files),
        // The game version and its file list: an update changes what "original" means.
        format!("getdown {}", describe(&game_root.join("getdown.txt"))),
        format!("digest {}", describe(&game_root.join("digest2.txt"))),
    ];
    let mut files = Vec::new();
    collect(mods_dir, mods_dir, 0, &mut files);
    files.sort();
    lines.extend(files);
    sha256_hex(lines.join("\n").as_bytes())
}

fn outputs_hash(game_root: &Path) -> String {
    let lines: Vec<String> = CODE_JARS
        .iter()
        .chain(std::iter::once(&APPLIED))
        .map(|rel| format!("{rel} {}", describe(&game_root.join(rel))))
        .collect();
    sha256_hex(lines.join("\n").as_bytes())
}

fn stamp_path(game_root: &Path) -> PathBuf {
    game_root.join(STAMP)
}

fn read_stamp(game_root: &Path) -> Option<Stamp> {
    let bytes = crate::bounded::read_regular(&stamp_path(game_root), 1024 * 1024).ok()?;
    let stamp: Stamp = serde_json::from_slice(&bytes).ok()?;
    (stamp.version == VERSION).then_some(stamp)
}

/// The report of the apply that is still in force, when nothing it was based on has changed since.
pub(super) fn unchanged(
    game_root: &Path,
    mods_dir: &Path,
    options: ApplyOptions,
) -> Option<ApplyReport> {
    let stamp = read_stamp(game_root)?;
    if stamp.inputs != inputs_hash(game_root, mods_dir, options)
        || stamp.outputs != outputs_hash(game_root)
    {
        return None;
    }
    Some(ApplyReport {
        warnings: stamp.warnings,
        skipped: stamp
            .skipped
            .into_iter()
            .map(|(file_name, reason)| SkippedMod { file_name, reason })
            .collect(),
        unchanged: true,
        ..ApplyReport::default()
    })
}

/// Records a successful apply. Failing to write the stamp only means the next apply is not skipped.
pub(super) fn record(
    game_root: &Path,
    mods_dir: &Path,
    options: ApplyOptions,
    report: &ApplyReport,
) {
    let stamp = Stamp {
        version: VERSION,
        inputs: inputs_hash(game_root, mods_dir, options),
        outputs: outputs_hash(game_root),
        warnings: report.warnings.clone(),
        skipped: report
            .skipped
            .iter()
            .map(|s| (s.file_name.clone(), s.reason.clone()))
            .collect(),
    };
    let write = || -> io::Result<()> {
        let mut json = serde_json::to_vec_pretty(&stamp).map_err(io::Error::other)?;
        json.push(b'\n');
        let path = stamp_path(game_root);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_atomic(&path, &json)
    };
    if write().is_err() {
        forget(game_root);
    }
}

/// Removes the stamp, so the next apply runs (after a failed apply).
pub(super) fn forget(game_root: &Path) {
    let _ = fs::remove_file(stamp_path(game_root));
}
