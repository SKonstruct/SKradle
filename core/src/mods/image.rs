//! A mod's picture. Mods are untrusted, so the picture is read like any other archive content: its name must be a
//! plain path inside the archive, its declared and actual size are capped, its bytes must be a PNG or a JPEG whose
//! header says a sensible size (no decoder ever sees a picture that claims to be enormous), and anything else is
//! refused. Nothing outside the archive is ever opened.
//!
//! Where it comes from, in order: the `icon` (or `image`) path in `mod.json`, relative to the archive root; then
//! `mod.png` at the archive root; then `mod.png` next to `mod.json`.

use std::io;

use crate::jar::{safe_entry_path, Archive};

use super::metadata::ModInfo;
use super::model::find_mod_json;

/// Largest picture accepted, in bytes.
pub const MAX_IMAGE_BYTES: u64 = 512 * 1024;
/// Largest width or height accepted, in pixels.
pub const MAX_IMAGE_SIDE: u32 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    Png,
    Jpeg,
}

impl ImageType {
    pub fn extension(self) -> &'static str {
        match self {
            ImageType::Png => "png",
            ImageType::Jpeg => "jpg",
        }
    }
}

/// A checked picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModImage {
    pub kind: ImageType,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProblem {
    /// The mod file could not be opened again.
    Unreadable,
    /// The name in `mod.json` is not a plain path inside the archive, or names nothing in it.
    BadPath,
    TooLarge,
    /// Not a PNG or a JPEG, or a header that does not hold together.
    NotAnImage,
    /// Wider or taller than [`MAX_IMAGE_SIDE`], or empty.
    BadSize,
}

impl std::fmt::Display for ImageProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ImageProblem::Unreadable => "the mod file could not be read",
            ImageProblem::BadPath => "the picture is not a file inside the mod",
            ImageProblem::TooLarge => "the picture is too large",
            ImageProblem::NotAnImage => "the picture is not a PNG or JPEG",
            ImageProblem::BadSize => "the picture has an unusable size",
        })
    }
}

impl std::error::Error for ImageProblem {}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn be16(b: &[u8]) -> u32 {
    u32::from(u16::from_be_bytes([b[0], b[1]]))
}

/// The type and pixel size a PNG or JPEG header declares; `None` for anything else.
pub fn sniff(bytes: &[u8]) -> Option<(ImageType, u32, u32)> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG) {
        // The first chunk must be IHDR: length 13, then width and height.
        let rest = bytes.get(PNG.len()..PNG.len() + 8 + 8)?;
        if &rest[4..8] != b"IHDR" || be32(&rest[0..4]) != 13 {
            return None;
        }
        return Some((ImageType::Png, be32(&rest[8..12]), be32(&rest[12..16])));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        // Walk the marker segments to the first start-of-frame.
        let mut i = 2;
        while i + 4 <= bytes.len() {
            if bytes[i] != 0xFF {
                return None;
            }
            let marker = bytes[i + 1];
            if marker == 0xFF {
                i += 1;
                continue;
            }
            if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                i += 2;
                continue;
            }
            let len = be16(&bytes[i + 2..i + 4]) as usize;
            if len < 2 {
                return None;
            }
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let frame = bytes.get(i + 4..i + 4 + 5)?;
                return Some((ImageType::Jpeg, be16(&frame[3..5]), be16(&frame[1..3])));
            }
            if marker == 0xDA {
                return None;
            }
            i += 2 + len;
        }
    }
    None
}

/// Checks the bytes of a picture.
pub fn check(bytes: Vec<u8>) -> Result<ModImage, ImageProblem> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(ImageProblem::TooLarge);
    }
    let (kind, width, height) = sniff(&bytes).ok_or(ImageProblem::NotAnImage)?;
    if width == 0 || height == 0 || width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE {
        return Err(ImageProblem::BadSize);
    }
    Ok(ModImage {
        kind,
        width,
        height,
        bytes,
    })
}

