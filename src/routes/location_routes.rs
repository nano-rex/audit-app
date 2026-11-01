//! Outlets, their locations, and zones.

use crate::api::{ok_true, send_error, Ctx, Reply};
use crate::clock;
use crate::common::location_qr_code;
use crate::db::{first_outlet, load_or, save_value, with_db};
use crate::error::{status, R};
use crate::json::Value;
use crate::locations::{add_locations_to_default_zone, assign_equipment, outlet_exists, protect_history, record, required_name, update_membership, zone_locations};
use crate::params;

fn outlet_of(db: &crate::db::Db, payload: &Value) -> R<Value> {
    Ok(if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(first_outlet(db)?) })
}

fn or_default(payload: &Value, key: &str, fallback: Value) -> Value {
    payload.get(key).cloned().unwrap_or(fallback)
}

pub fn post_location(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    with_db(|db| {
        db.begin_immediate()?;
        let outlet = outlet_of(db, payload)?;
        outlet_exists(db, &outlet)?;
        let name = required_name(payload.at("name"))?;
        if db.exists("SELECT 1 FROM locations WHERE outlet_code = ? AND lower(name) = lower(?)", params![outlet, name])? {
            return Err(status(409, "A location with this name already exists for the selected outlet"));
        }
        let order = payload.at("displayOrder").or(&Value::Int(0)).py_int().map_err(crate::error::bad)?;
        let qr = if payload.at("qrCode").truthy() { payload.at("qrCode").clone() } else { Value::str(location_qr_code(&outlet.text(), &name)) };
        db.exec(
            "INSERT INTO locations (outlet_code, name, floor, area, display_order, size, qr_code, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![outlet, name, or_default(payload, "floor", "".into()), or_default(payload, "area", "".into()), order, or_default(payload, "size", "".into()), qr, now],
        )?;
        add_locations_to_default_zone(db, &outlet.text(), now)?;
        if payload.has("equipmentIds") {
            // Absent means "leave the items as they are", not "remove them all".
            assign_equipment(db, &outlet.text(), &name, payload.at("equipmentIds").or(&Value::array()))?;
        }
        ok_true()
    })
}

pub fn post_zone(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    with_db(|db| {
        db.begin_immediate()?;
        let outlet = outlet_of(db, payload)?;
        outlet_exists(db, &outlet)?;
        let members = zone_locations(db, &outlet, payload.at("locations").or(&Value::array()))?;
        db.exec(
            "INSERT INTO zones (outlet_code, name, locations_data_id, description, created_at) VALUES (?, ?, ?, ?, ?)",
            params![outlet, required_name(payload.at("name"))?, save_value(db, &members)?, or_default(payload, "description", "".into()), now],
        )?;
        ok_true()
    })
}

pub fn post_outlet(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    with_db(|db| {
        db.begin_immediate()?;
        db.exec(
            "INSERT INTO outlets (code, location, description, created_at) VALUES (?, ?, ?, ?)",
            params![required_name(payload.at("code"))?.to_uppercase(), or_default(payload, "location", "".into()), or_default(payload, "description", "".into()), now],
        )?;
        ok_true()
    })
}

