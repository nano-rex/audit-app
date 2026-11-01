//! Audit exports: one audit (overall, or chosen locations in full), every audit of one location,
//! and the Reports page, each as a PDF and an Excel workbook.

use crate::accounts::branding_settings;
use crate::audits::summarize;
use crate::common::unique;
use crate::db::{all_settings, load_value, marks, with_db};
use crate::error::{bad, R};
use crate::inspections::inspection_session;
use crate::json::{obj, Value};
use crate::sqlite::ToSql;

pub fn app_settings() -> R<Value> {
    with_db(|db| all_settings(db, None))
}

fn item_location(session: &Value, item: &Value) -> String {
    for value in [item.at("location"), session.at("zone")] {
        if value.truthy() {
            return value.py_str();
        }
    }
    "Unassigned".into()
}

/// The audit's locations, in the order they were inspected.
pub fn session_locations(session: &Value) -> Vec<String> {
    unique(session.at("items").items().iter().map(|item| item_location(session, item)))
}

/// The register details of each asset checked (code, type, brand, model, serial, dates, status,
/// photos), by its id as text: items carry the id as the page sent it.
pub fn asset_records(items: &[Value]) -> R<Value> {
    let mut ids: Vec<i64> = items.iter().filter(|item| crate::common::value_is_digits(item.at("equipmentId")) && item.at("equipmentId").truthy()).filter_map(|item| item.at("equipmentId").py_str().parse().ok()).collect();
    ids.sort_unstable();
    ids.dedup();
    with_db(|db| {
        let mut records = Value::object();
        for chunk in ids.chunks(500) {
            let params: Vec<&dyn ToSql> = chunk.iter().map(|id| id as &dyn ToSql).collect();
            for row in db.rows(
                &format!("SELECT id, name, asset_id, code, kind, category, type, equipment_type, brand, model, serial_number, installation_date, warranty_date, expiry_date, operational_status, health_status, location, zone, photos_data_id FROM equipment WHERE id IN ({})", marks(chunk.len())),
                &params,
            )? {
                let mut record = row.to_object();
                record.remove("photos_data_id");
                let photos = if row.get("photos_data_id").is_null() { Value::array() } else { load_value(db, row.get("photos_data_id"))? };
                record.set("photos", if photos.truthy() { photos } else { Value::array() });
                record.set("type", if row.get("type").truthy() { row.value("type") } else if row.get("equipment_type").truthy() { row.value("equipment_type") } else { "".into() });
                record.set("status", if row.get("operational_status").truthy() { row.value("operational_status") } else if row.get("health_status").truthy() { row.value("health_status") } else { "".into() });
                records.set(&row.int("id").to_string(), record);
            }
        }
        Ok(records)
    })
}

/// One location of an audit: its checks, findings, and score.
pub struct Part {
    pub location: String,
    pub items: Vec<Value>,
    pub findings: Vec<Value>,
    pub summary: Value,
    pub assets: Value,
}

pub fn location_part(session: &Value, location: &str, settings: &Value) -> R<Part> {
    let items: Vec<Value> = session.at("items").items().iter().filter(|item| item_location(session, item) == location).cloned().collect();
    let findings: Vec<Value> = session
        .at("findings")
        .items()
        .iter()
        .filter(|finding| {
            let place = if finding.at("location").truthy() { finding.at("location") } else { session.at("zone") };
            place.as_str() == Some(location)
        })
        .cloned()
        .collect();
    let summary = summarize(&Value::Array(items.clone()), settings)?;
    let assets = asset_records(&items)?;
    Ok(Part { location: location.to_string(), items, findings, summary, assets })
}

/// A worksheet name Excel accepts (31 characters, none of []:*?/\), unique in the workbook.
pub fn sheet_title(name: &str, taken: &mut Vec<String>) -> String {
    let cleaned: String = name.chars().map(|c| if "[]:*?/\\".contains(c) { '-' } else { c }).collect();
    let mut base: String = cleaned.trim_matches(|c| c == '\'' || c == ' ').chars().take(31).collect();
    if base.is_empty() {
        base = "Location".into();
    }
    let mut title = base.clone();
    let mut number = 2;
    while taken.contains(&title.to_lowercase()) {
        let suffix = format!(" ({number})");
        title = format!("{}{suffix}", base.chars().take(31 - suffix.chars().count()).collect::<String>());
        number += 1;
    }
    taken.push(title.to_lowercase());
    title
}

