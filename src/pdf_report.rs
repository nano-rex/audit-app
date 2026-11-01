//! Paginated audit reports with actual evidence and signature images.
//!
//! They look like the web app: its font (Noto Sans), its colours (the organization's accent, soft
//! panels, green / red results), and compact type so more fits on a page.
//!
//! One audit is printed by build_report (overall) or build_locations_report (chosen locations in
//! full). A location report prints every audit of one location, each limited to that location.

use crate::activity::{duration_text, py_round};
use crate::audits::rating_for_score;
use crate::config;
use crate::error::{internal, R};
use crate::exports::{self, LocationAudit, Part};
use crate::figures;
use crate::json::{format_g, Value};
use crate::pdf::doc::{Canvas, Colour, Document, Face, Fonts, A4};
use crate::pdf::layout::{Align, Cmd, Drawing, Flow, Layout, Paragraph, Picture, Shape, Span, Style, Table, VAlign, ALL};

const SIGNATURES: [(&str, &str); 3] = [("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by")];
const MARGIN: f64 = 36.0;

fn width() -> f64 {
    A4.0 - 2.0 * MARGIN
}

// The web app's palette (web/css/base.css).
fn hex(text: &str) -> Colour {
    Colour::hex(text)
}

fn text_colour() -> Colour {
    hex("#17222b")
}
fn muted() -> Colour {
    hex("#5f6c66")
}
fn border() -> Colour {
    hex("#d3d9d5")
}
fn surface_alt() -> Colour {
    hex("#f7f9f8")
}
fn neutral_soft() -> Colour {
    hex("#eceff0")
}

fn tone(name: &str) -> (Colour, Colour) {
    match name {
        "pass" => (hex("#1c8f4b"), hex("#e3f4ea")),
        "fail" => (hex("#c9303a"), hex("#fde7e8")),
        "warn" => (hex("#b96100"), hex("#fdefd9")),
        _ => (muted(), neutral_soft()),
    }
}

fn result_tone(result: &str) -> Option<&'static str> {
    match result {
        "Pass" => Some("pass"),
        "Fail" => Some("fail"),
        "N/A" => Some("neutral"),
        _ => None,
    }
}

fn load_fonts() -> R<Fonts> {
    let fonts = config::root().join("fonts");
    let regular = std::fs::read(fonts.join("noto-sans-latin-400.ttf")).map_err(|error| internal(format!("PDF font missing: {error}")))?;
    let bold = std::fs::read(fonts.join("noto-sans-latin-700.ttf")).map_err(|error| internal(format!("PDF font missing: {error}")))?;
    // DejaVu covers characters outside the app's font, when the system has it.
    let fallback = ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/dejavu/DejaVuSans.ttf", "/usr/share/fonts/TTF/DejaVuSans.ttf"]
        .iter()
        .find_map(|path| std::fs::read(path).ok());
    Fonts::load(regular, bold, fallback).ok_or_else(|| internal("the PDF fonts could not be read"))
}

/// The organization's styles, and the story being written.
struct Writer {
    doc: Document,
    brand: Value,
    settings: Value,
    story: Vec<Flow>,
    accent: Colour,
    soft: Colour,
    with_media: bool,
}

fn style(face: Face, size: f64, leading: f64, colour: Colour) -> Style {
    Style { face, size, leading, colour, align: Align::Left, space_before: 0.0, space_after: 0.0 }
}

impl Writer {
    fn new(brand: &Value, settings: &Value, with_media: bool) -> R<Writer> {
        let theme = brand.at("theme");
        let accent_hex = if theme.at("accent").truthy() { theme.at("accent").text() } else { config::theme_preset_accent(&theme.at("preset").text()).to_string() };
        let accent = hex(&accent_hex);
        Ok(Writer { doc: Document::new(load_fonts()?), brand: brand.clone(), settings: settings.clone(), story: Vec::new(), accent, soft: accent.mix(0.88), with_media })
    }

    fn style(&self, name: &str) -> Style {
        let body = style(Face::Regular, 8.5, 11.5, text_colour());
        match name {
            "Caption" => style(Face::Regular, 7.5, 9.5, muted()),
            "Title" => style(Face::Bold, 13.0, 16.0, text_colour()),
            "Heading1" => Style { space_before: 10.0, space_after: 5.0, ..style(Face::Bold, 11.0, 14.0, text_colour()) },
            "Heading2" => Style { space_before: 4.0, space_after: 3.0, ..style(Face::Bold, 9.5, 12.5, text_colour()) },
            "Heading3" => style(Face::Bold, 9.0, 11.5, text_colour()),
            "TableHead" => style(Face::Bold, 7.5, 9.5, muted()),
            "Label" => style(Face::Regular, 7.0, 9.0, muted()),
            "Value" => style(Face::Bold, 13.0, 16.0, text_colour()),
            "Right" => Style { align: Align::Right, ..body },
            "RightCaption" => Style { align: Align::Right, ..style(Face::Regular, 7.5, 9.5, muted()) },
            "ReportTitle" => Style { align: Align::Right, ..style(Face::Bold, 10.5, 13.0, self.accent) },
            _ => body,
        }
    }

    fn paragraph(&self, text: impl Into<String>, name: &str) -> Flow {
        Flow::Para(Paragraph { spans: vec![Span::plain(text)], style: self.style(name) })
    }

    fn rich(&self, spans: Vec<Span>, name: &str) -> Flow {
        Flow::Para(Paragraph { spans, style: self.style(name) })
    }

    fn add(&mut self, flow: Flow) {
        self.story.push(flow);
    }

    fn text(&mut self, text: impl Into<String>, name: &str) {
        let flow = self.paragraph(text, name);
        self.add(flow);
    }

