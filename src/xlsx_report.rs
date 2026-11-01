//! Excel versions of the audit reports: laid out and coloured like the PDFs (pdf_report.rs), with
//! the same sections, charts, and photos, but in ordinary cells that can be edited.
//!
//! Every sheet uses eight equal columns (A-H); a table's column may span several of them, which
//! keeps the sheets aligned like the PDF's page and prints on one A4 width.

use crate::audits::rating_for_score;
use crate::config::theme_preset_accent;
use crate::error::R;
use crate::exports::{self, LocationAudit, Part};
use crate::figures::{self, FAIL as FAIL_HEX, PASS as PASS_HEX};
use crate::json::{format_g, Value};
use crate::xlsx::{column_letter, Alignment, Border, CellValue, Chart, ChartKind, Font, Picture, Range, Series, Sheet, Workbook};

const FONT: &str = "Noto Sans";
const TEXT: &str = "17222B";
const MUTED: &str = "5F6C66";
const BORDER: &str = "D3D9D5";
const SURFACE_ALT: &str = "F7F9F8";
const NEUTRAL_SOFT: &str = "ECEFF0";
const COLUMNS: u32 = 8;
const COLUMN_WIDTH: f64 = 15.0;
const SIGNATURES: [(&str, &str); 3] = [("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by")];
const CM_PER_COLUMN: f64 = 2.9;
const CM_PER_ROW: f64 = 0.53;

fn tone(name: &str) -> (&'static str, &'static str) {
    match name {
        "pass" => ("1C8F4B", "E3F4EA"),
        "fail" => ("C9303A", "FDE7E8"),
        "warn" => ("B96100", "FDEFD9"),
        _ => (MUTED, NEUTRAL_SOFT),
    }
}

fn is_tone(name: &str) -> bool {
    matches!(name, "pass" | "fail" | "warn" | "neutral")
}

fn result_tone(value: &str) -> Option<&'static str> {
    match value {
        "Pass" => Some("pass"),
        "Fail" => Some("fail"),
        "N/A" => Some("neutral"),
        _ => None,
    }
}

pub fn soft(hex: &str, amount: f64) -> String {
    (0..3)
        .map(|index| {
            let channel = u8::from_str_radix(hex.get(index * 2..index * 2 + 2).unwrap_or("00"), 16).unwrap_or(0) as f64;
            format!("{:02X}", (channel + (255.0 - channel) * amount).round() as u8)
        })
        .collect()
}

/// The organization's accent colour as RRGGBB.
pub fn accent_of(brand: &Value) -> String {
    let theme = brand.at("theme");
    let accent = if theme.at("accent").truthy() { theme.at("accent").py_str() } else { theme_preset_accent(&theme.at("preset").text()).to_string() };
    accent.trim_start_matches('#').to_uppercase()
}

/// A cell's value as the sheet keeps it.
fn cell_value(value: &Value) -> CellValue {
    match value {
        Value::Null => CellValue::Empty,
        Value::Int(number) => CellValue::Number(*number as f64),
        Value::Float(number) => CellValue::Number(*number),
        Value::Bool(flag) => CellValue::Text(if *flag { "TRUE".into() } else { "FALSE".into() }),
        Value::Str(text) => CellValue::Text(text.clone()),
        other => CellValue::Text(other.py_str()),
    }
}

/// How a value is written into a cell.
#[derive(Clone)]
struct Put<'a> {
    size: f64,
    bold: bool,
    colour: &'a str,
    background: Option<&'a str>,
    align: &'static str,
    wrap: bool,
    span: u32,
    valign: &'static str,
}

impl Default for Put<'_> {
    fn default() -> Self {
        Put { size: 9.0, bold: false, colour: TEXT, background: None, align: "left", wrap: true, span: 1, valign: "top" }
    }
}

/// Writes one sheet top to bottom, as the PDF writer writes a page.
struct SheetWriter<'a> {
    sheet: Sheet,
    brand: &'a Value,
    settings: &'a Value,
    row: u32,
    accent: String,
    accent_soft: String,
    with_media: bool,
}

