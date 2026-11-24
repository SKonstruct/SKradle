//! What the Mods page shows and does, as plain Rust: the mods folder, the list the page draws (read from the core's
//! scan), adding files, removing and switching mods, and the screenshot fixtures. `bridge/mods_model.rs` only moves
//! this between threads and Qt properties.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use skradle_core::jar::write_atomic;
use skradle_core::mods::import::import_file;
use skradle_core::mods::tags::{tags_of, Tag};
use skradle_core::mods::{self, Container, InstalledMod, Mod};

use crate::discovery::local_path_from_url_text;

/// The folder the player's mods live in, the one applied at launch: `SKRADLE_MODS_DIR` when set (tests), else
/// `<data dir>/SKradle/mods`.
pub fn mods_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SKRADLE_MODS_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::data_dir().map(|d| d.join("SKradle").join("mods"))
}

/// `file:///...` for a local path, percent-encoded, with the drive letter form on Windows.
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Where mod pictures are kept once checked: `SKRADLE_ICON_CACHE` when set (tests and screenshots), else
/// `<cache dir>/SKradle/mod-icons`.
fn icon_cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SKRADLE_ICON_CACHE").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::cache_dir().map(|d| d.join("SKradle").join("mod-icons"))
}

/// What was learned about a mod file's picture, by file name, size and modification time, so a rescan after a
/// switch does not open every archive again.
type IconKey = (Option<PathBuf>, String, u64, Option<SystemTime>);
static ICONS: Mutex<Option<HashMap<IconKey, String>>> = Mutex::new(None);

/// The `file://` URL of the mod's checked picture in the cache, or "" when it has none or it cannot be used (too
/// large, not a PNG or JPEG, a path outside the archive, ...): the page then draws its placeholder.
pub fn icon_url(dir: &Path, scanned: &Mod, cache: Option<&Path>) -> String {
    let path = dir.join(&scanned.file_name);
    let meta = std::fs::symlink_metadata(&path).ok();
    let key = (
        cache.map(Path::to_path_buf),
        scanned.file_name.clone(),
        meta.as_ref().map_or(0, |m| m.len()),
        meta.and_then(|m| m.modified().ok()),
    );
    if let Some(hit) = ICONS
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|m| m.get(&key).cloned()))
    {
        // A hit is only good while its cached file is still there.
        if hit.is_empty() || local_path_from_url_text(&hit).is_file() {
            return hit;
        }
    }
    let url = (|| {
        let cache = cache?;
        let image = scanned.read_image(&path).ok()??;
        let name = format!(
            "{}.{}",
            &skradle_core::net::sha256_hex(&image.bytes)[..32],
            image.kind.extension()
        );
        let target = cache.join(&name);
        if !target.is_file() {
            std::fs::create_dir_all(cache).ok()?;
            write_atomic(&target, &image.bytes).ok()?;
        }
        Some(file_url(&target))
    })()
    .unwrap_or_default();
    if let Ok(mut g) = ICONS.lock() {
        g.get_or_insert_with(HashMap::new).insert(key, url.clone());
    }
    url
}

/// Deletes cached pictures that no mod of the listing uses and that are over a month old.
fn prune_icon_cache(cache: &Path, used: &HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return;
    };
    let old = SystemTime::now()
        .checked_sub(Duration::from_secs(30 * 24 * 3600))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let ours = name.ends_with(".png") || name.ends_with(".jpg");
        let stale = e
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t < old);
        if ours && stale && !used.contains(&name) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The file name without its mod extension: what a mod without a `mod.json` name is called.
fn file_stem(file: &str) -> &str {
    match file.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && matches!(ext.to_ascii_lowercase().as_str(), "zip" | "jar" | "modpack") =>
        {
            stem
        }
        _ => file,
    }
}

/// One mod as the page draws it.
#[derive(Debug, Serialize, PartialEq)]
pub struct ItemView {
    pub file: String,
    /// The title: `mod.json`'s name, else the file name without its extension.
    pub name: String,
    /// The file name when it says something the title does not (the title is not the file name without its
    /// extension); empty otherwise, so no fact is shown twice.
    pub file_note: String,
    /// The `file://` URL of the mod's picture in the cache; empty when it has none.
    pub image: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// `zip`, `jar` or `modpack`.
    pub container: &'static str,
    pub tags: Vec<&'static str>,
    /// Mods inside (1 unless it is a modpack).
    pub parts: usize,
    /// Files it would write.
    pub files: usize,
    pub size: u64,
    pub enabled: bool,
    /// Why it cannot be used; `None` for a mod that scanned.
    pub error: Option<String>,
    /// The engine's own text for `error`, for a tooltip; empty when there is no error.
    pub error_detail: String,
    pub warnings: Vec<String>,
    /// A class mod built for another game version than the install.
    pub incompatible: bool,
    /// The Warning filter: the mod cannot be read, is built for another game version, or the scan noted something
    /// (a skipped part of a modpack, protected files left out, an unreadable `mod.json`).
    pub warning: bool,
    /// Lower-case text the search box matches.
    pub search: String,
    /// Mods that change some of the same files; the one lower in the list wins those files.
    pub conflicts: Vec<ConflictView>,
}