/// The picture of the mod in `archive`, whose `mod.json` said `info`.
pub fn read_image_from<R: io::Read + io::Seek>(
    archive: &mut Archive<R>,
    info: &ModInfo,
) -> Result<Option<ModImage>, ImageProblem> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(icon) = &info.icon {
        match safe_entry_path(icon) {
            Ok((path, false)) => candidates.push(path),
            // A name that is not a plain relative path is refused, not quietly replaced by the default.
            _ => return Err(ImageProblem::BadPath),
        }
    }
    candidates.push("mod.png".to_owned());
    if let Some(index) = find_mod_json(archive) {
        if let Some(dir) = archive
            .entries()
            .iter()
            .find(|e| e.index == index)
            .and_then(|e| e.path.rsplit_once('/'))
            .map(|(dir, _)| dir.to_owned())
        {
            candidates.push(format!("{dir}/mod.png"));
        }
    }
    let explicit = info.icon.is_some();
    for (n, name) in candidates.iter().enumerate() {
        let Some(entry) = archive.find(name).filter(|e| !e.is_dir).cloned() else {
            if n == 0 && explicit {
                return Err(ImageProblem::BadPath);
            }
            continue;
        };
        if entry.size > MAX_IMAGE_BYTES {
            return Err(ImageProblem::TooLarge);
        }
        let bytes = archive
            .read(entry.index)
            .map_err(|_| ImageProblem::Unreadable)?;
        return check(bytes).map(Some);
    }
    Ok(None)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::jar::testutil::simple_zip;
    use crate::jar::Limits;

    /// A PNG header (signature + IHDR) that declares `w` by `h`; enough for the checks, not a decodable picture.
    pub fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        v
    }

    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0];
        v.extend([0xFF, 0xC0, 0, 11, 8]);
        v.extend(h.to_be_bytes());
        v.extend(w.to_be_bytes());
        v.extend([1, 1, 0x11, 0]);
        v
    }

    fn read(
        entries: &[(&str, &[u8])],
        icon: Option<&str>,
    ) -> Result<Option<ModImage>, ImageProblem> {
        let mut archive = Archive::from_bytes(simple_zip(entries), &Limits::MOD).unwrap();
        let info = ModInfo {
            icon: icon.map(str::to_owned),
            ..ModInfo::default()
        };
        read_image_from(&mut archive, &info)
    }

    #[test]
    fn sniffs_png_and_jpeg_sizes() {
        assert_eq!(sniff(&png(64, 32)), Some((ImageType::Png, 64, 32)));
        assert_eq!(sniff(&jpeg(48, 40)), Some((ImageType::Jpeg, 48, 40)));
        assert_eq!(sniff(b"GIF89a....."), None);
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n"), None);
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF]), None);
        assert_eq!(sniff(&[]), None);
    }

    #[test]
    fn mod_png_at_the_root_is_the_default() {
        let img = png(64, 64);
        let got = read(&[("mod.png", &img), ("a/b.dat", b"1")], None)
            .unwrap()
            .unwrap();
        assert_eq!((got.kind, got.width, got.height), (ImageType::Png, 64, 64));
        assert_eq!(got.bytes, img);
        assert_eq!(read(&[("a/b.dat", b"1")], None), Ok(None));
    }

    #[test]
    fn mod_json_icon_wins_and_may_be_a_jpeg() {
        let jpg = jpeg(32, 32);
        let got = read(
            &[("art/logo.jpg", &jpg), ("mod.png", &png(8, 8))],
            Some("art/logo.jpg"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(got.kind, ImageType::Jpeg);
    }

    #[test]
    fn a_mod_file_gives_its_picture_and_the_icon_is_not_installed() {
        use crate::mods::model::Mod;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.zip");
        let img = png(20, 20);
        std::fs::write(
            &path,
            simple_zip(&[
                ("mod.json", br#"{"mod":{"icon":"art/logo.png"}}"#),
                ("art/logo.png", &img),
                ("a/b.dat", b"1"),
            ]),
        )
        .unwrap();
        let m = Mod::open(&path).unwrap();
        assert_eq!(m.read_image(&path).unwrap().unwrap().bytes, img);
        let files: Vec<_> = m.parts[0].files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(files, ["a/b.dat"]);
    }

    #[test]
    fn mod_png_beside_a_nested_mod_json() {
        let img = png(16, 16);
        let got = read(&[("pack/mod.json", b"{}"), ("pack/mod.png", &img)], None).unwrap();
        assert!(got.is_some());
    }

    #[test]
    fn paths_outside_the_archive_are_refused() {
        let img = png(8, 8);
        for bad in [
            "../x.png",
            "/etc/passwd",
            "a\\b.png",
            "C:/x.png",
            "a/../../b.png",
        ] {
            assert_eq!(
                read(&[("mod.png", &img)], Some(bad)),
                Err(ImageProblem::BadPath),
                "{bad}"
            );
        }
        // A named picture that is not in the archive is not replaced by mod.png.
        assert_eq!(
            read(&[("mod.png", &img)], Some("missing.png")),
            Err(ImageProblem::BadPath)
        );
        // A folder is not a picture.
        assert_eq!(
            read(&[("art/x.png", &img)], Some("art")),
            Err(ImageProblem::BadPath)
        );
    }

    #[test]
    fn oversized_or_fake_pictures_are_refused() {
        let mut big = png(8, 8);
        big.resize(MAX_IMAGE_BYTES as usize + 1, 0);
        assert_eq!(
            read(&[("mod.png", &big)], None),
            Err(ImageProblem::TooLarge)
        );
        assert_eq!(
            read(&[("mod.png", b"not a picture")], None),
            Err(ImageProblem::NotAnImage)
        );
        assert_eq!(
            read(&[("mod.png", &png(4000, 10))], None),
            Err(ImageProblem::BadSize)
        );
        assert_eq!(
            read(&[("mod.png", &png(0, 10))], None),
            Err(ImageProblem::BadSize)
        );
        assert_eq!(
            read(&[("mod.png", &jpeg(5000, 5000))], None),
            Err(ImageProblem::BadSize)
        );
    }
}
