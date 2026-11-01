//! Master-data checks that preserve audit snapshots and asset placement, and what a scheduled
//! visit covers.

use crate::clock::Date;
use crate::common::unique;
use crate::db::{load_or, marks, save_value, Db};
use crate::error::{bad, status, R};
use crate::json::{obj, Value};
use crate::params;
use crate::sqlite::{Row, ToSql};

pub const ALL_LOCATIONS: &str = "All Locations";

pub fn outlet_exists(db: &Db, code: &Value) -> R<()> {
    if !db.exists("SELECT 1 FROM outlets WHERE code = ?", params![code])? {
        return Err(bad("Select an existing outlet"));
    }
    Ok(())
}

pub fn required_name(value: &Value) -> R<String> {
    let value = value.text().trim().to_string();
    if value.is_empty() || value.chars().count() > 150 {
        return Err(bad("Enter a name or code of 1–150 characters"));
    }
    Ok(value)
}

pub fn record(db: &Db, table: &str, record_id: i64) -> R<Row> {
    db.row(&format!("SELECT * FROM {table} WHERE id = ?"), params![record_id])?.ok_or_else(|| status(404, "Record not found"))
}

pub fn protect_history(db: &Db, outlet: &str, name: Option<&str>) -> R<()> {
    let references = [("audits", "branch"), ("inspection_sessions", "zone"), ("schedules", "zone"), ("findings", "location"), ("work_orders", "zone")];
    for (table, column) in references {
        let used = match name {
            Some(name) => db.exists(&format!("SELECT 1 FROM {table} WHERE outlet = ? AND {column} = ? LIMIT 1"), params![outlet, name])?,
            None => db.exists(&format!("SELECT 1 FROM {table} WHERE outlet = ? LIMIT 1"), params![outlet])?,
        };
        if used {
            return Err(status(409, "This name is used by audit records or scheduled work. Keep it and edit its other details, or create a new entry."));
        }
    }
    let Some(name) = name else { return Ok(()) };
    for table in ["schedules", "inspection_sessions"] {
        for row in db.rows(&format!("SELECT locations_data_id FROM {table} WHERE outlet = ? AND locations_data_id IS NOT NULL"), params![outlet])? {
            if load_or(db, row.get("locations_data_id"), Value::array())?.items().iter().any(|value| value.as_str() == Some(name)) {
                return Err(status(409, "This location is chosen for scheduled work and must be retained"));
            }
        }
    }
    for session in db.rows("SELECT items_data_id FROM inspection_sessions WHERE outlet = ?", params![outlet])? {
        let items = load_or(db, session.get("items_data_id"), Value::array())?;
        if items.items().iter().any(|item| item.at("location").as_str() == Some(name)) {
            return Err(status(409, "This location is used by saved inspection items and must be retained"));
        }
    }
    Ok(())
}

pub fn zone_locations(db: &Db, outlet: &Value, values: &Value) -> R<Value> {
    let Value::Array(items) = values else { return Err(bad("Zone locations must be a list of location names")) };
    if items.iter().any(|value| !value.is_str()) {
        return Err(bad("Zone locations must be a list of location names"));
    }
    let available = db.strings("SELECT name FROM locations WHERE outlet_code = ?", params![outlet])?;
    if items.iter().any(|value| !available.iter().any(|name| Some(name.as_str()) == value.as_str())) {
        return Err(bad("Every zone location must belong to the selected outlet"));
    }
    Ok(Value::Array(unique(items.iter().cloned())))
}

/// The locations a scheduled visit covers; an empty list means every location of the outlet.
/// Older clients send one location as "zone"; it is read as a selection of that one location.
fn visit_locations(db: &Db, outlet: &Value, payload: &Value) -> R<Value> {
    let values = match payload.get("locations") {
        Some(value) if !value.is_null() => value.clone(),
        _ => {
            let zone = payload.at("zone");
            let found = zone.truthy() && db.exists("SELECT 1 FROM locations WHERE outlet_code = ? AND name = ?", params![outlet, zone])?;
            if found { Value::Array(vec![zone.clone()]) } else { Value::array() }
        }
    };
    zone_locations(db, outlet, &values)
}

