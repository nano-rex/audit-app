//! Reading the cell values of Excel workbooks (.xlsx): shared strings, inline strings, numbers,
//! booleans, and dates (numbers in a date format, given as YYYY-MM-DD).

use crate::clock::Date;
use crate::zip::read_archive;

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    Number(f64),
    Date(String),
    Bool(bool),
}

impl Cell {
    /// The value as text, as Python's str() of what openpyxl returns.
    pub fn text(&self) -> String {
        match self {
            Cell::Empty => String::new(),
            Cell::Text(text) | Cell::Date(text) => text.clone(),
            Cell::Number(number) if number.fract() == 0.0 && number.abs() < 1e15 => format!("{}", *number as i64),
            Cell::Number(number) => crate::json::float_repr(*number),
            Cell::Bool(flag) => if *flag { "True".into() } else { "False".into() },
        }
    }
}

pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

/// One XML element or text run.
#[derive(Debug)]
enum Token {
    Open(String, Vec<(String, String)>, bool),
    Close(String),
    Text(String),
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find(';') else {
            out.push_str(&rest[at..]);
            return out;
        };
        let entity = &rest[at + 1..at + end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32),
            _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(ch) => out.push(ch),
            None => out.push_str(&rest[at..at + end + 1]),
        }
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    out
}

fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn tokens(xml: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        match rest.find('<') {
            Some(0) => {
                if rest.starts_with("<?") || rest.starts_with("<!") {
                    let end = if rest.starts_with("<!--") { rest.find("-->").map(|at| at + 3) } else { rest.find('>').map(|at| at + 1) };
                    rest = &rest[end.unwrap_or(rest.len())..];
                    continue;
                }
                let end = rest.find('>').unwrap_or(rest.len() - 1);
                let inner = &rest[1..end];
                rest = &rest[(end + 1).min(rest.len())..];
                if let Some(name) = inner.strip_prefix('/') {
                    out.push(Token::Close(local(name.trim())));
                    continue;
                }
                let empty = inner.ends_with('/');
                let inner = inner.trim_end_matches('/');
                let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
                let name = local(&inner[..name_end]);
                let mut attributes = Vec::new();
                let mut attrs = &inner[name_end..];
                while let Some(eq) = attrs.find('=') {
                    let key = local(attrs[..eq].trim());
                    let after = attrs[eq + 1..].trim_start();
                    let quote = after.chars().next().unwrap_or('"');
                    let value_end = after[1..].find(quote).map(|at| at + 1).unwrap_or(after.len());
                    attributes.push((key, unescape(&after[1..value_end])));
                    attrs = &after[(value_end + 1).min(after.len())..];
                }
                out.push(Token::Open(name, attributes, empty));
            }
            Some(at) => {
                out.push(Token::Text(unescape(&rest[..at])));
                rest = &rest[at..];
            }
            None => {
                out.push(Token::Text(unescape(rest)));
                rest = "";
            }
        }
    }
    out
}

fn attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
}

fn shared_strings(xml: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut current: Option<String> = None;
    let (mut in_text, mut in_phonetic) = (false, false);
    for token in tokens(xml) {
        match token {
            Token::Open(name, _, empty) => match name.as_str() {
                "si" if !empty => current = Some(String::new()),
                "si" => strings.push(String::new()),
                "t" if !empty => in_text = true,
                "rPh" if !empty => in_phonetic = true,
                _ => {}
            },
            Token::Close(name) => match name.as_str() {
                "si" => strings.push(current.take().unwrap_or_default()),
                "t" => in_text = false,
                "rPh" => in_phonetic = false,
                _ => {}
            },
            Token::Text(text) => {
                if in_text && !in_phonetic {
                    if let Some(current) = current.as_mut() {
                        current.push_str(&text);
                    }
                }
            }
        }
    }
    strings
}

/// The style indexes whose number format shows a date.
fn date_styles(xml: &str) -> Vec<bool> {
    let mut custom: Vec<(u32, String)> = Vec::new();
    let mut formats = Vec::new();
    let mut in_xfs = false;
    for token in tokens(xml) {
        match token {
            Token::Open(name, attributes, empty) => match name.as_str() {
                "numFmt" => custom.push((attribute(&attributes, "numFmtId").and_then(|id| id.parse().ok()).unwrap_or(0), attribute(&attributes, "formatCode").unwrap_or("").to_string())),
                "cellXfs" if !empty => in_xfs = true,
                "xf" if in_xfs => formats.push(attribute(&attributes, "numFmtId").and_then(|id| id.parse().ok()).unwrap_or(0u32)),
                _ => {}
            },
            Token::Close(name) if name == "cellXfs" => in_xfs = false,
            _ => {}
        }
    }
    let is_date_code = |code: &str| {
        let mut plain = String::new();
        let mut quoted = false;
        let mut bracket = false;
        for ch in code.chars() {
            match ch {
                '"' => quoted = !quoted,
                '[' => bracket = true,
                ']' => bracket = false,
                _ if !quoted && !bracket => plain.push(ch.to_ascii_lowercase()),
                _ => {}
            }
        }
        plain != "general" && plain.chars().any(|c| matches!(c, 'd' | 'm' | 'y' | 'h' | 's'))
    };
    formats
        .into_iter()
        .map(|id| match id {
            14..=22 | 27..=36 | 45..=47 | 50..=58 => true,
            _ => custom.iter().find(|(known, _)| *known == id).is_some_and(|(_, code)| is_date_code(code)),
        })
        .collect()
}

/// An Excel serial day number (1900 system) as YYYY-MM-DD.
fn serial_date(serial: f64) -> String {
    let days = serial.floor() as i64;
    Date::from_days(Date { year: 1899, month: 12, day: 30 }.days() + days).iso()
}