    fn pill(&self, text: &str, name: &str) -> Span {
        let (foreground, background) = tone(name);
        Span::bold(format!("\u{a0}{text}\u{a0}")).coloured(foreground).on(background)
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

    /// A section heading with the accent bar the app's panels use.
    fn heading(&mut self, text: &str) {
        let bar = Table::new(vec![vec![vec![], vec![self.paragraph(text, "Heading2")]]], vec![Some(3.0), Some(width() - 3.0)]).style(vec![
            Cmd::Background((0, 0), (0, 0), self.accent),
            Cmd::LeftPadding((1, 0), (1, 0), 7.0),
            Cmd::TopPadding(ALL.0, ALL.1, 1.0),
            Cmd::BottomPadding(ALL.0, ALL.1, 1.0),
            Cmd::Valign(ALL.0, ALL.1, VAlign::Middle),
        ]);
        // A heading never ends a page on its own.
        self.add(Flow::CondPageBreak(110.0));
        self.add(Flow::Spacer(10.0));
        self.add(Flow::Table(Box::new(bar)));
        self.add(Flow::Spacer(6.0));
    }

    fn panel(&self, table: Table, background: Option<Colour>, padding: f64) -> Table {
        let mut commands = vec![
            Cmd::Valign(ALL.0, ALL.1, VAlign::Top),
            Cmd::LeftPadding(ALL.0, ALL.1, padding),
            Cmd::RightPadding(ALL.0, ALL.1, padding),
            Cmd::TopPadding(ALL.0, ALL.1, padding - 1.0),
            Cmd::BottomPadding(ALL.0, ALL.1, padding - 1.0),
        ];
        if let Some(colour) = background {
            commands.push(Cmd::Background(ALL.0, ALL.1, colour));
        }
        let mut table = table.style(commands);
        table.outline = Some((0.6, border()));
        table.radius = 4.0;
        table
    }

    /// A photo scaled to fit the box, registered with the document.
    fn photo(&mut self, image: &Value, marked: bool, box_width: f64, box_height: f64) -> R<Option<Flow>> {
        if !self.with_media {
            return Ok(None);
        }
        let Some(picture) = exports::picture(image, marked)? else { return Ok(None) };
        let Some((index, width, height)) = self.doc.image(&picture.data) else { return Ok(None) };
        let ratio = (box_width / width.max(1) as f64).min(box_height / height.max(1) as f64).min(1.0);
        Ok(Some(Flow::Image(Picture { index, width: width as f64 * ratio, height: height as f64 * ratio })))
    }

    /// Photos side by side, the marked copy beside its original; returns how many were placed.
    fn image_grid(&mut self, values: &[Value]) -> R<usize> {
        let columns = 4usize;
        let height = 95.0;
        let cell_width = (width() - (columns as f64 - 1.0) * 6.0) / columns as f64;
        let mut cells: Vec<Vec<Flow>> = Vec::new();
        for value in values {
            for marked in [false, true] {
                if let Some(image) = self.photo(value, marked, cell_width - 2.0, height)? {
                    let label = if marked { "Marked".to_string() } else { value.at("caption").or(value.at("name")).text() };
                    let label = if label.is_empty() { "Photo".to_string() } else { label };
                    cells.push(vec![image, self.paragraph(label, "Caption")]);
                }
            }
        }
        if cells.is_empty() {
            return Ok(0);
        }
        let count = cells.len();
        let mut rows: Vec<Vec<Vec<Flow>>> = Vec::new();
        let mut iter = cells.into_iter();
        loop {
            let row: Vec<Vec<Flow>> = iter.by_ref().take(columns).collect();
            if row.is_empty() {
                break;
            }
            let mut row = row;
            while row.len() < columns {
                row.push(Vec::new());
            }
            rows.push(row);
        }
        let mut widths = vec![Some(cell_width + 6.0); columns - 1];
        widths.push(Some(cell_width));
        let table = Table::new(rows, widths).style(vec![
            Cmd::Valign(ALL.0, ALL.1, VAlign::Top),
            Cmd::LeftPadding(ALL.0, ALL.1, 0.0),
            Cmd::RightPadding(ALL.0, ALL.1, 6.0),
            Cmd::TopPadding(ALL.0, ALL.1, 4.0),
            Cmd::BottomPadding(ALL.0, ALL.1, 2.0),
        ]);
        self.add(Flow::Table(Box::new(table)));
        Ok(count)
    }

    /// A table like the app's lists: soft header, light rules, results coloured.
    #[allow(clippy::too_many_arguments)]
    fn table_flow(&self, rows: &[Vec<Value>], widths: &[f64], header: bool, result_column: Option<usize>, pills: &[((usize, usize), Vec<Span>)], name: &str, total: f64) -> Table {
        let scale = total / widths.iter().sum::<f64>();
        let widths: Vec<Option<f64>> = widths.iter().map(|width| Some(width * scale)).collect();
        let mut commands = Vec::new();
        let mut cells = Vec::new();
        for (index, values) in rows.iter().enumerate() {
            let mut line = Vec::new();
            for (column, cell) in values.iter().enumerate() {
                let head = header && index == 0;
                let style_name = if head { "TableHead" } else { name };
                let text = if cell.is_null() { String::new() } else { cell.py_str() };
                if let Some((_, spans)) = pills.iter().find(|(key, _)| *key == (index, column)) {
                    line.push(vec![self.rich(spans.clone(), style_name)]);
                } else if let Some(result) = result_column.filter(|result| *result == column && !head).and_then(|_| result_tone(&text)) {
                    let (foreground, background) = tone(result);
                    commands.push(Cmd::Background((column as i32, index as i32), (column as i32, index as i32), background));
                    line.push(vec![self.rich(vec![Span::bold(text).coloured(foreground)], style_name)]);
                } else {
                    line.push(vec![self.paragraph(text, style_name)]);
                }
            }
            cells.push(line);
        }
        let padding = if name == "Caption" { 3.0 } else { 5.0 };
        commands.extend([
            Cmd::LineBelow((0, 0), (-1, -2), 0.4, border()),
            Cmd::Valign(ALL.0, ALL.1, VAlign::Top),
            Cmd::LeftPadding(ALL.0, ALL.1, padding),
            Cmd::RightPadding(ALL.0, ALL.1, padding),
            Cmd::TopPadding(ALL.0, ALL.1, 3.0),
            Cmd::BottomPadding(ALL.0, ALL.1, 4.0),
        ]);
        if header {
            commands.insert(0, Cmd::Background((0, 0), (-1, 0), self.soft));
        }
        let mut table = Table::new(cells, widths).style(commands);
        table.repeat_rows = header as usize;
        table.outline = Some((0.6, border()));
        table.radius = 4.0;
        table
    }

    fn table(&mut self, rows: &[Vec<Value>], widths: &[f64], result_column: Option<usize>, pills: &[((usize, usize), Vec<Span>)], name: &str) {
        let table = self.table_flow(rows, widths, true, result_column, pills, name, width());
        self.add(Flow::Table(Box::new(table)));
    }

    // ---- Sections ----

    /// The organization and the report's name, over the accent rule.
    fn letterhead(&mut self, title: &str) -> R<()> {
        let company = self.brand.at("companyName").or(self.brand.at("appTitle")).text();
        let left = vec![self.paragraph(company, "Title"), self.paragraph(self.brand.at("departmentHeader").text(), "Caption")];
        let right = vec![self.paragraph(title, "ReportTitle"), self.paragraph(format!("Printed {}", crate::clock::local_now().minutes_text()), "RightCaption")];
        let logo = self.brand.at("logoUrl").text();
        let image = if logo.starts_with("/api/media/") { self.photo(&crate::json::obj([("url", Value::str(&logo))]), false, 90.0, 34.0)? } else { None };
        let (cells, widths) = match image {
            Some(image) => (vec![vec![image], left, right], vec![Some(100.0), Some(width() - 280.0), Some(180.0)]),
            None => (vec![left, right], vec![Some(width() - 200.0), Some(200.0)]),
        };
        let table = Table::new(vec![cells], widths).style(vec![
            Cmd::Valign(ALL.0, ALL.1, VAlign::Middle),
            Cmd::LineBelow((0, 0), (-1, 0), 1.5, self.accent),
            Cmd::LeftPadding(ALL.0, ALL.1, 0.0),
            Cmd::RightPadding(ALL.0, ALL.1, 0.0),
            Cmd::BottomPadding(ALL.0, ALL.1, 7.0),
        ]);
        self.add(Flow::Table(Box::new(table)));
        self.add(Flow::Spacer(10.0));
        Ok(())
    }

    /// The audit's name, then its details in a soft panel, as the app shows an audit.
    fn audit_details(&mut self, session: &Value, location: Option<&str>) {
        let status = exports::status_of(session);
        let reference = if session.at("audit_ref").truthy() { session.at("audit_ref").text() } else { "Draft".into() };
        let name = if status == "Completed" || status == "Closed" { "pass" } else { "warn" };
        let title = vec![Span::bold(reference), Span::plain("\u{a0}"), self.pill(&status, name)];
        let flow = self.rich(title, "Heading2");
        self.add(flow);
        let location = match location {
            Some(location) => location.to_string(),
            None => if session.at("zone").truthy() { session.at("zone").text() } else { "All Locations".into() },
        };
        let mut fields = vec![
            ("Outlet", session.at("outlet").text()),
            ("Location", location),
            ("Date", format!("{} {}", session.at("audit_date").text(), session.at("audit_time").text()).trim().to_string()),
            ("Auditor", session.at("auditor").text()),
            ("Audit type", if session.at("audit_type").truthy() { session.at("audit_type").text() } else { "Standard".into() }),
        ];
        if session.at("closed_at").truthy() {
            fields.push(("Closed", exports::closed_text(session)));
        }
        let cells: Vec<Vec<Flow>> = fields.iter().map(|(label, value)| vec![self.paragraph(label.to_uppercase(), "Label"), self.paragraph(if value.is_empty() { "—".into() } else { value.clone() }, "BodyText")]).collect();
        let per_row = 3;
        let mut rows = Vec::new();
        let mut iter = cells.into_iter();
        loop {
            let mut row: Vec<Vec<Flow>> = iter.by_ref().take(per_row).collect();
            if row.is_empty() {
                break;
            }
            while row.len() < per_row {
                row.push(Vec::new());
            }
            rows.push(row);
        }
        let table = self.panel(Table::new(rows, vec![Some(width() / 3.0); 3]), Some(surface_alt()), 6.0);
        self.add(Flow::Table(Box::new(table)));
        if session.at("remarks").truthy() {
            self.add(Flow::Spacer(4.0));
            let flow = self.rich(vec![Span::plain("Remarks: ").coloured(muted()), Span::plain(session.at("remarks").text())], "BodyText");
            self.add(flow);
        }
        self.add(Flow::Spacer(8.0));
    }

    /// Four tiles like the dashboard's figures: score, checks, findings, follow-up.
    fn scorecard(&mut self, summary: &Value, findings: &[Value]) {
        let (closed, priority) = exports::finding_counts(findings);
        let colour = tone(if summary.at("meetsPassMark").truthy() { "pass" } else { "fail" }).0;
        let pass_mark = format_g(summary.at("passMark").as_f64().unwrap_or(70.0));
        let tiles: Vec<(&str, Vec<Span>, String)> = vec![
            ("Score", vec![Span::bold(summary.at("score").py_str()).coloured(colour), Span::bold("/100").sized(8.0).coloured(muted())], format!("{} · pass mark {pass_mark}", summary.at("rating").text())),
            ("Checks", vec![Span::bold(summary.at("total").py_str())], format!("{} passed · {} failed · {} N/A", summary.at("passed").py_str(), summary.at("failed").py_str(), summary.at("notApplicable").py_str())),
            ("Findings", vec![Span::bold(findings.len().to_string())], format!("{priority} priority · {} non-priority", findings.len() - priority)),
            ("Follow-up", vec![Span::bold(format!("{} open", findings.len() - closed))], format!("{closed} closed")),
        ];
        // One table, so the tiles share their height; empty columns keep them apart.
        let gap = 6.0;
        let tile = (width() - gap * 3.0) / 4.0;
        let mut rows: Vec<Vec<Vec<Flow>>> = vec![Vec::new(), Vec::new(), Vec::new()];
        for (index, (label, value, note)) in tiles.into_iter().enumerate() {
            if index > 0 {
                for row in rows.iter_mut() {
                    row.push(Vec::new());
                }
            }
            rows[0].push(vec![self.paragraph(label.to_uppercase(), "Label")]);
            rows[1].push(vec![self.rich(value, "Value")]);
            rows[2].push(vec![self.paragraph(note, "Caption")]);
        }
        let widths = vec![Some(tile), Some(gap), Some(tile), Some(gap), Some(tile), Some(gap), Some(tile)];
        let mut commands = vec![
            Cmd::Valign(ALL.0, ALL.1, VAlign::Top),
            Cmd::LeftPadding(ALL.0, ALL.1, 7.0),
            Cmd::RightPadding(ALL.0, ALL.1, 7.0),
            Cmd::TopPadding(ALL.0, ALL.1, 1.0),
            Cmd::BottomPadding(ALL.0, ALL.1, 1.0),
            Cmd::TopPadding((0, 0), (-1, 0), 6.0),
            Cmd::BottomPadding((0, 2), (-1, 2), 6.0),
        ];
        for column in (0..7).step_by(2) {
            commands.push(Cmd::Box((column, 0), (column, 2), 0.6, border()));
        }
        let mut table = Table::new(rows, widths).style(commands);
        table.h_align = Align::Left;
        self.add(Flow::KeepTogether(vec![Flow::Table(Box::new(table))]));
        self.add(Flow::Spacer(4.0));
    }

    fn location_grading(&mut self, session: &Value) {
        let mut groups: Vec<(String, Vec<&Value>)> = Vec::new();
        for item in session.at("items").items() {
            let location = item.at("location").or(session.at("zone")).text();
            let location = if location.is_empty() { "Unassigned".to_string() } else { location };
            match groups.iter_mut().find(|(name, _)| *name == location) {
                Some((_, items)) => items.push(item),
                None => groups.push((location, vec![item])),
            }
        }
        let mut rows = vec![["Location", "Checks", "Passed", "Failed", "N/A", "Score", "Grade"].iter().map(|name| Value::str(*name)).collect::<Vec<_>>()];
        let mut tones = Vec::new();
        for (location, items) in &groups {
            let applicable: Vec<&&Value> = items.iter().filter(|item| !item.at("notApplicable").truthy()).collect();
            let passed = applicable.iter().filter(|item| item.at("passed").truthy()).count();
            let score = if applicable.is_empty() { 0 } else { py_round(100.0 * passed as f64 / applicable.len() as f64) };
            rows.push(vec![
                Value::str(location),
                Value::Int(items.len() as i64),
                Value::Int(passed as i64),
                Value::Int((applicable.len() - passed) as i64),
                Value::Int((items.len() - applicable.len()) as i64),
                Value::str(format!("{score}/100")),
                Value::str(rating_for_score(score as f64, &self.settings)),
            ]);
            tones.push(self.rating_tone(score as f64));
        }
        if rows.len() == 1 {
            let zone = if session.at("zone").truthy() { session.at("zone").text() } else { "Unassigned".into() };
            rows.push(vec![Value::str(zone), Value::Int(0), Value::Int(0), Value::Int(0), Value::Int(0), Value::str("0/100"), Value::str("Critical")]);
            tones.push("fail");
        }
        self.heading("Location grading");
        let pills: Vec<((usize, usize), Vec<Span>)> = tones.iter().enumerate().map(|(index, name)| ((index + 1, 6), vec![self.pill(&rows[index + 1][6].text(), name)])).collect();
        self.table(&rows, &[170.0, 50.0, 50.0, 50.0, 40.0, 60.0, 100.0], None, &pills, "BodyText");
    }

    /// Every check, one asset at a time: the asset's details and own table of checks, then its
    /// photos (those taken in the audit, or its registered photo when none was taken).
    fn checklist(&mut self, items: &[Value], with_photos: bool, assets: &Value) -> R<()> {
        self.heading("Checklist");
        let empty = Value::object();
        for checks in exports::checks_by_asset(items) {
            let first = &checks[0];
            let failed = checks.iter().filter(|check| exports::result_of(check) == "Fail").count();
            let count = format!("{} check{}", checks.len(), if checks.len() != 1 { "s" } else { "" });
            let mut name = first.at("section").text();
            if name.is_empty() {
                name = "Item".into();
            }
            let asset = assets.get(&first.at("equipmentId").text()).unwrap_or(&empty);
            let group = first.at("groupCount").or(&Value::Int(1)).py_int().unwrap_or(1);
            let details = if group > 1 {
                // A casual audit graded these same-named assets together.
                name = format!("{name} ×{group}");
                exports::group_details_line(asset, group)
            } else {
                exports::asset_details_line(asset)
            };
            let status = if failed > 0 { self.pill(&format!("{failed} failed"), "fail") } else { self.pill("All passed", "pass") };
            let mut left = vec![self.rich(vec![Span::bold(name)], "BodyText")];
            if !details.is_empty() {
                left.push(self.paragraph(details, "Caption"));
            }
            let right = vec![self.rich(vec![Span::plain(count).sized(7.5).coloured(muted()), Span::plain("\u{a0}"), status], "Right")];
            let bar = self.panel(Table::new(vec![vec![left, right]], vec![Some(width() - 150.0), Some(150.0)]), Some(surface_alt()), 5.0).style(vec![Cmd::Valign(ALL.0, ALL.1, VAlign::Middle)]);
            let mut rows = vec![vec![Value::str("Check"), Value::str("Result"), Value::str("Remark")]];
            for check in &checks {
                rows.push(vec![Value::str(check.at("item").text()), Value::str(exports::result_of(check)), Value::str(exports::remark_with_assets(check))]);
            }
            let list = self.table_flow(&rows, &[250.0, 50.0, 223.0], true, Some(1), &[], "BodyText", width());
            self.add(Flow::KeepTogether(vec![Flow::Table(Box::new(bar)), Flow::Spacer(3.0), Flow::Table(Box::new(list))]));
            if with_photos && self.image_grid(&exports::check_photos(&checks))? == 0 && self.image_grid(&exports::registered_photos(asset))? == 0 {
                self.text("No photo.", "Caption");
            }
            self.add(Flow::Spacer(10.0));
        }
        Ok(())
    }

    /// The audit at a glance: results of every check, each location's score, and findings by asset type.
    fn charts(&mut self, summary: &Value, parts: &[Part], findings: &[Value]) {
        self.heading("At a glance");
        let mark = summary.at("passMark").as_f64().filter(|mark| *mark != 0.0).unwrap_or(70.0);
        let values = [summary.at("passed").as_i64().unwrap_or(0), summary.at("failed").as_i64().unwrap_or(0), summary.at("notApplicable").as_i64().unwrap_or(0)];
        let grey = hex("#b4bdb8");
        let mut results = Drawing::new(170.0, 118.0);
        let any = values.iter().any(|value| *value != 0);
        let (slices, colours): (Vec<f64>, Vec<Colour>) = if any {
            (values.iter().map(|value| *value as f64).collect(), vec![tone("pass").0, tone("fail").0, grey])
        } else {
            (vec![1.0], vec![neutral_soft()])
        };
        doughnut(&mut results, 6.0 + 46.0, 14.0 + 46.0, 46.0, 0.55, &slices, &colours);
        results.text(52.0, 56.0, summary.at("score").py_str(), Face::Bold, 13.0, text_colour(), Align::Center);
        results.text(52.0, 46.0, "score", Face::Regular, 6.5, muted(), Align::Center);
        for (index, (label, value, colour)) in [("Passed", values[0], tone("pass").0), ("Failed", values[1], tone("fail").0), ("N/A", values[2], grey)].iter().enumerate() {
            let y = 84.0 - index as f64 * 16.0;
            results.rect(108.0, y, 7.0, 7.0, *colour);
            results.text(119.0, y, format!("{label} {value}"), Face::Regular, 7.5, text_colour(), Align::Left);
        }
        results.text(6.0, 4.0, "Checks", Face::Bold, 7.5, muted(), Align::Left);
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
        let side = if categories.is_empty() {
            self.paragraph("No findings in this audit.", "Caption")
        } else {
            let labels: Vec<String> = categories.iter().map(|(name, _)| name.clone()).collect();
            let values: Vec<f64> = categories.iter().map(|(_, count)| *count as f64).collect();
            Flow::Drawing(self.bar_chart(&labels, &values, width() - 180.0, "Findings by asset type", &vec![tone("fail").0; values.len()], None, None))
        };
        let row = self.panel(Table::new(vec![vec![vec![Flow::Drawing(results)], vec![side]]], vec![Some(180.0), Some(width() - 180.0)]), Some(Colour::WHITE), 6.0)
            .style(vec![Cmd::LineBefore((1, 0), (1, 0), 0.4, border())]);
        self.add(Flow::KeepTogether(vec![Flow::Table(Box::new(row))]));
        self.add(Flow::Spacer(6.0));
        let mut scores: Vec<(String, f64)> = parts.iter().map(|part| (part.location.clone(), part.summary.at("score").as_f64().unwrap_or(0.0))).collect();
        if scores.len() > 1 {
            let more = scores.len() > 20;
            scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            scores.truncate(20);
            let title = format!("Score by location{}", if more { " (the 20 lowest)" } else { "" });
            let labels: Vec<String> = scores.iter().map(|(name, _)| name.clone()).collect();
            let values: Vec<f64> = scores.iter().map(|(_, score)| *score).collect();
            let colours: Vec<Colour> = values.iter().map(|score| match self.rating_tone(*score) {
                "pass" => tone("pass").0,
                "warn" => tone("warn").0,
                _ => tone("fail").0,
            }).collect();
            let chart = self.bar_chart(&labels, &values, width() - 12.0, &title, &colours, Some(100.0), Some(mark));
            let panel = self.panel(Table::new(vec![vec![vec![Flow::Drawing(chart)]]], vec![Some(width())]), Some(Colour::WHITE), 6.0);
            self.add(Flow::KeepTogether(vec![Flow::Table(Box::new(panel))]));
        }
    }

    /// Horizontal bars, one per label, coloured by their value, as reportlab's bar chart drew them.
    #[allow(clippy::too_many_arguments)]
    fn bar_chart(&self, labels: &[String], values: &[f64], chart_width: f64, title: &str, colours: &[Colour], maximum: Option<f64>, mark: Option<f64>) -> Drawing {
        let count = labels.len();
        let height = 26.0 + 13.0 * count as f64;
        let mut drawing = Drawing::new(chart_width, height);
        let label_width = (4.2 * labels.iter().map(|label| label.chars().count()).max().unwrap_or(0) as f64).clamp(60.0, 150.0);
        let (x, y) = (label_width + 4.0, 6.0);
        let (plot_width, plot_height) = (chart_width - label_width - 30.0, 13.0 * count as f64);
        let peak = values.iter().cloned().fold(1.0, f64::max);
        let (top, step) = match maximum {
            Some(100.0) => (100.0, 25.0),
            Some(value) => (value, nice_step(value / 4.0)),
            None => {
                let step = nice_step(peak / 4.0);
                (step * (peak / step).ceil(), step)
            }
        };
        axes(&mut drawing, x, y, plot_width, plot_height, top, step, None, true);
        let slot = plot_height / count.max(1) as f64;
        let thickness = slot * 8.0 / 13.0;
        for (index, (label, value)) in labels.iter().zip(values).enumerate() {
            let centre = y + plot_height - slot * (index as f64 + 0.5);
            let shown: String = label.chars().take(32).collect();
            drawing.text(x - 5.0, centre - 2.5, shown, Face::Regular, 7.0, text_colour(), Align::Right);
            let length = plot_width * value / top;
            drawing.rect(x, centre - thickness / 2.0, length.max(0.0), thickness, colours.get(index).copied().unwrap_or(self.accent));
            drawing.text(x + length + 3.0, centre - 2.3, format!("{}", *value as i64), Face::Regular, 6.5, text_colour(), Align::Left);
        }
        if let Some(mark) = mark {
            let mx = x + plot_width * mark / top;
            drawing.line(mx, y - 2.0, mx, y + plot_height + 2.0, muted(), 0.6, Some(vec![2.0, 2.0]));
            drawing.text(mx, y + plot_height + 5.0, format!("pass mark {}", format_g(mark)), Face::Regular, 6.5, muted(), Align::Center);
        }
        drawing.text(0.0, height - 9.0, title, Face::Bold, 7.5, muted(), Align::Left);
        drawing
    }

    /// The audit's assets as charts of their register attributes, each split into those that passed
    /// every check and those with a failure.
    fn asset_attributes(&mut self, items: &[Value], assets: &Value) {
        let (total, failing, attributes) = figures::asset_attributes(items, assets);
        self.heading("Assets inspected");
        self.text(format!("{total} asset{} inspected · {} passed every check · {failing} with a failure", if total != 1 { "s" } else { "" }, total - failing), "Caption");
        if attributes.is_empty() {
            return;
        }
        let half = (width() - 8.0) / 2.0;
        let mut cells = Vec::new();
        for (name, rows) in attributes {
            let labels: Vec<String> = rows.iter().map(|(label, ..)| label.clone()).collect();
            let series = vec![
                ("All passed".to_string(), rows.iter().map(|row| row.1 as f64).collect::<Vec<_>>(), vec![hex(figures::PASS)]),
                ("With failures".to_string(), rows.iter().map(|row| row.2 as f64).collect::<Vec<_>>(), vec![hex(figures::FAIL)]),
            ];
            let chart = hbar(self, &labels, &series, half - 14.0, HBar { stacked: true, legend: true, ..HBar::default() });
            cells.push(vec![
                self.paragraph(name, "Heading3"),
                self.paragraph(format!("{} value{}", rows.len(), if rows.len() != 1 { "s" } else { "" }), "Caption"),
                Flow::Spacer(4.0),
                Flow::Drawing(chart),
            ]);
        }
        self.add(Flow::Spacer(4.0));
        let grid = card_grid(cells, 2);
        self.add(grid);
    }

    /// Each finding as a card: reference and status, where and who, then what was found.
    fn findings(&mut self, findings: &[Value]) {
        self.heading("Findings");
        if findings.is_empty() {
            self.text("No findings.", "Caption");
            return;
        }
        for finding in findings {
            let status = if finding.at("status").truthy() { finding.at("status").text() } else { "Open".into() };
            let reference = if finding.at("finding_ref").truthy() { finding.at("finding_ref").text() } else { "Finding".into() };
            let head = vec![Span::bold(reference), Span::plain("\u{a0}"), self.pill(&status, if status == "Closed" { "pass" } else { "warn" })];
            let mut rows: Vec<Vec<Vec<Flow>>> = vec![vec![vec![self.rich(head, "BodyText")], vec![]], vec![vec![self.paragraph(exports::finding_meta(finding), "Caption")], vec![]]];
            for (key, label) in [("comment", "Finding"), ("cause", "Cause"), ("recommendation", "Recommendation"), ("required_action", "Required action")] {
                if finding.at(key).truthy() {
                    rows.push(vec![vec![self.paragraph(label, "Label")], vec![self.paragraph(finding.at(key).text(), "BodyText")]]);
                }
            }
            if finding.at("closed_at").truthy() {
                rows.push(vec![vec![self.paragraph("Closed", "Label")], vec![self.paragraph(finding.at("closed_at").text(), "BodyText")]]);
            }
            let stripe = if exports::is_priority(finding) { tone("fail").0 } else { tone("warn").0 };
            let card = self.panel(Table::new(rows, vec![Some(85.0), Some(width() - 85.0)]), Some(Colour::WHITE), 6.0).style(vec![
                Cmd::Span((0, 0), (-1, 0)),
                Cmd::Span((0, 1), (-1, 1)),
                Cmd::TopPadding((0, 1), (-1, -1), 1.0),
                Cmd::BottomPadding((0, 0), (-1, -2), 1.0),
                Cmd::LineBefore((0, 0), (0, -1), 2.5, stripe),
            ]);
            self.add(Flow::KeepTogether(vec![Flow::Table(Box::new(card))]));
            self.add(Flow::Spacer(5.0));
        }
    }

    /// The three signatures side by side.
    fn signatures(&mut self, session: &Value) -> R<()> {
        self.heading("Signatures");
        let box_width = (width() - 12.0) / 3.0;
        let mut inner = Vec::new();
        for (key, label) in SIGNATURES {
            let signature = session.at("signatures").at(key);
            let image = if signature.truthy() { self.photo(signature, false, box_width - 16.0, 42.0)? } else { None };
            let signed: String = signature.at("signedAt").text().chars().take(10).collect();
            let name = if signature.at("name").truthy() { signature.at("name").text() } else { "—".into() };
            let parts = vec![
                vec![self.paragraph(label.to_uppercase(), "Label")],
                vec![image.unwrap_or_else(|| self.paragraph("Not signed", "Caption"))],
                vec![self.paragraph(name, "Heading3")],
                vec![self.paragraph(if signed.is_empty() { String::new() } else { format!("Signed {signed}") }, "Caption")],
            ];
            let rows: Vec<Vec<Vec<Flow>>> = parts.into_iter().map(|part| vec![part]).collect();
            let panel = self.panel(Table::new(rows, vec![Some(box_width)]), Some(Colour::WHITE), 7.0).style(vec![Cmd::TopPadding(ALL.0, ALL.1, 2.0), Cmd::BottomPadding(ALL.0, ALL.1, 2.0)]);
            inner.push(vec![Flow::Table(Box::new(panel))]);
        }
        let row = Table::new(vec![inner], vec![Some(box_width + 6.0), Some(box_width + 6.0), Some(box_width)]).style(vec![
            Cmd::LeftPadding(ALL.0, ALL.1, 0.0),
            Cmd::RightPadding(ALL.0, ALL.1, 6.0),
            Cmd::Valign(ALL.0, ALL.1, VAlign::Top),
        ]);
        self.add(Flow::KeepTogether(vec![Flow::Table(Box::new(row))]));
        Ok(())
    }

    fn build(mut self, footer_text: &str, title: &str) -> Vec<u8> {
        let company = if self.brand.has("companyName") { self.brand.at("companyName").text() } else { "Ottotree".into() };
        self.doc.title = title.to_string();
        self.doc.author = company.clone();
        let footer_text = footer_text.to_string();
        let footer = move |doc: &mut Document, canvas: &mut Canvas, page: usize| {
            canvas.line(MARGIN, 30.0, A4.0 - MARGIN, 30.0, border(), 0.6, None);
            doc.text(canvas, MARGIN, 20.0, &footer_text, Face::Regular, 7.0, muted());
            let centre = doc.fonts.width(&company, Face::Regular, 7.0);
            doc.text(canvas, A4.0 / 2.0 - centre / 2.0, 20.0, &company, Face::Regular, 7.0, muted());
            let label = format!("Page {page}");
            let right = doc.fonts.width(&label, Face::Regular, 7.0);
            doc.text(canvas, A4.0 - MARGIN - right, 20.0, &label, Face::Regular, 7.0, muted());
        };
        let story = std::mem::take(&mut self.story);
        let mut layout = Layout::new(&mut self.doc, &footer);
        layout.flow(story);
        layout.finish();
        self.doc.finish()
    }
}

/// A doughnut of slices (reportlab's Pie with an inner radius): clockwise from the top.
fn doughnut(drawing: &mut Drawing, cx: f64, cy: f64, radius: f64, inner_fraction: f64, values: &[f64], colours: &[Colour]) {
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }
    let inner = radius * inner_fraction;
    let mut angle = 90.0f64;
    for (index, value) in values.iter().enumerate() {
        if *value <= 0.0 {
            continue;
        }
        let sweep = 360.0 * value / total;
        let steps = ((sweep / 2.0).ceil() as usize).max(2);
        let mut points = Vec::with_capacity(steps * 2 + 2);
        for step in 0..=steps {
            let a = (angle - sweep * step as f64 / steps as f64).to_radians();
            points.push((cx + radius * a.cos(), cy + radius * a.sin()));
        }
        for step in (0..=steps).rev() {
            let a = (angle - sweep * step as f64 / steps as f64).to_radians();
            points.push((cx + inner * a.cos(), cy + inner * a.sin()));
        }
        let colour = colours.get(index).copied().unwrap_or(neutral_soft());
        let stroke = if values.iter().filter(|value| **value > 0.0).count() > 1 { Some((Colour::WHITE, 1.0)) } else { None };
        drawing.add(Shape::Polygon { points, fill: colour, stroke });
        angle -= sweep;
    }
}

