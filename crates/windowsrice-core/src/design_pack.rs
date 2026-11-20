//! Design packages (`.ricepack`, Phase 12): a ZIP archive holding
//! `design.json` (RICE Design Code), an optional `preview.png` and approved
//! image and video assets under `assets/`.
//!
//! The reader is deliberately strict and small:
//! - only *stored* (uncompressed) entries are accepted, so an archive can
//!   never expand beyond its own size (no decompression bombs; PNG, JPEG and
//!   MP4 are already compressed);
//! - every entry name is checked (no absolute paths, drive letters,
//!   backslashes, `..`, symbolic links, duplicates or unexpected files);
//! - sizes and counts are limited, and every entry's CRC-32 is verified;
//! - every asset is checked by its file signature (not its extension), and
//!   image dimensions, video duration and video resolution are read from the
//!   file and limited;
//! - nothing in a package is ever executed.
//!
//! Imported assets are copied into the design library under generated
//! names; names from the package are never used as paths on disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::design_code::{
    Analysis, AnalyzeOptions, AssetKind, DesignCode, analyze, asset_kind_for, is_valid_package_path,
};

/// Largest package.
pub const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
/// Most entries in a package.
pub const MAX_ENTRIES: usize = 16;
/// Largest image file.
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
/// Largest video file.
pub const MAX_VIDEO_BYTES: usize = 48 * 1024 * 1024;
/// Largest image or video frame.
pub const MAX_WIDTH: u32 = 7680;
pub const MAX_HEIGHT: u32 = 4320;
/// Largest video frame (decoding cost).
pub const MAX_VIDEO_WIDTH: u32 = 3840;
pub const MAX_VIDEO_HEIGHT: u32 = 2160;
/// Longest video, milliseconds.
pub const MAX_VIDEO_MS: u64 = 120_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackError {
    #[error("the package is larger than {} MB", MAX_PACKAGE_BYTES / (1024 * 1024))]
    TooLarge,
    #[error("not a ZIP archive (or a damaged one): {0}")]
    NotZip(String),
    #[error("the package has more than {MAX_ENTRIES} files")]
    TooManyEntries,
    #[error("{0:?}: {1}")]
    Entry(String, String),
    #[error("the package has no design.json")]
    NoDesign,
    #[error("{0}")]
    Io(String),
}

/// A verified file of a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    pub name: String,
    pub data: Vec<u8>,
}

/// What the signature check found in an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaInfo {
    pub kind: AssetKind,
    pub width: u32,
    pub height: u32,
    /// Videos only.
    pub duration_ms: Option<u64>,
}

/// A read and checked package (its design code still needs [`analyze`]; the
/// result is in `analysis`).
#[derive(Debug, Clone)]
pub struct Package {
    pub design_text: String,
    pub preview: Option<Vec<u8>>,
    pub assets: Vec<(PackFile, MediaInfo)>,
    pub analysis: Analysis,
}

// ------------------------------------------------------------ CRC-32

fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        c = t[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
}
fn u32le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}
fn u32be(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}
fn u16be(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|x| u16::from_be_bytes([x[0], x[1]]))
}
fn u64be(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 8)
        .map(|x| u64::from_be_bytes([x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7]]))
}

/// An entry name allowed in a package.
fn valid_entry_name(n: &str) -> bool {
    n == "design.json"
        || n == "preview.png"
        || (is_valid_package_path(n) && asset_kind_for(n).is_some())
}