pub fn patch_outlet(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        let existing = record(db, "outlets", record_id)?;
        let old = existing.str("code");
        let code = required_name(&or_default(payload, "code", existing.value("code")))?.to_uppercase();
        if code != old {
            protect_history(db, &old, None)?;
            for room in db.rows("SELECT id, name, qr_code FROM locations WHERE outlet_code = ?", params![old])? {
                if room.str("qr_code") == location_qr_code(&old, &room.str("name")) {
                    db.exec("UPDATE locations SET qr_code = ? WHERE id = ?", params![location_qr_code(&code, &room.str("name")), room.int("id")])?;
                }
            }
            for (table, column) in [("locations", "outlet_code"), ("zones", "outlet_code"), ("equipment", "outlet")] {
                db.exec(&format!("UPDATE {table} SET {column} = ? WHERE {column} = ?"), params![code, old])?;
            }
        }
        let changed = db.exec(
            "UPDATE outlets SET code = ?, location = ?, description = ? WHERE id = ?",
            params![code, or_default(payload, "location", existing.value("location")), or_default(payload, "description", existing.value("description")), record_id],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn patch_location(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let location_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        let existing = record(db, "locations", location_id)?;
        let outlet = if payload.at("outlet").truthy() { payload.at("outlet").text() } else { existing.str("outlet_code") };
        if outlet != existing.str("outlet_code") {
            return Err(status(409, "Create a new location to move it to a different outlet"));
        }
        outlet_exists(db, &Value::str(&outlet))?;
        let old_name = existing.str("name");
        let name = required_name(&or_default(payload, "name", existing.value("name")))?;
        if name != old_name {
            protect_history(db, &outlet, Some(&old_name))?;
            update_membership(db, &outlet, &old_name, Some(&name))?;
            db.exec("UPDATE equipment SET location = ? WHERE outlet = ? AND location = ?", params![name, outlet, old_name])?;
            db.exec("UPDATE equipment SET zone = ? WHERE outlet = ? AND zone = ?", params![name, outlet, old_name])?;
        }
        let order = or_default(payload, "displayOrder", existing.value("display_order")).or(&Value::Int(0)).py_int().map_err(crate::error::bad)?;
        let qr = if payload.at("qrCode").truthy() {
            payload.at("qrCode").clone()
        } else if existing.str("qr_code") == location_qr_code(&outlet, &old_name) {
            Value::str(location_qr_code(&outlet, &name))
        } else {
            existing.value("qr_code")
        };
        let changed = db.exec(
            "UPDATE locations SET outlet_code = ?, name = ?, floor = ?, area = ?, display_order = ?, size = ?, qr_code = ? WHERE id = ?",
            params![outlet, name, or_default(payload, "floor", existing.value("floor")), or_default(payload, "area", existing.value("area")), order, or_default(payload, "size", existing.value("size")), qr, location_id],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        add_locations_to_default_zone(db, &outlet, 0)?;
        if payload.has("equipmentIds") {
            // Absent means "leave the items as they are", not "remove them all".
            assign_equipment(db, &outlet, &name, payload.at("equipmentIds").or(&Value::array()))?;
        }
        ok_true()
    })
}

pub fn patch_zone(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let zone_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        let existing = record(db, "zones", zone_id)?;
        let outlet = if payload.at("outlet").truthy() { payload.at("outlet").text() } else { existing.str("outlet_code") };
        if outlet != existing.str("outlet_code") {
            return Err(status(409, "Create a new zone to move it to a different outlet"));
        }
        let old_name = existing.str("name");
        let name = required_name(&or_default(payload, "name", existing.value("name")))?;
        if name != old_name {
            protect_history(db, &outlet, Some(&old_name))?;
            db.exec("UPDATE equipment SET zone = ? WHERE outlet = ? AND zone = ?", params![name, outlet, old_name])?;
        }
        let stored = load_or(db, existing.get("locations_data_id"), Value::Null)?;
        let stored = if stored.truthy() { stored } else { Value::array() };
        let members = zone_locations(db, &Value::str(&outlet), &or_default(payload, "locations", stored))?;
        let changed = db.exec(
            "UPDATE zones SET outlet_code = ?, name = ?, locations_data_id = ?, description = ? WHERE id = ?",
            params![outlet, name, save_value(db, &members)?, or_default(payload, "description", existing.value("description")), zone_id],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn delete_outlet(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        let existing = record(db, "outlets", record_id)?;
        let code = existing.str("code");
        protect_history(db, &code, None)?;
        for (table, column) in [("locations", "outlet_code"), ("zones", "outlet_code"), ("equipment", "outlet")] {
            if db.exists(&format!("SELECT 1 FROM {table} WHERE {column} = ? LIMIT 1"), params![code])? {
                return Err(status(409, "Remove this outlet's locations, zones, and equipment before deleting it"));
            }
        }
        if db.exec("DELETE FROM outlets WHERE id = ?", params![record_id])? == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn delete_location(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        let row = record(db, "locations", record_id)?;
        let (outlet, name) = (row.str("outlet_code"), row.str("name"));
        protect_history(db, &outlet, Some(&name))?;
        if db.exists("SELECT 1 FROM equipment WHERE outlet = ? AND (location = ? OR zone = ?) LIMIT 1", params![outlet, name, name])? {
            return Err(status(409, "Relocate this location's equipment before deleting it"));
        }
        update_membership(db, &outlet, &name, None)?;
        db.exec("DELETE FROM locations WHERE id = ?", params![record_id])?;
        ok_true()
    })
}

pub fn delete_zone(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        let row = record(db, "zones", record_id)?;
        let (outlet, name) = (row.str("outlet_code"), row.str("name"));
        protect_history(db, &outlet, Some(&name))?;
        if db.exists("SELECT 1 FROM equipment WHERE outlet = ? AND zone = ? LIMIT 1", params![outlet, name])? {
            return Err(status(409, "Relocate this zone's equipment before deleting it"));
        }
        if db.exec("DELETE FROM zones WHERE id = ?", params![record_id])? == 0 {
            return send_error(404);
        }
        ok_true()
    })
}
