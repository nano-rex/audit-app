//! Work requests, work orders, comments, and notifications.

use crate::activity::Entry;
use crate::api::{error_json, ok, ok_true, send_error, Ctx, Reply};
use crate::audits::validate_update;
use crate::clock;
use crate::common::{priority_due_date, sla_status, work_order_ref};
use crate::db::{data_value, first_department, first_outlet, with_db, Db};
use crate::error::{bad, status, R};
use crate::json::{obj, Value};
use crate::notify::notify_work_order;
use crate::params;
use crate::work::{claim_for_work_order, create_work_request, decline_work_request, edit_work_request, link_work_order, sync_finding_from_work_order};

fn or_default(payload: &Value, key: &str, fallback: &str) -> Value {
    payload.get(key).cloned().unwrap_or_else(|| Value::str(fallback))
}

fn cost(payload: &Value) -> R<f64> {
    payload.at("cost").or(&Value::Int(0)).py_float().map_err(bad)
}

pub fn post_work_order(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    let user = ctx.user()?;
    let request_id = payload.at("workRequestId").or(&Value::Int(0)).py_int().map_err(bad)?;
    if request_id == 0 {
        return Err(status(400, "Create a work order from a work request"));
    }
    let payload = validate_update(payload, None, &user)?;
    with_db(|db| {
        db.begin_immediate()?;
        let request = claim_for_work_order(db, &user, request_id)?;
        let outlet = if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(first_outlet(db)?) };
        let request_type = if payload.at("requestType").truthy() { payload.at("requestType").clone() } else { Value::str(first_department(db)?) };
        let order_status = or_default(&payload, "status", "Assigned");
        let priority = or_default(&payload, "priority", "Medium");
        let due_date = if payload.at("dueDate").truthy() { payload.at("dueDate").clone() } else { Value::str(priority_due_date(db, &priority, now)?) };
        let current = sla_status(&order_status.py_str(), &due_date);
        let unit = or_default(&payload, "businessUnit", "Ottotree");
        let order_id = db.insert(
            "INSERT INTO work_orders (business_unit, outlet, zone, request_type, category, priority, title, description, assignee, pic, status, closed_at, due_date, vendor, sla_status, cost, outlet_confirmed, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
            params![
                unit,
                outlet,
                or_default(&payload, "zone", "Unassigned"),
                request_type,
                payload.at("category").or(&Value::str("")),
                priority,
                or_default(&payload, "title", "Work order"),
                or_default(&payload, "description", ""),
                or_default(&payload, "assignee", "Technical Support"),
                or_default(&payload, "pic", ""),
                order_status,
                payload.at("closedAt"),
                due_date,
                or_default(&payload, "vendor", ""),
                current,
                cost(&payload)?,
                now
            ],
        )?;
        db.exec("UPDATE work_orders SET work_order_ref = ? WHERE id = ?", params![work_order_ref(order_id), order_id])?;
        save_finding_details(db, order_id, &payload)?;
        db.exec(
            "UPDATE work_orders SET source_audit_id = ?, images_data_id = COALESCE(images_data_id, ?) WHERE id = ?",
            params![request.at("audit_id"), request.at("images_data_id"), order_id],
        )?;
        link_work_order(db, request_id, order_id, &order_status, &or_default(&payload, "pic", ""))?;
        Entry::new(&user, "order_created", "work_order", order_id)
            .reference(work_order_ref(order_id))
            .outlet(outlet.text())
            .started(request.at("created_at").as_i64())
            .detail(request.at("request_ref").text())
            .unit(unit.text())
            .at(now)
            .log(db)?;
        notify_work_order(db, order_id, &order_status.py_str())?;
        ok(obj([("ok", true.into()), ("id", Value::Int(order_id))]))
    })
}

pub fn post_work_request(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let payload = if payload.is_object() { payload.clone() } else { Value::object() };
    ok(create_work_request(&ctx.user()?, &payload)?)
}

pub fn patch_work_request(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    match payload.at("action").as_str() {
        Some("edit") => ok(edit_work_request(&ctx.user()?, record_id, payload)?),
        Some("decline") => ok(decline_work_request(&ctx.user()?, record_id, payload.at("remark"))?),
        _ => Err(status(400, "Unknown work request action")),
    }
}

pub fn post_comment(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    let user = ctx.user()?;
    with_db(|db| {
        db.exec(
            "INSERT INTO comments (record_type, record_id, comment, author, created_at) VALUES (?, ?, ?, ?, ?)",
            params![
                or_default(payload, "recordType", "general"),
                payload.at("recordId").or(&Value::Int(0)).py_int().map_err(bad)?,
                or_default(payload, "comment", ""),
                user.get("name").cloned().unwrap_or_else(|| "".into()),
                now
            ],
        )?;
        ok_true()
    })
}

