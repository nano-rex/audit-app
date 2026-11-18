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

/// What the asset register page asks for: one page of one kind, filtered, with the filter choices.
#[derive(Default)]
pub struct AssetQuery {
    pub kind: String,
    pub search: String,
    pub outlet: String,
    pub location: String,
    pub asset_type: String,
    pub brand: String,
    pub status: String,
    pub warranty: String,
    pub expiry: String,
    /// An exact name, for the add form's template and for one bulk-edit group (with `group_type`).
    pub name: Option<String>,
    pub group_type: Option<String>,
    pub page: i64,
    pub size: i64,
    pub facets: bool,
}

const ASSET_KIND: &str = "COALESCE(NULLIF(kind, ''), 'asset')";
const ASSET_NAME: &str = "COALESCE(NULLIF(name, ''), asset_id, '')";
const ASSET_TYPE: &str = "COALESCE(NULLIF(type, ''), equipment_type, '')";
const ASSET_LOCATION: &str = "COALESCE(NULLIF(location, ''), zone, '')";
const ASSET_STATUS: &str = "COALESCE(NULLIF(operational_status, ''), health_status, '')";
const ASSET_SEARCH: &str = "lower(COALESCE(name, '') || ' ' || COALESCE(description, '') || ' ' || COALESCE(NULLIF(type, ''), equipment_type, '') || ' ' || COALESCE(NULLIF(operational_status, ''), health_status, '') || ' ' || COALESCE(NULLIF(code, ''), asset_id, '') || ' ' || COALESCE(model, '') || ' ' || COALESCE(serial_number, '') || ' ' || COALESCE(brand, '') || ' ' || COALESCE(outlet, '') || ' ' || COALESCE(NULLIF(location, ''), zone, '') || ' ' || COALESCE(category, ''))";
const ASSET_COLUMNS: &str = "id, kind, category, asset_id, qr_code, outlet, zone, equipment_type, health_status, last_checked, replacement_flag, notes, name, description, type, operational_status, code, model, serial_number, brand, location, installation_date, temporary_relocation, warranty_date, calibration_date, expiry_date, photos_data_id, inverter_model, motor_capacity, source_file, source_sheet, inspection_criteria_data_id";

/// Item dates are typed as text: 2026-10-09, 09.10.2026, 9/10/2026, or 09-10-2026 (day first).
/// Matches `parseItemDate` in web/js/dashboard.js.
pub fn parse_item_date(text: &str) -> Option<crate::clock::Date> {
    let parts: Vec<&str> = text.trim().split(['-', '.', '/']).collect();
    if parts.len() != 3 || !parts.iter().all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let number = |part: &str| part.parse::<i64>().ok();
    let (year, month, day) = if parts[0].len() == 4 && parts[1].len() <= 2 && parts[2].len() <= 2 {
        (number(parts[0])?, number(parts[1])?, number(parts[2])?)
    } else if parts[0].len() <= 2 && parts[1].len() <= 2 && (parts[2].len() == 2 || parts[2].len() == 4) {
        let year = number(parts[2])?;
        (if parts[2].len() == 2 { 2000 + year } else { year }, number(parts[1])?, number(parts[0])?)
    } else {
        return None;
    };
    crate::clock::Date::new(year, u32::try_from(month).ok()?, u32::try_from(day).ok()?)
}

/// Expired, ends within 30 or 90 days, still valid, or not recorded. Matches `matchesDateFilter`.
fn matches_date_filter(text: &str, filter: &str, today: i64) -> bool {
    if filter.is_empty() {
        return true;
    }
    let date = parse_item_date(text);
    if filter == "none" {
        return date.is_none();
    }
    let Some(date) = date else { return false };
    let days = date.days() - today;
    match filter {
        "expired" => days < 0,
        "valid" => days >= 0,
        other => other.parse::<i64>().map(|limit| (0..=limit).contains(&days)).unwrap_or(true),
    }
}

