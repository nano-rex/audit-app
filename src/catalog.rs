//! Setup records, roles, users, locations, zones, and the asset register as the pages list them.

use crate::config::{self, SUPER_ROLE, VARIABLE_ASSET_TYPES};
use crate::db::{all_settings, cached, hydrate_many, load_or, load_value, read_setting, with_db, Db, Prepared};
use crate::error::{status, R};
use crate::json::{obj, strings, Value};
use crate::params;
use crate::permissions::change_records_value;
use crate::sqlite::Row;
use std::sync::Arc;

fn role_rows(db: &Db, include_super: bool) -> R<Vec<Row>> {
    let query = "SELECT id, name, description, permissions_data_id, inspection_permissions_data_id, protected, action_permissions_data_id, department, reports_to_id FROM roles";
    if include_super {
        db.rows(&format!("{query} ORDER BY protected DESC, name"), params![])
    } else {
        db.rows(&format!("{query} WHERE name != ? ORDER BY protected DESC, name"), params![SUPER_ROLE])
    }
}

fn role_view(db: &Db, row: &Row) -> R<Value> {
    let mut item = row.to_object();
    item.remove("permissions_data_id");
    item.remove("inspection_permissions_data_id");
    item.remove("action_permissions_data_id");
    item.set("permissions", load_or(db, row.get("permissions_data_id"), Value::array())?);
    let actions = if row.get("action_permissions_data_id").truthy() { load_value(db, row.get("action_permissions_data_id"))? } else { Value::array() };
    item.set("actions", if actions.truthy() { actions } else { Value::array() });
    item.set("inspectionPermissions", load_or(db, row.get("inspection_permissions_data_id"), Value::array())?);
    Ok(item)
}

pub fn setup_records(include_super: bool) -> R<Value> {
    with_db(|db| {
        let departments: Vec<Value> = db.rows("SELECT id, code, description, responsibilities FROM departments ORDER BY code", params![])?.iter().map(Row::to_object).collect();
        let outlets: Vec<Value> = db.rows("SELECT id, code, location, description FROM outlets ORDER BY code", params![])?.iter().map(Row::to_object).collect();
        let mut zones = Vec::new();
        for row in db.rows("SELECT id, outlet_code, name, locations_data_id, description FROM zones ORDER BY outlet_code, name", params![])? {
            let mut zone = row.to_object();
            zone.remove("locations_data_id");
            zone.set("locations", load_or(db, row.get("locations_data_id"), Value::array())?);
            zones.push(zone);
        }
        // Asset types replace categories: the variable asset types, and every type in use.
        let stored = read_setting(db, "assets.variableTypes", Value::Null)?;
        let variable_types = if stored.truthy() { stored } else { strings(&VARIABLE_ASSET_TYPES) };
        let used = db.strings("SELECT DISTINCT type FROM equipment WHERE COALESCE(type, '') != '' ORDER BY type", params![])?;
        let mut asset_types: Vec<String> = variable_types.items().iter().map(Value::py_str).chain(used).collect();
        asset_types.sort_by_key(|name| name.to_lowercase());
        asset_types.dedup();
        let mut unique_types: Vec<String> = Vec::new();
        for name in asset_types {
            if !unique_types.contains(&name) {
                unique_types.push(name);
            }
        }
        let mut roles = Vec::new();
        for row in role_rows(db, include_super)? {
            let mut role = row.to_object();
            role.remove("inspection_permissions_data_id");
            role.remove("permissions_data_id");
            role.remove("action_permissions_data_id");
            role.set("inspectionPermissions", load_or(db, row.get("inspection_permissions_data_id"), Value::array())?);
            role.set("permissions", load_or(db, row.get("permissions_data_id"), Value::array())?);
            let actions = if row.get("action_permissions_data_id").truthy() { load_value(db, row.get("action_permissions_data_id"))? } else { Value::array() };
            role.set("actions", if actions.truthy() { actions } else { Value::array() });
            roles.push(role);
        }
        let priorities: Vec<Value> = db.rows("SELECT id, name, classification, due_days, active FROM priority_levels ORDER BY due_days, name", params![])?.iter().map(Row::to_object).collect();
        let audit_types: Vec<Value> = db.rows("SELECT id, name, description, active, style FROM audit_types ORDER BY name", params![])?.iter().map(Row::to_object).collect();
        Ok(obj([
            ("changeRecords", change_records_value()),
            ("departments", Value::Array(departments)),
            ("outlets", Value::Array(outlets)),
            ("zones", Value::Array(zones)),
            ("variableTypes", variable_types),
            ("assetTypes", strings(&unique_types)),
            ("roles", Value::Array(roles)),
            ("priorities", Value::Array(priorities)),
            ("auditTypes", Value::Array(audit_types)),
            ("settings", all_settings(db, None)?),
            ("tabs", config::tabs_value()),
        ]))
    })
}

pub fn role_items(include_super: bool) -> R<Value> {
    with_db(|db| {
        let items = role_rows(db, include_super)?.iter().map(|row| role_view(db, row)).collect::<R<Vec<_>>>()?;
        Ok(obj([("items", Value::Array(items)), ("tabs", config::tabs_value()), ("changeRecords", change_records_value())]))
    })
}