pub fn summary_of(session: &Value, settings: &Value) -> R<Value> {
    if session.at("scoring").truthy() {
        Ok(session.at("scoring").clone())
    } else {
        summarize(session.at("items"), settings)
    }
}

pub fn inspection_xlsx(session: &Value) -> R<Vec<u8>> {
    let settings = app_settings()?;
    let summary = summary_of(session, &settings)?;
    let parts = session_locations(session).iter().map(|location| location_part(session, location, &settings)).collect::<R<Vec<_>>>()?;
    let assets = asset_records(session.at("items").items())?;
    crate::xlsx_report::inspection_workbook(session, &summary, &parts, &assets, &branding_settings()?, &settings)
}

pub fn inspection_pdf(session: &Value) -> R<Vec<u8>> {
    let settings = app_settings()?;
    let summary = summary_of(session, &settings)?;
    let parts = session_locations(session).iter().map(|location| location_part(session, location, &settings)).collect::<R<Vec<_>>>()?;
    let assets = asset_records(session.at("items").items())?;
    crate::pdf_report::build_report(session, &branding_settings()?, &summary, &settings, &parts, &assets)
}

/// The chosen locations of one audit in full: every check and remark, photos, and findings.
pub fn inspection_locations_pdf(session: &Value, locations: &[String]) -> R<Vec<u8>> {
    let known = session_locations(session);
    let chosen = unique(locations.iter().cloned());
    if chosen.is_empty() || chosen.iter().any(|location| !known.contains(location)) {
        return Err(bad("Choose locations from this audit"));
    }
    let settings = app_settings()?;
    let parts = chosen.iter().map(|location| location_part(session, location, &settings)).collect::<R<Vec<_>>>()?;
    crate::pdf_report::build_locations_report(session, &parts, &branding_settings()?, &settings)
}

/// Every completed audit of an outlet that covered the location, each limited to it.
pub struct LocationAudit {
    pub session: Value,
    pub items: Vec<Value>,
    pub findings: Vec<Value>,
    pub summary: Value,
    pub assets: Value,
}

pub fn location_audits(outlet: &str, location: &str, date_from: &str, date_to: &str) -> R<Vec<LocationAudit>> {
    let mut clauses = vec!["outlet = ?", "status = 'Completed'"];
    let mut values = vec![Value::str(outlet)];
    if !date_from.is_empty() {
        clauses.push("audit_date >= ?");
        values.push(Value::str(date_from));
    }
    if !date_to.is_empty() {
        clauses.push("audit_date <= ?");
        values.push(Value::str(date_to));
    }
    let ids = with_db(|db| {
        let params: Vec<&dyn ToSql> = values.iter().map(|value| value as &dyn ToSql).collect();
        Ok(db.column(&format!("SELECT id FROM inspection_sessions WHERE {} ORDER BY audit_date, id", clauses.join(" AND ")), &params)?.iter().filter_map(|value| value.as_i64()).collect::<Vec<_>>())
    })?;
    let settings = app_settings()?;
    let mut audits = Vec::new();
    for id in ids {
        let Some(session) = inspection_session(id)? else { continue };
        let items: Vec<Value> = session
            .at("items")
            .items()
            .iter()
            .filter(|item| {
                let place = if item.at("location").truthy() { item.at("location") } else { session.at("zone") };
                place.as_str() == Some(location)
            })
            .cloned()
            .collect();
        if items.is_empty() {
            continue;
        }
        let findings: Vec<Value> = session.at("findings").items().iter().filter(|finding| finding.at("location").as_str() == Some(location)).cloned().collect();
        let summary = summarize(&Value::Array(items.clone()), &settings)?;
        let assets = asset_records(&items)?;
        audits.push(LocationAudit { session, items, findings, summary, assets });
    }
    Ok(audits)
}

pub fn period_label(date_from: &str, date_to: &str) -> String {
    match (date_from.is_empty(), date_to.is_empty()) {
        (false, false) => format!("{date_from} to {date_to}"),
        (false, true) => format!("From {date_from}"),
        (true, false) => format!("Up to {date_to}"),
        (true, true) => "All dates".into(),
    }
}

