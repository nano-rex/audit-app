//! Accounts as the application presents them, outlet access, and branding.

use crate::config::{self, ADMIN_ROLE, SUPER_ROLE};
use crate::db::{load_or, load_value, read_setting, with_db, Db};
use crate::error::{forbidden, R};
use crate::json::{obj, strings, Value};
use crate::params;
use crate::permissions::{resolve_actions, resolve_permissions};
use crate::sqlite::{Row, Sql};

/// The account as the application sees it: identity, pages, capabilities, and outlets.
pub fn public_user(row: &Row, db: &Db) -> R<Value> {
    let id = row.int("id");
    let record = db.row("SELECT permission_overrides_data_id, outlets_data_id FROM users WHERE id = ?", params![id])?;
    let overrides = match &record {
        Some(record) if !record.get("permission_overrides_data_id").is_null() => load_value(db, record.get("permission_overrides_data_id"))?,
        _ => Value::Null,
    };
    let navigation = db.strings("SELECT page_id FROM user_navigation WHERE user_id = ? ORDER BY position", params![id])?;
    let role = row.str("role");
    let (permissions, inspection) = resolve_permissions(db, &role, &overrides)?;
    let outlets = match &record {
        Some(record) => person_outlets(db, record.get("outlets_data_id"))?,
        None => None,
    };
    let actions = resolve_actions(db, &role, &overrides)?;
    Ok(obj([
        ("navigationOrder", strings(&navigation)),
        ("id", Value::Int(id)),
        ("name", row.value("name")),
        ("username", Value::str(row.str("username"))),
        ("role", row.value("role")),
        ("email", row.value("email")),
        ("department", Value::str(row.str("department"))),
        ("title", Value::str(row.str("title"))),
        ("responsibilities", Value::str(row.str("responsibilities"))),
        ("active", Value::Bool(row.int("active") != 0)),
        ("lastLoginAt", Value::str(row.str("last_login_at"))),
        ("resetRequired", Value::Bool(row.int("reset_required") != 0)),
        ("permissions", permissions),
        ("inspectionPermissions", inspection),
        ("permissionSource", if overrides.is_null() { "role" } else { "user" }.into()),
        ("permissionOverrides", overrides),
        ("actions", actions),
        ("outletScope", if outlets.is_none() { "all" } else { "selected" }.into()),
        ("outlets", outlets.map(Value::Array).unwrap_or_else(Value::array)),
        ("profilePhoto", if row.has("profile_photo_data_id") { load_or(db, row.get("profile_photo_data_id"), Value::object())? } else { Value::object() }),
        ("signatureImage", if row.has("signature_image_data_id") { load_or(db, row.get("signature_image_data_id"), Value::object())? } else { Value::object() }),
    ]))
}

pub fn is_super_user(user: &Value) -> bool {
    user.at("role").as_str() == Some(SUPER_ROLE)
}

pub fn is_company_admin_user(user: &Value) -> bool {
    matches!(user.at("role").as_str(), Some(SUPER_ROLE | ADMIN_ROLE))
}

pub fn branding_settings() -> R<Value> {
    with_db(branding)
}

pub fn branding(db: &Db) -> R<Value> {
    let mut brand = Value::object();
    for key in ["appTitle", "appSubtitle", "companyName", "departmentHeader", "logoText", "logoUrl", "businessUnitLabel", "reportHeading", "loginTitle"] {
        let fallback = if key == "logoUrl" { Value::str("") } else { Value::str(config::report_default(key)) };
        brand.set(key, read_setting(db, &format!("report.{key}"), fallback)?);
    }
    let mut theme = Value::object();
    for (key, value) in config::default_theme_settings().entries() {
        theme.set(key, read_setting(db, &format!("theme.{key}"), value.clone())?);
    }
    // The original listed the theme after the texts; the page reads it by name.
    brand.set("theme", theme);
    Ok(brand)
}

// ---- Which outlets an account may see and change ----
//
// Each person has their outlets: all of them (the default), or the ones ticked for them. People
// sharing a role can cover different outlets (two Regional Managers, two regions). Lists are
// filtered to them, and a change or a single record outside them is refused.

