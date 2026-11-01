//! Validated private image bytes stored as BLOBs in the application database.
//!
//! The browser shrinks photos before uploading them (web/js/utils.js), so the server keeps the
//! bytes it is given. It checks that they are a PNG, JPEG, or WebP image of sensible size by
//! reading the format's own header (and, for PNG, every chunk's checksum), and addresses each
//! image by the SHA-256 of its content.

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::clock;
use crate::crypto::{base64_decode, sha256_hex};
use crate::db::{self, Db};
use crate::error::{bad, R};
use crate::json::{obj, Value};
use crate::params;
use crate::sqlite::Sql;

/// What a client may send; the page sends far less.
pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
/// What is kept: photos arrive as small JPEGs, signatures as small PNGs.
pub const MAX_STORED_BYTES: usize = 2 * 1024 * 1024;
const MAX_PIXELS: u64 = 40_000_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Webp => "image/webp",
        }
    }
}

/// An image's format and dimensions, read from its header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Header {
    pub format: Format,
    pub width: u32,
    pub height: u32,
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32)
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 3).map(|b| b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16)
}

fn png_header(bytes: &[u8]) -> Option<Header> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut at = 8;
    let mut header = None;
    loop {
        let length = be32(bytes, at)? as usize;
        let kind = bytes.get(at + 4..at + 8)?;
        let data = bytes.get(at + 8..at + 8 + length)?;
        let crc = be32(bytes, at + 8 + length)?;
        if crate::flate::crc32(&bytes[at + 4..at + 8 + length]) != crc {
            return None;
        }
        if header.is_none() {
            if kind != b"IHDR" || length != 13 {
                return None;
            }
            header = Some(Header { format: Format::Png, width: be32(data, 0)?, height: be32(data, 4)? });
        }
        if kind == b"IEND" {
            return header;
        }
        at += 12 + length;
    }
}

fn jpeg_header(bytes: &[u8]) -> Option<Header> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut at = 2;
    loop {
        while bytes.get(at) == Some(&0xFF) && bytes.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if bytes.get(at) != Some(&0xFF) {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        at += 2;
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        let length = bytes.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)?;
        if length < 2 {
            return None;
        }
        let segment = bytes.get(at + 2..at + length)?;
        let frame = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if frame {
            let height = segment.get(1..3).map(|b| u16::from_be_bytes([b[0], b[1]]) as u32)?;
            let width = segment.get(3..5).map(|b| u16::from_be_bytes([b[0], b[1]]) as u32)?;
            // The image data follows; a JPEG cut short before its end marker is refused.
            if !bytes.windows(2).rev().take(64).any(|pair| pair == [0xFF, 0xD9]) {
                return None;
            }
            return Some(Header { format: Format::Jpeg, width, height });
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        at += length;
    }
}

fn webp_header(bytes: &[u8]) -> Option<Header> {
    if bytes.len() < 30 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if size + 8 > bytes.len() || size < 4 {
        return None;
    }
    let chunk = &bytes[12..16];
    let data = &bytes[20..];
    let (width, height) = match chunk {
        b"VP8 " => {
            if data.get(3..6)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            (le16(data, 6)? & 0x3fff, le16(data, 8)? & 0x3fff)
        }
        b"VP8L" => {
            if *data.first()? != 0x2f {
                return None;
            }
            let bits = u32::from_le_bytes([*data.get(1)?, *data.get(2)?, *data.get(3)?, *data.get(4)?]);
            ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
        }
        b"VP8X" => (le24(data, 4)? + 1, le24(data, 7)? + 1),
        _ => return None,
    };
    Some(Header { format: Format::Webp, width, height })
}

/// The image's format and size, or None when the bytes are not a whole PNG, JPEG, or WebP image.
pub fn read_header(bytes: &[u8]) -> Option<Header> {
    png_header(bytes).or_else(|| jpeg_header(bytes)).or_else(|| webp_header(bytes))
}

pub fn validate_identifier(identifier: &str) -> R<()> {
    let valid = identifier.split_once('.').is_some_and(|(hash, extension)| {
        hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) && matches!(extension, "png" | "jpg" | "webp")
    });
    if valid {
        Ok(())
    } else {
        Err(bad("Invalid image reference"))
    }
}

fn ready() -> &'static Mutex<Vec<(std::path::PathBuf, u64)>> {
    static READY: OnceLock<Mutex<Vec<(std::path::PathBuf, u64)>>> = OnceLock::new();
    READY.get_or_init(|| Mutex::new(Vec::new()))
}