/// A ring of parts with a figure in its middle, as the page draws them.
fn ring_drawing(parts: &[(f64, Colour)], centre: &str, caption: &str) -> Flow {
    let size = 96.0;
    let mut drawing = Drawing::new(size, size);
    let any = parts.iter().any(|(value, _)| *value != 0.0);
    let (values, colours): (Vec<f64>, Vec<Colour>) = if any { parts.iter().cloned().unzip() } else { (vec![1.0], vec![neutral_soft()]) };
    doughnut(&mut drawing, size / 2.0, size / 2.0, size / 2.0 - 2.0, 0.66, &values, &colours);
    drawing.text(size / 2.0, size / 2.0 - 1.0, centre, Face::Bold, 15.0, text_colour(), Align::Center);
    drawing.text(size / 2.0, size / 2.0 - 12.0, caption, Face::Regular, 7.0, muted(), Align::Center);
    let holder = Table::new(vec![vec![vec![Flow::Drawing(drawing)]]], vec![None]).style(vec![
        Cmd::Align(ALL.0, ALL.1, Align::Center),
        Cmd::LeftPadding(ALL.0, ALL.1, 0.0),
        Cmd::RightPadding(ALL.0, ALL.1, 0.0),
    ]);
    Flow::Table(Box::new(holder))
}