pub fn location_pdf(outlet: &str, location: &str, date_from: &str, date_to: &str) -> R<Vec<u8>> {
    let audits = location_audits(outlet, location, date_from, date_to)?;
    crate::pdf_report::build_location_report(outlet, location, &period_label(date_from, date_to), &audits, &branding_settings()?, &app_settings()?)
}

pub fn location_xlsx(outlet: &str, location: &str, date_from: &str, date_to: &str) -> R<Vec<u8>> {
    let audits = location_audits(outlet, location, date_from, date_to)?;
    crate::xlsx_report::location_workbook(outlet, location, &period_label(date_from, date_to), &audits, &branding_settings()?, &app_settings()?)
}

fn scope_label(filters: &Value) -> String {
    let period: Vec<String> = ["from", "to"].iter().map(|key| filters.at(key).text()).filter(|value| !value.is_empty()).collect();
    let period = if period.is_empty() { "All dates".into() } else { period.join(" to ") };
    let outlet = if filters.at("outlet").truthy() { filters.at("outlet").text() } else { "All outlets".into() };
    format!("{outlet} | {period}")
}

pub fn report_pdf(unit: &str, filters: &Value) -> R<Vec<u8>> {
    let data = crate::reports::report(unit, filters)?;
    let findings = crate::work::finding_items(unit, filters, &Value::Null)?;
    crate::pdf_report::build_summary_report(&data, findings.at("items").items(), &scope_label(filters), &branding_settings()?, &app_settings()?)
}

/// The Reports page as an editable workbook, laid out like the page and its PDF.
pub fn report_xlsx(unit: &str, filters: &Value) -> R<Vec<u8>> {
    let data = crate::reports::report(unit, filters)?;
    let findings = crate::work::finding_items(unit, filters, &Value::Null)?;
    crate::xlsx_report::report_workbook(&data, findings.at("items").items(), &scope_label(filters), &branding_settings()?, &app_settings()?)
}

/// Code · type · brand model · serial · installed · status, whichever the register has. A
/// variable asset has no code label, so its generated code is left out.
pub fn asset_details_line(asset: &Value) -> String {
    let text = |key: &str| asset.at(key).text();
    let mut parts = Vec::new();
    if asset.at("kind").as_str() != Some("fixture") {
        parts.push(text("code"));
    }
    parts.push(text("type"));
    parts.push([text("brand"), text("model")].into_iter().filter(|value| !value.is_empty()).collect::<Vec<_>>().join(" "));
    if asset.at("serial_number").truthy() {
        parts.push(format!("S/N {}", text("serial_number")));
    }
    if asset.at("installation_date").truthy() {
        parts.push(format!("Installed {}", text("installation_date")));
    }
    if asset.at("warranty_date").truthy() {
        parts.push(format!("Warranty {}", text("warranty_date")));
    }
    if asset.at("expiry_date").truthy() {
        parts.push(format!("Expires {}", text("expiry_date")));
    }
    parts.push(text("status"));
    parts.into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// The asset's details without its code and serial, for a casual audit's group.
pub fn group_details_line(asset: &Value, group: i64) -> String {
    let mut reduced = Value::object();
    for (key, value) in asset.entries() {
        if key != "code" && key != "serial_number" {
            reduced.set(key, value.clone());
        }
    }
    format!("{group} assets graded together · {}", asset_details_line(&reduced))
}

/// A check's remark, naming the assets of a casual audit's group that have the issue.
pub fn remark_with_assets(check: &Value) -> String {
    let labels: Vec<String> = check.at("affectedLabels").entries().iter().map(|(_, value)| value.py_str()).collect();
    let notes = check.at("notes").text();
    if labels.is_empty() {
        notes
    } else {
        format!("{notes} (affected: {})", labels.join(", "))
    }
}

pub fn result_of(item: &Value) -> &'static str {
    if item.at("notApplicable").truthy() {
        "N/A"
    } else if item.at("passed").truthy() {
        "Pass"
    } else {
        "Fail"
    }
}

/// Checks grouped by asset, in order.
pub fn checks_by_asset(items: &[Value]) -> Vec<Vec<Value>> {
    let mut grouped: Vec<(String, Vec<Value>)> = Vec::new();
    for item in items {
        let key = crate::figures::group_key(item);
        match grouped.iter_mut().find(|(name, _)| *name == key) {
            Some((_, checks)) => checks.push(item.clone()),
            None => grouped.push((key, vec![item.clone()])),
        }
    }
    grouped.into_iter().map(|(_, checks)| checks).collect()
}