fn inode(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|meta| meta.ino()).unwrap_or(0)
}

pub fn ensure_schema(db: &Db) -> R<()> {
    let key = (db.path.clone(), inode(&db.path));
    let mut prepared = ready().lock().unwrap_or_else(|poison| poison.into_inner());
    if prepared.contains(&key) {
        return Ok(());
    }
    db.exec(
        "CREATE TABLE IF NOT EXISTS media_images (id TEXT PRIMARY KEY, mime_type TEXT NOT NULL, content BLOB NOT NULL, byte_size INTEGER NOT NULL, created_at INTEGER NOT NULL, CHECK(typeof(content) = 'blob'), CHECK(length(content) = byte_size))",
        params![],
    )?;
    // The schema is ready once this is committed; the key is recorded for the database's file.
    prepared.push((db.path.clone(), inode(&db.path)));
    Ok(())
}

pub fn read(db: &Db, identifier: &str) -> R<Option<(Vec<u8>, String)>> {
    validate_identifier(identifier)?;
    ensure_schema(db)?;
    Ok(db.row("SELECT content, mime_type FROM media_images WHERE id = ?", params![identifier])?.map(|row| {
        let content = match row.get("content") {
            Sql::Blob(bytes) => bytes.clone(),
            other => other.to_value().py_str().into_bytes(),
        };
        (content, row.str("mime_type"))
    }))
}

pub fn exists(db: &Db, identifier: &str) -> R<bool> {
    validate_identifier(identifier)?;
    ensure_schema(db)?;
    db.exists("SELECT 1 FROM media_images WHERE id = ?", params![identifier])
}

/// Keep an image; returns its identifier and type.
pub fn put(db: &Db, content: &[u8]) -> R<(String, &'static str)> {
    if content.len() > MAX_UPLOAD_BYTES {
        return Err(bad("Each image must be at most 10 MiB"));
    }
    let header = read_header(content).ok_or_else(|| bad("The uploaded file is not a valid image"))?;
    if header.width == 0 || header.height == 0 || header.width as u64 * header.height as u64 > MAX_PIXELS {
        return Err(bad("Use PNG, JPEG, or WebP images under 40 megapixels"));
    }
    if content.len() > MAX_STORED_BYTES {
        return Err(bad("This image is too large to keep; the page shrinks photos before uploading, so reload the page and try again"));
    }
    ensure_schema(db)?;
    let identifier = format!("{}.{}", sha256_hex(content), header.format.extension());
    db.exec(
        "INSERT OR IGNORE INTO media_images(id, mime_type, content, byte_size, created_at) VALUES (?, ?, ?, ?, ?)",
        params![identifier, header.format.mime(), content.to_vec(), content.len() as i64, clock::now_ms()],
    )?;
    Ok((identifier, header.format.mime()))
}

/// Store a data: URL image and describe it as records keep it.
pub fn store(db: &Db, value: &Value, name: &Value) -> R<Value> {
    let text = value.as_str().unwrap_or("");
    if !text.starts_with("data:image/") || !text.contains(";base64,") {
        return Err(bad("Upload a PNG, JPEG, or WebP image"));
    }
    let encoded = text.split_once(";base64,").map(|(_, rest)| rest).unwrap_or("");
    if encoded.len() > MAX_UPLOAD_BYTES * 4 / 3 + 4 {
        return Err(bad("Each image must be at most 10 MiB"));
    }
    let content = base64_decode(encoded).map_err(|error| bad(format!("The image data is not valid base64: {error}")))?;
    let (identifier, mime) = put(db, &content)?;
    let name = name.py_str();
    let base = name.rsplit('/').next().unwrap_or(&name);
    Ok(obj([
        ("id", Value::str(&identifier)),
        ("url", Value::str(format!("/api/media/{identifier}"))),
        ("name", Value::str(base.chars().take(200).collect::<String>())),
        ("type", mime.into()),
        ("size", Value::Int(content.len() as i64)),
    ]))
}