pub fn nice_step(rough: f64) -> f64 {
    if rough <= 1.0 {
        return 1.0;
    }
    let power = 10f64.powf(rough.log10().floor());
    [1.0, 2.0, 5.0, 10.0].iter().map(|step| step * power).find(|step| *step >= rough).unwrap_or(10.0 * power)
}

/// A value axis along the bottom with vertical grid lines, and the category axis on the left.
#[allow(clippy::too_many_arguments)]
fn axes(drawing: &mut Drawing, x: f64, y: f64, width: f64, height: f64, top: f64, step: f64, label: Option<&dyn Fn(f64) -> String>, visible: bool) {
    let mut tick = 0.0;
    while tick <= top + 1e-9 {
        let tx = x + width * tick / top;
        drawing.line(tx, y, tx, y + height, neutral_soft(), 0.5, None);
        if visible {
            let text = label.map(|format| format(tick)).unwrap_or_else(|| format_g(tick));
            drawing.text(tx, y - 10.0, text, Face::Regular, 6.5, muted(), Align::Center);
        }
        tick += step;
    }
    if visible {
        drawing.line(x, y, x + width, y, border(), 0.5, None);
    }
    drawing.line(x, y, x, y + height, border(), 0.5, None);
}

#[derive(Default)]
struct HBar<'a> {
    maximum: Option<f64>,
    mark: Option<f64>,
    stacked: bool,
    overlap: bool,
    legend: bool,
    suffix: &'a str,
    label_format: Option<&'a dyn Fn(f64) -> String>,
}

