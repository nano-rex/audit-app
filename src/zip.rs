//! ZIP archives (PKWARE APPNOTE 6.3): writing deflated entries for workbooks, and reading them
//! back for imports.

use crate::flate::{crc32, deflate, inflate};

pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<(String, u32, u32, u32, u32, u16)>,
}

impl Default for ZipWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipWriter {
    pub fn new() -> ZipWriter {
        ZipWriter { out: Vec::new(), entries: Vec::new() }
    }

    /// Add a file; data already compressed (JPEG, PNG) is stored as it is.
    pub fn add(&mut self, name: &str, data: &[u8]) {
        let checksum = crc32(data);
        let packed = deflate(data);
        let (method, body): (u16, &[u8]) = if packed.len() < data.len() { (8, &packed) } else { (0, data) };
        let offset = self.out.len() as u32;
        // A fixed timestamp (1980-01-01) keeps identical workbooks byte-identical.
        let (time, date) = (0u16, 0x21u16);
        self.out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        self.out.extend_from_slice(&20u16.to_le_bytes());
        self.out.extend_from_slice(&0x0800u16.to_le_bytes()); // names are UTF-8
        self.out.extend_from_slice(&method.to_le_bytes());
        self.out.extend_from_slice(&time.to_le_bytes());
        self.out.extend_from_slice(&date.to_le_bytes());
        self.out.extend_from_slice(&checksum.to_le_bytes());
        self.out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        self.out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        self.out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(body);
        self.entries.push((name.to_string(), checksum, body.len() as u32, data.len() as u32, offset, method));
    }

    pub fn finish(mut self) -> Vec<u8> {
        let start = self.out.len() as u32;
        for (name, checksum, packed, size, offset, method) in &self.entries {
            self.out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            self.out.extend_from_slice(&20u16.to_le_bytes());
            self.out.extend_from_slice(&20u16.to_le_bytes());
            self.out.extend_from_slice(&0x0800u16.to_le_bytes());
            self.out.extend_from_slice(&method.to_le_bytes());
            self.out.extend_from_slice(&0u16.to_le_bytes());
            self.out.extend_from_slice(&0x21u16.to_le_bytes());
            self.out.extend_from_slice(&checksum.to_le_bytes());
            self.out.extend_from_slice(&packed.to_le_bytes());
            self.out.extend_from_slice(&size.to_le_bytes());
            self.out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            self.out.extend_from_slice(&[0u8; 8]); // extra, comment, disk, internal attributes
            self.out.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            self.out.extend_from_slice(&offset.to_le_bytes());
            self.out.extend_from_slice(name.as_bytes());
        }
        let size = self.out.len() as u32 - start;
        let count = self.entries.len() as u16;
        self.out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.out.extend_from_slice(&[0u8; 4]);
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&size.to_le_bytes());
        self.out.extend_from_slice(&start.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}

/// The files of a ZIP archive, by name, read from its central directory.
pub fn read_archive(data: &[u8], limit: usize) -> Result<Vec<(String, Vec<u8>)>, String> {
    let le16 = |at: usize| -> Result<usize, String> { data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or_else(|| "truncated archive".to_string()) };
    let le32 = |at: usize| -> Result<usize, String> { data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or_else(|| "truncated archive".to_string()) };
    let end = (0..data.len().saturating_sub(21)).rev().take(65_557).find(|&at| data[at..].starts_with(&[0x50, 0x4b, 5, 6])).ok_or("not a ZIP archive")?;
    let count = le16(end + 10)?;
    let mut at = le32(end + 16)?;
    let mut files = Vec::with_capacity(count);
    let mut total = 0usize;
    for _ in 0..count {
        if le32(at)? != 0x0201_4b50 {
            return Err("damaged ZIP directory".into());
        }
        let method = le16(at + 10)?;
        let packed = le32(at + 20)?;
        let size = le32(at + 24)?;
        let name_length = le16(at + 28)?;
        let extra = le16(at + 30)?;
        let comment = le16(at + 32)?;
        let offset = le32(at + 42)?;
        let name = String::from_utf8_lossy(data.get(at + 46..at + 46 + name_length).ok_or("truncated archive")?).into_owned();
        at += 46 + name_length + extra + comment;
        let local_name = le16(offset + 26)?;
        let local_extra = le16(offset + 28)?;
        let start = offset + 30 + local_name + local_extra;
        let body = data.get(start..start + packed).ok_or("truncated archive")?;
        total += size;
        if total > limit {
            return Err("the archive is too large".into());
        }
        let content = match method {
            0 => body.to_vec(),
            8 => inflate(body, size.max(1) * 2 + 1024)?,
            other => return Err(format!("unsupported ZIP compression method {other}")),
        };
        files.push((name, content));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut writer = ZipWriter::new();
        writer.add("a.txt", b"hello hello hello hello");
        writer.add("dir/b.bin", &[1, 2, 3]);
        let archive = writer.finish();
        let files = read_archive(&archive, 1 << 20).unwrap();
        assert_eq!(files[0], ("a.txt".to_string(), b"hello hello hello hello".to_vec()));
        assert_eq!(files[1].1, vec![1, 2, 3]);
    }
}
