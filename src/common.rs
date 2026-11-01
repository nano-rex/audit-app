//! Shared date, reference, password, and workflow helpers.

use crate::clock::{self, Date};
use crate::crypto;
use crate::db::Db;
use crate::error::R;
use crate::json::Value;
use crate::params;

pub fn today_date() -> String {
    clock::today().iso()
}

/// The audit date, with "Today" or nothing meaning today.
pub fn normalize_audit_date(value: &Value) -> String {
    match value {
        Value::Str(text) if !text.is_empty() && text != "Today" => text.clone(),
        other if other.truthy() && !other.is_str() => other.py_str(),
        _ => today_date(),
    }
}

/// datetime.strptime(normalize_audit_date(value), "%Y-%m-%d"), else today.
pub fn parse_date(value: &Value) -> Date {
    Date::parse_ymd(&normalize_audit_date(value)).unwrap_or_else(clock::today)
}

pub fn record_year(value: &Value) -> String {
    normalize_audit_date(value).split('-').next().unwrap_or("").to_string()
}

pub fn audit_ref(record_id: i64, audit_date: &Value) -> String {
    format!("AUD-{}-{record_id:04}", record_year(audit_date))
}

pub fn finding_ref(record_id: i64, audit_date: &Value) -> String {
    format!("F-{}-{record_id:05}", record_year(audit_date))
}

pub fn work_order_ref(record_id: i64) -> String {
    format!("WO-{}-{record_id:05}", record_year(&Value::Null))
}

/// Upper-case the text and replace each run of characters other than A-Z and 0-9 by a hyphen.
fn code_part(text: &str, fallback: &str) -> String {
    let upper = text.to_uppercase();
    let mut out = String::new();
    let mut gap = false;
    for ch in upper.chars() {
        if ch.is_ascii_uppercase() || ch.is_ascii_digit() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(ch);
        } else {
            gap = true;
        }
    }
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

pub fn location_qr_code(outlet: &str, name: &str) -> String {
    let outlet = if outlet.is_empty() { "OUTLET" } else { outlet };
    let name = if name.is_empty() { "LOCATION" } else { name };
    format!("LOC-{}-{}", code_part(outlet, "OUTLET"), code_part(name, "LOCATION"))
}

pub const PASSWORD_ITERATIONS: u32 = 600_000;

pub fn hash_password(password: &str) -> String {
    hash_password_with(password, &crypto::token_hex(16))
}

pub fn hash_password_with(password: &str, salt: &str) -> String {
    let digest = crypto::pbkdf2_sha256(password.as_bytes(), salt.as_bytes(), PASSWORD_ITERATIONS);
    format!("pbkdf2_sha256${PASSWORD_ITERATIONS}${salt}${}", crypto::hex(&digest))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    if stored.is_empty() || !stored.contains('$') {
        return false;
    }
    if stored.starts_with("pbkdf2_sha256$") {
        let parts: Vec<&str> = stored.split('$').collect();
        if parts.len() != 4 {
            return false;
        }
        let Ok(iterations) = parts[1].parse::<u32>() else { return false };
        if !parts[2].is_ascii() || iterations == 0 || iterations > 10_000_000 {
            return false;
        }
        let actual = crypto::hex(&crypto::pbkdf2_sha256(password.as_bytes(), parts[2].as_bytes(), iterations));
        return crypto::constant_time_eq(actual.as_bytes(), parts[3].as_bytes());
    }
    // The original salted SHA-256 form, upgraded at the next sign-in.
    let (salt, digest) = stored.split_once('$').unwrap_or(("", ""));
    let actual = crypto::sha256_hex(format!("{salt}:{password}").as_bytes());
    crypto::constant_time_eq(actual.as_bytes(), digest.as_bytes())
}

/// The local date `days` after the moment `created_at` (milliseconds), as "%Y-%m-%d".
pub fn calculate_due_date(created_at: i64, due_days: i64) -> String {
    let base = if created_at != 0 { created_at } else { clock::now_ms() };
    clock::local_at(base as f64 / 1000.0 + (due_days * 86_400) as f64).date.iso()
}

pub fn priority_due_date(db: &Db, priority: &Value, created_at: i64) -> R<String> {
    let row = db.row("SELECT due_days FROM priority_levels WHERE name = ? AND active = 1", params![priority])?;
    let days = match row {
        Some(row) => row.int("due_days"),
        None => match priority.as_str() {
            Some("High" | "Priority") => 3,
            _ => 14,
        },
    };
    Ok(calculate_due_date(created_at, days))
}

pub fn sla_status(status: &str, due_date: &Value) -> &'static str {
    if matches!(status, "Completed" | "Verified" | "Closed") {
        return "Closed";
    }
    if !due_date.truthy() {
        return "No due date";
    }
    let days = parse_date(due_date).days() - parse_date(&Value::str(today_date())).days();
    if days < 0 {
        "Overdue"
    } else if days <= 3 {
        "Due Soon"
    } else {
        "On Track"
    }
}