/// Horizontal bars per label: one series (each bar its own colour), several side by side, stacked,
/// or overlapping (the longest behind the average). Series: (name, values, colours: one, or one per bar).
fn hbar(writer: &Writer, labels: &[String], series: &[(String, Vec<f64>, Vec<Colour>)], chart_width: f64, options: HBar) -> Drawing {
    let count = labels.len();
    let per = if options.stacked || options.overlap || series.len() == 1 { 12.0 } else { 7.0 * series.len() as f64 + 4.0 };
    let legend_room = if options.legend { 14.0 } else { 0.0 };
    let height = 18.0 + per * count as f64 + legend_room;
    let mut drawing = Drawing::new(chart_width, height);
    let longest = labels.iter().map(|label| label.chars().count()).max().unwrap_or(0).max(4);
    let label_width = (4.1 * longest as f64).max(50.0).min(chart_width * 0.42);
    let (x, y) = (label_width + 4.0, 12.0 + legend_room);
    let (plot_width, plot_height) = (chart_width - label_width - 34.0, per * count as f64);
    let peak = if options.stacked {
        (0..count).map(|index| series.iter().map(|(_, values, _)| values.get(index).copied().unwrap_or(0.0)).sum::<f64>()).fold(1.0, f64::max)
    } else {
        series.iter().flat_map(|(_, values, _)| values.iter().copied()).fold(1.0, f64::max)
    };
    let (top, step) = match options.maximum {
        Some(100.0) => (100.0, 25.0),
        Some(value) => (value, nice_step(value / 4.0)),
        None => {
            let step = nice_step(peak / 4.0);
            (step * (peak / step).ceil(), step)
        }
    };
    axes(&mut drawing, x, y, plot_width, plot_height, top, step, None, options.label_format.is_none());
    let scale = plot_width / top;
    let characters = (label_width / 3.6) as usize;
    let accent = writer.accent;
    for (index, label) in labels.iter().enumerate() {
        let slot_top = y + plot_height - per * index as f64;
        let centre = slot_top - per / 2.0;
        let shown: String = label.chars().take(characters).collect();
        drawing.text(x - 5.0, centre - 2.5, shown, Face::Regular, 7.0, text_colour(), Align::Right);
        let colour_of = |colours: &Vec<Colour>| colours.get(if colours.len() > 1 { index } else { 0 }).copied().unwrap_or(accent);
        if options.overlap && series.len() >= 2 {
            let (longest_values, average_values) = (&series[0], &series[1]);
            let longest = longest_values.1.get(index).copied().unwrap_or(0.0);
            let average = average_values.1.get(index).copied().unwrap_or(0.0);
            drawing.rect(x, centre - 4.0, longest * scale, 8.0, colour_of(&longest_values.2));
            drawing.rect(x, centre - 4.0, average * scale, 8.0, colour_of(&average_values.2));
            let text = options.label_format.map(|format| format(average)).unwrap_or_else(|| format!("{}{}", format_g(average), options.suffix));
            drawing.text(x + average * scale + 3.0, centre - 2.3, text, Face::Regular, 6.5, text_colour(), Align::Left);
        } else if options.stacked {
            let mut start = 0.0;
            for (_, values, colours) in series {
                let value = values.get(index).copied().unwrap_or(0.0);
                if value > 0.0 {
                    drawing.rect(x + start * scale, centre - 4.0, value * scale, 8.0, colour_of(colours));
                }
                start += value;
            }
            drawing.text(x + start * scale + 3.0, centre - 2.5, format_g(start), Face::Bold, 6.5, text_colour(), Align::Left);
        } else {
            let thickness = if series.len() == 1 { per * 8.0 / 11.0 } else { (per - 4.0) / series.len() as f64 };
            for (number, (_, values, colours)) in series.iter().enumerate() {
                let value = values.get(index).copied().unwrap_or(0.0);
                let bar_top = if series.len() == 1 { centre + thickness / 2.0 } else { slot_top - 2.0 - thickness * number as f64 };
                drawing.rect(x, bar_top - thickness, (value * scale).max(0.0), thickness, colour_of(colours));
                let text = options.label_format.map(|format| format(value)).unwrap_or_else(|| format!("{}{}", format_g(value), options.suffix));
                drawing.text(x + value * scale + 3.0, bar_top - thickness / 2.0 - 2.3, text, Face::Regular, 6.5, text_colour(), Align::Left);
            }
        }
    }
    if let Some(mark) = options.mark {
        let mx = x + plot_width * mark / top;
        drawing.line(mx, y - 2.0, mx, y + plot_height + 2.0, muted(), 0.6, Some(vec![2.0, 2.0]));
        drawing.text(mx, y + plot_height + 4.0, format!("pass mark {}", format_g(mark)), Face::Regular, 6.5, muted(), Align::Center);
    }
    if options.legend {
        let mut lx = x;
        for (name, _, colours) in series {
            drawing.rect(lx, 2.0, 7.0, 7.0, colours.first().copied().unwrap_or(accent));
            drawing.text(lx + 10.0, 2.5, name.as_str(), Face::Regular, 7.0, text_colour(), Align::Left);
            lx += 18.0 + 4.1 * name.chars().count() as f64;
        }
    }
    drawing
}