/// One page of the asset register. `scope` limits an outlet-limited account to its outlets.
/// Filtering runs in SQLite and only the page shown is decoded, so the page stays small however
/// large the register grows.
pub fn asset_page(scope: Option<&[String]>, query: &AssetQuery) -> R<Value> {
    // Kind and permitted outlets: what "Edit all" groups count over.
    let (base_clauses, base_values) = asset_base(scope, &query.kind);
    let mut clauses = base_clauses.clone();
    let mut values = base_values.clone();
    if !query.outlet.is_empty() {
        clauses.push("outlet = ?".into());
        values.push(query.outlet.clone());
    }
    // The filter choices follow the kind, scope, and outlet, not the other filters.
    let (facet_clauses, facet_values) = (clauses.clone(), values.clone());
    let mut add = |expression: &str, value: &str| {
        clauses.push(format!("{expression} = ?"));
        values.push(value.to_string());
    };
    if let Some(name) = &query.name {
        add(ASSET_NAME, name);
    }
    if let Some(group_type) = &query.group_type {
        add(ASSET_TYPE, group_type);
    }
    for (expression, value) in [(ASSET_LOCATION, &query.location), (ASSET_TYPE, &query.asset_type), ("COALESCE(brand, '')", &query.brand), (ASSET_STATUS, &query.status)] {
        if !value.is_empty() {
            add(expression, value);
        }
    }
    let search = query.search.trim().to_lowercase();
    if !search.is_empty() {
        clauses.push(format!("instr({ASSET_SEARCH}, ?) > 0"));
        values.push(search);
    }
    // A bulk-edit group is fetched whole; the register itself is paged.
    let size = query.size.clamp(1, if query.name.is_some() && query.group_type.is_some() { 20_000 } else { 500 });
    let today = crate::clock::today().days();
    with_db(|db| {
        const ORDER: &str = "ORDER BY COALESCE(name, asset_id), id";
        // Ids of every match in order; date filters read the text dates people typed.
        let dated = !query.warranty.is_empty() || !query.expiry.is_empty();
        let columns = if dated { "id, warranty_date, expiry_date" } else { "id" };
        let mut ids: Vec<i64> = Vec::new();
        for row in db.rows(&format!("SELECT {columns} FROM equipment WHERE {} {ORDER}", clauses.join(" AND ")), &sql_params(&values))? {
            if !dated || (matches_date_filter(&row.str("warranty_date"), &query.warranty, today) && matches_date_filter(&row.str("expiry_date"), &query.expiry, today)) {
                ids.push(row.int("id"));
            }
        }
        let total = ids.len() as i64;
        let pages = ((total + size - 1) / size).max(1);
        let page = query.page.clamp(1, pages);
        let shown: Vec<i64> = ids.iter().skip(((page - 1) * size) as usize).take(size as usize).copied().collect();
        let mut items = Vec::new();
        if !shown.is_empty() {
            let marks = vec!["?"; shown.len()].join(", ");
            let id_params: Vec<&dyn crate::sqlite::ToSql> = shown.iter().map(|id| id as &dyn crate::sqlite::ToSql).collect();
            let rows = db.rows(&format!("SELECT {ASSET_COLUMNS} FROM equipment WHERE id IN ({marks}) {ORDER}"), &id_params)?;
            items = hydrate_many(db, &rows)?;
            // How many items share each shown item's name and asset type, for "Edit all".
            let names: Vec<String> = rows.iter().map(|row| { let name = row.str("name"); if name.is_empty() { row.str("asset_id") } else { name } }).collect();
            let mut group_values = base_values.clone();
            group_values.extend(names.iter().cloned());
            let groups = db.rows(
                &format!("SELECT {ASSET_NAME} AS group_name, {ASSET_TYPE} AS group_type, COUNT(*) AS size FROM equipment WHERE {} AND {ASSET_NAME} IN ({}) GROUP BY 1, 2", base_clauses.join(" AND "), vec!["?"; names.len()].join(", ")),
                &sql_params(&group_values),
            )?;
            for item in &mut items {
                let text = |key: &str, fallback: &str| {
                    let value = item.at(key).py_str();
                    if item.at(key).truthy() { value } else if item.at(fallback).truthy() { item.at(fallback).py_str() } else { String::new() }
                };
                let (name, kind_type) = (text("name", "asset_id"), text("type", "equipment_type"));
                let size = groups.iter().find(|row| row.str("group_name") == name && row.str("group_type") == kind_type).map(|row| row.int("size")).unwrap_or(1);
                item.set("group_size", Value::Int(size));
            }
        }
        let mut result = obj([("items", Value::Array(items)), ("total", Value::Int(total)), ("page", Value::Int(page)), ("pages", Value::Int(pages)), ("size", Value::Int(size))]);
        if query.facets {
            let distinct = |expression: &str| -> R<Value> {
                let sql = format!("SELECT DISTINCT {expression} AS value FROM equipment WHERE {} AND {expression} != '' ORDER BY 1", facet_clauses.join(" AND "));
                Ok(strings(&db.rows(&sql, &sql_params(&facet_values))?.iter().map(|row| row.str("value")).collect::<Vec<_>>()))
            };
            result.set("facets", obj([
                ("locations", distinct(ASSET_LOCATION)?),
                ("types", distinct(ASSET_TYPE)?),
                ("brands", distinct("COALESCE(brand, '')")?),
                ("statuses", distinct(ASSET_STATUS)?),
                ("names", distinct(ASSET_NAME)?),
            ]));
        }
        Ok(result)
    })
}

