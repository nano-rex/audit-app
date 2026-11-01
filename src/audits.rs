//! Audit headers and references, scoring, and work-order transitions.

use crate::clock::Date;
use crate::common::{audit_code, fold, old_audit_number, today_date};
use crate::db::{save_value, Db};
use crate::error::{bad, status, R};
use crate::json::{format_g, obj, Value};
use crate::params;

// ---- References ----

/// The next code of the audit's year: AUDIT-<outlet>-<date>-<number>.
pub fn allocate_reference(db: &Db, audit_date: &Value, outlet: &str) -> R<String> {
    let year: String = audit_date.py_str().chars().take(4).collect();
    let number = match db.scalar("SELECT next_number FROM audit_reference_counters WHERE year = ?", params![year])?.as_i64() {
        Some(number) => number,
        None => {
            let references = db.strings("SELECT audit_ref FROM audits UNION ALL SELECT audit_ref FROM inspection_sessions", params![])?;
            references
                .iter()
                .filter_map(|reference| {
                    if let Some(rest) = reference.strip_prefix(&format!("AUD-{year}-")) {
                        return if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) { rest.parse::<i64>().ok() } else { None };
                    }
                    // AUDIT-<outlet>-<year>MMDD-<number>
                    let rest = reference.strip_prefix("AUDIT-")?;
                    let (head, number) = rest.rsplit_once('-')?;
                    let (outlet, date) = head.rsplit_once('-')?;
                    let good = !outlet.is_empty() && date.len() == 8 && date.starts_with(year.as_str()) && date.bytes().all(|b| b.is_ascii_digit())
                        && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
                    if good { number.parse::<i64>().ok() } else { None }
                })
                .max()
                .unwrap_or(0)
                + 1
        }
    };
    db.exec(
        "INSERT INTO audit_reference_counters(year, next_number) VALUES (?, ?) ON CONFLICT(year) DO UPDATE SET next_number = excluded.next_number",
        params![year, number + 1],
    )?;
    Ok(audit_code(outlet, audit_date, number))
}

fn valid_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' || !bytes.iter().enumerate().all(|(index, b)| index == 2 || b.is_ascii_digit()) {
        return false;
    }
    let hours: u32 = text[..2].parse().unwrap_or(99);
    let minutes: u32 = text[3..].parse().unwrap_or(99);
    hours < 24 && minutes < 60
}

/// Validate supplied headers; unchanged legacy setup values remain readable and editable.
pub fn validate_metadata(db: &Db, payload: &Value, existing: Option<&Value>) -> R<()> {
    if let Some(value) = payload.get("auditDate") {
        if value.as_str().is_none_or(|text| Date::parse_iso(text).is_none()) {
            return Err(bad("Enter a valid audit date"));
        }
    }
    if let Some(value) = payload.get("auditTime") {
        if !value.as_str().is_some_and(valid_time) {
            return Err(bad("Enter a valid audit time"));
        }
    }
    for (key, table, column) in [("outlet", "outlets", "code"), ("auditType", "audit_types", "name")] {
        let Some(value) = payload.get(key) else { continue };
        let old_column = if key == "auditType" { "audit_type" } else { key };
        if let Some(existing) = existing {
            if value.py_eq(existing.at(old_column)) {
                continue;
            }
        }
        let query = format!("SELECT 1 FROM {table} WHERE {column} = ?{}", if key == "auditType" { " AND active = 1" } else { "" });
        if !value.is_str() || !db.exists(&query, params![value])? {
            return Err(bad(format!("Select a configured {}", if key == "auditType" { "audit type" } else { "outlet" })));
        }
    }
    if let Some(remarks) = payload.get("remarks") {
        if remarks.as_str().is_none_or(|text| text.chars().count() > 5000) {
            return Err(bad("Remarks must be text of at most 5000 characters"));
        }
    }
    Ok(())
}