pub const DENIED: &str = "That outlet is not assigned to your account";

/// Records reached by /api/<route>/<id>, and the column naming their outlet.
pub const OUTLET_RECORDS: [(&str, &str, &str); 8] = [
    ("schedules", "schedules", "outlet"),
    ("inspection-sessions", "inspection_sessions", "outlet"),
    ("work-orders", "work_orders", "outlet"),
    ("work-requests", "work_requests", "outlet"),
    ("equipment", "equipment", "outlet"),
    ("locations", "locations", "outlet_code"),
    ("zones", "zones", "outlet_code"),
    ("findings", "findings", "outlet"),
];

/// None when the person covers every outlet, otherwise the list of their outlet codes.
pub fn person_outlets(db: &Db, reference: &Sql) -> R<Option<Vec<Value>>> {
    if reference.is_null() {
        return Ok(None);
    }
    let value = load_value(db, reference)?;
    Ok(Some(value.items().to_vec()))
}

/// None (every outlet) or a list of existing outlet codes.
pub fn validate_outlets(db: &Db, outlets: &Value) -> R<Option<Value>> {
    if outlets.is_null() {
        return Ok(None);
    }
    let Value::Array(items) = outlets else { return Err(crate::error::bad("Outlets must be a list of outlet codes")) };
    if items.iter().any(|code| !code.is_str()) {
        return Err(crate::error::bad("Outlets must be a list of outlet codes"));
    }
    let outlets = crate::common::unique(items.iter().cloned());
    let known = db.strings("SELECT code FROM outlets", params![])?;
    if outlets.iter().any(|code| !known.iter().any(|known| Some(known.as_str()) == code.as_str())) {
        return Err(crate::error::bad("Select existing outlets"));
    }
    Ok(if outlets.is_empty() { None } else { Some(Value::Array(outlets)) })
}

/// None when the account sees every outlet, otherwise the outlet codes it may see.
pub fn allowed(user: &Value) -> Option<Vec<String>> {
    if user.is_null() || user.get("outletScope").map(|scope| scope.as_str() == Some("all")).unwrap_or(true) {
        return None;
    }
    let mut codes: Vec<String> = user.at("outlets").items().iter().map(Value::py_str).collect();
    codes.sort();
    codes.dedup();
    Some(codes)
}

pub fn in_scope(scope: &Option<Vec<String>>, outlet: &Value) -> bool {
    match scope {
        None => true,
        Some(codes) => outlet.as_str().is_some_and(|code| codes.iter().any(|allowed| allowed == code)),
    }
}

pub fn require(user: &Value, outlet: &str) -> R<()> {
    if in_scope(&allowed(user), &Value::str(outlet)) {
        Ok(())
    } else {
        Err(forbidden(DENIED))
    }
}

/// Keep the rows at the account's outlets.
pub fn keep(user: &Value, rows: Vec<Value>, key: &str) -> Vec<Value> {
    let scope = allowed(user);
    if scope.is_none() {
        return rows;
    }
    rows.into_iter().filter(|row| in_scope(&scope, row.at(key))).collect()
}

pub fn record_outlet(db: &Db, route: &str, record_id: i64) -> R<Value> {
    let Some((_, table, column)) = OUTLET_RECORDS.iter().find(|(name, ..)| *name == route) else { return Ok(Value::Null) };
    Ok(db.scalar(&format!("SELECT {column} FROM {table} WHERE id = ?"), params![record_id])?.to_value())
}

fn digit_ids(values: &Value) -> Vec<i64> {
    values.items().iter().filter(|value| crate::common::value_is_digits(value)).filter_map(|value| value.py_str().parse().ok()).collect()
}

