//! Inspectable items: fixed assets, and variable assets (stored as kind "fixture"): the building and
//! everything else without a code label of its own. Both have the same attributes; a variable
//! asset's code is generated (VAR-00012) and not shown, since it carries no QR label.

use crate::accounts::{self, DENIED};
use crate::api::{ok, ok_true, send_error, Ctx, Reply};
use crate::clock;
use crate::config::{DEFAULT_FIXTURE_CRITERIA, DEFAULT_INSPECTION_CRITERIA, ITEM_KINDS};
use crate::crypto::token_hex;
use crate::db::{data_value, first_outlet, insert_record, load_value, marks, save_value, update_record, with_db, Db};
use crate::error::{bad, forbidden, R};
use crate::json::{obj, strings, Value};
use crate::params;
use crate::sqlite::{Row, ToSql};

const EDITABLE: [(&str, &str); 21] = [
    ("name", "name"), ("category", "category"), ("businessUnit", "business_unit"), ("outlet", "outlet"), ("location", "location"),
    ("type", "type"), ("operationalStatus", "operational_status"), ("installationDate", "installation_date"),
    ("replacementFlag", "replacement_flag"), ("description", "description"), ("model", "model"), ("serialNumber", "serial_number"),
    ("brand", "brand"), ("temporaryRelocation", "temporary_relocation"), ("warrantyDate", "warranty_date"),
    ("calibrationDate", "calibration_date"), ("expiryDate", "expiry_date"), ("inverterModel", "inverter_model"),
    ("motorCapacity", "motor_capacity"), ("sourceFile", "source_file"), ("sourceSheet", "source_sheet"),
];

/// The first truthy value of these keys.
fn first<'a>(payload: &'a Value, keys: &[&str]) -> &'a Value {
    keys.iter().map(|key| payload.at(key)).find(|value| value.truthy()).unwrap_or(&crate::json::NULL)
}

/// Columns shared by create and edit, the same for fixed and variable assets.
fn item_values(db: &Db, payload: &Value, existing: Option<&Row>) -> R<Vec<(String, Value)>> {
    let kind = match first(payload, &["kind"]) {
        value if value.truthy() => value.clone(),
        _ => existing.map(|row| row.value("kind")).unwrap_or_else(|| "asset".into()),
    };
    if !kind.as_str().is_some_and(|kind| ITEM_KINDS.contains(&kind)) {
        return Err(bad("Choose Fixed Asset or Fixture & Finish"));
    }
    let fixture = kind.as_str() == Some("fixture");
    let name = first(payload, &["name", "assetId", "code"]).text().trim().to_string();
    if name.is_empty() {
        return Err(bad("Enter a name"));
    }
    // Categories were retired: an item is described by its asset type.
    let item_type = match first(payload, &["type", "equipmentType"]) {
        value if value.truthy() => value.text().trim().to_string(),
        _ => if fixture { "Others".into() } else { "Fixed Asset".into() },
    };
    let status = match first(payload, &["operationalStatus", "healthStatus"]) {
        value if value.truthy() => value.clone(),
        _ => "Operational".into(),
    };
    let location = first(payload, &["location", "zone"]).clone();
    let location = if location.truthy() { location } else { Value::str("") };
    let installed = first(payload, &["installationDate", "lastChecked"]).clone();
    let installed = if installed.truthy() { installed } else { Value::str("") };
    let text = |key: &str| Value::str(payload.at(key).text());
    let criteria = match payload.at("inspectionCriteria") {
        value if value.truthy() => value.clone(),
        _ => strings(if fixture { &DEFAULT_FIXTURE_CRITERIA } else { &DEFAULT_INSPECTION_CRITERIA }),
    };
    let outlet = if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(first_outlet(db)?) };
    let notes = match payload.at("description") {
        value if value.truthy() => value.clone(),
        _ => payload.get("notes").cloned().unwrap_or_else(|| "".into()),
    };
    let values: Vec<(&str, Value)> = vec![
        ("kind", kind),
        ("category", "".into()),
        ("name", Value::str(name)),
        ("business_unit", payload.get("businessUnit").cloned().unwrap_or_else(|| "Ottotree".into())),
        ("outlet", outlet),
        ("zone", if location.truthy() { location.clone() } else { "Unassigned".into() }),
        ("location", location),
        ("equipment_type", Value::str(&item_type)),
        ("type", Value::str(&item_type)),
        ("health_status", status.clone()),
        ("operational_status", status),
        ("last_checked", if installed.truthy() { installed.clone() } else { "Today".into() }),
        ("installation_date", installed),
        ("replacement_flag", Value::Int(payload.at("replacementFlag").truthy() as i64)),
        ("notes", notes),
        ("description", payload.get("description").cloned().unwrap_or_else(|| "".into())),
        ("model", text("model")),
        ("serial_number", text("serialNumber")),
        ("brand", text("brand")),
        ("temporary_relocation", text("temporaryRelocation")),
        ("warranty_date", text("warrantyDate")),
        ("calibration_date", text("calibrationDate")),
        ("expiry_date", text("expiryDate")),
        ("inverter_model", text("inverterModel")),
        ("motor_capacity", text("motorCapacity")),
        ("source_file", payload.get("sourceFile").cloned().unwrap_or_else(|| "".into())),
        ("source_sheet", payload.get("sourceSheet").cloned().unwrap_or_else(|| "".into())),
        ("photos_data_id", Value::Int(data_value(db, payload.at("photos"), Value::array())?)),
        ("inspection_criteria_data_id", Value::Int(save_value(db, &criteria)?)),
    ];
    Ok(values.into_iter().map(|(column, value)| (column.to_string(), value)).collect())
}