/// Once: old codes (AUD-2026-0004, with a separate name such as STP_2026-10-06_13) become
/// AUDIT-STP-20261006-000004, everywhere they were recorded.
pub fn convert_audit_codes(db: &Db) -> R<()> {
    if db.exists("SELECT 1 FROM app_settings WHERE key = 'system.auditCodesV2'", params![])? {
        return Ok(());
    }
    let mut renamed: Vec<(String, String)> = Vec::new();
    for table in ["inspection_sessions", "audits"] {
        for row in db.rows(&format!("SELECT audit_ref, outlet, audit_date FROM {table} WHERE audit_ref LIKE 'AUD-%'"), params![])? {
            let old = row.str("audit_ref");
            if let Some(number) = old_audit_number(&old) {
                if !renamed.iter().any(|(name, _)| *name == old) {
                    renamed.push((old, audit_code(&row.str("outlet"), &row.value("audit_date"), number)));
                }
            }
        }
    }
    for (old, new) in &renamed {
        db.exec("UPDATE inspection_sessions SET audit_ref = ?, inspection_name = ? WHERE audit_ref = ?", params![new, new, old])?;
        for table in ["audits", "findings", "work_requests"] {
            db.exec(&format!("UPDATE {table} SET audit_ref = ? WHERE audit_ref = ?"), params![new, old])?;
        }
        db.exec("UPDATE activity_log SET record_ref = ? WHERE record_ref = ?", params![new, old])?;
        let like = format!("%{old}%");
        db.exec(
            "UPDATE notifications SET title = REPLACE(title, ?, ?), message = REPLACE(message, ?, ?) WHERE title LIKE ? OR message LIKE ?",
            params![old, new, old, new, like, like],
        )?;
    }
    // Every audit is named by its code now.
    db.exec("UPDATE inspection_sessions SET inspection_name = audit_ref WHERE audit_ref IS NOT NULL AND audit_ref != ''", params![])?;
    db.exec("INSERT INTO app_settings (key, value_data_id) VALUES ('system.auditCodesV2', ?)", params![save_value(db, &Value::Bool(true))?])?;
    Ok(())
}

// ---- Scoring: one calculation shared by saved audits and their reports ----

fn setting_number(settings: &Value, key: &str, fallback: f64) -> f64 {
    match settings.get(key) {
        Some(value) => value.py_float().unwrap_or(fallback),
        None => fallback,
    }
}

pub fn rating_for_score(score: f64, settings: &Value) -> &'static str {
    let excellent = setting_number(settings, "scoring.excellentBand", 90.0);
    let good = setting_number(settings, "scoring.goodBand", 70.0);
    let below = setting_number(settings, "scoring.belowBand", 60.0);
    if score >= excellent {
        "Excellent"
    } else if score >= good {
        "Good"
    } else if score >= below {
        "Below Expectation"
    } else {
        "Critical"
    }
}

pub fn summarize(items: &Value, settings: &Value) -> R<Value> {
    let items = items.items();
    let applicable: Vec<&Value> = items.iter().filter(|item| !item.at("notApplicable").truthy()).collect();
    let weighted_mode = settings.at("scoring.weighting").as_str() == Some("Weighted");
    let weights = if weighted_mode { settings.get("scoring.weights").cloned().unwrap_or_else(Value::object) } else { Value::object() };
    let mut weighted = Vec::new();
    for item in &applicable {
        let category = item.at("category").py_str();
        let weight = match weights.get(&category) {
            Some(value) => value.py_float().map_err(bad)?,
            None => 1.0,
        };
        weighted.push((*item, weight));
    }
    if weighted.iter().any(|(_, weight)| *weight <= 0.0 || *weight > 100.0) {
        return Err(bad("Category weights must be greater than zero and at most 100"));
    }
    let total_weight: f64 = weighted.iter().map(|(_, weight)| weight).sum();
    let passed_weight: f64 = weighted.iter().filter(|(item, _)| item.at("passed").truthy()).map(|(_, weight)| weight).sum();
    let score = if total_weight != 0.0 { crate::activity::py_round(100.0 * passed_weight / total_weight) } else { 0 };
    let rating = rating_for_score(score as f64, settings);
    let passed = applicable.iter().filter(|item| item.at("passed").truthy()).count();
    let pass_mark = setting_number(settings, "scoring.passMark", 70.0);
    Ok(obj([
        ("score", Value::Int(score)),
        ("rating", rating.into()),
        ("total", Value::Int(items.len() as i64)),
        ("passed", Value::Int(passed as i64)),
        ("failed", Value::Int((applicable.len() - passed) as i64)),
        ("notApplicable", Value::Int((items.len() - applicable.len()) as i64)),
        ("passMark", Value::Float(pass_mark)),
        ("meetsPassMark", Value::Bool(!applicable.is_empty() && score as f64 >= pass_mark)),
        ("weighting", settings.get("scoring.weighting").cloned().unwrap_or_else(|| "Equal".into())),
        ("weights", weights),
    ]))
}

pub fn validate_scoring(settings: &Value) -> R<()> {
    const MESSAGE: &str = "Scoring thresholds must be numbers between 0 and 100";
    for key in ["passMark", "excellentBand", "goodBand", "belowBand"] {
        let value = settings.get(&format!("scoring.{key}")).ok_or_else(|| bad(MESSAGE))?.py_float().map_err(|_| bad(MESSAGE))?;
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(bad(MESSAGE));
        }
    }
    let band = |key: &str| settings.at(&format!("scoring.{key}")).py_float().unwrap_or(0.0);
    if !(band("excellentBand") > band("goodBand") && band("goodBand") > band("belowBand")) {
        return Err(bad("Excellent, Good, and Below Expectation thresholds must be in descending order"));
    }
    if !matches!(settings.at("scoring.weighting").as_str(), Some("Equal" | "Weighted")) {
        return Err(bad("Select Equal or Weighted scoring"));
    }
    let weights = settings.get("scoring.weights").cloned().unwrap_or_else(Value::object);
    if !weights.is_object() {
        return Err(bad("Category weights must map categories to numbers"));
    }
    for (_, weight) in weights.entries() {
        let ok = matches!(weight, Value::Int(_) | Value::Float(_) | Value::Bool(_)) && weight.as_f64().is_some_and(|value| value.is_finite() && value > 0.0 && value <= 100.0);
        if !ok {
            return Err(bad("Category weights must be greater than zero and at most 100"));
        }
    }
    Ok(())
}

