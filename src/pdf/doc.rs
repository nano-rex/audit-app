//! Writing PDF files (ISO 32000-1): pages of drawing operators, embedded TrueType fonts as
//! Unicode CID fonts, and JPEG or PNG images.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use super::font::Font;
use crate::flate::zlib;

pub const A4: (f64, f64) = (595.2756, 841.8898);

/// An RGB colour with channels from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colour(pub f64, pub f64, pub f64);

impl Colour {
    pub fn hex(text: &str) -> Colour {
        let text = text.trim_start_matches('#');
        let channel = |index: usize| u8::from_str_radix(text.get(index..index + 2).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0;
        Colour(channel(0), channel(2), channel(4))
    }

    pub const WHITE: Colour = Colour(1.0, 1.0, 1.0);

    /// The colour faded towards white, as the app's soft tints are.
    pub fn mix(self, amount: f64) -> Colour {
        Colour(self.0 + (1.0 - self.0) * amount, self.1 + (1.0 - self.1) * amount, self.2 + (1.0 - self.2) * amount)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Face {
    Regular,
    Bold,
}

/// The report's fonts: the app's regular and bold faces, and a fallback for other scripts.
pub struct Fonts {
    pub faces: Vec<Font>,
    regular: usize,
    bold: usize,
    fallback: Option<usize>,
}

impl Fonts {
    pub fn load(regular: Vec<u8>, bold: Vec<u8>, fallback: Option<Vec<u8>>) -> Option<Fonts> {
        let mut faces = vec![Font::parse(regular)?, Font::parse(bold)?];
        let fallback = fallback.and_then(Font::parse).map(|font| {
            faces.push(font);
            2
        });
        Some(Fonts { faces, regular: 0, bold: 1, fallback })
    }

    fn face(&self, face: Face) -> usize {
        match face {
            Face::Regular => self.regular,
            Face::Bold => self.bold,
        }
    }

    /// Runs of the text by the font that has their characters: (font index, text).
    pub fn runs(&self, text: &str, face: Face) -> Vec<(usize, String)> {
        let primary = self.face(face);
        let mut runs: Vec<(usize, String)> = Vec::new();
        for ch in text.chars() {
            let font = if self.faces[primary].has(ch) || ch.is_control() {
                primary
            } else {
                match self.fallback {
                    Some(fallback) if self.faces[fallback].has(ch) => fallback,
                    _ => primary,
                }
            };
            match runs.last_mut() {
                Some((last, run)) if *last == font => run.push(ch),
                _ => runs.push((font, ch.to_string())),
            }
        }
        runs
    }

    pub fn width(&self, text: &str, face: Face, size: f64) -> f64 {
        self.runs(text, face)
            .iter()
            .map(|(font, run)| {
                let font = &self.faces[*font];
                run.chars().map(|ch| font.advance_milli(font.glyph(ch).unwrap_or(0))).sum::<f64>()
            })
            .sum::<f64>()
            * size
            / 1000.0
    }
}

pub fn number(value: f64) -> String {
    if value.fract().abs() < 1e-6 {
        format!("{}", value.round() as i64)
    } else {
        let text = format!("{value:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

struct ImageObject {
    width: u32,
    height: u32,
    jpeg: Option<(Vec<u8>, u8)>,
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

/// The drawing operators of one page, and what they use.
#[derive(Default)]
pub struct Canvas {
    pub ops: String,
}

impl Canvas {
    pub fn fill_colour(&mut self, colour: Colour) {
        let _ = writeln!(self.ops, "{} {} {} rg", number(colour.0), number(colour.1), number(colour.2));
    }

    pub fn stroke_colour(&mut self, colour: Colour) {
        let _ = writeln!(self.ops, "{} {} {} RG", number(colour.0), number(colour.1), number(colour.2));
    }

    pub fn line_width(&mut self, width: f64) {
        let _ = writeln!(self.ops, "{} w", number(width));
    }

    pub fn dash(&mut self, pattern: &[f64]) {
        let parts: Vec<String> = pattern.iter().map(|value| number(*value)).collect();
        let _ = writeln!(self.ops, "[{}] 0 d", parts.join(" "));
    }

    pub fn save(&mut self) {
        self.ops.push_str("q\n");
    }

    pub fn restore(&mut self) {
        self.ops.push_str("Q\n");
    }

    pub fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, fill: Option<Colour>, stroke: Option<(Colour, f64)>) {
        self.save();
        if let Some(colour) = fill {
            self.fill_colour(colour);
        }
        if let Some((colour, width)) = stroke {
            self.stroke_colour(colour);
            self.line_width(width);
        }
        let _ = writeln!(self.ops, "{} {} {} {} re", number(x), number(y), number(width), number(height));
        self.ops.push_str(match (fill.is_some(), stroke.is_some()) {
            (true, true) => "B\n",
            (true, false) => "f\n",
            (false, true) => "S\n",
            _ => "n\n",
        });
        self.restore();
    }

    /// A rectangle with rounded corners.
    pub fn round_rect(&mut self, x: f64, y: f64, width: f64, height: f64, radius: f64, fill: Option<Colour>, stroke: Option<(Colour, f64)>) {
        let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
        if radius == 0.0 {
            return self.rect(x, y, width, height, fill, stroke);
        }
        let k = radius * 0.552_284_75;
        self.save();
        if let Some(colour) = fill {
            self.fill_colour(colour);
        }
        if let Some((colour, line)) = stroke {
            self.stroke_colour(colour);
            self.line_width(line);
        }
        let (right, top) = (x + width, y + height);
        let n = number;
        let _ = writeln!(
            self.ops,
            "{} {} m {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c h",
            n(x + radius), n(y),
            n(right - radius), n(y),
            n(right - radius + k), n(y), n(right), n(y + radius - k), n(right), n(y + radius),
            n(right), n(top - radius),
            n(right), n(top - radius + k), n(right - radius + k), n(top), n(right - radius), n(top),
            n(x + radius), n(top),
            n(x + radius - k), n(top), n(x), n(top - radius + k), n(x), n(top - radius),
            n(x), n(y + radius),
            n(x), n(y + radius - k), n(x + radius - k), n(y), n(x + radius), n(y)
        );
        self.ops.push_str(match (fill.is_some(), stroke.is_some()) {
            (true, true) => "B\n",
            (true, false) => "f\n",
            (false, true) => "S\n",
            _ => "n\n",
        });
        self.restore();
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, colour: Colour, width: f64, dash: Option<&[f64]>) {
        self.save();
        self.stroke_colour(colour);
        self.line_width(width);
        if let Some(pattern) = dash {
            self.dash(pattern);
        }
        let _ = writeln!(self.ops, "{} {} m {} {} l S", number(x1), number(y1), number(x2), number(y2));
        self.restore();
    }

    /// A filled path of points (polygon), closed.
    pub fn polygon(&mut self, points: &[(f64, f64)], fill: Colour, stroke: Option<(Colour, f64)>) {
        if points.is_empty() {
            return;
        }
        self.save();
        self.fill_colour(fill);
        if let Some((colour, width)) = stroke {
            self.stroke_colour(colour);
            self.line_width(width);
            self.ops.push_str("1 j\n");
        }
        let _ = write!(self.ops, "{} {} m ", number(points[0].0), number(points[0].1));
        for (x, y) in &points[1..] {
            let _ = write!(self.ops, "{} {} l ", number(*x), number(*y));
        }
        self.ops.push_str(if stroke.is_some() { "h B\n" } else { "h f\n" });
        self.restore();
    }

    pub fn circle(&mut self, cx: f64, cy: f64, radius: f64, fill: Colour, stroke: Option<(Colour, f64)>) {
        let k = radius * 0.552_284_75;
        self.save();
        self.fill_colour(fill);
        if let Some((colour, width)) = stroke {
            self.stroke_colour(colour);
            self.line_width(width);
        }
        let n = number;
        let _ = writeln!(
            self.ops,
            "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h {}",
            n(cx + radius), n(cy),
            n(cx + radius), n(cy + k), n(cx + k), n(cy + radius), n(cx), n(cy + radius),
            n(cx - k), n(cy + radius), n(cx - radius), n(cy + k), n(cx - radius), n(cy),
            n(cx - radius), n(cy - k), n(cx - k), n(cy - radius), n(cx), n(cy - radius),
            n(cx + k), n(cy - radius), n(cx + radius), n(cy - k), n(cx + radius), n(cy),
            if stroke.is_some() { "B" } else { "f" }
        );
        self.restore();
    }
}

/// A PDF being written: its pages, fonts, and images.
pub struct Document {
    pub fonts: Fonts,
    pages: Vec<String>,
    used: Vec<BTreeMap<u16, char>>,
    images: Vec<ImageObject>,
    image_keys: HashMap<[u8; 32], usize>,
    pub title: String,
    pub author: String,
}

impl Document {
    pub fn new(fonts: Fonts) -> Document {
        let used = vec![BTreeMap::new(); fonts.faces.len()];
        Document { fonts, pages: Vec::new(), used, images: Vec::new(), image_keys: HashMap::new(), title: String::new(), author: String::new() }
    }

    pub fn add_page(&mut self, canvas: Canvas) {
        self.pages.push(canvas.ops);
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Draw text with its baseline at (x, y).
    pub fn text(&mut self, canvas: &mut Canvas, x: f64, y: f64, text: &str, face: Face, size: f64, colour: Colour) {
        if text.is_empty() {
            return;
        }
        let mut cursor = x;
        canvas.fill_colour(colour);
        for (font_index, run) in self.fonts.runs(text, face) {
            let font = &self.fonts.faces[font_index];
            let mut hex = String::with_capacity(run.len() * 4);
            let mut advance = 0.0;
            for ch in run.chars() {
                let glyph = font.glyph(ch).unwrap_or(0);
                self.used[font_index].entry(glyph).or_insert(ch);
                let _ = write!(hex, "{glyph:04X}");
                advance += font.advance_milli(glyph);
            }
            let _ = writeln!(canvas.ops, "BT /F{font_index} {} Tf {} {} Td <{hex}> Tj ET", number(size), number(cursor), number(y));
            cursor += advance * size / 1000.0;
        }
    }

    /// Register an image (JPEG or PNG bytes); returns its index and size, or None when unreadable.
    pub fn image(&mut self, bytes: &[u8]) -> Option<(usize, u32, u32)> {
        let key = crate::crypto::sha256(bytes);
        if let Some(index) = self.image_keys.get(&key) {
            let image = &self.images[*index];
            return Some((*index, image.width, image.height));
        }
        let header = crate::media::read_header(bytes)?;
        let object = match header.format {
            crate::media::Format::Jpeg => {
                let components = jpeg_components(bytes).unwrap_or(3);
                ImageObject { width: header.width, height: header.height, jpeg: Some((bytes.to_vec(), components)), rgb: Vec::new(), alpha: None }
            }
            _ => {
                let decoded = match header.format {
                    crate::media::Format::Png => crate::png::decode(bytes).ok()?,
                    _ => crate::webp::decode(bytes).ok()?,
                };
                let alpha = decoded.has_alpha().then(|| decoded.alpha());
                ImageObject { width: decoded.width, height: decoded.height, jpeg: None, rgb: decoded.rgb(), alpha }
            }
        };
        let (width, height) = (object.width, object.height);
        self.images.push(object);
        self.image_keys.insert(key, self.images.len() - 1);
        Some((self.images.len() - 1, width, height))
    }

    pub fn draw_image(&self, canvas: &mut Canvas, index: usize, x: f64, y: f64, width: f64, height: f64) {
        let _ = writeln!(canvas.ops, "q {} 0 0 {} {} {} cm /Im{index} Do Q", number(width), number(height), number(x), number(y));
    }

    pub fn finish(self) -> Vec<u8> {
        let mut objects: Vec<Vec<u8>> = Vec::new();
        // 1: catalog, 2: pages; then fonts, images, page contents.
        objects.push(Vec::new());
        objects.push(Vec::new());
        let mut font_refs = Vec::new();
        for (index, font) in self.fonts.faces.iter().enumerate() {
            let used = &self.used[index];
            if used.is_empty() {
                font_refs.push(None);
                continue;
            }
            let glyphs: Vec<u16> = used.keys().copied().collect();
            let subset = font.subset(&glyphs);
            let file = zlib(&subset);
            objects.push(stream(&format!("/Length1 {} /Filter /FlateDecode", subset.len()), &file));
            let file_id = objects.len();
            let flags = 32; // nonsymbolic
            objects.push(format!(
                "<< /Type /FontDescriptor /FontName /AAAAA{index}+{} /Flags {flags} /FontBBox [{} {} {} {}] /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {file_id} 0 R >>",
                font.name,
                number(font.scale(font.bbox[0])), number(font.scale(font.bbox[1])), number(font.scale(font.bbox[2])), number(font.scale(font.bbox[3])),
                number(font.italic_angle),
                number(font.scale(font.ascent)),
                number(font.scale(font.descent)),
                number(font.scale(font.cap_height))
            ).into_bytes());
            let descriptor = objects.len();
            let mut widths = String::new();
            for glyph in used.keys() {
                let _ = write!(widths, "{glyph} [{}] ", number(font.advance_milli(*glyph)));
            }
            objects.push(format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /AAAAA{index}+{} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity /DW 0 /W [{widths}] >>",
                font.name
            ).into_bytes());
            let cid = objects.len();
            let mut cmap = String::from("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
            let entries: Vec<(&u16, &char)> = used.iter().collect();
            for chunk in entries.chunks(100) {
                let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
                for (glyph, ch) in chunk {
                    let mut units = [0u16; 2];
                    let encoded: String = ch.encode_utf16(&mut units).iter().map(|unit| format!("{unit:04X}")).collect();
                    let _ = writeln!(cmap, "<{glyph:04X}> <{encoded}>");
                }
                cmap.push_str("endbfchar\n");
            }
            cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
            objects.push(stream("/Filter /FlateDecode", &zlib(cmap.as_bytes())));
            let unicode = objects.len();
            objects.push(format!("<< /Type /Font /Subtype /Type0 /BaseFont /AAAAA{index}+{} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {unicode} 0 R >>", font.name).into_bytes());
            font_refs.push(Some(objects.len()));
        }
        let mut image_refs = Vec::new();
        for image in &self.images {
            match &image.jpeg {
                Some((data, components)) => {
                    let space = match components {
                        1 => "/DeviceGray",
                        4 => "/DeviceCMYK /Decode [1 0 1 0 1 0 1 0]",
                        _ => "/DeviceRGB",
                    };
                    objects.push(stream(&format!("/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {space} /BitsPerComponent 8 /Filter /DCTDecode", image.width, image.height), data));
                }
                None => {
                    let mut mask = String::new();
                    if let Some(alpha) = &image.alpha {
                        objects.push(stream(&format!("/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode", image.width, image.height), &zlib(alpha)));
                        mask = format!(" /SMask {} 0 R", objects.len());
                    }
                    objects.push(stream(&format!("/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode{mask}", image.width, image.height), &zlib(&image.rgb)));
                }
            }
            image_refs.push(objects.len());
        }
        let mut resources = String::from("<< /Font << ");
        for (index, reference) in font_refs.iter().enumerate() {
            if let Some(reference) = reference {
                let _ = write!(resources, "/F{index} {reference} 0 R ");
            }
        }
        resources.push_str(">> /XObject << ");
        for (index, reference) in image_refs.iter().enumerate() {
            let _ = write!(resources, "/Im{index} {reference} 0 R ");
        }
        resources.push_str(">> /ProcSet [/PDF /Text /ImageB /ImageC] >>");
        objects.push(resources.into_bytes());
        let resources_id = objects.len();
        let mut kids = Vec::new();
        for page in &self.pages {
            objects.push(stream("/Filter /FlateDecode", &zlib(page.as_bytes())));
            let content = objects.len();
            objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources {resources_id} 0 R /Contents {content} 0 R >>", number(A4.0), number(A4.1)).into_bytes());
            kids.push(format!("{} 0 R", objects.len()));
        }
        objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
        objects[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), self.pages.len()).into_bytes();
        let now = crate::clock::utc_now();
        objects.push(format!(
            "<< /Title {} /Author {} /Producer (Audit App) /CreationDate (D:{:04}{:02}{:02}{:02}{:02}{:02}Z) >>",
            text_string(&self.title),
            text_string(&self.author),
            now.date.year, now.date.month, now.date.day, now.hour, now.minute, now.second
        ).into_bytes());
        let info = objects.len();
        let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());
        for (index, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            out.extend_from_slice(object);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        out
    }
}

fn stream(dictionary: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("<< {dictionary} /Length {} >>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

/// A PDF text string: UTF-16 with its byte-order mark, as hex.
fn text_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        let _ = write!(out, "{unit:04X}");
    }
    out.push('>');
    out
}

fn jpeg_components(bytes: &[u8]) -> Option<u8> {
    let mut at = 2;
    while at + 4 < bytes.len() {
        if bytes[at] != 0xFF {
            return None;
        }
        let marker = bytes[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return bytes.get(at + 9).copied();
        }
        at += 2 + length;
    }
    None
}