/// Reads the entries of a stored-only ZIP archive, checking everything the
/// module documentation lists.
pub fn read_zip(bytes: &[u8]) -> Result<Vec<PackFile>, PackError> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(PackError::TooLarge);
    }
    let bad = |m: &str| PackError::NotZip(m.to_owned());
    // End of central directory: the last record with the signature.
    let min = bytes.len().saturating_sub(22 + 0xFFFF);
    let eocd = (min..=bytes.len().saturating_sub(22))
        .rev()
        .find(|&i| u32le(bytes, i) == Some(0x0605_4B50))
        .ok_or_else(|| bad("no end-of-directory record"))?;
    let disk = u16le(bytes, eocd + 4).ok_or_else(|| bad("short record"))?;
    let count = usize::from(u16le(bytes, eocd + 10).ok_or_else(|| bad("short record"))?);
    let cd_size = u32le(bytes, eocd + 12).ok_or_else(|| bad("short record"))? as usize;
    let cd_off = u32le(bytes, eocd + 16).ok_or_else(|| bad("short record"))? as usize;
    if disk != 0 || cd_off == 0xFFFF_FFFF || count == 0xFFFF {
        return Err(bad("multi-part and ZIP64 archives are not supported"));
    }
    if count > MAX_ENTRIES {
        return Err(PackError::TooManyEntries);
    }
    if cd_off.checked_add(cd_size).is_none_or(|e| e > eocd) {
        return Err(bad("the directory points outside the file"));
    }
    let mut files = Vec::with_capacity(count);
    let mut names = HashSet::new();
    let mut p = cd_off;
    let mut total = 0usize;
    for _ in 0..count {
        if u32le(bytes, p) != Some(0x0201_4B50) {
            return Err(bad("damaged directory entry"));
        }
        let field = |o: usize| u16le(bytes, p + o).ok_or_else(|| bad("short entry"));
        let flags = field(8)?;
        let method = field(10)?;
        let crc = u32le(bytes, p + 16).ok_or_else(|| bad("short entry"))?;
        let csize = u32le(bytes, p + 20).ok_or_else(|| bad("short entry"))? as usize;
        let usize_ = u32le(bytes, p + 24).ok_or_else(|| bad("short entry"))? as usize;
        let nlen = usize::from(field(28)?);
        let xlen = usize::from(field(30)?);
        let clen = usize::from(field(32)?);
        let ext_attr = u32le(bytes, p + 38).ok_or_else(|| bad("short entry"))?;
        let local = u32le(bytes, p + 42).ok_or_else(|| bad("short entry"))? as usize;
        let raw_name = bytes
            .get(p + 46..p + 46 + nlen)
            .ok_or_else(|| bad("short entry name"))?;
        let name = std::str::from_utf8(raw_name)
            .map_err(|_| bad("an entry name is not UTF-8"))?
            .to_owned();
        p += 46 + nlen + xlen + clen;
        let err = |m: &str| PackError::Entry(name.clone(), m.to_owned());
        if name.ends_with('/') {
            // Directory entries carry no data; only assets/ is expected.
            if name != "assets/" {
                return Err(err("unexpected folder"));
            }
            continue;
        }
        if !valid_entry_name(&name) {
            return Err(err(
                "not an allowed file name (design.json, preview.png or assets/<name>)",
            ));
        }
        if !names.insert(name.clone()) {
            return Err(err("appears twice"));
        }
        // Unix mode in the high 16 bits: refuse symbolic links.
        if (ext_attr >> 16) & 0o170_000 == 0o120_000 {
            return Err(err("symbolic links are not accepted"));
        }
        if flags & 0x0001 != 0 {
            return Err(err("encrypted entries are not accepted"));
        }
        if method != 0 {
            return Err(err(
                "compressed entries are not accepted: create the package with windowsRICE (Export), which stores files uncompressed",
            ));
        }
        if csize != usize_ {
            return Err(err("stored size mismatch"));
        }
        total = total.saturating_add(usize_);
        if total > MAX_PACKAGE_BYTES {
            return Err(PackError::TooLarge);
        }
        // Local header: same name and sizes; data follows it.
        if u32le(bytes, local) != Some(0x0403_4B50) {
            return Err(err("damaged local header"));
        }
        let lnlen = usize::from(u16le(bytes, local + 26).ok_or_else(|| err("short header"))?);
        let lxlen = usize::from(u16le(bytes, local + 28).ok_or_else(|| err("short header"))?);
        if bytes.get(local + 30..local + 30 + lnlen) != Some(raw_name) {
            return Err(err("local and directory names differ"));
        }
        let start = local + 30 + lnlen + lxlen;
        let data = bytes
            .get(start..start + csize)
            .ok_or_else(|| err("data outside the file"))?;
        if start + csize > cd_off {
            return Err(err("data overlaps the directory"));
        }
        if crc32(data) != crc {
            return Err(err("checksum (CRC-32) mismatch: the file is damaged"));
        }
        files.push(PackFile {
            name,
            data: data.to_vec(),
        });
    }
    Ok(files)
}