/// One conflict as seen from one of the two mods.
#[derive(Debug, Serialize, PartialEq)]
pub struct ConflictView {
    /// The title of the other mod.
    pub other: String,
    /// How many files both change.
    pub count: usize,
    /// True when the other mod is applied after this one, so it wins those files.
    pub other_wins: bool,
    /// A few of the files, for a tooltip.
    pub examples: Vec<String>,
}

// ---- plain words for engine errors ---------------------------------------------------------------------------------
// The engine's messages are for logs. The page gets a sentence that says what failed and why; the raw text stays in
// `error_detail` (a tooltip) and on stderr.

/// Capitalises the first letter.
fn capitalised(text: &str) -> String {
    // A file name ("bad.zip", "mod.json") at the start keeps its case.
    if text.split(' ').next().is_some_and(|w| w.contains('.')) {
        return text.to_owned();
    }
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The reason for a failure on an archive, a file or the disk, short and without a final period. `None` when the text
/// matches nothing known.
fn archive_reason(raw: &str) -> Option<&'static str> {
    let t = raw.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| t.contains(n));
    Some(
        if has(&["eocd", "not a readable zip", "invalid zip", "zip archive"]) {
            "The file is not a valid zip"
        } else if has(&["is encrypted"]) {
            "The archive is password protected"
        } else if has(&["unsafe entry name"]) {
            "It has a file with an unsafe path"
        } else if has(&["symbolic link"]) {
            "It contains a link, which is not allowed"
        } else if has(&[
            "appears more than once",
            "both a file and a folder",
            "not the size its header",
        ]) {
            "The archive is damaged"
        } else if has(&["more than", "too large"]) && !t.contains("mod.json") {
            "The file is too large"
        } else if has(&[
            "permission denied",
            "access is denied",
            "operation not permitted",
        ]) {
            "SKradle is not allowed to open it"
        } else if has(&["no such file", "not found", "cannot find", "os error 2"]) {
            "The file is missing"
        } else if has(&["mod.json is not valid", "mod.json is not a json"]) {
            "Its mod.json is not valid"
        } else if has(&["not a regular file"]) {
            "It is not a regular file"
        } else if has(&["not a .zip, .jar or .modpack"]) {
            "It is not a .zip, .jar or .modpack file"
        } else if has(&["changed while"]) {
            "The file changed while it was being read"
        } else {
            return None;
        },
    )
}

/// Why a file could not be used, for any engine text: the known reason, else a generic one.
pub fn plain_reason(raw: &str) -> String {
    archive_reason(raw)
        .unwrap_or("The file could not be read")
        .to_owned()
}

/// The row's message for a mod that cannot be read.
pub fn unusable(raw: &str) -> String {
    format!("Can't be used. {}.", plain_reason(raw))
}

/// A note from the scan (a warning): plain sentences stay, archive errors inside them become words.
pub fn plain_note(raw: &str) -> String {
    match raw.split_once(": ") {
        Some((head, rest)) if archive_reason(rest).is_some() => {
            format!(
                "{}: {}",
                capitalised(head),
                archive_reason(rest).unwrap_or_default()
            )
        }
        _ => match archive_reason(raw) {
            Some(why) if raw.to_lowercase().contains("zip") => why.to_owned(),
            _ => capitalised(raw),
        },
    }
}

/// Why a file was refused when adding it. The core's refusals are already words for the player; the ones that wrap an
/// engine error are replaced.
pub fn plain_refusal(raw: &str) -> String {
    if let Some(inner) = raw.strip_prefix("not a usable mod: ") {
        return plain_reason(inner);
    }
    if raw.starts_with("cannot be read") {
        return plain_reason(raw);
    }
    capitalised(raw)
}