/// Replace every uploaded image in a payload (data: URLs, alone or as dataUrl / markedDataUrl of a
/// photo) by the stored image's reference, and check that every referenced image exists.
pub fn normalize(db: &Db, value: &Value) -> R<Value> {
    match value {
        Value::Array(items) => Ok(Value::Array(items.iter().map(|item| normalize(db, item)).collect::<R<Vec<_>>>()?)),
        Value::Str(text) if text.starts_with("data:image/") => store(db, value, &Value::str("photo")),
        Value::Object(items) => {
            let mut result = Value::object();
            for (key, item) in items {
                if key != "dataUrl" && key != "markedDataUrl" {
                    result.set(key, normalize(db, item)?);
                }
            }
            if value.at("dataUrl").truthy() {
                let name = value.get("name").cloned().unwrap_or_else(|| "photo".into());
                for (key, item) in store(db, value.at("dataUrl"), &name)?.entries() {
                    result.set(key, item.clone());
                }
            }
            if value.at("markedDataUrl").truthy() {
                let name = value.get("markedName").cloned().unwrap_or_else(|| "marked-photo.png".into());
                let marked = store(db, value.at("markedDataUrl"), &name)?;
                result.set("markedUrl", marked.at("url").clone());
                result.set("markedId", marked.at("id").clone());
                result.set("markedName", marked.at("name").clone());
            }
            for key in ["url", "markedUrl"] {
                if let Some(url) = result.get(key) {
                    let found = match url.as_str().and_then(|text| text.strip_prefix("/api/media/")) {
                        Some(identifier) => validate_identifier(identifier).is_ok() && exists(db, identifier)?,
                        None => false,
                    };
                    if !found {
                        return Err(bad("Image reference was not found; upload the image again"));
                    }
                }
            }
            Ok(result)
        }
        other => Ok(other.clone()),
    }
}

/// The bytes of a photo (or its marked copy) for an export.
pub fn image_bytes(db: &Db, image: &Value, marked: bool) -> R<Option<Vec<u8>>> {
    if !image.is_object() {
        return Ok(None);
    }
    let url = image.at(if marked { "markedUrl" } else { "url" });
    if let Some(identifier) = url.as_str().and_then(|text| text.strip_prefix("/api/media/")) {
        if validate_identifier(identifier).is_err() {
            return Ok(None);
        }
        return Ok(read(db, identifier)?.map(|(bytes, _)| bytes));
    }
    let raw = image.at(if marked { "markedDataUrl" } else { "dataUrl" });
    if raw.truthy() {
        let stored = store(db, raw, image.get("name").unwrap_or(&Value::str("photo")))?;
        return Ok(read(db, &stored.at("id").py_str())?.map(|(bytes, _)| bytes));
    }
    Ok(None)
}

/// Copy legacy image files (data/media/<sha256>.<ext>) into the database; the files are kept.
pub fn migrate_directory(db_path: &Path, directory: &Path) -> R<usize> {
    if !directory.is_dir() {
        return Ok(0);
    }
    let mut names: Vec<_> = std::fs::read_dir(directory)?.filter_map(|entry| entry.ok()).map(|entry| entry.path()).collect();
    names.sort();
    let database = db::open(db_path)?;
    let mut imported = 0;
    let result = (|| -> R<usize> {
        for path in names {
            let Some(name) = path.file_name().and_then(|name| name.to_str()).map(str::to_string) else { continue };
            if !path.is_file() || validate_identifier(&name).is_err() || exists(&database, &name)? {
                continue;
            }
            let content = std::fs::read(&path)?;
            let stem = name.split('.').next().unwrap_or("");
            if sha256_hex(&content) != stem {
                return Err(bad(format!("Legacy image integrity check failed: {name}")));
            }
            let (identifier, _) = put(&database, &content)?;
            if identifier != name {
                return Err(bad(format!("Legacy image format does not match its filename: {name}")));
            }
            imported += 1;
        }
        Ok(imported)
    })();
    db::finish(&database, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut chunk = |kind: &[u8], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend_from_slice(data);
            out.extend_from_slice(&body);
            out.extend_from_slice(&crate::flate::crc32(&body).to_be_bytes());
        };
        let mut ihdr = width.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(b"IHDR", &ihdr);
        chunk(b"IDAT", &crate::flate::zlib(&vec![0u8; (1 + width as usize * 3) * height as usize]));
        chunk(b"IEND", &[]);
        out
    }

    #[test]
    fn headers() {
        let image = png(3, 2);
        assert_eq!(read_header(&image), Some(Header { format: Format::Png, width: 3, height: 2 }));
        let mut broken = image.clone();
        let last = broken.len() - 20;
        broken[last] ^= 1;
        assert_eq!(read_header(&broken), None);
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x10, 0x00, 0x20, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xD9];
        assert_eq!(read_header(&jpeg), Some(Header { format: Format::Jpeg, width: 32, height: 16 }));
        assert_eq!(read_header(b"GIF89a"), None);
        assert!(validate_identifier(&format!("{}.png", "a".repeat(64))).is_ok());
        assert!(validate_identifier("../x.png").is_err());
    }
}