/// Refuse a change that would read or write a record at an outlet the account may not see.
pub fn guard_mutation(db: &Db, user: &Value, method: &str, path: &str, payload: &Value) -> R<()> {
    let Some(scope) = allowed(user) else { return Ok(()) };
    let scope = Some(scope);
    let empty = Value::object();
    let payload = if payload.is_object() { payload } else { &empty };
    let parts: Vec<&str> = path.strip_prefix("/api/").unwrap_or(path).split('/').collect();
    let route = parts[0];
    if route == "setup" && parts.get(1) == Some(&"outlets") {
        return Err(forbidden("Outlets are managed by an account that covers every outlet"));
    }
    if route == "users" || route == "roles" {
        return Ok(());
    }
    if path == "/api/equipment/bulk" {
        // Several items: outlets named for a bulk add, or the outlets of the items in a bulk edit.
        let mut named = if method == "POST" { payload.at("outlets").clone() } else { Value::Null };
        let ids = if method == "PATCH" { digit_ids(payload.at("ids")) } else { Vec::new() };
        if !ids.is_empty() {
            let params: Vec<&dyn crate::sqlite::ToSql> = ids.iter().map(|id| id as &dyn crate::sqlite::ToSql).collect();
            named = Value::Array(db.column(&format!("SELECT outlet FROM equipment WHERE id IN ({})", crate::db::marks(ids.len())), &params)?.iter().map(Sql::to_value).collect());
        }
        if named.is_array() && named.items().iter().any(|outlet| !in_scope(&scope, outlet)) {
            return Err(forbidden(DENIED));
        }
        return Ok(());
    }
    let record_id = parts.get(1).filter(|part| crate::common::is_digits(part)).and_then(|part| part.parse::<i64>().ok());
    if let Some(id) = record_id {
        if OUTLET_RECORDS.iter().any(|(name, ..)| *name == route) {
            let outlet = record_outlet(db, route, id)?;
            if !outlet.is_null() && !in_scope(&scope, &outlet) {
                return Err(forbidden(DENIED));
            }
        }
    }
    if payload.has("outlet") && !in_scope(&scope, payload.at("outlet")) {
        return Err(forbidden(DENIED));
    }
    let as_id = |value: &Value| -> i64 { value.or(&Value::Int(0)).py_int().unwrap_or(0) };
    let mut linked = Vec::new();
    if path == "/api/schedules/start" {
        linked.push(record_outlet(db, "schedules", as_id(payload.at("scheduleId")))?);
    } else if path == "/api/inspection-sessions/close" {
        linked.push(record_outlet(db, "inspection-sessions", as_id(payload.at("id")))?);
    } else if path == "/api/work-orders" && method == "POST" {
        linked.push(record_outlet(db, "work-requests", as_id(payload.at("workRequestId")))?);
    } else if path == "/api/work-requests" && method == "POST" {
        let ids = digit_ids(payload.at("findingIds"));
        if !ids.is_empty() {
            let params: Vec<&dyn crate::sqlite::ToSql> = ids.iter().map(|id| id as &dyn crate::sqlite::ToSql).collect();
            linked.extend(db.column(&format!("SELECT outlet FROM findings WHERE id IN ({})", crate::db::marks(ids.len())), &params)?.iter().map(Sql::to_value));
        }
    } else if path == "/api/comments" && method == "POST" && payload.at("recordType").as_str() == Some("work_order") {
        linked.push(record_outlet(db, "work-orders", as_id(payload.at("recordId")))?);
    }
    if linked.iter().any(|outlet| !outlet.is_null() && !in_scope(&scope, outlet)) {
        return Err(forbidden(DENIED));
    }
    // Creating a record without naming an outlet would fall back to the first outlet, which may be another's.
    if method == "POST" && OUTLET_RECORDS.iter().any(|(name, ..)| *name == route) && record_id.is_none() && path == format!("/api/{route}")
        && route != "work-orders" && route != "work-requests" && !payload.at("outlet").truthy()
    {
        return Err(forbidden("Choose one of your outlets"));
    }
    if path == "/api/work-requests" && method == "POST" && !payload.at("findingIds").truthy() && !in_scope(&scope, payload.at("outlet")) {
        return Err(forbidden(DENIED));
    }
    Ok(())
}