pub fn locations_label(values: &Value) -> String {
    if values.items().is_empty() {
        ALL_LOCATIONS.into()
    } else {
        values.items().iter().map(Value::py_str).collect::<Vec<_>>().join(", ")
    }
}

/// A visit's priority (one of the priority levels, or none) and optional due date, which may not
/// come before the visit.
pub fn visit_terms(db: &Db, payload: &Value) -> R<(String, String)> {
    let priority = payload.at("priority").text().trim().to_string();
    if !priority.is_empty() && !db.exists("SELECT 1 FROM priority_levels WHERE name = ?", params![priority])? {
        return Err(bad("Choose one of the priority levels"));
    }
    let due = payload.at("dueDate").text().trim().to_string();
    if !due.is_empty() {
        let due_day = Date::parse_iso(&due).ok_or_else(|| bad("Enter a valid due date"))?;
        if let Some(scheduled) = Date::parse_iso(&payload.at("scheduledDate").text()) {
            if due_day < scheduled {
                return Err(bad("The due date cannot be before the scheduled date"));
            }
        }
    }
    Ok((priority, due))
}

/// The visit's audit type: one of the audit types, Detailed when there is one and none is chosen.
pub fn visit_audit_type(db: &Db, payload: &Value) -> R<String> {
    let name = payload.at("auditType").text().trim().to_string();
    if !name.is_empty() {
        if !db.exists("SELECT 1 FROM audit_types WHERE name = ?", params![name])? {
            return Err(bad("Choose one of the audit types"));
        }
        return Ok(name);
    }
    default_audit_type(db)
}

pub fn default_audit_type(db: &Db) -> R<String> {
    Ok(db.scalar("SELECT name FROM audit_types WHERE active = 1 ORDER BY name = 'Detailed' DESC, id LIMIT 1", params![])?.text())
}

/// What a visit covers, chosen on one of three tabs: locations (none ticked means all), zones
/// (their locations), or particular assets (and so their locations). Returns the locations, the
/// scope as chosen, and its label.
pub fn visit_scope(db: &Db, outlet: &Value, payload: &Value) -> R<(Value, Value, String)> {
    let by = payload.at("scope").or(&Value::Null);
    let by = if by.truthy() { by.py_str() } else { "locations".into() };
    if by == "zones" {
        let names = payload.at("zones");
        let Value::Array(names) = names else { return Err(bad("Choose at least one zone")) };
        if names.is_empty() || names.iter().any(|name| !name.is_str()) {
            return Err(bad("Choose at least one zone"));
        }
        let mut zones = Vec::new();
        for row in db.rows("SELECT name, locations_data_id FROM zones WHERE outlet_code = ?", params![outlet])? {
            zones.push((row.str("name"), load_or(db, row.get("locations_data_id"), Value::array())?));
        }
        let names = unique(names.iter().cloned());
        if names.iter().any(|name| !zones.iter().any(|(zone, _)| Some(zone.as_str()) == name.as_str())) {
            return Err(bad("Every zone must belong to the selected outlet"));
        }
        let locations = unique(names.iter().flat_map(|name| zones.iter().find(|(zone, _)| Some(zone.as_str()) == name.as_str()).map(|(_, list)| list.items().to_vec()).unwrap_or_default()));
        if locations.is_empty() {
            return Err(bad("The chosen zones have no locations yet"));
        }
        let label = format!("Zone{}: {}", if names.len() > 1 { "s" } else { "" }, names.iter().map(Value::py_str).collect::<Vec<_>>().join(", "));
        return Ok((Value::Array(locations), obj([("by", "zones".into()), ("zones", Value::Array(names))]), label));
    }
    if by == "assets" {
        let ids = payload.at("assets");
        let Value::Array(ids) = ids else { return Err(bad("Choose at least one asset")) };
        if ids.is_empty() || !ids.iter().all(crate::common::value_is_digits) {
            return Err(bad("Choose at least one asset"));
        }
        let ids: Vec<i64> = unique(ids.iter().filter_map(|value| value.py_str().parse::<i64>().ok()));
        let mut rows = Vec::new();
        for chunk in ids.chunks(500) {
            let mut params: Vec<&dyn ToSql> = vec![outlet];
            params.extend(chunk.iter().map(|id| id as &dyn ToSql));
            rows.extend(db.rows(&format!("SELECT id, location, zone FROM equipment WHERE outlet = ? AND id IN ({})", marks(chunk.len())), &params)?);
        }
        if rows.len() != ids.len() {
            return Err(bad("Every asset must belong to the selected outlet"));
        }
        let locations = unique(rows.iter().map(|row| {
            let location = row.str("location");
            if !location.is_empty() {
                return location;
            }
            let zone = row.str("zone");
            if zone.is_empty() { "Unassigned".to_string() } else { zone }
        }));
        let label: String = format!("{} asset{} in {}", ids.len(), if ids.len() != 1 { "s" } else { "" }, locations.join(", ")).chars().take(500).collect();
        return Ok((crate::json::strings(&locations), obj([("by", "assets".into()), ("assets", Value::from(ids))]), label));
    }
    if by != "locations" {
        return Err(bad("Choose locations, zones, or assets"));
    }
    let locations = visit_locations(db, outlet, payload)?;
    let label = locations_label(&locations);
    Ok((locations, obj([("by", "locations".into())]), label))
}

