//! PNG images (ISO/IEC 15948): decoding every standard colour type and bit depth, interlaced or
//! not, to 8-bit RGBA; and encoding RGBA.

use crate::flate::{crc32, unzlib, zlib};

/// Pixels as 8-bit RGBA, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn has_alpha(&self) -> bool {
        self.rgba.chunks_exact(4).any(|pixel| pixel[3] != 255)
    }

    pub fn rgb(&self) -> Vec<u8> {
        self.rgba.chunks_exact(4).flat_map(|pixel| [pixel[0], pixel[1], pixel[2]]).collect()
    }

    pub fn alpha(&self) -> Vec<u8> {
        self.rgba.chunks_exact(4).map(|pixel| pixel[3]).collect()
    }
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Undo the per-row filters of one (sub)image; returns the raw scanlines without filter bytes.
fn unfilter(data: &[u8], width: usize, height: usize, bits_per_pixel: usize) -> Result<Vec<u8>, String> {
    let stride = (width * bits_per_pixel).div_ceil(8);
    let bpp = bits_per_pixel.div_ceil(8).max(1);
    if data.len() < height * (stride + 1) {
        return Err("PNG image data is short".into());
    }
    let mut out = vec![0u8; height * stride];
    for row in 0..height {
        let filter = data[row * (stride + 1)];
        let line = &data[row * (stride + 1) + 1..(row + 1) * (stride + 1)];
        for x in 0..stride {
            let a = if x >= bpp { out[row * stride + x - bpp] } else { 0 };
            let b = if row > 0 { out[(row - 1) * stride + x] } else { 0 };
            let c = if row > 0 && x >= bpp { out[(row - 1) * stride + x - bpp] } else { 0 };
            out[row * stride + x] = match filter {
                0 => line[x],
                1 => line[x].wrapping_add(a),
                2 => line[x].wrapping_add(b),
                3 => line[x].wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => line[x].wrapping_add(paeth(a, b, c)),
                _ => return Err("unknown PNG filter".into()),
            };
        }
    }
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("not a PNG image".into());
    }
    let mut at = 8;
    let (mut width, mut height, mut depth, mut colour, mut interlace) = (0usize, 0usize, 0u8, 0u8, 0u8);
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut transparent: Option<Vec<u16>> = None;
    let mut idat = Vec::new();
    loop {
        let length = be32(bytes, at).ok_or("PNG ends early")? as usize;
        let kind = bytes.get(at + 4..at + 8).ok_or("PNG ends early")?;
        let data = bytes.get(at + 8..at + 8 + length).ok_or("PNG ends early")?;
        if crc32(&bytes[at + 4..at + 8 + length]) != be32(bytes, at + 8 + length).ok_or("PNG ends early")? {
            return Err("PNG chunk checksum mismatch".into());
        }
        match kind {
            b"IHDR" => {
                width = be32(data, 0).ok_or("bad IHDR")? as usize;
                height = be32(data, 4).ok_or("bad IHDR")? as usize;
                depth = data[8];
                colour = data[9];
                interlace = data[12];
            }
            b"PLTE" => palette = data.chunks_exact(3).map(|c| [c[0], c[1], c[2], 255]).collect(),
            b"tRNS" => match colour {
                3 => {
                    for (index, alpha) in data.iter().enumerate() {
                        if let Some(entry) = palette.get_mut(index) {
                            entry[3] = *alpha;
                        }
                    }
                }
                0 => transparent = Some(vec![u16::from_be_bytes([data[0], data[1]])]),
                2 => transparent = Some((0..3).map(|i| u16::from_be_bytes([data[i * 2], data[i * 2 + 1]])).collect()),
                _ => {}
            },
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + length;
    }
    if width == 0 || height == 0 || (width as u64 * height as u64) > 40_000_000 {
        return Err("PNG size is out of range".into());
    }
    let channels = match colour {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err("unknown PNG colour type".into()),
    };
    let bits = channels * depth as usize;
    let raw = unzlib(&idat, (width * bits).div_ceil(8) * height * 2 + height * 8 + 1024)?;
    let mut rgba = vec![0u8; width * height * 4];
    let max = (1u32 << depth) - 1;
    let sample = |line: &[u8], index: usize| -> u16 {
        match depth {
            8 => line[index] as u16,
            16 => u16::from_be_bytes([line[index * 2], line[index * 2 + 1]]),
            _ => {
                let bit = index * depth as usize;
                ((line[bit / 8] >> (8 - depth as usize - bit % 8)) & max as u8) as u16
            }
        }
    };
    let scale = |value: u16| -> u8 {
        match depth {
            16 => (value >> 8) as u8,
            8 => value as u8,
            _ => (value as u32 * 255 / max) as u8,
        }
    };
    let mut place = |pixels: &[u8], sub_width: usize, sub_height: usize, origin: (usize, usize), step: (usize, usize)| {
        let stride = (sub_width * bits).div_ceil(8);
        for y in 0..sub_height {
            let line = &pixels[y * stride..(y + 1) * stride];
            for x in 0..sub_width {
                let target = ((origin.1 + y * step.1) * width + origin.0 + x * step.0) * 4;
                let pixel: [u8; 4] = match colour {
                    0 => {
                        let value = sample(line, x);
                        let grey = scale(value);
                        let alpha = if transparent.as_ref().is_some_and(|t| t[0] == value) { 0 } else { 255 };
                        [grey, grey, grey, alpha]
                    }
                    2 => {
                        let values = [sample(line, x * 3), sample(line, x * 3 + 1), sample(line, x * 3 + 2)];
                        let alpha = if transparent.as_ref().is_some_and(|t| t[..] == values) { 0 } else { 255 };
                        [scale(values[0]), scale(values[1]), scale(values[2]), alpha]
                    }
                    3 => palette.get(sample(line, x) as usize).copied().unwrap_or([0, 0, 0, 255]),
                    4 => {
                        let grey = scale(sample(line, x * 2));
                        [grey, grey, grey, scale(sample(line, x * 2 + 1))]
                    }
                    _ => [scale(sample(line, x * 4)), scale(sample(line, x * 4 + 1)), scale(sample(line, x * 4 + 2)), scale(sample(line, x * 4 + 3))],
                };
                rgba[target..target + 4].copy_from_slice(&pixel);
            }
        }
    };
    if interlace == 0 {
        let pixels = unfilter(&raw, width, height, bits)?;
        place(&pixels, width, height, (0, 0), (1, 1));
    } else {
        // Adam7: seven passes over sparser grids.
        const PASSES: [(usize, usize, usize, usize); 7] = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];
        let mut offset = 0;
        for (x0, y0, dx, dy) in PASSES {
            let sub_width = (width + dx - 1 - x0) / dx;
            let sub_height = (height + dy - 1 - y0) / dy;
            if width <= x0 || height <= y0 || sub_width == 0 || sub_height == 0 {
                continue;
            }
            let size = sub_height * ((sub_width * bits).div_ceil(8) + 1);
            let pixels = unfilter(raw.get(offset..offset + size).ok_or("PNG image data is short")?, sub_width, sub_height, bits)?;
            place(&pixels, sub_width, sub_height, (x0, y0), (dx, dy));
            offset += size;
        }
    }
    Ok(Image { width: width as u32, height: height as u32, rgba })
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let checksum = crc32(&out[start..]);
    out.extend_from_slice(&checksum.to_be_bytes());
}