/// Why a modpack check or download failed.
pub fn plain_pack_error(raw: &str) -> String {
    let t = raw.to_lowercase();
    if t.contains("lists no mods") {
        "SirDank's Modpack is empty right now. Try again later.".to_owned()
    } else if t.contains("cannot read the modpack listing")
        || ["connect", "network", "timed out", "dns", "https", "status"]
            .iter()
            .any(|n| t.contains(n))
    {
        "Could not reach SirDank's Modpack. Check your internet connection and try again."
            .to_owned()
    } else if t.contains("permission denied") || t.contains("access is denied") {
        "SKradle is not allowed to write to the mods folder.".to_owned()
    } else {
        "SirDank's Modpack could not be updated. Try again.".to_owned()
    }
}

/// Why one modpack file failed.
pub fn plain_pack_file(raw: &str) -> String {
    let t = raw.to_lowercase();
    if t.contains("hash") || t.contains("sha") {
        "The download was damaged, so it was discarded".to_owned()
    } else if let Some(why) = archive_reason(raw) {
        why.to_owned()
    } else if ["connect", "network", "timed out", "dns", "https", "status"]
        .iter()
        .any(|n| t.contains(n))
    {
        "The download did not finish".to_owned()
    } else {
        "The file could not be downloaded".to_owned()
    }
}

/// Why the folder could not be listed.
pub fn plain_folder_error(raw: &str) -> String {
    let t = raw.to_lowercase();
    if t.contains("state file") {
        "The list of switched-off mods is damaged. Delete .skradle-mods.json in the mods folder and try again."
            .to_owned()
    } else if t.contains("permission denied") || t.contains("access is denied") {
        "SKradle is not allowed to open the mods folder. Check its permissions.".to_owned()
    } else {
        "Check that the folder exists and can be read.".to_owned()
    }
}

fn log_raw(what: &str, raw: &str) {
    eprintln!("skradle: mods: {what}: {raw}");
    skradle_core::log::warn("mods", format!("{what}: {raw}"));
}

fn container_id(c: Container) -> &'static str {
    match c {
        Container::Zip => "zip",
        Container::Jar => "jar",
        Container::Modpack => "modpack",
    }
}

/// The view of one scanned entry. `game_version` is the install's version, when known; `cache` is where its picture
/// is kept (none: no pictures).
pub fn item_view(
    dir: &Path,
    m: &InstalledMod,
    game_version: Option<&str>,
    cache: Option<&Path>,
) -> ItemView {
    let size = std::fs::symlink_metadata(dir.join(&m.file_name)).map_or(0, |md| md.len());
    let container = Container::from_file_name(&m.file_name).map_or("zip", container_id);
    let mut v = ItemView {
        file: m.file_name.clone(),
        name: file_stem(&m.file_name).to_owned(),
        file_note: String::new(),
        image: String::new(),
        version: String::new(),
        author: String::new(),
        description: String::new(),
        container,
        tags: Vec::new(),
        parts: 0,
        files: 0,
        size,
        enabled: m.enabled,
        error: None,
        error_detail: String::new(),
        warnings: Vec::new(),
        incompatible: false,
        warning: false,
        search: String::new(),
        conflicts: Vec::new(),
    };
    match &m.parsed {
        Err(e) => {
            let raw = e.to_string();
            log_raw(&m.file_name, &raw);
            v.error = Some(unusable(&raw));
            v.error_detail = raw;
        }
        Ok(scanned) => {
            let info = scanned.pack_info().unwrap_or(&scanned.info);
            v.name = info
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| file_stem(&m.file_name).to_owned());
            v.image = icon_url(dir, scanned, cache);
            v.version = info.version.clone().unwrap_or_default();
            v.author = info.author.clone().unwrap_or_default();
            v.description = info.description.clone().unwrap_or_default();
            v.tags = tags_of(scanned)
                .into_iter()
                .filter(|t| *t != Tag::Broken)
                .map(Tag::id)
                .collect();
            v.parts = scanned.parts.len();
            v.files = scanned
                .parts
                .iter()
                .map(|p| p.applicable_files().count())
                .sum();
            v.warnings = scanned
                .warnings
                .iter()
                .chain(scanned.parts.iter().flat_map(|p| p.warnings.iter()))
                .map(|w| plain_note(w))
                .collect();
            v.incompatible = scanned
                .parts
                .iter()
                .any(|p| !p.compatible_with(game_version));
        }
    }
    if !v.name.eq_ignore_ascii_case(file_stem(&m.file_name)) {
        v.file_note = m.file_name.clone();
    }
    v.warning = v.error.is_some() || v.incompatible || !v.warnings.is_empty();
    v.search = format!(
        "{} {} {} {} {}",
        v.name,
        v.file,
        v.author,
        v.description,
        v.tags.join(" ")
    )
    .to_lowercase();
    v
}