fn sql_params(values: &[String]) -> Vec<&dyn crate::sqlite::ToSql> {
    values.iter().map(|value| value as &dyn crate::sqlite::ToSql).collect()
}

/// The clauses every asset query starts from: one kind, and an outlet-limited account's outlets.
fn asset_base(scope: Option<&[String]>, kind: &str) -> (Vec<String>, Vec<String>) {
    let mut clauses = vec![format!("{ASSET_KIND} = ?")];
    let mut values = vec![if kind == "fixture" { "fixture".to_string() } else { "asset".to_string() }];
    if let Some(codes) = scope {
        if codes.is_empty() {
            clauses.push("0".into());
        } else {
            clauses.push(format!("outlet IN ({})", vec!["?"; codes.len()].join(", ")));
            values.extend(codes.iter().cloned());
        }
    }
    (clauses, values)
}

/// Groups of two or more items of one kind that share a name and asset type, for Bulk Edit.
pub fn asset_groups(scope: Option<&[String]>, kind: &str) -> R<Value> {
    let (clauses, values) = asset_base(scope, kind);
    with_db(|db| {
        let rows = db.rows(
            &format!("SELECT {ASSET_NAME} AS name, {ASSET_TYPE} AS type, COUNT(*) AS size FROM equipment WHERE {} GROUP BY 1, 2 HAVING COUNT(*) > 1 ORDER BY 1, 2", clauses.join(" AND ")),
            &sql_params(&values),
        )?;
        Ok(obj([("items", Value::Array(rows.iter().map(Row::to_object).collect()))]))
    })
}

/// Only what the location editor lists: each item's id, name, code, and current location.
pub fn asset_briefs(outlet: &str) -> R<Value> {
    with_db(|db| {
        let rows = db.rows(
            "SELECT id, name, asset_id, code, location, zone, outlet FROM equipment WHERE outlet = ? ORDER BY COALESCE(name, asset_id), id",
            params![outlet],
        )?;
        Ok(obj([("items", Value::Array(rows.iter().map(Row::to_object).collect()))]))
    })
}

#[cfg(test)]
mod asset_date_tests {
    use super::{matches_date_filter, parse_item_date};
    use crate::clock::Date;

    #[test]
    fn item_dates_read_the_formats_people_type() {
        let date = |y, m, d| Some(Date::new(y, m, d).unwrap());
        assert_eq!(parse_item_date("2026-10-09"), date(2026, 10, 9));
        assert_eq!(parse_item_date(" 09.10.2026 "), date(2026, 10, 9));
        assert_eq!(parse_item_date("9/10/2026"), date(2026, 10, 9));
        assert_eq!(parse_item_date("09-10-26"), date(2026, 10, 9));
        assert_eq!(parse_item_date("2026/1/5"), date(2026, 1, 5));
        for text in ["", "31.02.2026", "2026-13-01", "26-10-09-1", "9/10/202", "abc", "2026-10-9x"] {
            assert_eq!(parse_item_date(text), None, "{text}");
        }
    }

    #[test]
    fn date_filters_match_the_browser() {
        let today = Date::new(2026, 11, 18).unwrap().days();
        assert!(matches_date_filter("17.11.2026", "expired", today));
        assert!(!matches_date_filter("18.11.2026", "expired", today));
        assert!(matches_date_filter("18.11.2026", "valid", today));
        assert!(matches_date_filter("2026-12-18", "30", today));
        assert!(!matches_date_filter("2026-12-19", "30", today));
        assert!(matches_date_filter("", "none", today) && matches_date_filter("soon", "none", today));
        assert!(!matches_date_filter("", "valid", today));
        assert!(matches_date_filter("anything", "", today));
    }
}