/// Columns per label (stacked when several), with an optional 0-100 line such as the score.
#[allow(clippy::too_many_arguments)]
fn column_drawing(points: &[(String, Vec<f64>)], colours: &[Colour], names: &[&str], line: Option<&[f64]>, mark: Option<f64>, chart_width: f64, height: f64) -> Drawing {
    let mut drawing = Drawing::new(chart_width, height);
    let (left, right, top, bottom) = (26.0, if line.is_some() { 30.0 } else { 8.0 }, 16.0, 34.0);
    let (plot_width, plot_height) = (chart_width - left - right, height - top - bottom);
    let totals: Vec<f64> = points.iter().map(|(_, values)| values.iter().sum()).collect();
    let maximum = totals.iter().cloned().fold(1.0, f64::max);
    let step = plot_width / points.len().max(1) as f64;
    let bar = 40.0f64.min(step * 0.6);
    let y_of = |value: f64| bottom + value * plot_height / maximum;
    let mut ticks = vec![0.0, ((maximum as i64 + 1) / 2) as f64, maximum];
    ticks.dedup();
    for tick in ticks {
        drawing.line(left, y_of(tick), chart_width - right, y_of(tick), neutral_soft(), 0.6, None);
        drawing.text(left - 4.0, y_of(tick) - 2.5, format_g(tick), Face::Regular, 6.5, muted(), Align::Right);
    }
    for (index, (label, values)) in points.iter().enumerate() {
        let x = left + step * index as f64 + (step - bar) / 2.0;
        let mut base = bottom;
        for (value, colour) in values.iter().zip(colours) {
            if *value != 0.0 {
                drawing.rect(x, base, bar, value * plot_height / maximum, *colour);
                base += value * plot_height / maximum;
            }
        }
        drawing.text(x + bar / 2.0, base + 3.0, format_g(totals[index]), Face::Bold, 7.0, text_colour(), Align::Center);
        drawing.text(x + bar / 2.0, bottom - 10.0, label.as_str(), Face::Regular, 6.5, muted(), Align::Center);
    }
    if let Some(line) = line {
        let line_y = |value: f64| bottom + value * plot_height / 100.0;
        let coords: Vec<(f64, f64)> = line.iter().enumerate().map(|(index, value)| (left + step * index as f64 + step / 2.0, line_y(*value))).collect();
        if let Some(mark) = mark {
            drawing.line(left, line_y(mark), chart_width - right, line_y(mark), muted(), 0.6, Some(vec![3.0, 3.0]));
        }
        for pair in coords.windows(2) {
            drawing.line(pair[0].0, pair[0].1, pair[1].0, pair[1].1, text_colour(), 1.4, None);
        }
        for ((x, y), value) in coords.iter().zip(line) {
            let shade = match mark {
                Some(mark) => if *value >= mark { tone("pass").0 } else if *value >= mark - 15.0 { tone("warn").0 } else { tone("fail").0 },
                None => if *value >= 70.0 { tone("pass").0 } else if *value >= 55.0 { tone("warn").0 } else { tone("fail").0 },
            };
            drawing.add(Shape::Circle { x: *x, y: *y, radius: 3.0, fill: shade, stroke: Some((Colour::WHITE, 1.0)) });
            drawing.text(*x, y + 5.0, format_g(*value), Face::Bold, 6.5, text_colour(), Align::Center);
        }
        for tick in [0.0, 50.0, 100.0] {
            drawing.text(chart_width - right + 4.0, line_y(tick) - 2.5, format_g(tick), Face::Regular, 6.5, muted(), Align::Left);
        }
    }
    // The legend, under the chart.
    let mut x = left;
    let mut entries: Vec<(String, Colour)> = names.iter().zip(colours).map(|(name, colour)| (name.to_string(), *colour)).collect();
    if line.is_some() {
        entries.push(("Average score (0–100)".into(), text_colour()));
        if let Some(mark) = mark {
            entries.push((format!("Pass mark {}", format_g(mark)), muted()));
        }
    }
    for (name, colour) in entries {
        drawing.rect(x, 4.0, 7.0, 7.0, colour);
        drawing.text(x + 10.0, 4.5, name.as_str(), Face::Regular, 7.0, text_colour(), Align::Left);
        x += 16.0 + 4.1 * name.chars().count() as f64;
    }
    drawing
}

/// Cards side by side, as the page's chart grid; each card is a list of flowables.
fn card_grid(cells: Vec<Vec<Flow>>, columns: usize) -> Flow {
    let gap = 8.0;
    let card = (width() - gap * (columns as f64 - 1.0)) / columns as f64;
    let mut rows: Vec<Vec<Vec<Flow>>> = Vec::new();
    let mut commands = vec![
        Cmd::Valign(ALL.0, ALL.1, VAlign::Top),
        Cmd::LeftPadding(ALL.0, ALL.1, 7.0),
        Cmd::RightPadding(ALL.0, ALL.1, 7.0),
        Cmd::TopPadding(ALL.0, ALL.1, 7.0),
        Cmd::BottomPadding(ALL.0, ALL.1, 8.0),
    ];
    let mut iter = cells.into_iter().peekable();
    let mut row_index = 0i32;
    while iter.peek().is_some() {
        let mut row = Vec::new();
        for column in 0..columns {
            if column > 0 {
                row.push(Vec::new());
            }
            match iter.next() {
                Some(cell) => {
                    commands.push(Cmd::Box(((column * 2) as i32, row_index), ((column * 2) as i32, row_index), 0.6, border()));
                    row.push(cell);
                }
                None => row.push(Vec::new()),
            }
        }
        if row_index > 0 {
            commands.push(Cmd::TopPadding((0, row_index), (-1, row_index), 9.0));
        }
        rows.push(row);
        row_index += 1;
    }
    let mut widths = Vec::new();
    for column in 0..columns {
        if column > 0 {
            widths.push(Some(gap));
        }
        widths.push(Some(card));
    }
    Flow::Table(Box::new(Table::new(rows, widths).style(commands)))
}

/// One audit overall: details, score, charts, grading by location, the assets inspected as charts
/// of their attributes, findings, and signatures.
pub fn build_report(session: &Value, brand: &Value, summary: &Value, settings: &Value, parts: &[Part], assets: &Value) -> R<Vec<u8>> {
    let mut writer = Writer::new(brand, settings, true)?;
    writer.letterhead("Facilities Audit Report")?;
    writer.audit_details(session, None);
    let findings: Vec<Value> = session.at("findings").items().to_vec();
    writer.scorecard(summary, &findings);
    writer.charts(summary, parts, &findings);
    // One grading row per location, then the assets by attribute; the location report has every check.
    writer.location_grading(session);
    if !parts.is_empty() {
        writer.asset_attributes(session.at("items").items(), assets);
    }
    writer.findings(&findings);
    writer.add(Flow::CondPageBreak(130.0));
    writer.signatures(session)?;
    let reference = if session.at("audit_ref").truthy() { session.at("audit_ref").text() } else { "Draft audit".into() };
    let title = format!("Audit {}", if session.at("audit_ref").truthy() { session.at("audit_ref").text() } else { session.at("id").py_str() });
    Ok(writer.build(&reference, &title))
}

/// A location's section starts with a solid accent banner, so each stands apart.
fn location_banner(writer: &mut Writer, text: &str) {
    let flow = writer.rich(vec![Span::bold(text).coloured(Colour::WHITE)], "Heading2");
    let mut banner = Table::new(vec![vec![vec![flow]]], vec![Some(width())]).style(vec![
        Cmd::Background(ALL.0, ALL.1, writer.accent),
        Cmd::LeftPadding(ALL.0, ALL.1, 8.0),
        Cmd::TopPadding(ALL.0, ALL.1, 2.0),
        Cmd::BottomPadding(ALL.0, ALL.1, 3.0),
    ]);
    banner.radius = 4.0;
    writer.add(Flow::Table(Box::new(banner)));
    writer.add(Flow::Spacer(6.0));
}