/// Writes a stored-only ZIP archive (for Export).
pub fn write_zip(files: &[PackFile]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for f in files {
        let crc = crc32(&f.data);
        let off = out.len() as u32;
        let n = f.name.as_bytes();
        let size = f.data.len() as u32;
        // Local file header.
        out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&0u32.to_le_bytes()); // time and date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(n.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(n);
        out.extend_from_slice(&f.data);
        // Central directory entry.
        central.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // made by
        central.extend_from_slice(&20u16.to_le_bytes()); // needed
        central.extend_from_slice(&0x0800u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(n.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0u8; 8]); // extra, comment, disk, internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&off.to_le_bytes());
        central.extend_from_slice(n);
    }
    let cd_off = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

// ------------------------------------------------------------ media checks

fn png_size(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(..8)? != b"\x89PNG\r\n\x1a\n" || d.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((u32be(d, 16)?, u32be(d, 20)?))
}

fn jpeg_size(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(..3)? != [0xFF, 0xD8, 0xFF] {
        return None;
    }
    let mut i = 2;
    while i + 9 < d.len() {
        if d[i] != 0xFF {
            return None;
        }
        let marker = d[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        let len = usize::from(u16be(d, i + 2)?);
        // Start-of-frame markers (baseline, progressive, ...), not DHT/JPG/DAC.
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u32::from(u16be(d, i + 5)?);
            let w = u32::from(u16be(d, i + 7)?);
            return Some((w, h));
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

/// MP4: `ftyp` brand, duration from `mvhd`, frame size from the first video
/// `tkhd` with a non-zero size.
fn mp4_info(d: &[u8]) -> Option<(u32, u32, u64)> {
    if d.get(4..8)? != b"ftyp" {
        return None;
    }
    let brand = d.get(8..12)?;
    const BRANDS: [&[u8; 4]; 9] = [
        b"isom", b"iso2", b"iso4", b"iso5", b"iso6", b"mp41", b"mp42", b"avc1", b"M4V ",
    ];
    if !BRANDS.iter().any(|b| *b == brand) {
        return None;
    }
    let mut dur = None;
    let mut size = None;
    // Walks boxes; descends into moov and trak only (bounded depth).
    fn walk(
        d: &[u8],
        start: usize,
        end: usize,
        depth: u32,
        dur: &mut Option<u64>,
        size: &mut Option<(u32, u32)>,
    ) -> Option<()> {
        let mut i = start;
        while i + 8 <= end {
            let mut len = u32be(d, i)? as u64;
            let kind = d.get(i + 4..i + 8)?;
            let mut hdr = 8;
            if len == 1 {
                len = u64be(d, i + 8)?;
                hdr = 16;
            } else if len == 0 {
                len = (end - i) as u64;
            }
            let len = usize::try_from(len).ok()?;
            if len < hdr || i + len > end {
                return None;
            }
            let body = i + hdr;
            match kind {
                b"moov" | b"trak" if depth < 4 => {
                    walk(d, body, i + len, depth + 1, dur, size)?;
                }
                b"mvhd" => {
                    let v = *d.get(body)?;
                    let (scale, duration) = if v == 1 {
                        (u32be(d, body + 20)?, u64be(d, body + 24)?)
                    } else {
                        (u32be(d, body + 12)?, u64::from(u32be(d, body + 16)?))
                    };
                    if scale > 0 {
                        *dur = Some(duration.saturating_mul(1000) / u64::from(scale));
                    }
                }
                b"tkhd" => {
                    let v = *d.get(body)?;
                    let at = if v == 1 { body + 88 } else { body + 76 };
                    let w = u32be(d, at)? >> 16;
                    let h = u32be(d, at + 4)? >> 16;
                    if w > 0 && h > 0 && size.is_none() {
                        *size = Some((w, h));
                    }
                }
                _ => {}
            }
            i += len;
        }
        Some(())
    }
    walk(d, 0, d.len(), 0, &mut dur, &mut size)?;
    let (w, h) = size?;
    Some((w, h, dur?))
}

/// Checks an asset by its content: the signature must match the extension,
/// and size, dimensions and duration must be within the limits.
pub fn check_media(name: &str, data: &[u8]) -> Result<MediaInfo, String> {
    let kind = asset_kind_for(name).ok_or("only PNG, JPEG and MP4 files are accepted")?;
    let lower = name.to_ascii_lowercase();
    match kind {
        AssetKind::Image => {
            if data.len() > MAX_IMAGE_BYTES {
                return Err(format!(
                    "images may be at most {} MB",
                    MAX_IMAGE_BYTES / (1024 * 1024)
                ));
            }
            let dims = if lower.ends_with(".png") {
                png_size(data).ok_or("the file is not a PNG image (wrong signature)")?
            } else {
                jpeg_size(data).ok_or("the file is not a JPEG image (wrong signature)")?
            };
            if dims.0 == 0 || dims.1 == 0 || dims.0 > MAX_WIDTH || dims.1 > MAX_HEIGHT {
                return Err(format!(
                    "image size {}×{} is outside 1×1–{MAX_WIDTH}×{MAX_HEIGHT}",
                    dims.0, dims.1
                ));
            }
            Ok(MediaInfo {
                kind,
                width: dims.0,
                height: dims.1,
                duration_ms: None,
            })
        }
        AssetKind::Video => {
            if data.len() > MAX_VIDEO_BYTES {
                return Err(format!(
                    "videos may be at most {} MB",
                    MAX_VIDEO_BYTES / (1024 * 1024)
                ));
            }
            let (w, h, ms) = mp4_info(data).ok_or(
                "the file is not an MP4 video with a readable header (wrong signature or damaged)",
            )?;
            if w > MAX_VIDEO_WIDTH || h > MAX_VIDEO_HEIGHT {
                return Err(format!(
                    "video size {w}×{h} is larger than {MAX_VIDEO_WIDTH}×{MAX_VIDEO_HEIGHT}"
                ));
            }
            if ms == 0 || ms > MAX_VIDEO_MS {
                return Err(format!(
                    "video length {:.1} s is outside 0–{} s",
                    ms as f64 / 1000.0,
                    MAX_VIDEO_MS / 1000
                ));
            }
            Ok(MediaInfo {
                kind,
                width: w,
                height: h,
                duration_ms: Some(ms),
            })
        }
    }
}

// ------------------------------------------------------------ packages

/// Reads and checks a package. Container problems are errors; the design
/// code and its references are analysed into `analysis`.
pub fn read_package(bytes: &[u8], opts: &AnalyzeOptions<'_>) -> Result<Package, PackError> {
    let files = read_zip(bytes)?;
    let mut design = None;
    let mut preview = None;
    let mut assets = Vec::new();
    for f in files {
        match f.name.as_str() {
            "design.json" => {
                design = Some(String::from_utf8(f.data).map_err(|_| {
                    PackError::Entry("design.json".into(), "not UTF-8 text".into())
                })?);
            }
            "preview.png" => {
                png_size(&f.data).ok_or_else(|| {
                    PackError::Entry("preview.png".into(), "not a PNG image".into())
                })?;
                if f.data.len() > MAX_IMAGE_BYTES {
                    return Err(PackError::Entry("preview.png".into(), "too large".into()));
                }
                preview = Some(f.data);
            }
            _ => {
                let info = check_media(&f.name, &f.data)
                    .map_err(|m| PackError::Entry(f.name.clone(), m))?;
                assets.push((f, info));
            }
        }
    }
    let design_text = design.ok_or(PackError::NoDesign)?;
    let names: Vec<String> = assets.iter().map(|(f, _)| f.name.clone()).collect();
    let analysis = analyze(
        &design_text,
        &AnalyzeOptions {
            package_files: Some(&names),
            ..opts.clone()
        },
    );
    Ok(Package {
        design_text,
        preview,
        assets,
        analysis,
    })
}

/// Builds a package for a design and its asset files (name inside the
/// package, contents).
pub fn build_package(
    design: &DesignCode,
    preview: Option<&[u8]>,
    assets: &[(String, Vec<u8>)],
) -> Vec<u8> {
    let mut files = vec![PackFile {
        name: "design.json".into(),
        data: design.to_text().into_bytes(),
    }];
    if let Some(p) = preview {
        files.push(PackFile {
            name: "preview.png".into(),
            data: p.to_vec(),
        });
    }
    for (n, d) in assets {
        files.push(PackFile {
            name: n.clone(),
            data: d.clone(),
        });
    }
    write_zip(&files)
}

/// A content-derived library id (FNV-1a 64 of the bytes): the same file
/// imported twice gets the same id, and nothing from the package is used.
pub fn library_id(data: &[u8], kind: AssetKind) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    let k = match kind {
        AssetKind::Image => "img",
        AssetKind::Video => "vid",
    };
    format!("{k}-{h:016x}")
}

/// The library file for an asset id.
pub fn library_file(dir: &Path, id: &str, kind: AssetKind, source_name: &str) -> PathBuf {
    let ext = match kind {
        AssetKind::Video => "mp4",
        AssetKind::Image if source_name.to_ascii_lowercase().ends_with(".png") => "png",
        AssetKind::Image => "jpg",
    };
    dir.join(format!("{id}.{ext}"))
}

/// Copies a package's assets into the design library (`dir`) under
/// generated names and returns the design with its asset references
/// rewritten to the library ids. Writes go through a temporary file and a
/// rename. Only called for a package whose analysis allows applying.
pub fn import_assets(pkg: &Package, dir: &Path) -> Result<DesignCode, PackError> {
    let mut design = pkg.analysis.design.clone().ok_or(PackError::NoDesign)?;
    std::fs::create_dir_all(dir).map_err(|e| PackError::Io(e.to_string()))?;
    for a in &mut design.assets {
        let Some((f, info)) = pkg.assets.iter().find(|(f, _)| f.name == a.file) else {
            continue;
        };
        let id = library_id(&f.data, info.kind);
        let target = library_file(dir, &id, info.kind, &f.name);
        if !target.exists() {
            let tmp = target.with_extension("part");
            std::fs::write(&tmp, &f.data).map_err(|e| PackError::Io(e.to_string()))?;
            std::fs::rename(&tmp, &target).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                PackError::Io(e.to_string())
            })?;
        }
        for bar in std::iter::once(&mut design.top_bar).chain(design.bottom_bar.as_mut()) {
            if bar.style.background_asset.as_deref() == Some(a.id.as_str()) {
                bar.style.background_asset = Some(id.clone());
            }
        }
        a.id = id;
    }
    Ok(design)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::BarConfig;
    use crate::config::ThemeSelection;
    use crate::design_code::{AssetRef, CheckStatus};

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut d = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        d.extend_from_slice(&w.to_be_bytes());
        d.extend_from_slice(&h.to_be_bytes());
        d.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        d
    }

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(kind);
        b.extend_from_slice(body);
        b
    }

    fn mp4(w: u32, h: u32, ms: u32) -> Vec<u8> {
        let mut mvhd = vec![0u8; 100];
        mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&ms.to_be_bytes());
        let mut tkhd = vec![0u8; 84];
        tkhd[76..80].copy_from_slice(&(w << 16).to_be_bytes());
        tkhd[80..84].copy_from_slice(&(h << 16).to_be_bytes());
        let trak = bx(b"trak", &bx(b"tkhd", &tkhd));
        let mut moov_body = bx(b"mvhd", &mvhd);
        moov_body.extend_from_slice(&trak);
        let mut f = bx(b"ftyp", b"isom\0\0\0\0isom");
        f.extend_from_slice(&bx(b"moov", &moov_body));
        f
    }

    fn design_with(asset: Option<(&str, AssetKind)>) -> DesignCode {
        let mut d = DesignCode::new("Pack", ThemeSelection::default(), BarConfig::default());
        if let Some((file, kind)) = asset {
            d.assets.push(AssetRef {
                id: "bg".into(),
                kind,
                file: file.into(),
                trim_start_ms: None,
                trim_end_ms: None,
                poster_ms: None,
                r#loop: None,
                speed: None,
            });
            d.top_bar.style.background_asset = Some("bg".into());
        }
        d
    }

    #[test]
    fn round_trip_package_and_import() {
        let d = design_with(Some(("assets/sky.png", AssetKind::Image)));
        let bytes = build_package(
            &d,
            Some(&png(64, 16)),
            &[("assets/sky.png".into(), png(1920, 40))],
        );
        let pkg = read_package(&bytes, &AnalyzeOptions::default()).unwrap();
        assert!(pkg.analysis.can_apply(), "{:#?}", pkg.analysis.diagnostics);
        assert_eq!(pkg.analysis.assets, CheckStatus::Passed);
        assert_eq!(pkg.assets[0].1.width, 1920);
        let dir = tempfile::tempdir().unwrap();
        let imported = import_assets(&pkg, dir.path()).unwrap();
        let id = imported.top_bar.style.background_asset.clone().unwrap();
        assert!(id.starts_with("img-"), "{id}");
        assert!(dir.path().join(format!("{id}.png")).exists());
        // Only the generated file is written.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn video_header_is_read_and_limited() {
        let ok = check_media("assets/v.mp4", &mp4(1920, 1080, 15_000)).unwrap();
        assert_eq!(
            (ok.width, ok.height, ok.duration_ms),
            (1920, 1080, Some(15_000))
        );
        assert!(check_media("assets/v.mp4", &mp4(7680, 4320, 5000)).is_err());
        assert!(check_media("assets/v.mp4", &mp4(1280, 720, 600_000)).is_err());
    }

    #[test]
    fn signatures_must_match_extensions() {
        // A PNG renamed to .jpg, an executable renamed to .png, text as .mp4.
        assert!(check_media("assets/a.jpg", &png(10, 10)).is_err());
        assert!(check_media("assets/a.png", b"MZ\x90\0\x03\0\0\0").is_err());
        assert!(check_media("assets/a.mp4", b"hello world, not a video").is_err());
        assert!(check_media("assets/a.gif", b"GIF89a").is_err());
        assert!(check_media("assets/big.png", &png(10_000, 10)).is_err());
    }

    #[test]
    fn unsafe_entries_are_refused() {
        for name in [
            "../evil.png",
            "assets/../../evil.png",
            "C:/Windows/evil.png",
            "/etc/passwd",
            "assets\\evil.png",
            "tool.exe",
            "assets/run.ps1",
            "script.js",
        ] {
            let z = write_zip(&[
                PackFile {
                    name: "design.json".into(),
                    data: design_with(None).to_text().into_bytes(),
                },
                PackFile {
                    name: name.into(),
                    data: png(4, 4),
                },
            ]);
            let e = read_zip(&z).unwrap_err();
            assert!(matches!(e, PackError::Entry(..)), "{name}: {e:?}");
        }
    }

    #[test]
    fn damaged_compressed_symlinked_and_oversized_archives_are_refused() {
        let good = write_zip(&[PackFile {
            name: "design.json".into(),
            data: design_with(None).to_text().into_bytes(),
        }]);
        assert!(read_zip(&good).is_ok());
        // Flip a data byte: CRC mismatch.
        // Local header (30 bytes) and the name "design.json" (11), then data.
        let mut bad = good.clone();
        bad[30 + 11 + 5] ^= 0xFF;
        assert!(matches!(read_zip(&bad), Err(PackError::Entry(_, m)) if m.contains("CRC")));
        // Mark the entry compressed (method 8) in the central directory.
        let cd = good
            .windows(4)
            .rposition(|w| w == [0x50, 0x4B, 0x01, 0x02])
            .unwrap();
        let mut comp = good.clone();
        comp[cd + 10] = 8;
        assert!(matches!(read_zip(&comp), Err(PackError::Entry(_, m)) if m.contains("compressed")));
        // Unix symbolic link attributes.
        let mut link = good.clone();
        link[cd + 38..cd + 42].copy_from_slice(&((0o120_777u32) << 16).to_le_bytes());
        assert!(matches!(read_zip(&link), Err(PackError::Entry(_, m)) if m.contains("symbolic")));
        // Not a zip at all; too many entries.
        assert!(read_zip(b"just text").is_err());
        let many: Vec<PackFile> = (0..=MAX_ENTRIES)
            .map(|i| PackFile {
                name: format!("assets/a{i}.png"),
                data: png(2, 2),
            })
            .collect();
        assert_eq!(read_zip(&write_zip(&many)), Err(PackError::TooManyEntries));
    }

    #[test]
    fn package_design_problems_are_reported_not_hidden() {
        // The design refers to a file the package does not contain.
        let d = design_with(Some(("assets/missing.png", AssetKind::Image)));
        let bytes = build_package(&d, None, &[]);
        let pkg = read_package(&bytes, &AnalyzeOptions::default()).unwrap();
        assert!(!pkg.analysis.can_apply());
        assert_eq!(pkg.analysis.assets, CheckStatus::Failed);
        // No design.json.
        let z = write_zip(&[PackFile {
            name: "assets/a.png".into(),
            data: png(2, 2),
        }]);
        assert_eq!(
            read_package(&z, &AnalyzeOptions::default()).unwrap_err(),
            PackError::NoDesign
        );
    }

    #[test]
    fn crc32_matches_the_standard() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