fn value_of<'a>(values: &'a [(String, Value)], column: &str) -> &'a Value {
    values.iter().find(|(name, _)| name == column).map(|(_, value)| value).unwrap_or(&crate::json::NULL)
}

fn set(values: &mut Vec<(String, Value)>, column: &str, value: Value) {
    match values.iter_mut().find(|(name, _)| name == column) {
        Some(slot) => slot.1 = value,
        None => values.push((column.to_string(), value)),
    }
}

/// The code a user typed, or a generated one: every item needs a unique code for its QR label.
fn item_code(payload: &Value, kind: &Value, record_id: i64) -> String {
    let code = first(payload, &["code", "assetId"]).text().trim().to_string();
    if !code.is_empty() {
        return code;
    }
    format!("{}-{record_id:05}", if kind.as_str() == Some("fixture") { "VAR" } else { "AST" })
}

/// The first serial number among these that another item already has, if any.
fn serial_clash(db: &Db, serials: &[String], skip_ids: &[i64]) -> R<Option<String>> {
    let serials: Vec<&String> = serials.iter().filter(|serial| !serial.is_empty()).collect();
    for serial in &serials {
        if serials.iter().filter(|other| other == &serial).count() > 1 {
            return Ok(Some((*serial).clone()));
        }
    }
    let skip: Vec<i64> = if skip_ids.is_empty() { vec![0] } else { skip_ids.to_vec() };
    for serial in serials {
        let mut params: Vec<&dyn ToSql> = vec![serial];
        params.extend(skip.iter().map(|id| id as &dyn ToSql));
        if db.exists(&format!("SELECT 1 FROM equipment WHERE serial_number = ? AND id NOT IN ({})", marks(skip.len())), &params)? {
            return Ok(Some(serial.clone()));
        }
    }
    Ok(None)
}

fn insert_item(db: &Db, values: &[(String, Value)]) -> R<i64> {
    let placeholder = format!("pending-{}", token_hex(8));
    let mut record: Vec<(&str, Value)> = values.iter().map(|(column, value)| (column.as_str(), value.clone())).collect();
    record.extend([("asset_id", Value::str(&placeholder)), ("qr_code", Value::str(&placeholder)), ("code", Value::str(&placeholder)), ("created_at", Value::Int(clock::now_ms()))]);
    insert_record(db, "equipment", &record)
}

