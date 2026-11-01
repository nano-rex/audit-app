//! Role inheritance and explicit per-user permission overrides.

use crate::clock;
use crate::common::unique;
use crate::config::{tab_ids, SUPER_ROLE};
use crate::db::{load_or, load_value, Db};
use crate::error::{bad, forbidden, R};
use crate::json::{strings, Value};
use crate::params;

pub const INSPECTION_PERMISSIONS: [&str; 3] = ["auditor", "verifier", "acknowledger"];

/// Records whose changes are permitted per person: adding, editing and deleting them ("manage"),
/// and approving changes made by those who may manage but not approve ("approve").
/// Each entry: id, label, the page that shows them, and the API routes that change them.
pub const CHANGE_RECORDS: [(&str, &str, &str, &[&str]); 7] = [
    ("assets", "Assets (fixed assets, fixtures & finishes)", "equipment", &["equipment"]),
    ("users", "Users", "users", &["users"]),
    ("outlets", "Outlets", "outlets", &["setup/outlets"]),
    ("zones", "Zones", "outlets", &["zones"]),
    ("locations", "Locations", "outlets", &["locations"]),
    ("departments", "Departments", "departments", &["setup/departments"]),
    ("roles", "Roles", "roles", &["roles"]),
];

pub fn change_record(id: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static [&'static str])> {
    CHANGE_RECORDS.iter().find(|record| record.0 == id)
}

pub fn action_permissions() -> Vec<String> {
    CHANGE_RECORDS.iter().flat_map(|(record, ..)| ["manage", "approve"].map(|level| format!("{record}.{level}"))).collect()
}

pub fn change_records_value() -> Value {
    Value::Array(CHANGE_RECORDS.iter().map(|(id, label, ..)| crate::json::obj([("id", (*id).into()), ("label", (*label).into())])).collect())
}

/// The listed names, each allowed, without repeats.
pub fn validate_list(value: &Value, allowed: &[&str]) -> R<Value> {
    let Value::Array(items) = value else { return Err(bad("Select valid permissions")) };
    if items.iter().any(|item| !item.as_str().is_some_and(|name| allowed.contains(&name))) {
        return Err(bad("Select valid permissions"));
    }
    Ok(Value::Array(unique(items.iter().cloned())))
}

fn actions_allowed() -> Vec<String> {
    action_permissions()
}

pub fn validate_actions(value: &Value) -> R<Value> {
    let allowed = actions_allowed();
    let names: Vec<&str> = allowed.iter().map(String::as_str).collect();
    validate_list(value, &names)
}

pub fn validate_overrides(value: &Value) -> R<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    if !value.is_object() {
        return Err(bad("Permission overrides must be an object or null"));
    }
    let tabs = tab_ids();
    let mut validated = crate::json::obj([
        ("permissions", validate_list(value.get("permissions").unwrap_or(&Value::array()), &tabs)?),
        ("inspectionPermissions", validate_list(value.get("inspectionPermissions").unwrap_or(&Value::array()), &INSPECTION_PERMISSIONS)?),
    ]);
    // Personal change permissions are optional; without them the role's apply.
    if let Some(actions) = value.get("actions") {
        validated.set("actions", validate_actions(actions)?);
    }
    Ok(validated)
}

/// (pages, inspection capabilities) of a person.
pub fn resolve_permissions(db: &Db, role: &str, overrides: &Value) -> R<(Value, Value)> {
    if role == SUPER_ROLE {
        return Ok((strings(&tab_ids()), strings(&INSPECTION_PERMISSIONS)));
    }
    if !overrides.is_null() {
        let value = validate_overrides(overrides)?;
        return Ok((value.at("permissions").clone(), value.at("inspectionPermissions").clone()));
    }
    let Some(row) = db.row("SELECT permissions_data_id, inspection_permissions_data_id FROM roles WHERE name = ?", params![role])? else {
        return Ok((Value::array(), Value::array()));
    };
    Ok((
        load_or(db, row.get("permissions_data_id"), Value::array())?,
        load_or(db, row.get("inspection_permissions_data_id"), Value::array())?,
    ))
}

