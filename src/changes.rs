//! Changes that wait for approval.
//!
//! Someone allowed to add, edit, and delete a kind of record ("<record>.manage") but not to approve
//! its changes ("<record>.approve") does not change it directly: the request is kept here, the
//! approvers are told, and an approver applies or rejects it. Applying replays the original
//! request as the approver, so it goes through exactly the same checks as a direct change.

use crate::activity::Entry;
use crate::api::{self, ok, Ctx, Reply};
use crate::clock;
use crate::db::{load_or, load_value, save_value, with_db, Db};
use crate::error::{bad, forbidden, status, R};
use crate::json::{obj, Value};
use crate::params;
use crate::permissions::{change_record, list_has, resolve_actions, resolve_permissions, CHANGE_RECORDS};

fn table_of(record: &str) -> &'static str {
    match record {
        "assets" => "equipment",
        "users" => "users",
        "outlets" => "outlets",
        "zones" => "zones",
        "locations" => "locations",
        "departments" => "departments",
        _ => "roles",
    }
}

/// Never kept in a request or shown to an approver.
const HIDDEN: [&str; 2] = ["password_hash", "password"];

/// (record kind, record id) for a change to one of the managed records.
pub fn record_for(method: &str, path: &str) -> Option<(&'static str, Option<i64>)> {
    if !matches!(method, "POST" | "PATCH" | "DELETE") {
        return None;
    }
    let rest = path.strip_prefix("/api/").unwrap_or(path);
    for (record, _, _, routes) in CHANGE_RECORDS {
        for route in routes.iter() {
            if rest == *route || rest == format!("{route}/bulk") {
                return Some((record, None));
            }
            if let Some(tail) = rest.strip_prefix(&format!("{route}/")) {
                if crate::common::is_digits(tail) {
                    return Some((record, tail.parse().ok()));
                }
            }
        }
    }
    None
}

pub fn allowed(user: &Value, record: &str, level: &str) -> bool {
    list_has(user.at("actions"), &format!("{record}.{level}"))
}

/// A short name for the record: what the request or the record calls it.
fn describe(payload: &Value, before: &Value) -> String {
    for source in [payload, before] {
        for key in ["name", "code", "title", "email", "assetId", "asset_id"] {
            if source.at(key).truthy() {
                return source.at(key).py_str();
            }
        }
    }
    String::new()
}

fn snapshot(db: &Db, record: &str, record_id: Option<i64>) -> R<Value> {
    let Some(id) = record_id else { return Ok(Value::Null) };
    let Some(row) = db.row(&format!("SELECT * FROM {} WHERE id = ?", table_of(record)), params![id])? else { return Ok(Value::Null) };
    let mut result = Value::object();
    for (column, value) in row.columns.iter().zip(row.values.iter()) {
        if !HIDDEN.contains(&column.as_str()) && !column.ends_with("_data_id") {
            result.set(column, value.to_value());
        }
    }
    Ok(result)
}

/// Active people whose role or own permissions let them approve this kind of change and open its page.
fn approvers(db: &Db, record: &str, requester: &Value) -> R<Vec<i64>> {
    let page = change_record(record).map(|entry| entry.2).unwrap_or("");
    let mut people = Vec::new();
    for row in db.rows("SELECT id, role, permission_overrides_data_id FROM users WHERE active = 1", params![])? {
        let overrides = if row.get("permission_overrides_data_id").is_null() { Value::Null } else { load_value(db, row.get("permission_overrides_data_id"))? };
        let (pages, _) = resolve_permissions(db, &row.str("role"), &overrides)?;
        if requester.as_i64() != Some(row.int("id")) && list_has(&pages, page) && list_has(&resolve_actions(db, &row.str("role"), &overrides)?, &format!("{record}.approve")) {
            people.push(row.int("id"));
        }
    }
    Ok(people)
}

/// Keep the change for approval instead of making it.
pub fn hold(user: &Value, method: &str, path: &str, payload: &Value, record: &str, record_id: Option<i64>) -> R<Value> {
    let mut kept = Value::object();
    for (key, value) in payload.entries() {
        if !HIDDEN.contains(&key.as_str()) {
            kept.set(key, value.clone());
        }
    }
    let action = match method {
        "POST" => "add",
        "PATCH" => "edit",
        _ => "delete",
    };
    let now = clock::now_ms();
    let label = change_record(record).map(|entry| entry.1).unwrap_or(record);
    with_db(|db| {
        let before = snapshot(db, record, record_id)?;
        if record_id.is_some() && before.is_null() {
            return Err(status(404, "That record no longer exists"));
        }
        let fields = kept.at("fields");
        let mut name = describe(if fields.truthy() { fields } else { &kept }, &before);
        if path.ends_with("/bulk") {
            name = match kept.at("ids") {
                Value::Array(ids) => format!("{name} ({} items)", ids.len()),
                _ => format!("{name} (bulk)"),
            };
        }
        let noun = label.split(" (").next().unwrap_or(label).trim_end_matches('s').to_lowercase();
        let mut verb = action.to_string();
        verb[..1].make_ascii_uppercase();
        let summary = format!("{verb} {noun} {name}").trim().to_string();
        let before_value = if before.is_null() { Value::object() } else { before };
        let change_id = db.insert(
            "INSERT INTO change_requests (record, action, record_id, method, path, payload_data_id, before_data_id, summary, status, requested_by, requested_by_user_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'Pending', ?, ?, ?)",
            params![record, action, record_id, method, path, save_value(db, &kept)?, save_value(db, &before_value)?, summary, user.get("name").cloned().unwrap_or_else(|| "".into()), user.at("id"), now],
        )?;
        let by = if user.at("name").truthy() { user.at("name").py_str() } else { "someone".into() };
        for approver in approvers(db, record, user.at("id"))? {
            db.exec(
                "INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES ('Change waiting for your approval', ?, 'In-App', 'Unread', 'change', ?, ?, ?)",
                params![format!("{summary}, requested by {by}"), change_id, now, approver],
            )?;
        }
        Entry::new(user, "change_requested", "change", change_id).reference(format!("CHG-{change_id:05}")).detail(summary.clone()).at(now).log(db)?;
        Ok(obj([
            ("ok", true.into()),
            ("pending", true.into()),
            ("changeId", Value::Int(change_id)),
            ("message", Value::str(format!("Sent for approval: {summary}. It is applied once an approver accepts it; you can follow it under Approvals."))),
        ]))
    })
}

