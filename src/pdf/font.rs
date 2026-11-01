//! TrueType fonts: the tables a PDF needs to measure text and embed the font whole
//! (cmap, hmtx, head, hhea, maxp, OS/2, post, name).

use std::collections::HashMap;

pub struct Font {
    pub data: Vec<u8>,
    pub name: String,
    pub units_per_em: f64,
    pub ascent: i16,
    pub descent: i16,
    pub cap_height: i16,
    pub bbox: [i16; 4],
    pub italic_angle: f64,
    advances: Vec<u16>,
    glyphs: HashMap<u32, u16>,
}

fn be16(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let count = be16(data, 4)? as usize;
    for index in 0..count {
        let entry = 12 + index * 16;
        if data.get(entry..entry + 4)? == tag {
            let offset = be32(data, entry + 8)? as usize;
            let length = be32(data, entry + 12)? as usize;
            return data.get(offset..offset + length);
        }
    }
    None
}

fn read_cmap(cmap: &[u8]) -> Option<HashMap<u32, u16>> {
    let count = be16(cmap, 2)? as usize;
    let mut best: Option<(u8, usize)> = None;
    for index in 0..count {
        let record = 4 + index * 8;
        let platform = be16(cmap, record)?;
        let encoding = be16(cmap, record + 2)?;
        let offset = be32(cmap, record + 4)? as usize;
        let format = be16(cmap, offset)?;
        let rank = match (platform, encoding, format) {
            (3, 10, 12) | (0, 4, 12) | (0, 6, 12) => 3,
            (3, 1, 4) | (0, 3, 4) => 2,
            (0, _, 4) => 1,
            _ => 0,
        };
        if rank > 0 && best.is_none_or(|(known, _)| rank > known) {
            best = Some((rank, offset));
        }
    }
    let (_, offset) = best?;
    let sub = &cmap[offset..];
    let mut glyphs = HashMap::new();
    match be16(sub, 0)? {
        4 => {
            let segments = be16(sub, 6)? as usize / 2;
            let ends = 14;
            let starts = ends + segments * 2 + 2;
            let deltas = starts + segments * 2;
            let ranges = deltas + segments * 2;
            for segment in 0..segments {
                let end = be16(sub, ends + segment * 2)? as u32;
                let start = be16(sub, starts + segment * 2)? as u32;
                let delta = be16(sub, deltas + segment * 2)?;
                let range = be16(sub, ranges + segment * 2)? as usize;
                if start == 0xFFFF {
                    continue;
                }
                for code in start..=end {
                    let glyph = if range == 0 {
                        (code as u16).wrapping_add(delta)
                    } else {
                        let at = ranges + segment * 2 + range + (code - start) as usize * 2;
                        match be16(sub, at)? {
                            0 => 0,
                            glyph => glyph.wrapping_add(delta),
                        }
                    };
                    if glyph != 0 {
                        glyphs.insert(code, glyph);
                    }
                }
            }
        }
        12 => {
            let groups = be32(sub, 12)? as usize;
            for group in 0..groups {
                let at = 16 + group * 12;
                let (start, end, first) = (be32(sub, at)?, be32(sub, at + 4)?, be32(sub, at + 8)?);
                for code in start..=end.min(start + 0x10000) {
                    glyphs.insert(code, (first + code - start) as u16);
                }
            }
        }
        _ => return None,
    }
    Some(glyphs)
}

fn read_name(name: &[u8]) -> Option<String> {
    let count = be16(name, 2)? as usize;
    let strings = be16(name, 4)? as usize;
    for index in 0..count {
        let record = 6 + index * 12;
        let (platform, id, length, offset) = (be16(name, record)?, be16(name, record + 6)?, be16(name, record + 8)? as usize, be16(name, record + 10)? as usize);
        if id != 6 {
            continue;
        }
        let bytes = name.get(strings + offset..strings + offset + length)?;
        let text = if platform == 3 || platform == 0 {
            String::from_utf16_lossy(&bytes.chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect::<Vec<_>>())
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        };
        let clean: String = text.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        if !clean.is_empty() {
            return Some(clean);
        }
    }
    None
}

impl Font {
    pub fn parse(data: Vec<u8>) -> Option<Font> {
        let head = table(&data, b"head")?;
        let hhea = table(&data, b"hhea")?;
        let maxp = table(&data, b"maxp")?;
        let hmtx = table(&data, b"hmtx")?;
        let units_per_em = be16(head, 18)? as f64;
        let bbox = [be16(head, 36)? as i16, be16(head, 38)? as i16, be16(head, 40)? as i16, be16(head, 42)? as i16];
        let glyph_count = be16(maxp, 4)? as usize;
        let metrics = be16(hhea, 34)? as usize;
        let mut advances = Vec::with_capacity(glyph_count);
        for glyph in 0..glyph_count {
            advances.push(be16(hmtx, glyph.min(metrics.saturating_sub(1)) * 4)?);
        }
        let (mut ascent, mut descent, mut cap_height) = (be16(hhea, 4)? as i16, be16(hhea, 6)? as i16, 0i16);
        if let Some(os2) = table(&data, b"OS/2") {
            if let (Some(typo_ascent), Some(typo_descent)) = (be16(os2, 68), be16(os2, 70)) {
                ascent = typo_ascent as i16;
                descent = typo_descent as i16;
            }
            if be16(os2, 0).unwrap_or(0) >= 2 {
                cap_height = be16(os2, 88).unwrap_or(0) as i16;
            }
        }
        if cap_height == 0 {
            cap_height = (ascent as f64 * 0.7) as i16;
        }
        let italic_angle = table(&data, b"post").and_then(|post| be32(post, 4)).map(|fixed| fixed as i32 as f64 / 65536.0).unwrap_or(0.0);
        let glyphs = read_cmap(table(&data, b"cmap")?)?;
        let name = table(&data, b"name").and_then(read_name).unwrap_or_else(|| "EmbeddedFont".into());
        Some(Font { data, name, units_per_em, ascent, descent, cap_height, bbox, italic_angle, advances, glyphs })
    }