fn column_index(reference: &str) -> usize {
    let mut index = 0usize;
    for ch in reference.chars().take_while(|c| c.is_ascii_alphabetic()) {
        index = index * 26 + (ch.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    index.saturating_sub(1)
}

fn worksheet(xml: &str, strings: &[String], dates: &[bool]) -> Vec<Vec<Cell>> {
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut row: Vec<Cell> = Vec::new();
    let mut row_number = 0usize;
    let mut cell: Option<(usize, String, usize)> = None;
    let mut value = String::new();
    let mut in_value = false;
    let mut in_inline = false;
    let mut inline = String::new();
    for token in tokens(xml) {
        match token {
            Token::Open(name, attributes, empty) => match name.as_str() {
                "row" => {
                    let number = attribute(&attributes, "r").and_then(|r| r.parse().ok()).unwrap_or(row_number + 1);
                    while rows.len() + 1 < number {
                        rows.push(Vec::new());
                    }
                    row_number = number;
                    row = Vec::new();
                    if empty {
                        rows.push(Vec::new());
                    }
                }
                "c" => {
                    let column = attribute(&attributes, "r").map(column_index).unwrap_or(row.len());
                    let kind = attribute(&attributes, "t").unwrap_or("n").to_string();
                    let style = attribute(&attributes, "s").and_then(|s| s.parse().ok()).unwrap_or(0);
                    value.clear();
                    inline.clear();
                    if empty {
                        while row.len() <= column {
                            row.push(Cell::Empty);
                        }
                    } else {
                        cell = Some((column, kind, style));
                    }
                }
                "v" if !empty => in_value = true,
                "is" if !empty => in_inline = true,
                _ => {}
            },
            Token::Close(name) => match name.as_str() {
                "v" => in_value = false,
                "is" => in_inline = false,
                "c" => {
                    if let Some((column, kind, style)) = cell.take() {
                        let parsed = match kind.as_str() {
                            "s" => value.trim().parse::<usize>().ok().and_then(|index| strings.get(index)).map(|text| Cell::Text(text.clone())).unwrap_or(Cell::Empty),
                            "inlineStr" => Cell::Text(inline.clone()),
                            "str" | "e" => Cell::Text(value.clone()),
                            "b" => Cell::Bool(value.trim() == "1"),
                            _ => match value.trim().parse::<f64>() {
                                Ok(number) if dates.get(style).copied().unwrap_or(false) => Cell::Date(serial_date(number)),
                                Ok(number) => Cell::Number(number),
                                Err(_) => Cell::Empty,
                            },
                        };
                        while row.len() <= column {
                            row.push(Cell::Empty);
                        }
                        row[column] = parsed;
                    }
                }
                "row" => rows.push(std::mem::take(&mut row)),
                _ => {}
            },
            Token::Text(text) => {
                if in_value {
                    value.push_str(&text);
                } else if in_inline {
                    inline.push_str(&text);
                }
            }
        }
    }
    rows
}

/// Every worksheet of a workbook, in order, with its rows of cells.
pub fn read_workbook(bytes: &[u8]) -> Result<Vec<Sheet>, String> {
    let files = read_archive(bytes, 512 * 1024 * 1024)?;
    let file = |name: &str| -> Option<String> { files.iter().find(|(path, _)| path.trim_start_matches('/') == name).map(|(_, data)| String::from_utf8_lossy(data).into_owned()) };
    let workbook = file("xl/workbook.xml").ok_or("not an Excel workbook")?;
    let relations = file("xl/_rels/workbook.xml.rels").unwrap_or_default();
    let strings = file("xl/sharedStrings.xml").map(|xml| shared_strings(&xml)).unwrap_or_default();
    let dates = file("xl/styles.xml").map(|xml| date_styles(&xml)).unwrap_or_default();
    let targets: Vec<(String, String)> = tokens(&relations)
        .into_iter()
        .filter_map(|token| match token {
            Token::Open(name, attributes, _) if name == "Relationship" => Some((attribute(&attributes, "Id")?.to_string(), attribute(&attributes, "Target")?.to_string())),
            _ => None,
        })
        .collect();
    let mut sheets = Vec::new();
    for token in tokens(&workbook) {
        if let Token::Open(name, attributes, _) = token {
            if name != "sheet" {
                continue;
            }
            let title = attribute(&attributes, "name").unwrap_or("").to_string();
            let id = attribute(&attributes, "id").unwrap_or("");
            let Some((_, target)) = targets.iter().find(|(key, _)| key == id) else { continue };
            let path = if let Some(absolute) = target.strip_prefix('/') { absolute.to_string() } else { format!("xl/{target}") };
            let Some(xml) = file(&path) else { continue };
            sheets.push(Sheet { name: title, rows: worksheet(&xml, &strings, &dates) });
        }
    }
    Ok(sheets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_writer_wrote() {
        let mut book = crate::xlsx::Workbook::new("Noto Sans");
        let mut sheet = crate::xlsx::Sheet::new("Assets & more");
        sheet.cell(1, 1).value = crate::xlsx::CellValue::Text("Name <A>".into());
        sheet.cell(2, 3).value = crate::xlsx::CellValue::Number(12.0);
        book.sheets.push(sheet);
        let sheets = read_workbook(&book.save()).unwrap();
        assert_eq!(sheets[0].name, "Assets & more");
        assert_eq!(sheets[0].rows[0][0], Cell::Text("Name <A>".into()));
        assert_eq!(sheets[0].rows[1][2].text(), "12");
        assert_eq!(serial_date(45_000.0), "2023-03-15");
    }
}