impl<'a> SheetWriter<'a> {
    fn new(title: &str, brand: &'a Value, settings: &'a Value, with_media: bool) -> SheetWriter<'a> {
        let mut sheet = Sheet::new(title);
        sheet.show_grid = false;
        for column in 1..=COLUMNS {
            sheet.column_widths.insert(column, COLUMN_WIDTH);
        }
        sheet.fit_to_width = true;
        sheet.margins = Some(0.5);
        let accent = accent_of(brand);
        let accent_soft = soft(&accent, 0.88);
        SheetWriter { sheet, brand, settings, row: 1, accent, accent_soft, with_media }
    }

    fn put(&mut self, row: u32, column: u32, value: impl Into<Value>, style: Put) {
        let value = value.into();
        let cell = self.sheet.cell(row, column);
        cell.value = cell_value(&value);
        cell.font = Some(Font { size: (style.size * 100.0).round() as u32, bold: style.bold, colour: style.colour.to_string() });
        cell.alignment = Some(Alignment { horizontal: style.align, vertical: style.valign, wrap: style.wrap });
        if style.span > 1 {
            self.sheet.merge(row, column, column + style.span - 1);
            for offset in 1..style.span {
                self.sheet.cell(row, column + offset).value = CellValue::Empty;
            }
        }
        if let Some(background) = style.background {
            for offset in 0..style.span {
                self.sheet.cell(row, column + offset).fill = Some(background.to_string());
            }
        }
    }

    /// A border round a block, optionally with a rule under each row inside.
    fn boxed(&mut self, top: u32, left: u32, bottom: u32, right: u32, inner_rows: bool) {
        for row in top..=bottom {
            for column in left..=right {
                let side = |on: bool| on.then(|| ("thin", BORDER.to_string()));
                self.sheet.cell(row, column).border = Border {
                    left: side(column == left),
                    right: side(column == right),
                    top: side(row == top),
                    bottom: side(row == bottom || inner_rows),
                };
            }
        }
    }

    /// Rows grow with their wrapped text, as the PDF's do (Excel does not measure on its own).
    fn height(&mut self, row: u32, texts: &[(Value, u32)], size: f64) {
        let mut lines = 1.0f64;
        for (text, width) in texts {
            let text = if text.is_null() { String::new() } else { text.py_str() };
            let length = text.chars().count() as f64;
            let estimate = (length * size / 7.5 / (*width as f64 * COLUMN_WIDTH * 1.05).max(1.0)).ceil() + text.matches('\n').count() as f64;
            lines = lines.max(estimate);
        }
        self.sheet.row_heights.insert(row, (lines * size * 1.45 + 4.0).max(15.0));
    }

    fn pill(&mut self, row: u32, column: u32, text: impl Into<Value>, name: &str, span: u32, align: &'static str) {
        let (foreground, background) = tone(name);
        self.put(row, column, text, Put { size: 8.0, bold: true, colour: foreground, background: Some(background), align, span, valign: "center", ..Put::default() });
    }

    fn rating_tone(&self, score: f64) -> &'static str {
        let mark = self.settings.at("scoring.passMark").as_f64().filter(|mark| *mark != 0.0).unwrap_or(70.0);
        if score >= mark {
            "pass"
        } else if score >= mark - 15.0 {
            "warn"
        } else {
            "fail"
        }
    }

    // ---- Sections ----

    fn letterhead(&mut self, title: &str, printed: &str) {
        let row = self.row;
        let company = self.brand.at("companyName").or(self.brand.at("appTitle")).text();
        let department = self.brand.at("departmentHeader").text();
        let accent = self.accent.clone();
        self.put(row, 1, company, Put { size: 14.0, bold: true, span: 5, wrap: false, ..Put::default() });
        self.put(row, 6, title, Put { size: 11.0, bold: true, colour: &accent, align: "right", span: 3, wrap: false, ..Put::default() });
        self.put(row + 1, 1, department, Put { size: 8.0, colour: MUTED, span: 5, wrap: false, ..Put::default() });
        self.put(row + 1, 6, format!("Printed {printed}"), Put { size: 8.0, colour: MUTED, align: "right", span: 3, wrap: false, ..Put::default() });
        self.sheet.row_heights.insert(row, 22.0);
        for column in 1..=COLUMNS {
            self.sheet.cell(row + 1, column).border = Border { bottom: Some(("medium", accent.clone())), ..Border::default() };
        }
        self.row = row + 3;
    }

    /// The accent bar and bold title of a section.
    fn heading(&mut self, text: &str) {
        self.row += 1;
        let row = self.row;
        self.put(row, 1, text, Put { size: 11.0, bold: true, span: COLUMNS, wrap: false, ..Put::default() });
        self.sheet.cell(row, 1).border = Border { left: Some(("thick", self.accent.clone())), ..Border::default() };
        self.sheet.row_heights.insert(row, 19.0);
        self.row += 2;
    }

    fn banner(&mut self, text: &str) {
        self.row += 1;
        let accent = self.accent.clone();
        self.put(self.row, 1, text, Put { size: 11.0, bold: true, colour: "FFFFFF", background: Some(&accent), span: COLUMNS, wrap: false, valign: "center", ..Put::default() });
        self.sheet.row_heights.insert(self.row, 21.0);
        self.row += 2;
    }

    fn audit_details(&mut self, session: &Value, location: Option<&str>) {
        let status = exports::status_of(session);
        let reference = if session.at("audit_ref").truthy() { session.at("audit_ref").clone() } else { "Draft".into() };
        self.put(self.row, 1, reference, Put { size: 11.0, bold: true, span: 6, wrap: false, ..Put::default() });
        let name = if status == "Completed" || status == "Closed" { "pass" } else { "warn" };
        self.pill(self.row, 8, status, name, 1, "center");
        self.sheet.row_heights.insert(self.row, 19.0);
        self.row += 1;
        let location = match location {
            Some(location) => Value::str(location),
            None => if session.at("zone").truthy() { session.at("zone").clone() } else { "All Locations".into() },
        };
        let mut fields = vec![
            ("Outlet", session.at("outlet").clone()),
            ("Location", location),
            ("Date", Value::str(format!("{} {}", session.at("audit_date").text(), session.at("audit_time").text()).trim())),
            ("Auditor", session.at("auditor").clone()),
            ("Audit type", if session.at("audit_type").truthy() { session.at("audit_type").clone() } else { "Standard".into() }),
        ];
        if session.at("closed_at").truthy() {
            fields.push(("Closed", Value::str(exports::closed_text(session))));
        }
        if session.at("remarks").truthy() {
            fields.push(("Remarks", session.at("remarks").clone()));
        }
        let top = self.row;
        for chunk in fields.chunks(4) {
            for (index, (label, value)) in chunk.iter().enumerate() {
                let column = 1 + index as u32 * 2;
                self.put(self.row, column, label.to_uppercase(), Put { size: 7.0, colour: MUTED, background: Some(SURFACE_ALT), span: 2, ..Put::default() });
                let shown = if value.truthy() { value.clone() } else { Value::str("—") };
                self.put(self.row + 1, column, shown, Put { background: Some(SURFACE_ALT), span: 2, ..Put::default() });
            }
            for column in 1..=COLUMNS {
                for row in [self.row, self.row + 1] {
                    self.sheet.cell(row, column).fill = Some(SURFACE_ALT.into());
                }
            }
            let texts: Vec<(Value, u32)> = chunk.iter().map(|(_, value)| (value.clone(), 2)).collect();
            self.height(self.row + 1, &texts, 9.0);
            self.row += 2;
        }
        self.boxed(top, 1, self.row - 1, COLUMNS, false);
        self.row += 1;
    }

    fn scorecard(&mut self, summary: &Value, findings: &[Value]) {
        let (closed, priority) = exports::finding_counts(findings);
        let score_colour = tone(if summary.at("meetsPassMark").truthy() { "pass" } else { "fail" }).0;
        let pass_mark = format_g(summary.at("passMark").as_f64().unwrap_or(70.0));
        let tiles: [(&str, Value, &str, String); 4] = [
            ("Score", summary.at("score").clone(), score_colour, format!("{} · pass mark {pass_mark}", summary.at("rating").text())),
            ("Checks", summary.at("total").clone(), TEXT, format!("{} passed · {} failed · {} N/A", summary.at("passed").py_str(), summary.at("failed").py_str(), summary.at("notApplicable").py_str())),
            ("Findings", Value::Int(findings.len() as i64), TEXT, format!("{priority} priority · {} non-priority", findings.len() - priority)),
            ("Open follow-up", Value::Int((findings.len() - closed) as i64), TEXT, format!("{closed} closed")),
        ];
        for (index, (label, value, colour, note)) in tiles.iter().enumerate() {
            let column = 1 + index as u32 * 2;
            self.put(self.row, column, label.to_uppercase(), Put { size: 7.0, colour: MUTED, span: 2, ..Put::default() });
            self.put(self.row + 1, column, value.clone(), Put { size: 16.0, bold: true, colour, span: 2, valign: "center", ..Put::default() });
            self.put(self.row + 2, column, note.as_str(), Put { size: 8.0, colour: MUTED, span: 2, ..Put::default() });
            self.boxed(self.row, column, self.row + 2, column + 1, false);
        }
        self.sheet.row_heights.insert(self.row + 1, 24.0);
        let notes: Vec<(Value, u32)> = tiles.iter().map(|(.., note)| (Value::str(note), 2)).collect();
        self.height(self.row + 2, &notes, 8.0);
        self.row += 4;
    }

    /// A list like the app's: soft header, rules between rows, results coloured. Spans give each
    /// column's width in sheet columns from `start`; tones colour chosen cells by (row index,
    /// column): a tone name, or a hex colour for the text. Returns the header's row.
    fn table(&mut self, headers: &[&str], rows: &[Vec<Value>], spans: &[u32], result_column: Option<usize>, tones: &[((usize, usize), String)], size: f64, start: u32) -> u32 {
        let starts: Vec<u32> = (0..spans.len()).map(|index| start + spans[..index].iter().sum::<u32>()).collect();
        let top = self.row;
        let soft = self.accent_soft.clone();
        for (index, header) in headers.iter().enumerate() {
            self.put(self.row, starts[index], *header, Put { size: 8.0, bold: true, colour: MUTED, background: Some(&soft), span: spans[index], wrap: false, ..Put::default() });
        }
        self.row += 1;
        for (row_index, values) in rows.iter().enumerate() {
            for (index, value) in values.iter().enumerate() {
                let chosen = tones.iter().find(|(key, _)| *key == (row_index, index)).map(|(_, tone)| tone.clone());
                let chosen = chosen.or_else(|| if Some(index) == result_column { result_tone(&value.py_str()).map(str::to_string) } else { None });
                match chosen {
                    Some(name) if !is_tone(&name) => self.put(self.row, starts[index], value.clone(), Put { size, bold: true, colour: &name, span: spans[index], ..Put::default() }),
                    Some(name) => self.pill(self.row, starts[index], value.clone(), &name, spans[index], "left"),
                    None => self.put(self.row, starts[index], value.clone(), Put { size, span: spans[index], ..Put::default() }),
                }
            }
            let texts: Vec<(Value, u32)> = values.iter().enumerate().map(|(index, value)| (value.clone(), spans[index])).collect();
            self.height(self.row, &texts, size);
            self.row += 1;
        }
        let right = start + spans.iter().sum::<u32>() - 1;
        self.boxed(top, start, self.row - 1, right, true);
        top
    }

    fn place_picture(&mut self, picture: exports::Picture, row: u32, column: u32, box_width: u32, box_height: u32) {
        let ratio = (box_width as f64 / picture.width.max(1) as f64).min(box_height as f64 / picture.height.max(1) as f64).min(1.0);
        self.sheet.pictures.push(Picture {
            anchor: (row, column),
            width_px: ((picture.width as f64 * ratio).round() as u32).max(1),
            height_px: ((picture.height as f64 * ratio).round() as u32).max(1),
            data: picture.data,
            extension: if picture.png { "png" } else { "jpeg" },
        });
    }

    /// Photos side by side, the marked copy beside its original; returns how many were placed.
    fn photos(&mut self, values: &[Value], label: Option<&str>) -> R<usize> {
        if !self.with_media {
            return Ok(0);
        }
        let mut placed = Vec::new();
        for value in values {
            for marked in [false, true] {
                if let Some(picture) = exports::picture(value, marked)? {
                    let caption = if marked {
                        "Marked".to_string()
                    } else {
                        label.map(str::to_string).unwrap_or_else(|| if value.at("name").truthy() { value.at("name").text() } else { "Photo".into() })
                    };
                    placed.push((picture, caption));
                }
            }
        }
        if placed.is_empty() {
            return Ok(0);
        }
        let count = placed.len();
        let span = COLUMNS / 4;
        let mut placed = placed.into_iter().peekable();
        while placed.peek().is_some() {
            self.sheet.row_heights.insert(self.row, 82.0);
            for index in 0..4 {
                let Some((picture, caption)) = placed.next() else { break };
                let column = 1 + index * span;
                self.place_picture(picture, self.row, column, (span as f64 * COLUMN_WIDTH * 7.0) as u32 - 6, 106);
                self.put(self.row + 1, column, caption, Put { size: 7.5, colour: MUTED, span, ..Put::default() });
            }
            self.row += 2;
        }
        Ok(count)
    }

    /// Each asset: a bar with its details and how it did, its checks, then its photos.
    fn checklist(&mut self, items: &[Value], assets: &Value) -> R<()> {
        self.heading("Checklist");
        for checks in exports::checks_by_asset(items) {
            let first = &checks[0];
            let empty = Value::object();
            let asset = assets.get(&first.at("equipmentId").text()).unwrap_or(&empty);
            let failed = checks.iter().filter(|check| exports::result_of(check) == "Fail").count();
            let mut name = first.at("section").or(asset.at("name")).text();
            if name.is_empty() {
                name = "Item".into();
            }
            let group = first.at("groupCount").or(&Value::Int(1)).py_int().unwrap_or(1);
            if group > 1 {
                name = format!("{name} ×{group}");
            }
            let row = self.row;
            self.put(row, 1, name, Put { size: 9.5, bold: true, background: Some(SURFACE_ALT), span: 6, wrap: false, ..Put::default() });
            let count = format!("{} check{}", checks.len(), if checks.len() != 1 { "s" } else { "" });
            self.put(row, 7, count, Put { size: 8.0, colour: MUTED, background: Some(SURFACE_ALT), align: "right", ..Put::default() });
            if failed > 0 {
                self.pill(row, 8, format!("{failed} failed"), "fail", 1, "center");
            } else {
                self.pill(row, 8, "All passed", "pass", 1, "center");
            }
            let details = if group == 1 { exports::asset_details_line(asset) } else { exports::group_details_line(asset, group) };
            self.put(row + 1, 1, details.as_str(), Put { size: 8.0, colour: MUTED, background: Some(SURFACE_ALT), span: COLUMNS, ..Put::default() });
            self.height(row + 1, &[(Value::str(&details), COLUMNS)], 8.0);
            self.boxed(row, 1, row + 1, COLUMNS, false);
            self.row += 2;
            let rows: Vec<Vec<Value>> = checks.iter().map(|check| vec![Value::str(check.at("item").text()), exports::result_of(check).into(), Value::str(exports::remark_with_assets(check))]).collect();
            self.table(&["Check", "Result", "Remark"], &rows, &[4, 1, 3], Some(1), &[], 9.0, 1);
            if self.photos(&exports::check_photos(&checks), None)? == 0 && self.photos(&exports::images_of(asset.at("photos")).into_iter().take(4).collect::<Vec<_>>(), Some("Registered photo"))? == 0 {
                self.put(self.row, 1, "No photo.", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
                self.row += 1;
            }
            self.row += 1;
        }
        Ok(())
    }

    /// Each finding as a card: reference and status, where and who, then what was found.
    fn findings(&mut self, findings: &[Value]) {
        self.heading("Findings");
        if findings.is_empty() {
            self.put(self.row, 1, "No findings.", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
            self.row += 2;
            return;
        }
        for finding in findings {
            let status = if finding.at("status").truthy() { finding.at("status").text() } else { "Open".into() };
            let top = self.row;
            let reference = if finding.at("finding_ref").truthy() { finding.at("finding_ref").clone() } else { "Finding".into() };
            self.put(self.row, 1, reference, Put { size: 9.5, bold: true, span: 6, wrap: false, ..Put::default() });
            let name = if status == "Closed" { "pass" } else { "warn" };
            self.pill(self.row, 8, status, name, 1, "center");
            self.row += 1;
            let meta = exports::finding_meta(finding);
            self.put(self.row, 1, meta.as_str(), Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
            self.height(self.row, &[(Value::str(&meta), COLUMNS)], 8.0);
            self.row += 1;
            for (key, label) in [("comment", "Finding"), ("cause", "Cause"), ("recommendation", "Recommendation"), ("required_action", "Required action"), ("closed_at", "Closed")] {
                if finding.at(key).truthy() {
                    self.put(self.row, 1, label, Put { size: 8.0, colour: MUTED, ..Put::default() });
                    self.put(self.row, 2, finding.at(key).clone(), Put { span: COLUMNS - 1, ..Put::default() });
                    self.height(self.row, &[(finding.at(key).clone(), COLUMNS - 1)], 9.0);
                    self.row += 1;
                }
            }
            self.boxed(top, 1, self.row - 1, COLUMNS, false);
            let stripe = tone(if exports::is_priority(finding) { "fail" } else { "warn" }).0;
            for row in top..self.row {
                let cell = self.sheet.cell(row, 1);
                cell.border = Border { left: Some(("thick", stripe.to_string())), right: None, top: cell.border.top.clone(), bottom: cell.border.bottom.clone() };
            }
            self.row += 1;
        }
    }

    fn signatures(&mut self, session: &Value) -> R<()> {
        self.heading("Signatures");
        let top = self.row;
        for (index, (key, label)) in SIGNATURES.iter().enumerate() {
            let signature = session.at("signatures").at(key);
            let column = if index < 2 { 1 + index as u32 * 3 } else { 7 };
            let span = 2;
            self.put(top, column, label.to_uppercase(), Put { size: 7.0, colour: MUTED, span, ..Put::default() });
            let name = if signature.at("name").truthy() { signature.at("name").clone() } else { Value::str("—") };
            self.put(top + 2, column, name, Put { size: 9.5, bold: true, span, ..Put::default() });
            let signed: String = signature.at("signedAt").text().chars().take(10).collect();
            let line = if signed.is_empty() { "Not signed".to_string() } else { format!("Signed {signed}") };
            self.put(top + 3, column, line, Put { size: 8.0, colour: MUTED, span, ..Put::default() });
            if self.with_media && signature.truthy() {
                if let Some(picture) = exports::picture(signature, false)? {
                    self.place_picture(picture, top + 1, column, 200, 52);
                }
            }
            self.boxed(top, column, top + 3, column + span - 1, false);
        }
        self.sheet.row_heights.insert(top + 1, 44.0);
        self.row = top + 5;
        Ok(())
    }

    // ---- Charts ----

    fn bar_chart(&self, title: &str, values: Range, labels: Range, colours: &[String], maximum: Option<f64>) -> Chart {
        let mut chart = Chart::new(ChartKind::Bar);
        chart.title = Some(title.to_string());
        chart.legend = None;
        chart.value_min = Some(0.0);
        chart.value_max = maximum;
        let mut series = Series::new(values);
        series.categories = Some(labels);
        series.points = colours.iter().cloned().enumerate().collect();
        series.labels = true;
        chart.series.push(series);
        chart
    }

    /// The PDF's "At a glance": results of every check, findings by asset type. Each chart reads a
    /// small table above it, so editing the numbers redraws it.
    fn charts(&mut self, summary: &Value, findings: &[Value]) {
        self.heading("At a glance");
        let top = self.row;
        let rows = vec![
            vec![Value::str("Passed"), summary.at("passed").clone()],
            vec![Value::str("Failed"), summary.at("failed").clone()],
            vec![Value::str("N/A"), summary.at("notApplicable").clone()],
        ];
        self.table(&["Checks", "Count"], &rows, &[2, 1], None, &[((0, 0), "pass".into()), ((1, 0), "fail".into()), ((2, 0), "neutral".into())], 9.0, 1);
        let mut categories: Vec<(String, i64)> = Vec::new();
        for finding in findings {
            let name = if finding.at("category").truthy() { finding.at("category").text() } else { "No asset type".into() };
            match categories.iter_mut().find(|(known, _)| *known == name) {
                Some(entry) => entry.1 += 1,
                None => categories.push((name, 1)),
            }
        }
        categories.sort_by(|a, b| b.1.cmp(&a.1));
        categories.truncate(8);
        self.row = top;
        if !categories.is_empty() {
            let soft = self.accent_soft.clone();
            self.put(self.row, 5, "Findings by asset type", Put { size: 8.0, bold: true, colour: MUTED, background: Some(&soft), span: 3, wrap: false, ..Put::default() });
            self.put(self.row, 8, "Count", Put { size: 8.0, bold: true, colour: MUTED, background: Some(&soft), wrap: false, ..Put::default() });
            self.row += 1;
            for (category, count) in &categories {
                self.put(self.row, 5, category.as_str(), Put { span: 3, ..Put::default() });
                self.put(self.row, 8, *count, Put::default());
                self.row += 1;
            }
            self.boxed(top, 5, self.row - 1, 8, true);
        }
        self.row = self.row.max(top + 4) + 1;
        let mut doughnut = Chart::new(ChartKind::Doughnut);
        doughnut.hole = 55;
        doughnut.title = Some("Checks".into());
        let mut series = Series::new(Range::column(3, top + 1, top + 3));
        series.categories = Some(Range::column(1, top + 1, top + 3));
        series.points = vec![(0, tone("pass").0.into()), (1, tone("fail").0.into()), (2, "B4BDB8".into())];
        series.point_outline = true;
        doughnut.series.push(series);
        doughnut.anchor = (self.row, 1);
        doughnut.size_cm = (7.6, 6.2);
        self.sheet.charts.push(doughnut);
        if !categories.is_empty() {
            let count = categories.len() as u32;
            let mut bars = self.bar_chart("Findings by asset type", Range::column(8, top + 1, top + count), Range::column(5, top + 1, top + count), &vec![tone("fail").0.to_string(); categories.len()], None);
            bars.anchor = (self.row, 4);
            bars.size_cm = (13.5, 6.2);
            self.sheet.charts.push(bars);
        }
        self.row += 13;
    }

    /// One row per location, its grade coloured; the score column feeds the chart below.
    fn location_grading(&mut self, parts: &[Part]) {
        self.heading("Location grading");
        let mut rows = Vec::new();
        let mut tones = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let summary = &part.summary;
            let score = summary.at("score").as_f64().unwrap_or(0.0);
            rows.push(vec![
                Value::str(&part.location),
                summary.at("total").clone(),
                summary.at("passed").clone(),
                summary.at("failed").clone(),
                summary.at("notApplicable").clone(),
                summary.at("score").clone(),
                Value::str(rating_for_score(score, self.settings)),
            ]);
            tones.push(((index, 6), self.rating_tone(score).to_string()));
        }
        let top = self.table(&["Location", "Checks", "Passed", "Failed", "N/A", "Score", "Grade"], &rows, &[2, 1, 1, 1, 1, 1, 1], None, &tones, 9.0, 1);
        if parts.len() > 1 {
            self.row += 1;
            let colours: Vec<String> = parts.iter().map(|part| tone(self.rating_tone(part.summary.at("score").as_f64().unwrap_or(0.0))).0.to_string()).collect();
            let count = parts.len() as u32;
            let mut chart = self.bar_chart("Score by location", Range::column(7, top + 1, top + count), Range::column(1, top + 1, top + count), &colours, Some(100.0));
            let height = (0.55 * parts.len() as f64 + 2.0).max(5.0);
            chart.size_cm = (21.5, height);
            chart.anchor = (self.row, 1);
            self.sheet.charts.push(chart);
            self.row += (height / 0.53) as u32 + 2;
        }
    }

    /// The PDF's "Assets inspected": the audit's assets counted by each register attribute, split
    /// into all passed and with failures, each table with its stacked bars beside it.
    fn asset_attributes(&mut self, items: &[Value], assets: &Value) {
        let (total, failing, attributes) = figures::asset_attributes(items, assets);
        self.heading("Assets inspected");
        let summary = format!("{total} asset{} inspected · {} passed every check · {failing} with a failure", if total != 1 { "s" } else { "" }, total - failing);
        self.put(self.row - 1, 1, summary, Put { size: 8.0, colour: MUTED, span: COLUMNS, wrap: false, ..Put::default() });
        for (name, rows) in attributes {
            self.put(self.row, 1, name, Put { size: 9.5, bold: true, span: COLUMNS, wrap: false, ..Put::default() });
            self.row += 1;
            let values: Vec<Vec<Value>> = rows.iter().map(|(label, passed, failed)| vec![Value::str(label), Value::Int(*passed), Value::Int(*failed)]).collect();
            let mut tones = Vec::new();
            for index in 0..rows.len() {
                tones.push(((index, 1), PASS_HEX.to_string()));
                tones.push(((index, 2), FAIL_HEX.to_string()));
            }
            let top = self.table(&[name, "All passed", "With failures"], &values, &[2, 1, 1], None, &tones, 9.0, 1);
            let count = rows.len() as u32;
            let mut chart = Chart::new(ChartKind::Bar);
            chart.stacked = true;
            chart.overlap = Some(100);
            for (column, colour) in [(3, PASS_HEX), (4, FAIL_HEX)] {
                let mut series = Series::new(Range::column(column, top + 1, top + count));
                series.title = Some((top, column));
                series.categories = Some(Range::column(1, top + 1, top + count));
                series.fill = Some(colour.into());
                chart.series.push(series);
            }
            chart.reverse_categories = true;
            chart.legend = Some("b");
            let rows_tall = (rows.len() as u32 + 4).max(6);
            chart.size_cm = (4.0 * CM_PER_COLUMN, rows_tall as f64 * CM_PER_ROW);
            chart.anchor = (top, 5);
            self.sheet.charts.push(chart);
            self.row = self.row.max(top + rows_tall) + 1;
        }
    }

    fn finish(self) -> Sheet {
        self.sheet
    }
}

fn printed() -> String {
    crate::clock::local_now().minutes_text()
}

fn workbook() -> Workbook {
    Workbook::new(FONT)
}

/// The audit as an editable workbook laid out like its PDFs: Overall first (details, score,
/// charts, grading, the assets by attribute, findings, signatures), then one sheet per location in
/// full (each asset's checks and photos, and the findings there).
pub fn inspection_workbook(session: &Value, summary: &Value, parts: &[Part], assets: &Value, brand: &Value, settings: &Value) -> R<Vec<u8>> {
    let printed = printed();
    let findings: Vec<Value> = session.at("findings").items().to_vec();
    let mut book = workbook();
    let mut sheet = SheetWriter::new("Overall", brand, settings, true);
    sheet.letterhead("Facilities Audit Report", &printed);
    sheet.audit_details(session, None);
    sheet.scorecard(summary, &findings);
    sheet.charts(summary, &findings);
    sheet.location_grading(parts);
    sheet.asset_attributes(session.at("items").items(), assets);
    sheet.findings(&findings);
    sheet.signatures(session)?;
    book.sheets.push(sheet.finish());
    let mut taken = vec!["overall".to_string()];
    for part in parts {
        let mut sheet = SheetWriter::new(&exports::sheet_title(&part.location, &mut taken), brand, settings, true);
        sheet.letterhead("Audit Report — Location", &printed);
        sheet.banner(&format!("Location: {}", part.location));
        sheet.audit_details(session, Some(&part.location));
        sheet.scorecard(&part.summary, &part.findings);
        sheet.checklist(&part.items, &part.assets)?;
        sheet.findings(&part.findings);
        book.sheets.push(sheet.finish());
    }
    Ok(book.save())
}

/// Every audit of one location, laid out like the location PDF: an overview of the audits, then a
/// sheet per audit with its checks, photos, findings, and signatures there.
pub fn location_workbook(outlet: &str, location: &str, period: &str, audits: &[LocationAudit], brand: &Value, settings: &Value) -> R<Vec<u8>> {
    let printed = printed();
    let mut book = workbook();
    let mut sheet = SheetWriter::new("Overview", brand, settings, true);
    sheet.letterhead("Location Audit Report", &printed);
    let row = sheet.row;
    sheet.put(row, 1, format!("{outlet} · {location}"), Put { size: 11.0, bold: true, span: 8, wrap: false, ..Put::default() });
    sheet.put(row + 1, 1, format!("Period: {period}"), Put { size: 8.0, colour: "5F6C66", span: 8, wrap: false, ..Put::default() });
    sheet.row += 2;
    sheet.heading("Audits of this location");
    let mut rows = Vec::new();
    let mut tones = Vec::new();
    for (index, audit) in audits.iter().enumerate() {
        let (session, summary) = (&audit.session, &audit.summary);
        rows.push(vec![
            session.at("audit_date").clone(),
            Value::str(session.at("audit_ref").text()),
            Value::str(session.at("auditor").text()),
            summary.at("total").clone(),
            summary.at("failed").clone(),
            summary.at("score").clone(),
            summary.at("rating").clone(),
            Value::Int(audit.findings.len() as i64),
        ]);
        tones.push(((index, 6), sheet.rating_tone(summary.at("score").as_f64().unwrap_or(0.0)).to_string()));
    }
    if rows.is_empty() {
        sheet.put(sheet.row, 1, "No completed audit covered this location in this period.", Put { size: 8.0, colour: "5F6C66", span: 8, ..Put::default() });
    } else {
        sheet.table(&["Date", "Audit", "Auditor", "Checks", "Failed", "Score", "Rating", "Findings"], &rows, &[1; 8], None, &tones, 9.0, 1);
    }
    book.sheets.push(sheet.finish());
    let mut taken = vec!["overview".to_string()];
    for audit in audits {
        let session = &audit.session;
        let name = if session.at("audit_ref").truthy() { session.at("audit_ref").text() } else { format!("Audit {}", session.at("id").py_str()) };
        let mut sheet = SheetWriter::new(&exports::sheet_title(&name, &mut taken), brand, settings, true);
        sheet.letterhead("Location Audit Report", &printed);
        let reference = if session.at("audit_ref").truthy() { session.at("audit_ref").text() } else { "Audit".into() };
        sheet.banner(&format!("{reference} · {} · {location}", session.at("audit_date").text()));
        sheet.audit_details(session, Some(location));
        sheet.scorecard(&audit.summary, &audit.findings);
        sheet.checklist(&audit.items, &audit.assets)?;
        sheet.findings(&audit.findings);
        sheet.signatures(session)?;
        book.sheets.push(sheet.finish());
    }
    Ok(book.save())
}

// ---- The Reports page ----

fn place(sheet: &mut SheetWriter, mut chart: Chart, column: u32, row: u32, columns: f64, rows: f64) {
    chart.size_cm = (columns * CM_PER_COLUMN, rows * CM_PER_ROW);
    chart.anchor = (row, column);
    sheet.sheet.charts.push(chart);
}

fn plain(kind: ChartKind, title: Option<&str>) -> Chart {
    let mut chart = Chart::new(kind);
    chart.title = title.map(str::to_string);
    chart
}

fn coloured(mut series: Series, colour: &str) -> Series {
    series.fill = Some(colour.to_string());
    series
}

fn line(mut series: Series, colour: &str) -> Series {
    series.line = Some(colour.to_string());
    series.marker = true;
    series
}

/// The Reports page as a workbook laid out like the page (and its PDF): the four rings, audits and
/// findings by month, outlets, breakdowns, time to act, and people on the first sheet; the People
/// table and Findings on their own sheets. Each chart reads the table beside it.
pub fn report_workbook(data: &Value, findings: &[Value], scope: &str, brand: &Value, settings: &Value) -> R<Vec<u8>> {
    let default_scoring = crate::json::obj([("passMark", Value::Int(70)), ("goodBand", Value::Int(70)), ("belowBand", Value::Int(60))]);
    let scoring = if data.at("scoring").truthy() { data.at("scoring").clone() } else { default_scoring };
    let charts = data.at("charts");
    let printed = printed();
    let mut book = workbook();
    let mut sheet = SheetWriter::new("Report", brand, settings, false);
    let accent = sheet.accent.clone();
    let colour = |code: Option<&str>| code.map(str::to_string).unwrap_or_else(|| accent.clone());
    sheet.letterhead("Audit Report", &printed);
    sheet.put(sheet.row, 1, scope, Put { size: 11.0, bold: true, span: COLUMNS, wrap: false, ..Put::default() });
    sheet.row += 2;

    // The four rings, two columns each.
    let top = sheet.row;
    let mut deepest = top;
    for (index, ring) in figures::overview(data).iter().enumerate() {
        let column = 1 + index as u32 * 2;
        sheet.row = top;
        sheet.put(sheet.row, column, ring.title, Put { size: 10.5, bold: true, span: 2, wrap: false, ..Put::default() });
        sheet.put(sheet.row + 1, column, ring.note.as_str(), Put { size: 8.0, colour: MUTED, span: 2, wrap: false, ..Put::default() });
        let chart_row = sheet.row + 2;
        sheet.row = chart_row + 10;
        let total: i64 = ring.parts.iter().map(|(_, value, _)| value).sum();
        let first = sheet.row;
        for (label, value, code) in &ring.parts {
            let share = if total != 0 { crate::activity::py_round(*value as f64 * 100.0 / total as f64) } else { 0 };
            let tint = colour(Some(code));
            sheet.put(sheet.row, column, format!("{label} ({share}%)"), Put { size: 8.5, bold: true, colour: &tint, ..Put::default() });
            sheet.put(sheet.row, column + 1, *value, Put { align: "right", ..Put::default() });
            sheet.row += 1;
        }
        if !ring.footer.is_empty() {
            sheet.put(sheet.row, column, ring.footer.as_str(), Put { size: 8.0, colour: MUTED, span: 2, wrap: false, ..Put::default() });
            sheet.row += 1;
        }
        let mut doughnut = Chart::new(ChartKind::Doughnut);
        doughnut.hole = 60;
        let mut series = Series::new(Range::column(column + 1, first, first + ring.parts.len() as u32 - 1));
        series.categories = Some(Range::column(column, first, first + ring.parts.len() as u32 - 1));
        series.points = ring.parts.iter().enumerate().map(|(point, (_, _, code))| (point, colour(Some(code)))).collect();
        series.point_outline = true;
        doughnut.series.push(series);
        doughnut.legend = None;
        doughnut.title = Some(format!("{} {}", ring.centre, ring.caption));
        place(&mut sheet, doughnut, column, chart_row, 2.0, 10.0);
        sheet.boxed(top, column, sheet.row - 1, column + 1, false);
        deepest = deepest.max(sheet.row);
    }
    sheet.row = deepest + 1;

    // Audits by month: columns for the count, a line for the score, and the pass mark.
    sheet.heading("Audits by month");
    let monthly = figures::monthly_audits(charts);
    if monthly.is_empty() {
        sheet.put(sheet.row, 1, "No data yet", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
        sheet.row += 2;
    } else {
        let rows: Vec<Vec<Value>> = monthly.iter().map(|(month, count, score)| vec![Value::str(month), Value::Int(*count), Value::Int(*score), scoring.at("passMark").clone()]).collect();
        let top = sheet.table(&["Month", "Audits", "Average score", "Pass mark"], &rows, &[2, 2, 2, 2], None, &[], 9.0, 1);
        let last = top + monthly.len() as u32;
        let mut bars = plain(ChartKind::Column, None);
        let mut series = coloured(Series::new(Range::column(3, top + 1, last)), &accent);
        series.title = Some((top, 3));
        series.categories = Some(Range::column(1, top + 1, last));
        bars.series.push(series);
        bars.value_title = Some("Audits".into());
        let mut lines = plain(ChartKind::Line, None);
        for (column, code, dashed) in [(5, TEXT, false), (7, "B4BDB8", true)] {
            let mut series = line(Series::new(Range::column(column, top + 1, last)), code);
            series.title = Some((top, column));
            series.categories = Some(Range::column(1, top + 1, last));
            series.dashed = dashed;
            series.marker = !dashed;
            lines.series.push(series);
        }
        lines.value_min = Some(0.0);
        lines.value_max = Some(100.0);
        lines.value_title = Some("Score".into());
        bars.overlay = Some(Box::new(lines));
        bars.legend = Some("b");
        sheet.row += 1;
        let row = sheet.row;
        place(&mut sheet, bars, 1, row, COLUMNS as f64, 14.0);
        sheet.row += 15;
    }

    // Findings by month, stacked.
    sheet.heading("Findings by month");
    let found = figures::monthly_findings(charts);
    if found.is_empty() {
        sheet.put(sheet.row, 1, "No data yet", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
        sheet.row += 2;
    } else {
        let rows: Vec<Vec<Value>> = found.iter().map(|(month, priority, other)| vec![Value::str(month), Value::Int(*priority), Value::Int(*other)]).collect();
        let mut tones = Vec::new();
        for index in 0..found.len() {
            tones.push(((index, 1), figures::FAIL.to_string()));
            tones.push(((index, 2), figures::INFO.to_string()));
        }
        let top = sheet.table(&["Month", "Priority", "Non-priority"], &rows, &[2, 3, 3], None, &tones, 9.0, 1);
        let last = top + found.len() as u32;
        let mut stacked = plain(ChartKind::Column, None);
        stacked.stacked = true;
        stacked.overlap = Some(100);
        for (column, code) in [(3, figures::FAIL), (6, figures::INFO)] {
            let mut series = coloured(Series::new(Range::column(column, top + 1, last)), code);
            series.title = Some((top, column));
            series.categories = Some(Range::column(1, top + 1, last));
            stacked.series.push(series);
        }
        stacked.legend = Some("b");
        sheet.row += 1;
        let row = sheet.row;
        place(&mut sheet, stacked, 1, row, COLUMNS as f64, 13.0);
        sheet.row += 14;
    }

    // A table on the left and its bars on the right, as each card of the page.
    let bar_beside = |sheet: &mut SheetWriter, title: &str, note: &str, headers: &[&str], rows: &[Vec<Value>], value_column: u32, spans: &[u32], colours: &[String], maximum: Option<f64>| {
        sheet.heading(title);
        if !note.is_empty() {
            sheet.put(sheet.row - 1, 1, note, Put { size: 8.0, colour: MUTED, span: COLUMNS, wrap: false, ..Put::default() });
        }
        if rows.is_empty() {
            sheet.put(sheet.row, 1, "No data yet", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
            sheet.row += 2;
            return;
        }
        let top = sheet.table(headers, rows, spans, None, &[], 9.0, 1);
        let last = top + rows.len() as u32;
        let mut chart = plain(ChartKind::Bar, None);
        let mut series = Series::new(Range::column(value_column, top + 1, last));
        series.categories = Some(Range::column(1, top + 1, last));
        series.points = colours.iter().cloned().enumerate().collect();
        series.labels = true;
        chart.series.push(series);
        chart.reverse_categories = true;
        chart.legend = None;
        if let Some(maximum) = maximum {
            chart.value_min = Some(0.0);
            chart.value_max = Some(maximum);
        }
        let rows_tall = (rows.len() as u32 + 3).max(6);
        let width: u32 = spans.iter().sum();
        place(sheet, chart, width + 1, top, (COLUMNS - width) as f64, rows_tall as f64);
        sheet.row = sheet.row.max(top + rows_tall) + 1;
    };

    // Outlets.
    let outlets = figures::outlet_scores(data);
    let rows: Vec<Vec<Value>> = outlets.iter().map(|(name, score, average, count, _)| vec![Value::str(name), Value::Int(*score), Value::Int(*average), Value::Int(*count)]).collect();
    let colours: Vec<String> = outlets.iter().map(|(_, score, ..)| figures::score_colour(*score as f64, &scoring).to_string()).collect();
    bar_beside(&mut sheet, "Latest Scores by Outlet", "Each outlet's latest audit score, coloured by grade", &["Outlet", "Latest", "Average", "Audits"], &rows, 2, &[1, 1, 1, 1], &colours, Some(100.0));
    sheet.heading("Performance Distribution");
    let bands = figures::performance_bands(charts);
    let rows: Vec<Vec<Value>> = bands.iter().map(|(label, count, _)| vec![Value::str(*label), Value::Int(*count)]).collect();
    let tones: Vec<((usize, usize), String)> = bands.iter().enumerate().map(|(index, (_, _, code))| ((index, 0), code.to_string())).collect();
    let top = sheet.table(&["Grade", "Audits"], &rows, &[2, 1], None, &tones, 9.0, 1);
    if bands.iter().map(|(_, count, _)| count).sum::<i64>() > 0 {
        let mut doughnut = Chart::new(ChartKind::Doughnut);
        doughnut.hole = 60;
        let mut series = Series::new(Range::column(3, top + 1, top + bands.len() as u32));
        series.categories = Some(Range::column(1, top + 1, top + bands.len() as u32));
        series.points = bands.iter().enumerate().map(|(index, (_, _, code))| (index, code.to_string())).collect();
        doughnut.series.push(series);
        doughnut.legend = Some("r");
        place(&mut sheet, doughnut, 4, top, 5.0, 9.0);
        sheet.row = sheet.row.max(top + 9) + 1;
    }
    let pairs = figures::comparison(charts);
    sheet.heading("Previous vs Current Audit");
    if pairs.is_empty() {
        sheet.put(sheet.row, 1, "No outlet has two audits yet", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
        sheet.row += 2;
    } else {
        let rows: Vec<Vec<Value>> = pairs.iter().map(|(name, previous, current)| vec![Value::str(name), Value::from(*previous), Value::from(*current)]).collect();
        let top = sheet.table(&["Outlet", "Previous", "Current"], &rows, &[1, 1, 1], None, &[], 9.0, 1);
        let last = top + pairs.len() as u32;
        let mut grouped = plain(ChartKind::Bar, None);
        for (column, code) in [(2, "B4BDB8".to_string()), (3, accent.clone())] {
            let mut series = coloured(Series::new(Range::column(column, top + 1, last)), &code);
            series.title = Some((top, column));
            series.categories = Some(Range::column(1, top + 1, last));
            grouped.series.push(series);
        }
        grouped.value_min = Some(0.0);
        grouped.value_max = Some(100.0);
        grouped.legend = Some("b");
        let rows_tall = (2 * pairs.len() as u32 + 4).max(7);
        place(&mut sheet, grouped, 4, top, 5.0, rows_tall as f64);
        sheet.row = sheet.row.max(top + rows_tall) + 1;
    }
    sheet.heading("Outlet Rankings");
    if outlets.is_empty() {
        sheet.put(sheet.row, 1, "No completed audits", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
        sheet.row += 1;
    } else {
        let rows: Vec<Vec<Value>> = outlets.iter().enumerate().map(|(index, (name, score, average, count, date))| vec![Value::Int(index as i64 + 1), Value::str(name), Value::Int(*score), Value::Int(*average), Value::Int(*count), Value::str(date)]).collect();
        let tones: Vec<((usize, usize), String)> = outlets.iter().enumerate().map(|(index, (_, score, ..))| ((index, 2), sheet.rating_tone(*score as f64).to_string())).collect();
        sheet.table(&["#", "Outlet", "Latest", "Average", "Audits", "Last audit"], &rows, &[1, 2, 1, 1, 1, 2], None, &tones, 9.0, 1);
    }
    sheet.heading("Critical issues");
    let critical = data.at("criticalIssues").items();
    if critical.is_empty() {
        sheet.put(sheet.row, 1, "No open high-priority work orders.", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
        sheet.row += 1;
    } else {
        let rows: Vec<Vec<Value>> = critical
            .iter()
            .map(|row| vec![row.at("work_order_ref").or(row.at("id")).clone(), row.at("outlet").clone(), Value::str(row.at("zone").text()), Value::str(row.at("title").text()), row.at("status").clone(), Value::str(row.at("due_date").text())])
            .collect();
        sheet.table(&["Work order", "Outlet", "Location", "Title", "Status", "Due"], &rows, &[1, 1, 1, 3, 1, 1], None, &[], 8.5, 1);
    }

    // Breakdowns.
    for breakdown in figures::breakdowns(charts, &scoring) {
        let value_header = match breakdown.suffix {
            "%" => "Closed %",
            "/100" => "Score",
            _ => "Findings",
        };
        let label_header = {
            let tail = breakdown.title.rsplit(" by ").next().unwrap_or(breakdown.title);
            let mut chars = tail.chars();
            chars.next().map(|first| first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()).unwrap_or_default()
        };
        let extra = breakdown.rows.iter().any(|(.., note)| !note.is_empty());
        let headers: Vec<&str> = if extra { vec![&label_header, value_header, "Closed"] } else { vec![&label_header, value_header] };
        let rows: Vec<Vec<Value>> = breakdown
            .rows
            .iter()
            .map(|(label, value, _, note)| {
                let mut row = vec![Value::str(label), Value::Int(*value)];
                if extra {
                    row.push(Value::str(note));
                }
                row
            })
            .collect();
        let colours: Vec<String> = breakdown.rows.iter().map(|(_, _, code, _)| colour(*code)).collect();
        let spans: &[u32] = if extra { &[2, 1, 1] } else { &[2, 1] };
        bar_beside(&mut sheet, breakdown.title, breakdown.note, &headers, &rows, 3, spans, &colours, breakdown.maximum);
    }

    // Time to act: the average and the longest of each step, in minutes for the chart.
    let steps = figures::time_to_act(data);
    sheet.heading("Time to act");
    let minutes = |seconds: i64| ((seconds as f64 / 60.0) * 10.0).round() / 10.0;
    let rows: Vec<Vec<Value>> = steps
        .iter()
        .map(|(label, average, longest, count)| vec![
            Value::str(label),
            Value::str(crate::activity::duration_text(&Value::Int(*average))),
            Value::str(crate::activity::duration_text(&Value::Int(*longest))),
            Value::Int(*count),
            Value::Float(minutes(*average)),
            Value::Float(minutes(*longest)),
        ])
        .collect();
    let top = sheet.table(&["Step", "Average", "Longest", "Times", "Avg. minutes", "Longest minutes"], &rows, &[3, 1, 1, 1, 1, 1], None, &[], 8.5, 1);
    if steps.iter().any(|(.., count)| *count != 0) {
        let last = top + steps.len() as u32;
        let mut timing = plain(ChartKind::Bar, None);
        for (column, code) in [(7, accent.clone()), (8, sheet.accent_soft.clone())] {
            let mut series = coloured(Series::new(Range::column(column, top + 1, last)), &code);
            series.title = Some((top, column));
            series.categories = Some(Range::column(1, top + 1, last));
            timing.series.push(series);
        }
        timing.reverse_categories = true;
        timing.legend = Some("b");
        sheet.row += 1;
        let rows_tall = (2 * steps.len() as u32 + 4).max(7);
        let row = sheet.row;
        place(&mut sheet, timing, 1, row, COLUMNS as f64, rows_tall as f64);
        sheet.row += rows_tall + 1;
    }

    // People: what each person did, stacked as on the page; the full table is on its own sheet.
    let workers = figures::people(data);
    sheet.heading("People");
    if workers.is_empty() {
        sheet.put(sheet.row, 1, "No activity in this period.", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
    } else {
        let rows: Vec<Vec<Value>> = workers.iter().map(|(name, audits, requests, orders)| vec![Value::str(name), Value::Int(*audits), Value::Int(*requests), Value::Int(*orders)]).collect();
        let top = sheet.table(&["Person", "Audits completed", "Requests acted on", "Orders closed"], &rows, &[2, 2, 2, 2], None, &[], 9.0, 1);
        let last = top + workers.len() as u32;
        let mut stacked = plain(ChartKind::Bar, None);
        stacked.stacked = true;
        stacked.overlap = Some(100);
        for (index, (_, code)) in figures::PEOPLE_STACKS.iter().enumerate() {
            let column = 3 + index as u32 * 2;
            let mut series = coloured(Series::new(Range::column(column, top + 1, last)), &colour(*code));
            series.title = Some((top, column));
            series.categories = Some(Range::column(1, top + 1, last));
            stacked.series.push(series);
        }
        stacked.reverse_categories = true;
        stacked.legend = Some("b");
        sheet.row += 1;
        let rows_tall = (workers.len() as u32 + 5).max(7);
        let row = sheet.row;
        place(&mut sheet, stacked, 1, row, COLUMNS as f64, rows_tall as f64);
        sheet.row += rows_tall + 1;
    }
    book.sheets.push(sheet.finish());

    // The lists, each on its own sheet in the same style.
    let (header, rows) = figures::people_table(data);
    book.sheets.push(wide_list("People", brand, settings, &printed, &header, &rows, &[]));
    let columns = [("finding_ref", "Finding"), ("audit_ref", "Audit"), ("audit_date", "Audit date"), ("auditor", "Auditor"), ("outlet", "Outlet"), ("location", "Location"),
                   ("item_name", "Item"), ("criterion", "Check"), ("category", "Asset type"), ("priority", "Priority"), ("assigned_department", "Department"), ("pic", "PIC"),
                   ("status", "Status"), ("comment", "Remark"), ("closed_at", "Closed")];
    let headers: Vec<&str> = columns.iter().map(|(_, label)| *label).collect();
    let rows: Vec<Vec<Value>> = findings.iter().map(|row| columns.iter().map(|(key, _)| if row.at(key).truthy() { row.at(key).clone() } else { Value::str("") }).collect()).collect();
    let tones: Vec<((usize, usize), String)> = findings.iter().enumerate().map(|(index, row)| ((index, 12), if row.at("status").as_str() == Some("Closed") { "pass".into() } else { "warn".into() })).collect();
    book.sheets.push(wide_list("Findings", brand, settings, &printed, &headers, &rows, &tones));
    Ok(book.save())
}

/// A long list on its own sheet: one sheet column per field, so it can be filtered and sorted.
fn wide_list(title: &str, brand: &Value, settings: &Value, printed: &str, header: &[&str], rows: &[Vec<Value>], tones: &[((usize, usize), String)]) -> Sheet {
    let mut sheet = SheetWriter::new(title, brand, settings, false);
    let columns = header.len() as u32;
    for column in 1..=columns.max(COLUMNS) {
        sheet.sheet.column_widths.insert(column, if column > COLUMNS { 16.0 } else { COLUMN_WIDTH });
    }
    sheet.letterhead(title, printed);
    if rows.is_empty() {
        sheet.put(sheet.row, 1, "Nothing in this period.", Put { size: 8.0, colour: MUTED, span: COLUMNS, ..Put::default() });
        return sheet.finish();
    }
    let top = sheet.table(header, rows, &vec![1; header.len()], None, tones, 8.5, 1);
    sheet.sheet.auto_filter = Some(format!("A{top}:{}{}", column_letter(columns), sheet.row - 1));
    sheet.sheet.freeze = Some((top + 1, 1));
    sheet.finish()
}