/// Tag id and how many mods have it, in a fixed order.
#[derive(Debug, Serialize, PartialEq)]
pub struct TagCount {
    pub id: &'static str,
    pub count: usize,
}

/// Only tags that exist among the mods.
pub fn tag_counts(items: &[ItemView]) -> Vec<TagCount> {
    let mut counts: BTreeMap<usize, (&'static str, usize)> = BTreeMap::new();
    let order = [
        Tag::Modpack,
        Tag::Code,
        Tag::Textures,
        Tag::Audio,
        Tag::Text,
        Tag::Resources,
    ];
    for item in items {
        for t in &item.tags {
            if let Some(i) = order.iter().position(|o| o.id() == *t) {
                counts.entry(i).or_insert((order[i].id(), 0)).1 += 1;
            }
        }
    }
    counts
        .into_values()
        .map(|(id, count)| TagCount { id, count })
        .collect()
}

/// The folder as the page shows it.
#[derive(Debug, Default, Serialize)]
pub struct Listing {
    pub items: Vec<ItemView>,
    pub tags: Vec<TagCount>,
    /// Why the folder could not be read (a damaged state file, no permission); empty items then.
    pub error: Option<String>,
}

/// Puts each conflict on both mods it concerns, with the other one's title.
fn attach_conflicts(items: &mut [ItemView], conflicts: &[mods::Conflict]) {
    let title = |file: &str| {
        items
            .iter()
            .find(|i| i.file == file)
            .map_or_else(|| file.to_owned(), |i| i.name.clone())
    };
    let views: Vec<(String, String, ConflictView, ConflictView)> = conflicts
        .iter()
        .map(|c| {
            (
                c.loser.clone(),
                c.winner.clone(),
                ConflictView {
                    other: title(&c.winner),
                    count: c.count,
                    other_wins: true,
                    examples: c.examples.clone(),
                },
                ConflictView {
                    other: title(&c.loser),
                    count: c.count,
                    other_wins: false,
                    examples: c.examples.clone(),
                },
            )
        })
        .collect();
    for (loser, winner, for_loser, for_winner) in views {
        if let Some(item) = items.iter_mut().find(|i| i.file == loser) {
            item.conflicts.push(for_loser);
        }
        if let Some(item) = items.iter_mut().find(|i| i.file == winner) {
            item.conflicts.push(for_winner);
        }
    }
}

/// Moves a mod in the apply order (0 is applied first; a later mod wins).
pub fn move_one(dir: &Path, file: &str, to: usize) -> Outcome {
    skradle_core::log::info("mods", format!("moving {file} to position {to}"));
    let mut out = Outcome {
        kind: "move",
        ..Outcome::default()
    };
    match mods::move_mod(dir, file, to) {
        Ok(()) => out.ok = 1,
        Err(e) => {
            log_raw(file, &e.to_string());
            out.failed
                .push((file.to_owned(), plain_reason(&e.to_string())));
        }
    }
    out
}

/// Scans the folder. A missing folder is an empty list.
pub fn build_listing(dir: &Path, game_version: Option<&str>) -> Listing {
    match mods::list_mods(dir) {
        Ok(found) => {
            let cache = icon_cache_dir();
            let mut items: Vec<ItemView> = found
                .iter()
                .map(|m| item_view(dir, m, game_version, cache.as_deref()))
                .collect();
            attach_conflicts(&mut items, &mods::conflicts(&found));
            if let Some(cache) = &cache {
                let used = items
                    .iter()
                    .filter_map(|i| i.image.rsplit('/').next().map(str::to_owned))
                    .collect();
                prune_icon_cache(cache, &used);
            }
            Listing {
                tags: tag_counts(&items),
                items,
                error: None,
            }
        }
        Err(e) => Listing {
            error: Some({
                log_raw("folder", &e.to_string());
                plain_folder_error(&e.to_string())
            }),
            ..Listing::default()
        },
    }
}

/// What an action did, for the page's toast and report.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Outcome {
    /// `add`, `enable`, `disable` or `remove`.
    pub kind: &'static str,
    pub ok: usize,
    /// Files added under another name: (name asked for, name used).
    pub renamed: Vec<(String, String)>,
    /// File and why it was not handled.
    pub failed: Vec<(String, String)>,
}