/// Encode as an 8-bit RGBA PNG (RGB when every pixel is opaque).
pub fn encode(image: &Image) -> Vec<u8> {
    let alpha = image.has_alpha();
    let channels = if alpha { 4 } else { 3 };
    let width = image.width as usize;
    let stride = width * channels;
    let mut raw = Vec::with_capacity((stride + 1) * image.height as usize);
    let mut previous = vec![0u8; stride];
    let mut line = vec![0u8; stride];
    for row in 0..image.height as usize {
        for x in 0..width {
            let pixel = &image.rgba[(row * width + x) * 4..(row * width + x) * 4 + 4];
            line[x * channels..x * channels + channels].copy_from_slice(&pixel[..channels]);
        }
        // Paeth suits photographs and flat drawings alike.
        raw.push(4);
        for x in 0..stride {
            let a = if x >= channels { line[x - channels] } else { 0 };
            let c = if x >= channels { previous[x - channels] } else { 0 };
            raw.push(line[x].wrapping_sub(paeth(a, previous[x], c)));
        }
        std::mem::swap(&mut previous, &mut line);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = image.width.to_be_bytes().to_vec();
    header.extend_from_slice(&image.height.to_be_bytes());
    header.extend_from_slice(&[8, if alpha { 6 } else { 2 }, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut rgba = Vec::new();
        for y in 0..7u32 {
            for x in 0..9u32 {
                rgba.extend_from_slice(&[(x * 20) as u8, (y * 30) as u8, 128, if x == 3 { 0 } else { 255 }]);
            }
        }
        let image = Image { width: 9, height: 7, rgba };
        assert_eq!(decode(&encode(&image)).unwrap(), image);
        let opaque = Image { width: 2, height: 1, rgba: vec![1, 2, 3, 255, 4, 5, 6, 255] };
        assert_eq!(decode(&encode(&opaque)).unwrap(), opaque);
    }
}