pub fn post_equipment(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    with_db(|db| {
        db.begin_immediate()?;
        let values = item_values(db, payload, None)?;
        let serial = value_of(&values, "serial_number").text();
        if serial_clash(db, &[serial.clone()], &[])?.is_some() {
            return Err(bad(format!("Serial number {serial} is already used by another item")));
        }
        let record_id = insert_item(db, &values)?;
        let code = item_code(payload, value_of(&values, "kind"), record_id);
        // A code that is already in use is refused; it used to replace the other item silently.
        db.exec("UPDATE equipment SET asset_id = ?, qr_code = ?, code = ? WHERE id = ?", params![code, code, code, record_id])?;
        ok(obj([("ok", true.into()), ("id", Value::Int(record_id)), ("code", Value::str(code))]))
    })
}

/// The stored fields of an item as a payload, so fields that are not sent keep their values.
fn stored_payload(db: &Db, existing: &Row) -> R<Value> {
    let mut stored = Value::object();
    for (key, column) in EDITABLE {
        stored.set(key, existing.value(column));
    }
    stored.set("photos", if existing.get("photos_data_id").is_null() { Value::array() } else { load_value(db, existing.get("photos_data_id"))? });
    stored.set("inspectionCriteria", if existing.get("inspection_criteria_data_id").is_null() { Value::Null } else { load_value(db, existing.get("inspection_criteria_data_id"))? });
    Ok(stored)
}

pub fn patch_equipment(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let item_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        let Some(existing) = db.row("SELECT * FROM equipment WHERE id = ?", params![item_id])? else { return send_error(404) };
        let merged = stored_payload(db, &existing)?.merged(payload);
        let mut values = item_values(db, &merged, Some(&existing))?;
        // Serial numbers repeated in imported records are left alone until one of them is changed.
        let serial = value_of(&values, "serial_number").text();
        if serial != existing.str("serial_number") && serial_clash(db, &[serial.clone()], &[item_id])?.is_some() {
            return Err(bad(format!("Serial number {serial} is already used by another item")));
        }
        let typed = first(&merged, &["code", "assetId"]).text().trim().to_string();
        let code = if typed.is_empty() { existing.value("code") } else { Value::str(typed) };
        set(&mut values, "asset_id", code.clone());
        set(&mut values, "qr_code", code.clone());
        set(&mut values, "code", code);
        update_record(db, "equipment", &values, item_id)?;
        ok_true()
    })
}