/// "pass mark 70" as Python's f"{mark:g}" writes it.
pub fn mark_text(value: f64) -> String {
    format_g(value)
}

// ---- Work orders: transitions and validation ----

/// A work order is raised, assigned, worked on, and closed. There is no separate completion or
/// verification step: whoever it is assigned to (or an auditor/administrator) closes it.
pub fn transitions(status: &str) -> Option<&'static [&'static str]> {
    match status {
        "Open" => Some(&["Assigned", "In Progress", "Pending", "Closed"]),
        "Assigned" => Some(&["In Progress", "Pending", "Closed"]),
        "In Progress" => Some(&["Assigned", "Pending", "Closed"]),
        "Pending" => Some(&["Assigned", "In Progress", "Closed"]),
        "Closed" => Some(&[]),
        _ => None,
    }
}

pub const ORDER_FIELDS: [(&str, &str); 15] = [
    ("businessUnit", "business_unit"), ("outlet", "outlet"), ("zone", "zone"),
    ("requestType", "request_type"), ("category", "category"), ("priority", "priority"),
    ("title", "title"), ("description", "description"), ("assignee", "assignee"), ("pic", "pic"),
    ("status", "status"), ("closedAt", "closed_at"), ("dueDate", "due_date"), ("vendor", "vendor"), ("cost", "cost"),
];

/// Whether the work order (or finding) is assigned to this person: by name or e-mail as its
/// PIC, else as its assignee, else through their department.
pub fn assigned_to(user: &Value, record: &Value) -> bool {
    let identities: Vec<String> = ["name", "email"].iter().map(|key| fold(&user.at(key).text())).filter(|value| !value.is_empty()).collect();
    let pic = fold(&record.at("pic").text());
    if !pic.is_empty() {
        return identities.contains(&pic);
    }
    if identities.contains(&fold(&record.at("assignee").text())) {
        return true;
    }
    let department = fold(&user.at("department").text());
    !department.is_empty() && department == fold(&record.at("request_type").text())
}

/// The order's fields merged with the change; checks the status change and who may make it.
pub fn validate_update(payload: &Value, existing: Option<&Value>, user: &Value) -> R<Value> {
    let mut merged = Value::object();
    if let Some(existing) = existing {
        for (key, column) in ORDER_FIELDS {
            merged.set(key, existing.at(column).clone());
        }
    }
    for (key, value) in payload.entries() {
        if ORDER_FIELDS.iter().any(|(name, _)| name == key) || ["cause", "recommendation", "requiredAction", "images"].contains(&key.as_str()) {
            merged.set(key, value.clone());
        }
    }
    let status_value = merged.get("status").cloned().unwrap_or_else(|| "Assigned".into());
    let status_text = status_value.as_str().unwrap_or("");
    if status_value.as_str().and_then(transitions).is_none() {
        return Err(status(400, "Unknown work-order status"));
    }
    match existing {
        None => {
            if !matches!(status_text, "Open" | "Assigned") {
                return Err(status(409, "New work orders must start Open or Assigned"));
            }
        }
        Some(existing) => {
            let previous = existing.at("status").py_str();
            if previous == "Closed" {
                return Err(status(409, "Closed work orders are read-only"));
            }
            if status_text != previous && !transitions(&previous).unwrap_or(&[]).contains(&status_text) {
                return Err(status(409, format!("Cannot change {previous} directly to {status_text}")));
            }
            if user.at("role").as_str() == Some("Department/PIC") {
                if !assigned_to(user, existing) {
                    return Err(status(403, "This work order is assigned to another person or department"));
                }
                for key in ["outlet", "zone", "requestType", "category", "priority", "assignee", "pic", "dueDate"] {
                    let column = ORDER_FIELDS.iter().find(|(name, _)| *name == key).map(|(_, column)| *column).unwrap_or(key);
                    if merged.at(key).text() != existing.at(column).text() {
                        return Err(status(403, "Only an auditor or administrator can change the assignment"));
                    }
                }
            }
        }
    }
    // The closing date is recorded by the server, not chosen by the caller.
    merged.set("closedAt", if status_text == "Closed" { today_date() } else { String::new() });
    Ok(merged)
}