/// The photos taken of an asset's checks, each once.
pub fn check_photos(checks: &[Value]) -> Vec<Value> {
    let mut seen: Vec<Value> = Vec::new();
    let mut photos = Vec::new();
    for check in checks {
        for image in check.at("images").items() {
            if image.is_object() && !seen.iter().any(|url| url.py_eq(image.at("url"))) {
                seen.push(image.at("url").clone());
                photos.push(image.clone());
            }
        }
    }
    photos
}

/// The closing time of an audit as the reports print it.
pub fn closed_text(session: &Value) -> String {
    let at = crate::clock::DateTime::from_unix(session.at("closed_at").as_f64().unwrap_or(0.0) / 1000.0, 0);
    format!("{} {:02}:{:02} UTC by {}", at.date.iso(), at.hour, at.minute, session.at("closed_by").text()).trim().to_string()
}

pub fn status_of(session: &Value) -> String {
    if session.at("closed_at").truthy() {
        "Closed".into()
    } else if session.at("status").truthy() {
        session.at("status").py_str()
    } else {
        "Draft".into()
    }
}

/// Counts of findings: (closed, priority).
pub fn finding_counts(findings: &[Value]) -> (usize, usize) {
    let closed = findings.iter().filter(|row| matches!(row.at("status").as_str(), Some("Completed" | "Verified" | "Closed"))).count();
    let priority = findings
        .iter()
        .filter(|row| {
            let class = row.at("priority_classification");
            let class = if class.truthy() {
                class.py_str()
            } else if matches!(row.at("priority").as_str(), Some("High" | "Priority")) {
                "Priority".into()
            } else {
                "Non-Priority".into()
            };
            class == "Priority"
        })
        .count();
    (closed, priority)
}

/// Whether a finding is priority, for its stripe.
pub fn is_priority(finding: &Value) -> bool {
    finding.at("priority_classification").as_str() == Some("Priority") || matches!(finding.at("priority").as_str(), Some("High" | "Priority"))
}

pub fn finding_meta(finding: &Value) -> String {
    let mut parts: Vec<String> = ["item_name", "location", "category", "priority", "assigned_department"].iter().map(|key| finding.at(key).text()).filter(|value| !value.is_empty()).collect();
    parts.push(format!("PIC {}", if finding.at("pic").truthy() { finding.at("pic").text() } else { "unassigned".into() }));
    parts.push(format!("Due {}", if finding.at("due_date").truthy() { finding.at("due_date").text() } else { "not set".into() }));
    parts.join(" · ")
}

/// The image of a stored photo for an export, as JPEG or PNG bytes with its size.
pub struct Picture {
    pub data: Vec<u8>,
    pub png: bool,
    pub width: u32,
    pub height: u32,
}

/// A photo (or its marked copy) ready to place in an export. WebP images, which earlier
/// releases stored, are decoded and re-encoded as PNG.
pub fn picture(image: &Value, marked: bool) -> R<Option<Picture>> {
    let bytes = with_db(|db| crate::media::image_bytes(db, image, marked))?;
    let Some(bytes) = bytes else { return Ok(None) };
    Ok(picture_from_bytes(bytes))
}

pub fn picture_from_bytes(bytes: Vec<u8>) -> Option<Picture> {
    let header = crate::media::read_header(&bytes)?;
    match header.format {
        crate::media::Format::Jpeg => Some(Picture { data: bytes, png: false, width: header.width, height: header.height }),
        crate::media::Format::Png => Some(Picture { data: bytes, png: true, width: header.width, height: header.height }),
        crate::media::Format::Webp => {
            let image = crate::webp::decode(&bytes).ok()?;
            Some(Picture { data: crate::png::encode(&image), png: true, width: image.width, height: image.height })
        }
    }
}

pub fn images_of(value: &Value) -> Vec<Value> {
    value.items().iter().filter(|image| image.is_object()).cloned().collect()
}

pub fn registered_photos(asset: &Value) -> Vec<Value> {
    images_of(asset.at("photos")).into_iter().take(4).map(|image| image.merged(&obj([("caption", "Registered photo".into())]))).collect()
}