pub fn delete_equipment(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        if db.exec("DELETE FROM equipment WHERE id = ?", params![record_id])? == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

// ---- Several items at once ----

/// Set on each item by itself in a bulk edit: every item needs its own code (its QR label), and a
/// serial number belongs to one item.
const OWN_FIELDS: [&str; 4] = ["code", "assetId", "qrCode", "serialNumber"];
/// Where an item is stays with each item in a bulk edit.
const PLACE_FIELDS: [&str; 4] = ["outlet", "location", "zone", "kind"];
const BULK_LIMIT: usize = 2000;

/// Each (outlet, location) a bulk add makes an item at. "all" outlets are the ones the person
/// covers; "all" locations are every location of each outlet; a named location is used at the
/// outlets that have it; no location makes one item per outlet.
fn bulk_targets(db: &Db, user: &Value, outlets: &Value, locations: &Value) -> R<Vec<(String, String)>> {
    let scope = accounts::allowed(user);
    let known = db.strings("SELECT code FROM outlets ORDER BY code", params![])?;
    let codes: Vec<String> = if outlets.as_str() == Some("all") {
        known.iter().filter(|code| accounts::in_scope(&scope, &Value::str(code.as_str()))).cloned().collect()
    } else if let Value::Array(items) = outlets.clone() {
        if items.is_empty() || !items.iter().all(Value::is_str) {
            return Err(bad("Choose an outlet, or all outlets"));
        }
        let codes: Vec<String> = crate::common::unique(items.iter().map(Value::py_str));
        if codes.iter().any(|code| !known.contains(code)) {
            return Err(bad("Select existing outlets"));
        }
        if codes.iter().any(|code| !accounts::in_scope(&scope, &Value::str(code.as_str()))) {
            return Err(forbidden(DENIED));
        }
        codes
    } else {
        return Err(bad("Choose an outlet, or all outlets"));
    };
    let all = locations.as_str() == Some("all");
    if !all && !locations.is_null() && !(locations.is_array() && locations.items().iter().all(Value::is_str)) {
        return Err(bad("Choose a location, or all locations"));
    }
    let chosen: Vec<String> = crate::common::unique(locations.items().iter().map(Value::py_str));
    let mut targets = Vec::new();
    for code in codes {
        let names = db.strings("SELECT name FROM locations WHERE outlet_code = ? ORDER BY name", params![code])?;
        if all {
            targets.extend(names.iter().map(|name| (code.clone(), name.clone())));
        } else if locations.truthy() {
            targets.extend(chosen.iter().filter(|name| names.contains(name)).map(|name| (code.clone(), name.clone())));
        } else {
            targets.push((code.clone(), String::new()));
        }
    }
    if targets.is_empty() {
        return Err(bad("None of the chosen outlets has that location"));
    }
    if targets.len() > BULK_LIMIT {
        return Err(bad(format!("That would add {} items; add at most {BULK_LIMIT} at a time", targets.len())));
    }
    Ok(targets)
}

/// The same item at every chosen outlet and location, each with its own generated code.
pub fn post_equipment_bulk(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let mut shared = Value::object();
    for (key, value) in payload.entries() {
        if !OWN_FIELDS.contains(&key.as_str()) && key != "outlets" && key != "locations" {
            shared.set(key, value.clone());
        }
    }
    let user = ctx.user()?;
    with_db(|db| {
        db.begin_immediate()?;
        let mut created = Vec::new();
        for (outlet, location) in bulk_targets(db, &user, payload.at("outlets"), payload.at("locations"))? {
            let mut item = shared.clone();
            item.set("outlet", outlet.as_str());
            item.set("location", location.as_str());
            let values = item_values(db, &item, None)?;
            let record_id = insert_item(db, &values)?;
            let code = item_code(&Value::object(), value_of(&values, "kind"), record_id);
            db.exec("UPDATE equipment SET asset_id = ?, qr_code = ?, code = ? WHERE id = ?", params![code, code, code, record_id])?;
            created.push(obj([("id", Value::Int(record_id)), ("code", Value::str(code)), ("outlet", Value::str(outlet)), ("location", Value::str(location))]));
        }
        ok(obj([("ok", true.into()), ("count", Value::Int(created.len() as i64)), ("items", Value::Array(created))]))
    })
}

/// One change to several items: the shared fields sent apply to all of them, while each keeps
/// its own outlet and location, and its own code and serial number (sent per item).
pub fn patch_equipment_bulk(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let ids = payload.at("ids");
    let Value::Array(ids) = ids else { return Err(bad("Choose the items to edit")) };
    if ids.is_empty() || !ids.iter().all(crate::common::value_is_digits) {
        return Err(bad("Choose the items to edit"));
    }
    let ids: Vec<i64> = crate::common::unique(ids.iter().filter_map(|value| value.py_str().parse::<i64>().ok()));
    let mut fields = Value::object();
    for (key, value) in payload.at("fields").entries() {
        if !OWN_FIELDS.contains(&key.as_str()) && !PLACE_FIELDS.contains(&key.as_str()) {
            fields.set(key, value.clone());
        }
    }
    let mut own: Vec<(i64, Value)> = Vec::new();
    for item in payload.at("items").items() {
        if item.is_object() && crate::common::value_is_digits(item.get("id").unwrap_or(&Value::str(""))) {
            let mut values = Value::object();
            for key in ["code", "serialNumber"] {
                if let Some(value) = item.get(key).filter(|value| !value.is_null()) {
                    values.set(key, value.py_str().trim());
                }
            }
            let id = item.at("id").py_str().parse().unwrap_or(0);
            own.retain(|(known, _)| *known != id);
            own.push((id, values));
        }
    }
    let own_of = |id: i64| own.iter().find(|(known, _)| *known == id).map(|(_, values)| values.clone()).unwrap_or_else(Value::object);
    with_db(|db| {
        db.begin_immediate()?;
        let params: Vec<&dyn ToSql> = ids.iter().map(|id| id as &dyn ToSql).collect();
        let rows = db.rows(&format!("SELECT * FROM equipment WHERE id IN ({})", marks(ids.len())), &params)?;
        if rows.len() != ids.len() {
            return Err(bad("Some of these items no longer exist"));
        }
        let row_of = |id: i64| rows.iter().find(|row| row.int("id") == id).expect("every id was found");
        let mut updates: Vec<(i64, Vec<(String, Value)>)> = Vec::new();
        for id in &ids {
            let existing = row_of(*id);
            let item = stored_payload(db, existing)?.merged(&fields).merged(&own_of(*id));
            let mut values = item_values(db, &item, Some(existing))?;
            let code = own_of(*id).at("code").clone();
            set(&mut values, "code", if code.truthy() { code } else { existing.value("code") });
            updates.push((*id, values));
        }
        let codes: Vec<String> = updates.iter().map(|(_, values)| value_of(values, "code").py_str()).collect();
        if let Some(repeated) = codes.iter().find(|code| codes.iter().filter(|other| other == code).count() > 1) {
            return Err(bad(format!("Code {repeated} is given to more than one item; each item needs its own code")));
        }
        let mut params: Vec<&dyn ToSql> = codes.iter().map(|code| code as &dyn ToSql).collect();
        params.extend(ids.iter().map(|id| id as &dyn ToSql));
        if let Some(taken) = db.row(&format!("SELECT code FROM equipment WHERE code IN ({}) AND id NOT IN ({})", marks(codes.len()), marks(ids.len())), &params)? {
            return Err(bad(format!("Code {} is already used by another item", taken.str("code"))));
        }
        let serial_of = |values: &[(String, Value)]| value_of(values, "serial_number").text();
        let changed: Vec<String> = updates.iter().filter(|(id, values)| serial_of(values) != row_of(*id).str("serial_number")).map(|(_, values)| serial_of(values)).collect();
        let kept: Vec<String> = updates.iter().filter(|(id, values)| serial_of(values) == row_of(*id).str("serial_number")).map(|(_, values)| serial_of(values)).collect();
        let clash = match serial_clash(db, &changed, &ids)? {
            Some(serial) => Some(serial),
            None => changed.iter().find(|serial| !serial.is_empty() && kept.contains(serial)).cloned(),
        };
        if let Some(serial) = clash {
            return Err(bad(format!("Serial number {serial} is already used by another item")));
        }
        // Codes may be swapped between the items, so each first gives its code up.
        for id in &ids {
            let placeholder = format!("pending-{}", token_hex(8));
            db.exec("UPDATE equipment SET code = ?, asset_id = ?, qr_code = ? WHERE id = ?", params![placeholder, placeholder, placeholder, id])?;
        }
        for (id, mut values) in updates {
            let code = value_of(&values, "code").clone();
            set(&mut values, "asset_id", code.clone());
            set(&mut values, "qr_code", code);
            update_record(db, "equipment", &values, id)?;
        }
        ok(obj([("ok", true.into()), ("count", Value::Int(ids.len() as i64))]))
    })
}