    pub fn glyph(&self, ch: char) -> Option<u16> {
        self.glyphs.get(&(ch as u32)).copied()
    }

    pub fn has(&self, ch: char) -> bool {
        self.glyph(ch).is_some()
    }

    /// Advance of a glyph in thousandths of the font size (PDF glyph space).
    pub fn advance_milli(&self, glyph: u16) -> f64 {
        self.advances.get(glyph as usize).copied().unwrap_or(0) as f64 * 1000.0 / self.units_per_em
    }

    pub fn scale(&self, value: i16) -> f64 {
        value as f64 * 1000.0 / self.units_per_em
    }

    /// The font with only these glyphs' outlines kept (and glyph 0, and the parts of composite
    /// glyphs); every other glyph keeps its number but has no outline, so a PDF can still address
    /// glyphs by their original ids. Tables a PDF does not need (layout, kerning) are left out.
    pub fn subset(&self, used: &[u16]) -> Vec<u8> {
        self.try_subset(used).unwrap_or_else(|| self.data.clone())
    }

    fn try_subset(&self, used: &[u16]) -> Option<Vec<u8>> {
        let data = &self.data;
        let head = table(data, b"head")?;
        let long = be16(head, 50)? == 1;
        let loca = table(data, b"loca")?;
        let glyf = table(data, b"glyf")?;
        let count = self.advances.len();
        let offset = |glyph: usize| -> Option<usize> {
            if long {
                be32(loca, glyph * 4).map(|value| value as usize)
            } else {
                be16(loca, glyph * 2).map(|value| value as usize * 2)
            }
        };
        let mut keep = vec![false; count];
        let mut pending: Vec<usize> = used.iter().map(|glyph| *glyph as usize).filter(|glyph| *glyph < count).collect();
        pending.push(0);
        while let Some(glyph) = pending.pop() {
            if keep[glyph] {
                continue;
            }
            keep[glyph] = true;
            let (start, end) = (offset(glyph)?, offset(glyph + 1)?);
            let outline = glyf.get(start..end)?;
            if outline.len() >= 10 && (be16(outline, 0)? as i16) < 0 {
                // A composite glyph: follow its components.
                let mut at = 10;
                loop {
                    let flags = be16(outline, at)?;
                    let component = be16(outline, at + 2)? as usize;
                    if component < count {
                        pending.push(component);
                    }
                    at += 4 + if flags & 1 != 0 { 4 } else { 2 };
                    at += if flags & 8 != 0 { 2 } else if flags & 0x40 != 0 { 4 } else if flags & 0x80 != 0 { 8 } else { 0 };
                    if flags & 0x20 == 0 {
                        break;
                    }
                }
            }
        }
        let mut new_glyf = Vec::new();
        let mut new_loca = Vec::with_capacity((count + 1) * 4);
        for glyph in 0..count {
            new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
            if keep[glyph] {
                new_glyf.extend_from_slice(glyf.get(offset(glyph)?..offset(glyph + 1)?)?);
                while new_glyf.len() % 4 != 0 {
                    new_glyf.push(0);
                }
            }
        }
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        let mut new_head = head.to_vec();
        new_head[8..12].copy_from_slice(&[0; 4]); // checksum adjustment, set below
        new_head[50..52].copy_from_slice(&1u16.to_be_bytes()); // long offsets
        let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for tag in [b"OS/2", b"cmap", b"cvt ", b"fpgm", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp", b"name", b"post", b"prep"] {
            let body = match tag {
                b"glyf" => new_glyf.clone(),
                b"loca" => new_loca.clone(),
                b"head" => new_head.clone(),
                _ => match table(data, tag) {
                    Some(body) => body.to_vec(),
                    None => continue,
                },
            };
            tables.push((*tag, body));
        }
        let checksum = |bytes: &[u8]| -> u32 {
            bytes.chunks(4).fold(0u32, |sum, chunk| {
                let mut word = [0u8; 4];
                word[..chunk.len()].copy_from_slice(chunk);
                sum.wrapping_add(u32::from_be_bytes(word))
            })
        };
        let count_tables = tables.len() as u16;
        let entry_selector = 15 - count_tables.leading_zeros() as u16;
        let search_range = (1u16 << entry_selector) * 16;
        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&count_tables.to_be_bytes());
        out.extend_from_slice(&search_range.to_be_bytes());
        out.extend_from_slice(&entry_selector.to_be_bytes());
        out.extend_from_slice(&(count_tables * 16 - search_range).to_be_bytes());
        let mut offset_now = 12 + tables.len() * 16;
        let mut head_at = 0;
        for (tag, body) in &tables {
            out.extend_from_slice(tag);
            out.extend_from_slice(&checksum(body).to_be_bytes());
            out.extend_from_slice(&(offset_now as u32).to_be_bytes());
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            offset_now += body.len().div_ceil(4) * 4;
        }
        for (tag, body) in &tables {
            if tag == b"head" {
                head_at = out.len();
            }
            out.extend_from_slice(body);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        }
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[head_at + 8..head_at + 12].copy_from_slice(&adjustment.to_be_bytes());
        Some(out)
    }
}
