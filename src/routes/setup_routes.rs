//! Departments, priority levels, audit types, and settings.

use crate::accounts::{is_company_admin_user, is_super_user};
use crate::api::{json_status, ok_true, send_error, Ctx, Reply};
use crate::audits::validate_scoring;
use crate::clock;
use crate::config::{AUDIT_STYLES, THEME_CHOICES};
use crate::db::{all_settings, save_value, with_db};
use crate::error::{bad, R};
use crate::json::{obj, Value};
use crate::params;

fn department_code(payload: &Value) -> String {
    payload.at("code").or(&Value::str("Department")).py_str().to_uppercase()
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> Value {
    payload.get(key).cloned().unwrap_or_else(|| Value::str(fallback))
}

pub fn post_department(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    with_db(|db| {
        db.exec(
            "INSERT OR REPLACE INTO departments (code, description, responsibilities, created_at) VALUES (?, ?, ?, ?)",
            params![department_code(payload), or_default(payload, "description", ""), or_default(payload, "responsibilities", ""), now],
        )?;
        ok_true()
    })
}

fn validate_theme(key: &str, value: &Value) -> R<()> {
    let name = key.strip_prefix("theme.").unwrap_or(key);
    if let Some((_, choices)) = THEME_CHOICES.iter().find(|(choice, _)| *choice == name) {
        if !value.as_str().is_some_and(|text| choices.contains(&text)) {
            return Err(bad(format!("Choose a valid theme {name}")));
        }
        return Ok(());
    }
    match name {
        "accent" => {
            let valid = value.as_str() == Some("") || value.as_str().is_some_and(|text| text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit()));
            if !valid {
                return Err(bad("The accent colour must look like #1e99b4"));
            }
        }
        "userChoice" => {
            if value.as_bool().is_none() {
                return Err(bad("Choose whether users may pick light or dark"));
            }
        }
        _ => return Err(bad("Unknown theme setting")),
    }
    Ok(())
}

fn active_flag(payload: &Value) -> i64 {
    payload.get("active").map(Value::truthy).unwrap_or(true) as i64
}

fn due_days(payload: &Value) -> R<i64> {
    payload.at("dueDays").or(&Value::Int(0)).py_int().map_err(bad)
}

pub fn post_priority(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    with_db(|db| {
        db.exec(
            "INSERT OR REPLACE INTO priority_levels (name, classification, due_days, active, created_at) VALUES (?, ?, ?, ?, ?)",
            params![or_default(payload, "name", "Priority"), or_default(payload, "classification", "Priority"), due_days(payload)?, active_flag(payload), now],
        )?;
        ok_true()
    })
}

fn audit_style(payload: &Value) -> R<String> {
    let style = payload.at("style").or(&Value::str("Detailed")).py_str();
    if !AUDIT_STYLES.contains(&style.as_str()) {
        return Err(bad("Choose Detailed or Casual"));
    }
    Ok(style)
}

pub fn post_audit_type(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    with_db(|db| {
        db.exec(
            "INSERT OR REPLACE INTO audit_types (name, description, active, created_at, style) VALUES (?, ?, ?, ?, ?)",
            params![or_default(payload, "name", "Standard"), or_default(payload, "description", ""), active_flag(payload), now, audit_style(payload)?],
        )?;
        ok_true()
    })
}

pub fn post_settings(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    if !is_company_admin_user(&user) {
        return json_status(403, obj([("ok", false.into()), ("error", "Admin access required".into())]));
    }
    let incoming = payload.at("settings");
    let mut chosen = Value::object();
    for (key, value) in incoming.entries() {
        if is_super_user(&user) || key == "system.findingsEnabled" || key == "system.requirePhotoEveryAsset" {
            chosen.set(key, value.clone());
        }
    }
    for (key, value) in chosen.entries() {
        if key.starts_with("theme.") {
            validate_theme(key, value)?;
        }
    }
    with_db(|db| {
        if chosen.entries().iter().any(|(key, _)| key.starts_with("scoring.")) {
            let saved = all_settings(db, Some("scoring.%"))?;
            validate_scoring(&saved.merged(&chosen))?;
        }
        for (key, value) in chosen.entries() {
            if key == "report.logoUrl" && value.truthy() {
                crate::media::normalize(db, &obj([("url", value.clone())]))?;
            }
            db.exec("INSERT OR REPLACE INTO app_settings (key, value_data_id) VALUES (?, ?)", params![key, save_value(db, value)?])?;
        }
        ok_true()
    })
}

pub fn patch_priority(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        let changed = db.exec(
            "UPDATE priority_levels SET name = ?, classification = ?, due_days = ?, active = ? WHERE id = ?",
            params![or_default(payload, "name", "Priority"), or_default(payload, "classification", "Priority"), due_days(payload)?, active_flag(payload), record_id],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn patch_audit_type(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        let changed = db.exec(
            "UPDATE audit_types SET name = ?, description = ?, active = ?, style = ? WHERE id = ?",
            params![or_default(payload, "name", "Standard"), or_default(payload, "description", ""), active_flag(payload), audit_style(payload)?, record_id],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn patch_department(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        let changed = db.exec(
            "UPDATE departments SET code = ?, description = ?, responsibilities = ? WHERE id = ?",
            params![department_code(payload), or_default(payload, "description", ""), or_default(payload, "responsibilities", ""), record_id],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

fn delete_from(ctx: &Ctx, table: &str) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        if db.exec(&format!("DELETE FROM {table} WHERE id = ?"), params![record_id])? == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn delete_department(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    delete_from(ctx, "departments")
}

pub fn delete_priority(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    delete_from(ctx, "priority_levels")
}

pub fn delete_audit_type(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    delete_from(ctx, "audit_types")
}