/// One audit, limited to the chosen locations, each in full: its score, every check and remark,
/// photos, and findings.
pub fn build_locations_report(session: &Value, parts: &[Part], brand: &Value, settings: &Value) -> R<Vec<u8>> {
    let mut writer = Writer::new(brand, settings, true)?;
    writer.letterhead("Audit Report — Selected Locations")?;
    writer.audit_details(session, None);
    let mut rows = vec![["Location", "Checks", "Failed", "Score", "Findings"].iter().map(|name| Value::str(*name)).collect::<Vec<_>>()];
    for part in parts {
        let summary = &part.summary;
        rows.push(vec![
            Value::str(&part.location),
            summary.at("total").clone(),
            summary.at("failed").clone(),
            Value::str(format!("{}/100 · {}", summary.at("score").py_str(), summary.at("rating").text())),
            Value::Int(part.findings.len() as i64),
        ]);
    }
    writer.heading("Locations in this report");
    writer.table(&rows, &[200.0, 60.0, 60.0, 130.0, 70.0], None, &[], "BodyText");
    for (index, part) in parts.iter().enumerate() {
        writer.add(if index == 0 { Flow::PageBreak } else { Flow::CondPageBreak(260.0) });
        location_banner(&mut writer, &format!("Location: {}", part.location));
        writer.scorecard(&part.summary, &part.findings);
        writer.checklist(&part.items, true, &part.assets)?;
        writer.findings(&part.findings);
    }
    writer.add(Flow::CondPageBreak(130.0));
    writer.signatures(session)?;
    let reference = if session.at("audit_ref").truthy() { session.at("audit_ref").text() } else { "Draft audit".into() };
    let footer: String = format!("{reference} · {}", parts.iter().map(|part| part.location.as_str()).collect::<Vec<_>>().join(", ")).chars().take(90).collect();
    Ok(writer.build(&footer, &format!("Audit {reference} locations")))
}

/// Every audit of one location: an overview, then each audit's checks, photos, and findings there.
pub fn build_location_report(outlet: &str, location: &str, period: &str, audits: &[LocationAudit], brand: &Value, settings: &Value) -> R<Vec<u8>> {
    let mut writer = Writer::new(brand, settings, true)?;
    writer.letterhead("Location Audit Report")?;
    let flow = writer.rich(vec![Span::bold(format!("{outlet} · {location}"))], "Heading2");
    writer.add(flow);
    writer.text(format!("Period: {period}"), "Caption");
    let mut rows = vec![["Date", "Audit", "Auditor", "Checks", "Failed", "Score", "Findings"].iter().map(|name| Value::str(*name)).collect::<Vec<_>>()];
    for audit in audits {
        let summary = &audit.summary;
        rows.push(vec![
            audit.session.at("audit_date").clone(),
            Value::str(audit.session.at("audit_ref").text()),
            Value::str(audit.session.at("auditor").text()),
            summary.at("total").clone(),
            summary.at("failed").clone(),
            Value::str(format!("{}/100 · {}", summary.at("score").py_str(), summary.at("rating").text())),
            Value::Int(audit.findings.len() as i64),
        ]);
    }
    writer.heading("Audits of this location");
    if rows.len() == 1 {
        writer.text("No completed audit covered this location in this period.", "Caption");
    } else {
        writer.table(&rows, &[62.0, 92.0, 92.0, 45.0, 42.0, 90.0, 57.0], None, &[], "BodyText");
    }
    for audit in audits {
        writer.add(Flow::PageBreak);
        let reference = if audit.session.at("audit_ref").truthy() { audit.session.at("audit_ref").text() } else { "Audit".into() };
        location_banner(&mut writer, &format!("{reference} · {}", audit.session.at("audit_date").text()));
        writer.audit_details(&audit.session, Some(location));
        writer.scorecard(&audit.summary, &audit.findings);
        writer.checklist(&audit.items, true, &audit.assets)?;
        writer.findings(&audit.findings);
        writer.add(Flow::CondPageBreak(130.0));
        writer.signatures(&audit.session)?;
    }
    Ok(writer.build(&format!("{outlet} · {location}"), &format!("Location report {outlet} {location}")))
}