/// Switches each named mod. The first failure of the state file stops (every other name would fail the same way).
pub fn set_enabled_all(dir: &Path, files: &[String], enabled: bool) -> Outcome {
    skradle_core::log::info(
        "mods",
        format!(
            "{} {} mod(s)",
            if enabled { "enabling" } else { "disabling" },
            files.len()
        ),
    );
    let mut out = Outcome {
        kind: if enabled { "enable" } else { "disable" },
        ..Outcome::default()
    };
    for f in files {
        match mods::set_enabled(dir, f, enabled) {
            Ok(()) => out.ok += 1,
            Err(e) => {
                log_raw(f, &e.to_string());
                out.failed.push((f.clone(), plain_reason(&e.to_string())));
            }
        }
    }
    out
}

pub fn remove_all(dir: &Path, files: &[String]) -> Outcome {
    skradle_core::log::info("mods", format!("removing {} mod(s)", files.len()));
    let mut out = Outcome {
        kind: "remove",
        ..Outcome::default()
    };
    for f in files {
        match mods::remove(dir, f) {
            Ok(()) => out.ok += 1,
            Err(e) => {
                log_raw(f, &e.to_string());
                out.failed.push((f.clone(), plain_reason(&e.to_string())));
            }
        }
    }
    out
}

/// Adds every path (a `file://` URL or a plain path, as a drop or a dialog gives it) after validating it.
pub fn add_all(dir: &Path, sources: &[String]) -> Outcome {
    skradle_core::log::info("mods", format!("adding {} file(s)", sources.len()));
    let mut out = Outcome {
        kind: "add",
        ..Outcome::default()
    };
    for s in sources {
        let path = local_path_from_url_text(s);
        let shown = path
            .file_name()
            .map_or_else(|| s.clone(), |n| n.to_string_lossy().into_owned());
        if path.as_os_str().is_empty() {
            out.failed
                .push((shown, "This is not a file on this computer".to_owned()));
            continue;
        }
        match import_file(&path, dir) {
            Ok(done) => {
                out.ok += 1;
                if done.renamed {
                    out.renamed.push((shown, done.file_name));
                }
            }
            Err(why) => out.failed.push((shown, plain_refusal(&why.to_string()))),
        }
    }
    out
}