/// The change permissions a person has: their own if set, otherwise their role's.
pub fn resolve_actions(db: &Db, role: &str, overrides: &Value) -> R<Value> {
    if role == SUPER_ROLE {
        return Ok(strings(&action_permissions()));
    }
    if overrides.is_object() && overrides.has("actions") {
        return Ok(validate_overrides(overrides)?.at("actions").clone());
    }
    let row = db.row("SELECT action_permissions_data_id FROM roles WHERE name = ?", params![role])?;
    match row {
        Some(row) if row.get("action_permissions_data_id").truthy() => {
            let value = load_value(db, row.get("action_permissions_data_id"))?;
            Ok(if value.truthy() { value } else { Value::array() })
        }
        _ => Ok(Value::array()),
    }
}

pub fn list_has(list: &Value, name: &str) -> bool {
    list.items().iter().any(|item| item.as_str() == Some(name))
}

/// Check and complete an inspection save: returns the merged record and whether its checklist or
/// details changed. `existing` holds the stored session with its signatures already decoded.
pub fn authorize_inspection_update(db: &Db, user: &Value, payload: &Value, existing: Option<&Value>) -> R<(Value, bool)> {
    let mut payload = payload.clone();
    payload.set("auditor", match existing {
        Some(record) => record.at("auditor").clone(),
        None => user.at("name").clone(),
    });
    let capabilities = user.at("inspectionPermissions");
    let can = |name: &str| list_has(capabilities, name);
    let (mut merged, previous_signatures, changed) = match existing {
        None => {
            if !can("auditor") {
                return Err(forbidden("Auditor permission is required to create an inspection"));
            }
            (payload.clone(), Value::object(), true)
        }
        Some(record) => {
            let fields = [("businessUnit", "business_unit"), ("outlet", "outlet"), ("zone", "zone"), ("auditDate", "audit_date"), ("auditor", "auditor"),
                          ("auditTime", "audit_time"), ("auditType", "audit_type"), ("remarks", "remarks")];
            let mut merged = Value::object();
            for (key, column) in fields {
                merged.set(key, record.at(column).clone());
            }
            merged.set("items", load_or(db, &crate::sqlite::Sql::from_value(record.at("items_data_id")), Value::array())?);
            let previous = record.at("signatures_data_id").clone();
            let previous = if previous.truthy() { previous } else { Value::object() };
            merged.set("signatures", previous.clone());
            let changed = merged.entries().iter().any(|(key, value)| key != "signatures" && payload.get(key).is_some_and(|sent| !sent.py_eq(value)));
            let completing = payload.at("complete").truthy() && record.at("status").as_str() != Some("Completed");
            if !can("auditor") && (changed || completing) {
                return Err(forbidden("Auditor permission is required to edit or complete inspection items"));
            }
            let merged = merged.merged(&payload);
            (merged, previous, changed)
        }
    };
    let signatures = merged.at("signatures").clone();
    let mut signatures = if signatures.truthy() { signatures } else { Value::object() };
    if !signatures.is_object() {
        return Err(bad("Signatures must be an object"));
    }
    let required = [("auditedBy", "auditor"), ("verifiedBy", "verifier"), ("acknowledgedBy", "acknowledger")];
    let mut keys: Vec<String> = signatures.entries().iter().map(|(key, _)| key.clone()).collect();
    for (key, _) in previous_signatures.entries() {
        if !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    for key in keys {
        let Some((_, capability)) = required.iter().find(|(name, _)| *name == key) else {
            return Err(bad("Unknown inspection signature"));
        };
        let now = signatures.at(&key).clone();
        let before = previous_signatures.at(&key).clone();
        if !now.py_eq(&before) && !can(capability) {
            let mut label = capability.to_string();
            label[..1].make_ascii_uppercase();
            return Err(forbidden(format!("{label} permission is required for this signature")));
        }
        if now.truthy() && !now.py_eq(&before) {
            if !now.is_object() || !now.at("url").as_str().unwrap_or("").starts_with("/api/media/") {
                return Err(bad("Upload or draw a signature image before signing"));
            }
            let mut signed = now.clone();
            signed.set("name", user.at("name").clone());
            signed.set("signedByUserId", user.at("id").clone());
            signed.set("signedAt", clock::utc_now().iso_aware());
            signatures.set(&key, signed);
        }
    }
    merged.set("signatures", signatures);
    Ok((merged, changed))
}