/// The Reports page on paper, in its order and colours: the four rings, audits and findings by
/// month, outlets, breakdowns, time to act, people, then the findings.
pub fn build_summary_report(data: &Value, findings: &[Value], scope: &str, brand: &Value, settings: &Value) -> R<Vec<u8>> {
    let mut writer = Writer::new(brand, settings, false)?;
    let default_scoring = crate::json::obj([("passMark", Value::Int(70)), ("goodBand", Value::Int(70)), ("belowBand", Value::Int(60))]);
    let scoring = if data.at("scoring").truthy() { data.at("scoring").clone() } else { default_scoring };
    let charts = data.at("charts");
    let accent = writer.accent;
    let colour = |code: Option<&str>| code.map(hex).unwrap_or(accent);
    let pass_mark = scoring.at("passMark").as_f64();
    writer.letterhead("Audit Report")?;
    let flow = writer.paragraph(scope, "Heading2");
    writer.add(flow);
    writer.add(Flow::Spacer(4.0));

    // The four rings.
    let ring_width = (width() - 24.0) / 4.0 - 14.0;
    let mut cells = Vec::new();
    for ring in figures::overview(data) {
        let total: i64 = ring.parts.iter().map(|(_, value, _)| value).sum();
        let legend_rows: Vec<Vec<Vec<Flow>>> = ring
            .parts
            .iter()
            .map(|(label, value, code)| {
                let share = if total != 0 { py_round(*value as f64 * 100.0 / total as f64) } else { 0 };
                vec![
                    vec![writer.rich(vec![Span::plain("●").coloured(colour(Some(code))), Span::plain(format!(" {label}"))], "Caption")],
                    vec![writer.paragraph(value.to_string(), "Right")],
                    vec![writer.paragraph(format!("{share}%"), "RightCaption")],
                ]
            })
            .collect();
        let legend = Table::new(legend_rows, vec![Some(ring_width - 52.0), Some(22.0), Some(30.0)]).style(vec![
            Cmd::LeftPadding(ALL.0, ALL.1, 0.0),
            Cmd::RightPadding(ALL.0, ALL.1, 0.0),
            Cmd::TopPadding(ALL.0, ALL.1, 1.0),
            Cmd::BottomPadding(ALL.0, ALL.1, 1.0),
        ]);
        let parts: Vec<(f64, Colour)> = ring.parts.iter().map(|(_, value, code)| (*value as f64, colour(Some(code)))).collect();
        let mut cell = vec![writer.paragraph(ring.title, "Heading3"), writer.paragraph(ring.note.clone(), "Caption"), Flow::Spacer(4.0), ring_drawing(&parts, &ring.centre, ring.caption), Flow::Spacer(4.0), Flow::Table(Box::new(legend))];
        if !ring.footer.is_empty() {
            cell.push(writer.paragraph(ring.footer.clone(), "Caption"));
        }
        cells.push(cell);
    }
    writer.add(card_grid(cells, 4));

    // Trends.
    let monthly = figures::monthly_audits(charts);
    writer.heading("Audits by month");
    writer.text("Completed audits each month, and their average score against the pass mark.", "Caption");
    writer.add(Flow::Spacer(4.0));
    if monthly.is_empty() {
        writer.text("No data yet", "Caption");
    } else {
        let points: Vec<(String, Vec<f64>)> = monthly.iter().map(|(month, count, _)| (month.clone(), vec![*count as f64])).collect();
        let line: Vec<f64> = monthly.iter().map(|(_, _, score)| *score as f64).collect();
        writer.add(Flow::Drawing(column_drawing(&points, &[accent], &["Audits"], Some(&line), pass_mark, width(), 170.0)));
    }
    let found = figures::monthly_findings(charts);
    writer.heading("Findings by month");
    writer.text("Priority and non-priority findings raised each month.", "Caption");
    writer.add(Flow::Spacer(4.0));
    if found.is_empty() {
        writer.text("No data yet", "Caption");
    } else {
        let points: Vec<(String, Vec<f64>)> = found.iter().map(|(month, priority, other)| (month.clone(), vec![*priority as f64, *other as f64])).collect();
        writer.add(Flow::Drawing(column_drawing(&points, &[hex(figures::FAIL), hex(figures::INFO)], &["Priority", "Non-priority"], None, None, width(), 170.0)));
    }

    // Outlets.
    let outlets = figures::outlet_scores(data);
    let bands = figures::performance_bands(charts);
    let total_bands: i64 = bands.iter().map(|(_, count, _)| count).sum();
    let pairs = figures::comparison(charts);
    let half = (width() - 8.0) / 2.0;
    let latest = if outlets.is_empty() {
        writer.paragraph("No completed audits", "Caption")
    } else {
        let labels: Vec<String> = outlets.iter().map(|(name, ..)| name.clone()).collect();
        let values: Vec<f64> = outlets.iter().map(|(_, score, ..)| *score as f64).collect();
        let colours: Vec<Colour> = outlets.iter().map(|(_, score, ..)| hex(figures::score_colour(*score as f64, &scoring))).collect();
        Flow::Drawing(hbar(&writer, &labels, &[("Latest".into(), values, colours)], half - 14.0, HBar { maximum: Some(100.0), mark: pass_mark, ..HBar::default() }))
    };
    let distribution: Vec<Flow> = if total_bands > 0 {
        let parts: Vec<(f64, Colour)> = bands.iter().map(|(_, count, code)| (*count as f64, hex(code))).collect();
        vec![ring_drawing(&parts, &total_bands.to_string(), "audits"), writer.paragraph(bands.iter().map(|(label, count, _)| format!("{label} {count}")).collect::<Vec<_>>().join(" · "), "Caption")]
    } else {
        vec![writer.paragraph("No completed audits", "Caption")]
    };
    let compared = if pairs.is_empty() {
        writer.paragraph("No outlet has two audits yet", "Caption")
    } else {
        let labels: Vec<String> = pairs.iter().map(|(name, ..)| name.clone()).collect();
        let series = vec![
            ("Previous".to_string(), pairs.iter().map(|(_, previous, _)| previous.unwrap_or(0) as f64).collect::<Vec<_>>(), vec![hex(figures::MUTED)]),
            ("Current".to_string(), pairs.iter().map(|(.., current)| current.unwrap_or(0) as f64).collect::<Vec<_>>(), vec![accent]),
        ];
        Flow::Drawing(hbar(&writer, &labels, &series, half - 14.0, HBar { maximum: Some(100.0), legend: true, ..HBar::default() }))
    };
    let rankings = if outlets.is_empty() {
        writer.paragraph("No completed audits", "Caption")
    } else {
        let mut rows = vec![["#", "Outlet", "Latest", "Average", "Audits"].iter().map(|name| Value::str(*name)).collect::<Vec<_>>()];
        let mut pills = Vec::new();
        for (index, (name, score, average, count, _)) in outlets.iter().enumerate() {
            rows.push(vec![Value::Int(index as i64 + 1), Value::str(name), Value::Int(*score), Value::Int(*average), Value::Int(*count)]);
            pills.push(((index + 1, 2), vec![writer.pill(&score.to_string(), writer.rating_tone(*score as f64))]));
        }
        Flow::Table(Box::new(writer.table_flow(&rows, &[20.0, 70.0, 45.0, 45.0, 40.0], true, None, &pills, "Caption", half - 14.0)))
    };
    writer.heading("Outlets");
    let mut distribution_cell = vec![writer.paragraph("Performance Distribution", "Heading3"), writer.paragraph(format!("{total_bands} completed audits"), "Caption"), Flow::Spacer(4.0)];
    distribution_cell.extend(distribution);
    let grid = card_grid(
        vec![
            vec![writer.paragraph("Latest Scores by Outlet", "Heading3"), writer.paragraph("Each outlet's latest audit score, coloured by grade", "Caption"), Flow::Spacer(4.0), latest],
            distribution_cell,
            vec![writer.paragraph("Previous vs Current Audit", "Heading3"), writer.paragraph("Each outlet's last two audit scores", "Caption"), Flow::Spacer(4.0), compared],
            vec![writer.paragraph("Outlet Rankings", "Heading3"), Flow::Spacer(4.0), rankings],
        ],
        2,
    );
    writer.add(grid);
    writer.heading("Critical issues");
    let critical = data.at("criticalIssues").items();
    if critical.is_empty() {
        writer.text("No open high-priority work orders.", "Caption");
    } else {
        let mut rows = vec![["Work order", "Outlet", "Location", "Title", "Status", "Due"].iter().map(|name| Value::str(*name)).collect::<Vec<_>>()];
        for row in critical {
            rows.push(vec![row.at("work_order_ref").or(row.at("id")).clone(), row.at("outlet").clone(), Value::str(row.at("zone").text()), Value::str(row.at("title").text()), row.at("status").clone(), Value::str(row.at("due_date").text())]);
        }
        writer.table(&rows, &[80.0, 55.0, 70.0, 155.0, 60.0, 60.0], None, &[], "Caption");
    }

    // Breakdowns, two to a row.
    writer.heading("Breakdowns");
    let mut cells = Vec::new();
    for breakdown in figures::breakdowns(charts, &scoring) {
        let shown: Vec<&(String, i64, Option<&str>, String)> = breakdown.rows.iter().take(12).collect();
        let chart = if shown.is_empty() {
            writer.paragraph("No data yet", "Caption")
        } else {
            let labels: Vec<String> = shown.iter().map(|(label, ..)| label.clone()).collect();
            let values: Vec<f64> = shown.iter().map(|(_, value, ..)| *value as f64).collect();
            let colours: Vec<Colour> = shown.iter().map(|(_, _, code, _)| colour(*code)).collect();
            let mark = if breakdown.suffix == "/100" { pass_mark } else { None };
            Flow::Drawing(hbar(&writer, &labels, &[(String::new(), values, colours)], half - 14.0, HBar { maximum: breakdown.maximum, mark, suffix: breakdown.suffix, ..HBar::default() }))
        };
        let note = format!("{}{}", breakdown.note, if breakdown.rows.len() > 12 { " (the first 12)" } else { "" });
        cells.push(vec![writer.paragraph(breakdown.title, "Heading3"), writer.paragraph(note, "Caption"), Flow::Spacer(4.0), chart]);
    }
    writer.add(card_grid(cells, 2));

    // Time to act and people.
    writer.heading("Time to act");
    writer.text("Average time each step took in this period; the faint bar behind is the longest.", "Caption");
    let steps: Vec<(String, i64, i64, i64)> = figures::time_to_act(data).into_iter().filter(|row| row.3 != 0).collect();
    writer.add(Flow::Spacer(4.0));
    if !steps.is_empty() {
        let labels: Vec<String> = steps.iter().map(|(label, ..)| label.clone()).collect();
        let series = vec![
            ("Longest".to_string(), steps.iter().map(|row| row.2 as f64).collect::<Vec<_>>(), vec![writer.soft]),
            ("Average".to_string(), steps.iter().map(|row| row.1 as f64).collect::<Vec<_>>(), vec![accent]),
        ];
        let format = |seconds: f64| duration_text(&Value::Float(seconds));
        writer.add(Flow::Drawing(hbar(&writer, &labels, &series, width(), HBar { overlap: true, label_format: Some(&format), ..HBar::default() })));
    }
    let mut rows = vec![["Step", "Average", "Longest", "Times"].iter().map(|name| Value::str(*name)).collect::<Vec<_>>()];
    for (label, average, longest, count) in figures::time_to_act(data) {
        rows.push(vec![Value::str(label), Value::str(duration_text(&Value::Int(average))), Value::str(duration_text(&Value::Int(longest))), Value::Int(count)]);
    }
    writer.table(&rows, &[260.0, 90.0, 90.0, 50.0], None, &[], "Caption");
    writer.heading("People");
    writer.text("Who completed audits, acted on work requests, and closed work orders.", "Caption");
    let workers = figures::people(data);
    let stack_colours = [accent, hex(figures::INFO), hex(figures::PASS)];
    if !workers.is_empty() {
        let labels: Vec<String> = workers.iter().map(|(name, ..)| name.clone()).collect();
        let series: Vec<(String, Vec<f64>, Vec<Colour>)> = figures::PEOPLE_STACKS
            .iter()
            .enumerate()
            .map(|(index, (label, _))| {
                let values = workers.iter().map(|row| [row.1, row.2, row.3][index] as f64).collect();
                (label.to_string(), values, vec![stack_colours[index]])
            })
            .collect();
        writer.add(Flow::Spacer(4.0));
        writer.add(Flow::Drawing(hbar(&writer, &labels, &series, width(), HBar { stacked: true, legend: true, ..HBar::default() })));
    }
    let (header, rows) = figures::people_table(data);
    if rows.is_empty() {
        writer.text("No activity in this period.", "Caption");
    } else {
        writer.add(Flow::Spacer(6.0));
        let mut table = vec![header.iter().map(|name| Value::str(*name)).collect::<Vec<_>>()];
        table.extend(rows);
        writer.table(&table, &[58.0, 40.0, 46.0, 44.0, 34.0, 40.0, 42.0, 44.0, 44.0, 42.0, 40.0, 49.0], None, &[], "Caption");
    }

    // The lists.
    writer.heading("Findings");
    if findings.is_empty() {
        writer.text("No findings in this period.", "Caption");
    } else {
        let mut rows = vec![["Finding", "Audit", "Outlet", "Location", "Item / check", "Priority", "Status"].iter().map(|name| Value::str(*name)).collect::<Vec<_>>()];
        let mut pills = Vec::new();
        for (index, row) in findings.iter().enumerate() {
            let item: Vec<String> = [row.at("item_name").text(), row.at("criterion").text()].into_iter().filter(|value| !value.is_empty()).collect();
            let item = if item.is_empty() { row.at("comment").text() } else { item.join(" · ") };
            let status = row.at("status").text();
            rows.push(vec![Value::str(row.at("finding_ref").text()), Value::str(row.at("audit_ref").text()), Value::str(row.at("outlet").text()), Value::str(row.at("location").text()), Value::str(item), Value::str(row.at("priority").text()), Value::str(&status)]);
            pills.push(((index + 1, 6), vec![writer.pill(&status, if status == "Closed" { "pass" } else { "warn" })]));
        }
        writer.table(&rows, &[58.0, 118.0, 36.0, 50.0, 121.0, 40.0, 50.0], None, &pills, "Caption");
    }
    Ok(writer.build(scope, "Audit report"))
}