/// A small real PNG (a coloured tile with a lighter diamond) for the fixtures: uncompressed deflate blocks, so no
/// encoder is needed.
fn fixture_png(size: u32, back: [u8; 3], front: [u8; 3]) -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc32(&body).to_be_bytes());
    }
    let mut raw = Vec::new();
    let half = (size / 2) as i64;
    for y in 0..size as i64 {
        raw.push(0);
        for x in 0..size as i64 {
            let inside = (x - half).abs() + (y - half).abs() < half * 3 / 4;
            let c = if inside { front } else { back };
            raw.extend(c);
        }
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (n, block) in blocks.iter().enumerate() {
        z.push(u8::from(n + 1 == blocks.len()));
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend(size.to_be_bytes());
    ihdr.extend(size.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Screenshot and self-test runs: a folder with mods in every state the page shows (pictures and none, long
/// names, a file name that differs from the title, warnings, a broken file), never the player's own.
pub fn write_fixtures(dir: &Path) -> std::io::Result<()> {
    use skradle_core::jar::Jar;
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut jar = Jar::default();
        for (name, data) in entries {
            let _ = jar.set(name, data.to_vec());
        }
        jar.to_bytes().unwrap_or_default()
    }
    std::fs::create_dir_all(dir)?;
    let write = |name: &str, bytes: Vec<u8>| std::fs::write(dir.join(name), bytes);
    let blue = fixture_png(64, [38, 70, 130], [120, 170, 240]);
    let green = fixture_png(64, [30, 100, 70], [130, 220, 160]);
    let rust = fixture_png(96, [130, 60, 40], [240, 170, 110]);
    write(
        "Sharp HUD.zip",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Sharp HUD","author":"Fixture Author","version":"1.4","description":"Draws the interface at full resolution."}}"#,
            ),
            ("mod.png", &blue),
            ("ui/hud.png", b"png"),
        ]),
    )?;
    write(
        "Damage Numbers.jar",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Damage Numbers","author":"Fixture Author","version":"0.9","type":"class","description":"Larger, clearer hit numbers."}}"#,
            ),
            ("com/example/Numbers.class", b"class"),
        ]),
    )?;
    write(
        "Plain Speech.zip",
        zip(&[(
            "mod.json",
            br#"{"mod":{"name":"Plain Speech","version":"2.0","description":"Shorter wording in menus.","locale":{"messages.properties":{"menu.play":"Play"}}}}"#,
        )]),
    )?;
    write("Fanfare.zip", zip(&[("sound/fanfare.ogg", b"ogg")]))?;
    let inner_a = zip(&[("ui/a.png", b"1")]);
    let inner_b = zip(&[("sound/b.ogg", b"2")]);
    write(
        "Starter Pack.modpack",
        zip(&[
            ("mod.json", br#"{"mod":{"name":"Starter Pack","author":"Fixture Author","description":"A small collection."}}"#),
            ("mod.png", &green),
            ("one.zip", &inner_a),
            ("two.zip", &inner_b),
        ]),
    )?;
    write("Broken Mod.zip", b"this is not a zip file".to_vec())?;
    // A title that differs from the file name, with a picture named in mod.json.
    write(
        "hd-ui-v2-final.zip",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Crisp Menus","author":"Fixture Author","version":"2.1","icon":"art/logo.png","description":"Sharper buttons and menu frames."}}"#,
            ),
            ("art/logo.png", &rust),
            ("ui/menu.png", b"png"),
            // The same file as Sharp HUD's, so the page has a conflict to show.
            ("ui/hud.png", b"png-crisp"),
        ]),
    )?;
    // Long text everywhere: the title wraps to two lines, the rest is cut to one line.
    write(
        "Ultimate Texture Overhaul and Interface Remaster for Every Gate.zip",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Ultimate Texture Overhaul and Interface Remaster for Every Gate","author":"A Very Long Author Name Of The Fixture Maker","version":"12.0.3-beta.4","description":"Replaces every texture in the game with a hand-painted version, rebuilds the interface frames and adds a quieter colour palette to all menus and loading screens."}}"#,
            ),
            ("mod.png", &rust),
            ("texture/a.png", b"png"),
        ]),
    )?;
    // A name that fits on one line but is long, no picture, no author.
    write(
        "Extended Tooltips With Item Statistics.zip",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Extended Tooltips With Item Statistics","version":"1.0"}}"#,
            ),
            ("config/other.xml", b"x"),
        ]),
    )?;
    // Warnings from the scan: every file under rsrc/, and a protected game file.
    write(
        "Rsrc Prefixed.zip",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Prefixed Paths","description":"Its files sit under rsrc/ already."}}"#,
            ),
            ("rsrc/ui/x.png", b"png"),
        ]),
    )?;
    write(
        "Protected Items.zip",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Item Tweaks","author":"Fixture Author","version":"0.3","description":"Changes item data."}}"#,
            ),
            ("config/item.dat", b"protected"),
            ("item/x/model.dat", b"model"),
        ]),
    )?;
    // A modpack with a part that cannot be read: warned, the rest still counts.
    write(
        "Half Pack.modpack",
        zip(&[
            ("mod.json", br#"{"mod":{"name":"Half Pack"}}"#),
            ("good.zip", &inner_a),
            ("bad.zip", b"not a zip"),
        ]),
    )?;
    // Pictures that cannot be used: the page falls back to the placeholder.
    write(
        "Bad Picture.zip",
        zip(&[
            (
                "mod.json",
                br#"{"mod":{"name":"Bad Picture","version":"1.0"}}"#,
            ),
            ("mod.png", b"this is not an image"),
            ("ui/b.png", b"png"),
        ]),
    )?;
    mods::set_enabled(dir, "Fanfare.zip", false)?;
    mods::set_enabled(dir, "Plain Speech.zip", false)?;
    mods::set_enabled(dir, "Protected Items.zip", false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_are_encoded() {
        assert_eq!(
            file_url(Path::new("/home/me/My Mods")),
            "file:///home/me/My%20Mods"
        );
        assert_eq!(
            file_url(Path::new("C:\\Users\\me\\mods")),
            "file:///C:/Users/me/mods"
        );
        // The URL reads back as the same path.
        assert_eq!(
            local_path_from_url_text(&file_url(Path::new("/a b/ü"))),
            PathBuf::from("/a b/ü")
        );
    }

    fn listing_with_cache(dir: &Path, cache: &Path, version: Option<&str>) -> Vec<ItemView> {
        mods::list_mods(dir)
            .unwrap()
            .iter()
            .map(|m| item_view(dir, m, version, Some(cache)))
            .collect()
    }

    #[test]
    fn the_fixture_folder_lists_with_tags_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        let l = build_listing(dir.path(), Some("20261110044614"));
        assert!(l.error.is_none());
        let by = |f: &str| l.items.iter().find(|i| i.file == f).unwrap();
        assert_eq!(by("Sharp HUD.zip").tags, ["textures"]);
        assert_eq!(by("Sharp HUD.zip").author, "Fixture Author");
        assert_eq!(by("Damage Numbers.jar").tags, ["code"]);
        assert_eq!(by("Plain Speech.zip").tags, ["text"]);
        assert!(!by("Plain Speech.zip").enabled);
        assert_eq!(by("Starter Pack.modpack").parts, 2);
        assert_eq!(
            by("Starter Pack.modpack").tags,
            ["modpack", "textures", "audio"]
        );
        let broken = by("Broken Mod.zip");
        assert!(broken.tags.is_empty(), "a broken mod has no type");
        assert!(broken.error.is_some() && broken.warning);
        let counts: Vec<_> = l.tags.iter().map(|t| (t.id, t.count)).collect();
        assert!(counts.iter().all(|(id, _)| *id != "broken"));
        assert!(counts.contains(&("modpack", 2)));
    }

    #[test]
    fn titles_and_file_names_are_shown_once() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        let l = build_listing(dir.path(), None);
        let by = |f: &str| l.items.iter().find(|i| i.file == f).unwrap();
        // Same as the file name without its extension: no second line for the file.
        assert_eq!(by("Damage Numbers.jar").name, "Damage Numbers");
        assert_eq!(by("Damage Numbers.jar").file_note, "");
        // No mod.json at all: the title is the file name without the extension.
        assert_eq!(by("Fanfare.zip").name, "Fanfare");
        assert_eq!(by("Fanfare.zip").file_note, "");
        assert_eq!(by("Broken Mod.zip").name, "Broken Mod");
        assert_eq!(by("Broken Mod.zip").file_note, "");
        // A title that says something else keeps the file name as a note.
        assert_eq!(by("hd-ui-v2-final.zip").name, "Crisp Menus");
        assert_eq!(by("hd-ui-v2-final.zip").file_note, "hd-ui-v2-final.zip");
    }

    #[test]
    fn warnings_come_from_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        let l = build_listing(dir.path(), None);
        let by = |f: &str| l.items.iter().find(|i| i.file == f).unwrap();
        for f in [
            "Rsrc Prefixed.zip",
            "Protected Items.zip",
            "Half Pack.modpack",
            "Broken Mod.zip",
        ] {
            assert!(by(f).warning, "{f}");
        }
        for f in [
            "Sharp HUD.zip",
            "Damage Numbers.jar",
            "Fanfare.zip",
            "Starter Pack.modpack",
        ] {
            assert!(!by(f).warning, "{f}");
        }
        assert!(by("Half Pack.modpack").warnings[0].contains("bad.zip"));
    }

    #[test]
    fn pictures_are_cached_and_bad_ones_are_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        let items = listing_with_cache(dir.path(), cache.path(), None);
        let by = |f: &str| items.iter().find(|i| i.file == f).unwrap();
        for f in [
            "Sharp HUD.zip",
            "Starter Pack.modpack",
            "hd-ui-v2-final.zip",
        ] {
            let url = &by(f).image;
            let path = local_path_from_url_text(url);
            assert!(
                path.starts_with(cache.path()) && path.is_file(),
                "{f}: {url}"
            );
            assert!(std::fs::read(&path).unwrap().starts_with(b"\x89PNG"));
        }
        for f in [
            "Bad Picture.zip",
            "Fanfare.zip",
            "Broken Mod.zip",
            "Damage Numbers.jar",
        ] {
            assert_eq!(by(f).image, "", "{f}");
        }
        // The cache is not made when there is nowhere to put it.
        let none: Vec<_> = mods::list_mods(dir.path())
            .unwrap()
            .iter()
            .map(|m| item_view(dir.path(), m, None, None))
            .collect();
        assert!(none.iter().all(|i| i.image.is_empty()));
    }

    #[test]
    fn the_fixture_picture_is_a_valid_png() {
        let png = fixture_png(64, [1, 2, 3], [4, 5, 6]);
        let checked = skradle_core::mods::image::check(png).unwrap();
        assert_eq!((checked.width, checked.height), (64, 64));
    }

    #[test]
    fn tags_exist_only_for_what_is_installed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(build_listing(dir.path(), None).tags.is_empty());
        assert!(build_listing(&dir.path().join("missing"), None)
            .items
            .is_empty());
    }

    #[test]
    fn a_class_mod_for_another_version_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        // Rewrite the class mod with a version.
        let mut jar = skradle_core::jar::Jar::default();
        jar.set(
            "mod.json",
            br#"{"type":"class","pxVersion":"111"}"#.to_vec(),
        )
        .unwrap();
        jar.set("A.class", b"x".to_vec()).unwrap();
        std::fs::write(
            dir.path().join("Damage Numbers.jar"),
            jar.to_bytes().unwrap(),
        )
        .unwrap();
        let l = build_listing(dir.path(), Some("222"));
        assert!(
            l.items
                .iter()
                .find(|i| i.file == "Damage Numbers.jar")
                .unwrap()
                .incompatible
        );
        let l = build_listing(dir.path(), None);
        assert!(
            !l.items
                .iter()
                .find(|i| i.file == "Damage Numbers.jar")
                .unwrap()
                .incompatible
        );
    }

    #[test]
    fn adding_removing_and_switching() {
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(src.path()).unwrap();
        std::fs::write(src.path().join("notes.txt"), b"x").unwrap();
        let sources: Vec<String> = [
            "Sharp HUD.zip",
            "Broken Mod.zip",
            "notes.txt",
            "Sharp HUD.zip",
        ]
        .iter()
        .map(|n| file_url(&src.path().join(n)))
        .collect();
        let out = add_all(dir.path(), &sources);
        assert_eq!(out.ok, 2);
        assert_eq!(
            out.renamed,
            [("Sharp HUD.zip".into(), "Sharp HUD (2).zip".into())]
        );
        let failed: Vec<_> = out.failed.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(failed, ["Broken Mod.zip", "notes.txt"]);

        let files = vec!["Sharp HUD.zip".to_owned(), "missing.zip".to_owned()];
        let off = set_enabled_all(dir.path(), &files, false);
        assert_eq!(off.ok, 2, "a disabled entry for a missing mod is harmless");
        let listing = build_listing(dir.path(), None);
        assert!(
            !listing
                .items
                .iter()
                .find(|i| i.file == "Sharp HUD.zip")
                .unwrap()
                .enabled
        );
        let gone = remove_all(dir.path(), &files);
        assert_eq!(gone.ok, 1);
        assert_eq!(gone.failed.len(), 1);
        assert_eq!(build_listing(dir.path(), None).items.len(), 1);
    }

    #[test]
    fn a_damaged_state_file_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        std::fs::write(dir.path().join(".skradle-mods.json"), b"{ nope").unwrap();
        let l = build_listing(dir.path(), None);
        assert!(l.error.is_some() && l.items.is_empty());
    }

    #[test]
    fn engine_errors_become_plain_sentences() {
        let raw = "not a readable zip archive: invalid Zip archive: Could not find EOCD";
        assert_eq!(unusable(raw), "Can't be used. The file is not a valid zip.");
        assert_eq!(
            unusable("entry \"a.png\" is encrypted"),
            "Can't be used. The archive is password protected."
        );
        assert_eq!(
            unusable("No such file or directory (os error 2)"),
            "Can't be used. The file is missing."
        );
        assert_eq!(
            unusable("something odd"),
            "Can't be used. The file could not be read."
        );
        assert_eq!(
            plain_refusal("not a usable mod: not a readable zip archive: Could not find EOCD"),
            "The file is not a valid zip"
        );
        assert_eq!(
            plain_refusal("it is a folder; add the mod files inside it"),
            "It is a folder; add the mod files inside it"
        );
        assert_eq!(
            plain_note("bad.zip was skipped: not a readable zip archive: x"),
            "bad.zip was skipped: The file is not a valid zip"
        );
        assert_eq!(
            plain_note("mod.json is too large and was ignored"),
            "mod.json is too large and was ignored"
        );
        assert!(
            plain_pack_error("cannot read the modpack listing: tls handshake")
                .starts_with("Could not reach")
        );
        assert!(plain_pack_file("sha256 mismatch").contains("damaged"));
        for text in [
            unusable(raw),
            plain_pack_error("io error"),
            plain_folder_error("the mods state file is damaged: expected value"),
        ] {
            assert!(!text.contains("EOCD") && !text.contains("os error"));
        }
    }

    #[test]
    fn a_broken_mod_shows_words_and_keeps_the_raw_text_as_detail() {
        let dir = tempfile::tempdir().unwrap();
        write_fixtures(dir.path()).unwrap();
        let l = build_listing(dir.path(), None);
        let broken = l.items.iter().find(|i| i.file == "Broken Mod.zip").unwrap();
        let shown = broken.error.as_deref().unwrap();
        assert!(shown.starts_with("Can't be used. "), "{shown}");
        assert!(!shown.contains("EOCD"));
        assert!(!broken.error_detail.is_empty());
    }
}