pub fn update_membership(db: &Db, outlet: &str, old_name: &str, new_name: Option<&str>) -> R<()> {
    for row in db.rows("SELECT id, locations_data_id FROM zones WHERE outlet_code = ?", params![outlet])? {
        let previous = load_or(db, row.get("locations_data_id"), Value::array())?;
        if previous.items().iter().any(|name| name.as_str() == Some(old_name)) {
            let values: Vec<Value> = previous
                .items()
                .iter()
                .filter(|name| name.as_str() != Some(old_name) || new_name.is_some_and(|new| !new.is_empty()))
                .map(|name| if name.as_str() == Some(old_name) { Value::str(new_name.unwrap_or("")) } else { name.clone() })
                .collect();
            db.exec("UPDATE zones SET locations_data_id = ? WHERE id = ?", params![save_value(db, &Value::Array(unique(values)))?, row.int("id")])?;
        }
    }
    Ok(())
}

pub fn assign_equipment(db: &Db, outlet: &str, name: &str, identifiers: &Value) -> R<()> {
    let Value::Array(items) = identifiers else { return Err(bad("Equipment IDs must be a list")) };
    let mut chosen = Vec::new();
    for identifier in items {
        let id = identifier.py_int().map_err(bad)?;
        let row = db.row("SELECT outlet FROM equipment WHERE id = ?", params![id])?;
        if row.is_none_or(|row| row.str("outlet") != outlet) {
            return Err(bad("Select equipment from this outlet; relocate other equipment in the asset editor first"));
        }
        if !chosen.contains(&id) {
            chosen.push(id);
        }
    }
    for id in &chosen {
        db.exec("UPDATE equipment SET location = ?, zone = ? WHERE id = ?", params![name, name, id])?;
    }
    // The list is the location's full membership: an item left out is no longer here.
    for row in db.rows("SELECT id FROM equipment WHERE outlet = ? AND location = ?", params![outlet, name])? {
        if !chosen.contains(&row.int("id")) {
            db.exec("UPDATE equipment SET location = '', zone = 'Unassigned' WHERE id = ?", params![row.int("id")])?;
        }
    }
    Ok(())
}

/// Every location of the outlet in its default zone, "Zone-1".
pub fn add_locations_to_default_zone(db: &Db, outlet: &str, now: i64) -> R<()> {
    let now = if now == 0 { crate::clock::now_ms() } else { now };
    let locations = db.strings("SELECT name FROM locations WHERE outlet_code = ? ORDER BY name", params![outlet])?;
    if locations.is_empty() {
        return Ok(());
    }
    let reference = save_value(db, &crate::json::strings(&locations))?;
    match db.row("SELECT id, locations_data_id FROM zones WHERE outlet_code = ? AND lower(name) = 'zone-1'", params![outlet])? {
        Some(existing) => {
            db.exec("UPDATE zones SET locations_data_id = ? WHERE id = ?", params![reference, existing.int("id")])?;
        }
        None => {
            db.exec(
                "INSERT OR IGNORE INTO zones (outlet_code, name, locations_data_id, description, created_at) VALUES (?, 'Zone-1', ?, 'Default inspection zone', ?)",
                params![outlet, reference, now],
            )?;
        }
    }
    Ok(())
}