pub fn image_label(image: &Value) -> String {
    if image.is_object() {
        let marked = image.at("markedName");
        let name = image.at("name");
        if marked.truthy() {
            return marked.py_str();
        }
        if name.truthy() {
            return name.py_str();
        }
        return "Image".into();
    }
    image.py_str()
}

pub fn image_labels(images: &Value) -> Vec<String> {
    images.items().iter().map(image_label).collect()
}

pub fn rating(score: f64) -> &'static str {
    if score >= 90.0 {
        "Excellent"
    } else if score >= 70.0 {
        "Good"
    } else if score >= 60.0 {
        "Below Expectation"
    } else {
        "Critical"
    }
}

/// The business-unit filter of a report: (where clause, parameters).
pub fn scope(unit: &str, table: &str) -> (String, Vec<Value>) {
    if unit == "Mini Studio" || unit == "Loudspeaker" {
        (format!("{table}.business_unit = ?"), vec![Value::str(unit)])
    } else {
        ("1 = 1".into(), Vec::new())
    }
}

/// The share of checks done: passed, not applicable, or with a remark.
pub fn inspection_progress(items: &Value) -> i64 {
    let items = items.items();
    if items.is_empty() {
        return 0;
    }
    let complete = items
        .iter()
        .filter(|item| item.at("passed").truthy() || item.at("notApplicable").truthy() || !item.at("notes").text().trim().is_empty())
        .count();
    (complete * 100 / items.len()) as i64
}

/// An audit's code: AUDIT-<outlet>-<date>-<number>, e.g. AUDIT-STP-20261006-000004.
pub fn audit_code(outlet: &str, audit_date: &Value, number: i64) -> String {
    let outlet = if outlet.is_empty() { "OUTLET" } else { outlet };
    format!("AUDIT-{outlet}-{}-{number:06}", normalize_audit_date(audit_date).replace('-', ""))
}

/// The number in an audit code (AUDIT-…-YYYYMMDD-NNNNNN or AUD-YYYY-N), or None.
pub fn audit_number(reference: &str) -> Option<i64> {
    if let Some(rest) = reference.strip_prefix("AUDIT-") {
        let mut parts = rest.rsplitn(3, '-');
        let number = parts.next()?;
        let date = parts.next()?;
        let outlet = parts.next()?;
        if !outlet.is_empty() && date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) && number.len() >= 6 && number.bytes().all(|b| b.is_ascii_digit()) {
            return number.parse().ok();
        }
        return None;
    }
    old_audit_number(reference)
}

/// The number of an old-style code AUD-YYYY-N.
pub fn old_audit_number(reference: &str) -> Option<i64> {
    let rest = reference.strip_prefix("AUD-")?;
    let (year, number) = rest.split_once('-')?;
    if year.len() == 4 && year.bytes().all(|b| b.is_ascii_digit()) && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
        number.parse().ok()
    } else {
        None
    }
}

/// The audit's code, kept in step with its outlet and date (a draft's date can change).
pub fn inspection_name(audit_ref: &str, outlet: &str, audit_date: &Value) -> String {
    match audit_number(audit_ref) {
        None if !audit_ref.is_empty() => audit_ref.to_string(),
        None => format!("AUDIT-{}-{}", if outlet.is_empty() { "OUTLET" } else { outlet }, normalize_audit_date(audit_date).replace('-', "")),
        Some(number) => audit_code(outlet, audit_date, number),
    }
}

pub fn normalized_inspection_name(session: &Value) -> String {
    for key in ["audit_ref", "inspection_name"] {
        if session.at(key).truthy() {
            return session.at(key).py_str();
        }
    }
    inspection_name("", &session.at("outlet").text(), session.at("audit_date"))
}

/// Python's list(dict.fromkeys(values)): the values in order, without repeats.
pub fn unique<T: PartialEq + Clone>(values: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for value in values {
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

/// Whether text holds only ASCII digits (Python's str.isdigit for request paths).
pub fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

/// Python's str(value).isdigit() for a JSON value.
pub fn value_is_digits(value: &Value) -> bool {
    is_digits(&value.py_str())
}

/// Python's str.casefold for comparisons of names and e-mail addresses.
pub fn fold(text: &str) -> String {
    text.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_passwords() {
        assert_eq!(location_qr_code("stp", "Room 01 / A"), "LOC-STP-ROOM-01-A");
        assert_eq!(location_qr_code("", "!!"), "LOC-OUTLET-LOCATION");
        assert_eq!(audit_number("AUDIT-STP-20261006-000004"), Some(4));
        assert_eq!(audit_number("AUDIT-A-B-20261006-000012"), Some(12));
        assert_eq!(audit_number("AUD-2026-0004"), Some(4));
        assert_eq!(audit_number("STP_2026-10-06_13"), None);
        assert_eq!(audit_code("STP", &Value::str("2026-10-06"), 4), "AUDIT-STP-20261006-000004");
        let stored = hash_password_with("secret", "abc");
        assert!(verify_password("secret", &stored));
        assert!(!verify_password("Secret", &stored));
        let legacy = format!("salt${}", crypto::sha256_hex(b"salt:pw"));
        assert!(verify_password("pw", &legacy));
    }
}