pub fn users(include_super: bool) -> R<Value> {
    with_db(|db| {
        let query = "SELECT id, name, username, role, email, department, active, reset_required, last_login_at, login_count, title, responsibilities, permission_overrides_data_id, outlets_data_id, profile_photo_data_id, EXISTS(SELECT 1 FROM password_reset_requests WHERE user_id = users.id AND resolved_at IS NULL) AS reset_requested FROM users";
        let rows = if include_super {
            db.rows(&format!("{query} ORDER BY role, name"), params![])?
        } else {
            db.rows(&format!("{query} WHERE role != ? ORDER BY role, name"), params![SUPER_ROLE])?
        };
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut item = row.to_object();
            item.remove("permission_overrides_data_id");
            item.remove("outlets_data_id");
            item.remove("profile_photo_data_id");
            item.set("permissionOverrides", if row.get("permission_overrides_data_id").is_null() { Value::Null } else { load_value(db, row.get("permission_overrides_data_id"))? });
            // None: every outlet; otherwise the outlets ticked for this person.
            item.set("outlets", if row.get("outlets_data_id").is_null() {
                Value::Null
            } else {
                let outlets = load_value(db, row.get("outlets_data_id"))?;
                if outlets.truthy() { outlets } else { Value::array() }
            });
            // The picture is shown on the organization chart; only its address is sent.
            let picture = if row.get("profile_photo_data_id").truthy() { load_value(db, row.get("profile_photo_data_id"))? } else { Value::object() };
            let mut photo = Value::object();
            if picture.is_object() {
                for key in ["url", "name"] {
                    if picture.at(key).truthy() {
                        photo.set(key, picture.at(key).clone());
                    }
                }
            }
            item.set("profilePhoto", photo);
            items.push(item);
        }
        Ok(obj([("items", Value::Array(items))]))
    })
}

/// An outlet's locations, or every outlet's when none is named. Brief: names only, without the
/// items in each (for pickers).
pub fn locations(outlet: &str, brief: bool) -> R<Value> {
    with_db(|db| {
        if brief {
            let rows = db.rows("SELECT id, outlet_code, name FROM locations WHERE ? = '' OR outlet_code = ? ORDER BY outlet_code, name", params![outlet, outlet])?;
            return Ok(obj([("items", Value::Array(rows.iter().map(Row::to_object).collect()))]));
        }
        let rows = db.rows(
            "SELECT locations.id, locations.outlet_code, locations.name, locations.floor, locations.area, locations.display_order, locations.size, locations.qr_code,
                    GROUP_CONCAT(COALESCE(equipment.name, equipment.asset_id), ', ') equipment
             FROM locations
             LEFT JOIN equipment ON equipment.outlet = locations.outlet_code AND equipment.location = locations.name
             WHERE ? = '' OR locations.outlet_code = ?
             GROUP BY locations.id
             ORDER BY locations.display_order, locations.floor, locations.area, locations.name",
            params![outlet, outlet],
        )?;
        Ok(obj([("items", Value::Array(hydrate_many(db, &rows)?))]))
    })
}

pub fn zones(outlet: &str) -> R<Value> {
    with_db(|db| {
        let rows = if outlet.is_empty() {
            db.rows("SELECT id, outlet_code, name, locations_data_id, description FROM zones ORDER BY outlet_code, name", params![])?
        } else {
            db.rows("SELECT id, outlet_code, name, locations_data_id, description FROM zones WHERE outlet_code = ? ORDER BY name", params![outlet])?
        };
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut item = row.to_object();
            item.remove("locations_data_id");
            item.set("locations", load_or(db, row.get("locations_data_id"), Value::array())?);
            items.push(item);
        }
        Ok(obj([("items", Value::Array(items))]))
    })
}

pub fn equipment_items(outlet: Option<&str>, compact: bool) -> R<Arc<Prepared>> {
    let key = format!("equipment|{}|{compact}", outlet.unwrap_or("\u{0}"));
    cached(&key, || query_equipment_items(outlet, compact))
}

fn query_equipment_items(outlet: Option<&str>, compact: bool) -> R<Value> {
    let columns = if compact {
        "id, kind, category, asset_id, outlet, zone, equipment_type, name, type, code, location, serial_number, inspection_criteria_data_id"
    } else {
        "id, kind, category, asset_id, qr_code, outlet, zone, equipment_type, health_status, last_checked, replacement_flag, notes, name, description, type, operational_status, code, model, serial_number, brand, location, installation_date, temporary_relocation, warranty_date, calibration_date, expiry_date, photos_data_id, inverter_model, motor_capacity, source_file, source_sheet, inspection_criteria_data_id"
    };
    with_db(|db| {
        let rows = match outlet.filter(|outlet| !outlet.is_empty()) {
            Some(outlet) => db.rows(&format!("SELECT {columns} FROM equipment WHERE outlet = ? ORDER BY COALESCE(name, asset_id), id"), params![outlet])?,
            None => db.rows(&format!("SELECT {columns} FROM equipment ORDER BY COALESCE(name, asset_id), id"), params![])?,
        };
        Ok(obj([("items", Value::Array(hydrate_many(db, &rows)?))]))
    })
}

pub fn user_login_activity(user_id: i64, offset: &str) -> R<Value> {
    let offset = Value::str(offset).py_int().map_err(crate::error::bad)?.max(0);
    with_db(|db| {
        let user = db.row("SELECT name FROM users WHERE id = ?", params![user_id])?.ok_or_else(|| status(404, "User not found"))?;
        let rows = db.rows(
            "SELECT id, email, logged_at, remember_me, user_agent FROM user_login_activity WHERE user_id = ? ORDER BY logged_at DESC, id DESC LIMIT 51 OFFSET ?",
            params![user_id, offset],
        )?;
        Ok(obj([
            ("name", user.value("name")),
            ("items", Value::Array(rows.iter().take(50).map(Row::to_object).collect())),
            ("offset", Value::Int(offset)),
            ("hasMore", Value::Bool(rows.len() > 50)),
        ]))
    })
}