pub fn patch_notification(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let user_id = ctx.user()?.at("id").as_i64().unwrap_or(0);
    if ctx.tail() == "all" {
        let updated = with_db(|db| db.exec("UPDATE notifications SET status = 'Read', read_at = ? WHERE status = 'Unread' AND recipient_user_id = ?", params![clock::now_ms(), user_id]))?;
        return ok(obj([("ok", true.into()), ("updated", Value::Int(updated))]));
    }
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        let changed = db.exec(
            "UPDATE notifications SET status = 'Read', read_at = ? WHERE id = ? AND (recipient_user_id IS NULL OR recipient_user_id = ?)",
            params![clock::now_ms(), record_id, user_id],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn patch_work_order(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let item_id = crate::id_or_reply!(ctx);
    let user = ctx.user()?;
    with_db(|db| {
        db.begin_immediate()?;
        let Some(existing) = db.row("SELECT * FROM work_orders WHERE id = ?", params![item_id])? else { return error_json(404, "Work order not found") };
        let existing = existing.to_object();
        let payload = validate_update(payload, Some(&existing), &user)?;
        let order_status = payload.at("status").clone();
        let priority = or_default(&payload, "priority", "Medium");
        let due_date = if payload.at("dueDate").truthy() { payload.at("dueDate").clone() } else { Value::str(priority_due_date(db, &priority, clock::now_ms())?) };
        let current = sla_status(&order_status.py_str(), &due_date);
        let outlet = if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(first_outlet(db)?) };
        let request_type = if payload.at("requestType").truthy() { payload.at("requestType").clone() } else { Value::str(first_department(db)?) };
        let changed = db.exec(
            "UPDATE work_orders SET business_unit = ?, outlet = ?, zone = ?, request_type = ?, category = ?, priority = ?, title = ?, description = ?, assignee = ?, pic = ?, status = ?, closed_at = ?, due_date = ?, vendor = ?, sla_status = ?, cost = ? WHERE id = ?",
            params![
                or_default(&payload, "businessUnit", "Ottotree"),
                outlet,
                or_default(&payload, "zone", "Unassigned"),
                request_type,
                payload.at("category").or(&Value::str("")),
                priority,
                or_default(&payload, "title", "Work order"),
                or_default(&payload, "description", ""),
                or_default(&payload, "assignee", "Technical Support"),
                or_default(&payload, "pic", ""),
                order_status,
                payload.at("closedAt"),
                due_date,
                or_default(&payload, "vendor", ""),
                current,
                cost(&payload)?,
                item_id
            ],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        save_finding_details(db, item_id, &payload)?;
        sync_finding_from_work_order(db, item_id)?;
        let previous = existing.at("status").py_str();
        let now_status = order_status.py_str();
        if now_status != previous {
            let message = format!("Status changed from {previous} to {now_status}");
            let closing = now_status == "Closed";
            Entry::new(&user, if closing { "order_closed" } else { "order_status" }, "work_order", item_id)
                .reference(existing.at("work_order_ref").text())
                .outlet(existing.at("outlet").text())
                .started(if closing { existing.at("created_at").as_i64() } else { None })
                .detail(message.clone())
                .unit(existing.at("business_unit").text())
                .log(db)?;
            db.exec(
                "INSERT INTO comments(record_type, record_id, comment, author, created_at, system_generated) VALUES ('work_order', ?, ?, ?, ?, 1)",
                params![item_id, message, user.at("name"), clock::now_ms()],
            )?;
            notify_work_order(db, item_id, &now_status)?;
        } else if [("pic", "pic"), ("assignee", "assignee"), ("requestType", "request_type")].iter().any(|(key, column)| {
            let sent = payload.get(key).cloned().unwrap_or_else(|| "".into());
            let stored = existing.at(column).or(&Value::str("")).clone();
            !sent.py_eq(&stored)
        }) {
            notify_work_order(db, item_id, "Assigned")?;
        }
        ok_true()
    })
}

/// Retain evidence and diagnostic fields when updating a work order.
fn save_finding_details(db: &Db, work_order_id: i64, payload: &Value) -> R<()> {
    for (key, column) in [("cause", "cause"), ("recommendation", "recommendation"), ("requiredAction", "required_action"), ("images", "images_data_id")] {
        let Some(sent) = payload.get(key) else { continue };
        let value = if key == "images" { Value::Int(data_value(db, sent, Value::array())?) } else { sent.clone() };
        db.exec(&format!("UPDATE work_orders SET {column} = ? WHERE id = ?"), params![value, work_order_id])?;
        db.exec(&format!("UPDATE findings SET {column} = ? WHERE id = (SELECT source_finding_id FROM work_orders WHERE id = ?)"), params![value, work_order_id])?;
    }
    Ok(())
}

pub fn delete_notification(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    let user_id = ctx.user()?.at("id").as_i64().unwrap_or(0);
    with_db(|db| {
        if db.exec("DELETE FROM notifications WHERE id = ? AND (recipient_user_id IS NULL OR recipient_user_id = ?)", params![record_id, user_id])? == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn delete_comment(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        if db.row("SELECT system_generated FROM comments WHERE id = ?", params![record_id])?.is_some_and(|row| row.int("system_generated") != 0) {
            return Err(status(409, "Workflow history cannot be deleted"));
        }
        if db.exec("DELETE FROM comments WHERE id = ?", params![record_id])? == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn delete_work_order(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    if ctx.user()?.at("role").as_str() == Some("Department/PIC") {
        return Err(status(403, "Department/PIC users cannot delete work orders"));
    }
    with_db(|db| {
        db.begin_immediate()?;
        let row = db.row("SELECT source_finding_id, work_request_id, status FROM work_orders WHERE id = ?", params![record_id])?;
        if let Some(row) = &row {
            if row.get("source_finding_id").truthy() || row.str("status") == "Closed" {
                return Err(status(409, "Audit-linked or closed work orders must be retained"));
            }
        }
        let changed = db.exec("DELETE FROM work_orders WHERE id = ?", params![record_id])?;
        if let Some(request) = row.as_ref().and_then(|row| row.opt_int("work_request_id")).filter(|id| *id != 0) {
            let now = clock::now_ms();
            db.exec("UPDATE work_requests SET status = 'Open', work_order_id = NULL, updated_at = ? WHERE id = ?", params![now, request])?;
            db.exec("UPDATE findings SET status = 'Requested', updated_at = ? WHERE work_request_id = ?", params![now, request])?;
        }
        if changed == 0 {
            return send_error(404);
        }
        ok_true()
    })
}