/// Changes this person may decide, and the ones they asked for.
pub fn change_items(user: &Value) -> R<Value> {
    let decides: Vec<&str> = CHANGE_RECORDS.iter().map(|record| record.0).filter(|record| allowed(user, record, "approve")).collect();
    with_db(|db| {
        let mut items = Vec::new();
        for row in db.rows("SELECT * FROM change_requests ORDER BY CASE status WHEN 'Pending' THEN 0 ELSE 1 END, created_at DESC LIMIT 300", params![])? {
            let mine = row.value("requested_by_user_id").py_eq(user.at("id")) && !row.get("requested_by_user_id").is_null();
            let record = row.str("record");
            let decide = decides.contains(&record.as_str()) && !mine;
            if !(mine || decide) {
                continue;
            }
            let mut item = row.to_object();
            item.remove("payload_data_id");
            item.remove("before_data_id");
            let payload = load_value(db, row.get("payload_data_id"))?;
            let before = load_or(db, row.get("before_data_id"), Value::object())?;
            item.set("payload", if payload.truthy() { payload } else { Value::object() });
            item.set("before", if before.truthy() { before } else { Value::object() });
            item.set("mine", mine);
            item.set("canDecide", decide && row.str("status") == "Pending");
            item.set("recordLabel", change_record(&record).map(|entry| entry.1).unwrap_or(""));
            items.push(item);
        }
        Ok(obj([("items", Value::Array(items))]))
    })
}

pub fn decide(user: &Value, change_id: i64, decision: &Value, remark: &Value) -> R<Value> {
    let decision = decision.as_str().unwrap_or("");
    if decision != "approve" && decision != "reject" {
        return Err(bad("Choose approve or reject"));
    }
    let (row, payload) = with_db(|db| {
        let row = db.row("SELECT * FROM change_requests WHERE id = ?", params![change_id])?.ok_or_else(|| status(404, "Change not found"))?;
        let payload = load_value(db, row.get("payload_data_id"))?;
        Ok((row, if payload.truthy() { payload } else { Value::object() }))
    })?;
    if row.str("status") != "Pending" {
        return Err(status(409, "This change has already been decided"));
    }
    if row.value("requested_by_user_id").py_eq(user.at("id")) {
        return Err(forbidden("Someone else must decide on your own change"));
    }
    let record = row.str("record");
    if !allowed(user, &record, "approve") {
        let label = change_record(&record).map(|entry| entry.1).unwrap_or("");
        return Err(forbidden(format!("You cannot approve {} changes", label.to_lowercase())));
    }
    let now = clock::now_ms();
    if decision == "approve" {
        let method = row.str("method");
        let path = row.str("path");
        let path = path.split('?').next().unwrap_or("").to_string();
        with_db(|db| crate::accounts::guard_mutation(db, user, &method, &path, &payload))?;
        let replay = Ctx::replay(user.clone(), &method, &path);
        let (code, body) = match api::dispatch(&replay, &payload) {
            Ok(Some(reply)) => api::reply_parts(reply),
            Ok(None) => return Err(status(410, "This change can no longer be applied")),
            Err(error) => {
                let (code, message) = api::error_parts(&error);
                (code, obj([("error", message.into())]))
            }
        };
        if code >= 400 {
            // Left pending, so the approver can see why and reject it instead.
            let message = if body.at("error").truthy() { body.at("error").py_str() } else { "The change could not be applied".into() };
            return Err(status(code, message));
        }
    }
    let decided = if decision == "approve" { "Approved" } else { "Rejected" };
    let remark = remark.text().trim().to_string();
    with_db(|db| {
        db.exec(
            "UPDATE change_requests SET status = ?, decided_by = ?, decided_at = ?, remark = ? WHERE id = ? AND status = 'Pending'",
            params![decided, user.get("name").cloned().unwrap_or_else(|| "".into()), now, remark, change_id],
        )?;
        if row.get("requested_by_user_id").truthy() {
            let mut message = format!("{}: {} by {}", row.str("summary"), decided.to_lowercase(), user.at("name").py_str());
            if !remark.is_empty() {
                message.push_str(&format!(" ({remark})"));
            }
            db.exec(
                "INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?, ?, 'In-App', 'Unread', 'change', ?, ?, ?)",
                params![format!("Your change was {}", decided.to_lowercase()), message, change_id, now, row.int("requested_by_user_id")],
            )?;
        }
        let action = format!("change_{}", decided.to_lowercase());
        Entry::new(user, &action, "change", change_id).reference(format!("CHG-{change_id:05}")).detail(row.str("summary")).started(row.opt_int("created_at")).at(now).log(db)
    })?;
    Ok(obj([("ok", true.into()), ("status", decided.into())]))
}

pub fn patch_change(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let change_id = crate::id_or_reply!(ctx);
    let remark = payload.get("remark").cloned().unwrap_or_else(|| "".into());
    ok(decide(&ctx.user()?, change_id, payload.at("decision"), &remark)?)
}
