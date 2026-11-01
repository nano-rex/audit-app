//! Inspections and their scheduled visits: saving, completing, signing, and closing audits.

use crate::activity::{last_time, Entry};
use crate::api::{error_json, ok, ok_true, send_error, Ctx, Reply};
use crate::audits::{allocate_reference, validate_metadata};
use crate::clock;
use crate::common::{inspection_name, inspection_progress, normalize_audit_date};
use crate::db::{first_outlet, insert_record, load_or, load_value, save_value, with_db, Db};
use crate::error::{bad, forbidden, status, R};
use crate::inspections::{complete_item_details, finalize_inspection, visit_locations_of, visit_scope_of};
use crate::json::{obj, Value};
use crate::locations::{default_audit_type, visit_audit_type, visit_scope, visit_terms};
use crate::notify::{assignees_of, notify_inspection, save_assignment, validate_assignees};
use crate::params;
use crate::permissions::{authorize_inspection_update, list_has};

/// Add newly uploaded inspection evidence to each asset's photo library.
fn append_inspection_photos(db: &Db, items: &Value, now: i64) -> R<()> {
    for item in items.items() {
        let Ok(equipment_id) = item.at("equipmentId").or(&Value::Int(0)).py_int() else { continue };
        let images = item.at("images");
        if equipment_id == 0 || !images.is_array() || images.items().is_empty() {
            continue;
        }
        let Some(row) = db.row("SELECT photos_data_id FROM equipment WHERE id = ?", params![equipment_id])? else { continue };
        let existing = load_or(db, row.get("photos_data_id"), Value::array())?;
        let mut existing: Vec<Value> = if existing.is_array() { existing.items().to_vec() } else { Vec::new() };
        let key_of = |image: &Value| -> Value { image.at("id").or(image.at("url")).clone() };
        let mut known: Vec<Value> = existing.iter().filter(|image| image.is_object()).map(key_of).collect();
        let mut added = false;
        for image in images.items() {
            if !image.is_object() {
                continue;
            }
            let key = key_of(image);
            if !key.truthy() || known.iter().any(|seen| seen.py_eq(&key)) {
                continue;
            }
            let mut addition = image.clone();
            if !image.at("uploadedAt").truthy() {
                addition.set("uploadedAt", clock::local_from_ms(now).iso_naive());
            }
            existing.push(addition);
            known.push(key);
            added = true;
        }
        if added {
            db.exec("UPDATE equipment SET photos_data_id = ? WHERE id = ?", params![save_value(db, &Value::Array(existing))?, equipment_id])?;
        }
    }
    Ok(())
}

fn save_session_metadata(db: &Db, session_id: i64, payload: &Value) -> R<()> {
    for (key, column) in [("auditTime", "audit_time"), ("auditType", "audit_type"), ("remarks", "remarks")] {
        if let Some(value) = payload.get(key) {
            db.exec(&format!("UPDATE inspection_sessions SET {column} = ? WHERE id = ?"), params![value, session_id])?;
        }
    }
    Ok(())
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> Value {
    payload.get(key).cloned().unwrap_or_else(|| Value::str(fallback))
}

pub fn post_inspection(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    let now = clock::now_ms();
    with_db(|db| {
        let (mut payload, _) = authorize_inspection_update(db, &user, payload, None)?;
        let default_outlet = first_outlet(db)?;
        let (items, _) = complete_item_details(db, payload.at("items").or(&Value::array()))?;
        payload.set("items", items.clone());
        let progress = inspection_progress(&items);
        let complete = payload.at("complete").truthy();
        if complete && progress < 100 {
            return error_json(409, "Inspection is incomplete");
        }
        let state = if complete && progress == 100 { "Completed" } else { "Draft" };
        let outlet = if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(&default_outlet) };
        let unit = or_default(&payload, "businessUnit", "Ottotree");
        let audit_date = normalize_audit_date(payload.at("auditDate"));
        let signatures = payload.at("signatures").or(&Value::object()).clone();
        let session_id = db.insert(
            "INSERT INTO inspection_sessions (business_unit, outlet, zone, audit_date, auditor, items_data_id, progress, status, signatures_data_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![unit, outlet, or_default(&payload, "zone", "Unassigned"), audit_date, or_default(&payload, "auditor", "Unnamed Inspector"), save_value(db, &items)?, progress, state, save_value(db, &signatures)?, now, now],
        )?;
        let session_name = allocate_reference(db, &Value::str(&audit_date), &outlet.text())?;
        db.exec("UPDATE inspection_sessions SET inspection_name = ?, audit_ref = ?, owner_user_id = ? WHERE id = ?", params![session_name, session_name, user.at("id"), session_id])?;
        save_session_metadata(db, session_id, &payload)?;
        append_inspection_photos(db, &items, now)?;
        let reference = db.scalar("SELECT audit_ref FROM inspection_sessions WHERE id = ?", params![session_id])?.text();
        let entry = |action: &'static str| Entry::new(&user, action, "inspection", session_id).reference(reference.clone()).outlet(outlet.text()).unit(unit.text()).at(now);
        entry("audit_started").log(db)?;
        let mut audit_id = Value::Null;
        if state == "Completed" {
            audit_id = Value::Int(finalize_inspection(db, session_id, &payload, now)?);
            entry("audit_completed").started(Some(now)).log(db)?;
        }
        notify_inspection(db, session_id, &user, &payload, None, true)?;
        ok(obj([
            ("ok", true.into()),
            ("id", Value::Int(session_id)),
            ("inspectionName", Value::str(session_name)),
            ("progress", Value::Int(progress)),
            ("status", state.into()),
            ("auditId", audit_id),
        ]))
    })
}

pub fn post_schedule(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    let user = ctx.user()?;
    with_db(|db| {
        let outlet = if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(first_outlet(db)?) };
        let (locations, scope, label) = visit_scope(db, &outlet, payload)?;
        let (priority, due_date) = visit_terms(db, payload)?;
        let audit_type = visit_audit_type(db, payload)?;
        let scheduled = or_default(payload, "scheduledDate", "Today");
        let unit = or_default(payload, "businessUnit", "Ottotree");
        // A new visit always starts as Pending; starting and completing the audit move it on.
        let schedule_id = db.insert(
            "INSERT INTO schedules (business_unit, outlet, zone, locations_data_id, scope_data_id, priority, due_date, audit_type, scheduled_date, auditor, remarks, status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'Pending', ?)",
            params![unit, outlet, label, save_value(db, &locations)?, save_value(db, &scope)?, priority, due_date, audit_type, scheduled, or_default(payload, "auditor", "Unassigned"), or_default(payload, "remarks", ""), now],
        )?;
        if payload.has("assignees") {
            let people = validate_assignees(db, &outlet.text(), payload.at("assignees"))?;
            save_assignment(db, &user, schedule_id, &outlet.text(), &scheduled, &people, &[], &unit.text())?;
        }
        ok(obj([("ok", true.into()), ("id", Value::Int(schedule_id)), ("scheduleRef", Value::str(format!("SCH-{schedule_id:05}")))]))
    })
}

pub fn start_schedule(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    let schedule_id = payload.at("scheduleId").or(&Value::Int(0)).py_int().map_err(bad)?;
    let now = clock::now_ms();
    with_db(|db| {
        db.begin_immediate()?;
        let Some(schedule) = db.row("SELECT * FROM schedules WHERE id = ?", params![schedule_id])? else { return error_json(404, "Schedule not found") };
        let session_id = match db.scalar("SELECT id FROM inspection_sessions WHERE schedule_id = ?", params![schedule_id])?.as_i64() {
            Some(id) => id,
            None => {
                if !list_has(user.at("inspectionPermissions"), "auditor") {
                    return Err(forbidden("An auditor must start this scheduled inspection first"));
                }
                let audit_type = if schedule.get("audit_type").truthy() { schedule.value("audit_type") } else { Value::str(default_audit_type(db)?) };
                let session_id = insert_record(db, "inspection_sessions", &[
                    ("business_unit", schedule.value("business_unit")),
                    ("outlet", schedule.value("outlet")),
                    ("zone", schedule.value("zone")),
                    // The audit covers the locations the visit was scheduled for, and no others.
                    ("locations_data_id", Value::Int(save_value(db, &visit_locations_of(db, schedule.get("locations_data_id"))?)?)),
                    // And, when the visit was for particular assets, those assets only.
                    ("scope_data_id", Value::Int(save_value(db, &visit_scope_of(db, schedule.get("scope_data_id"))?)?)),
                    ("audit_date", Value::str(normalize_audit_date(&schedule.value("scheduled_date")))),
                    ("auditor", user.at("name").clone()),
                    ("items_data_id", Value::Int(save_value(db, &Value::array())?)),
                    ("signatures_data_id", Value::Int(save_value(db, &Value::object())?)),
                    ("progress", Value::Int(0)),
                    ("status", "Draft".into()),
                    ("created_at", Value::Int(now)),
                    ("updated_at", Value::Int(now)),
                    ("owner_user_id", user.at("id").clone()),
                    ("schedule_id", Value::Int(schedule_id)),
                    ("audit_ref", Value::str(allocate_reference(db, &schedule.value("scheduled_date"), &schedule.str("outlet"))?)),
                    ("audit_time", Value::str(clock::local_now().hm())),
                    // The visit's audit type, which also sets how it is graded (Detailed or Casual).
                    ("audit_type", audit_type),
                    ("remarks", Value::str(schedule.str("remarks"))),
                ])?;
                db.exec("UPDATE inspection_sessions SET inspection_name = audit_ref WHERE id = ?", params![session_id])?;
                db.exec("UPDATE schedules SET status = 'In Progress' WHERE id = ?", params![schedule_id])?;
                let started = db.scalar("SELECT audit_ref FROM inspection_sessions WHERE id = ?", params![session_id])?.text();
                Entry::new(&user, "audit_started", "inspection", session_id).reference(started).outlet(schedule.str("outlet")).unit(schedule.str("business_unit")).at(now).log(db)?;
                session_id
            }
        };
        ok(obj([("ok", true.into()), ("id", Value::Int(session_id)), ("scheduleId", Value::Int(schedule_id)), ("scheduleRef", Value::str(format!("SCH-{schedule_id:05}")))]))
    })
}

/// Create an immediately available scheduled draft without fabricating a result.
pub fn start_audit(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    let now = clock::now_ms();
    with_db(|db| {
        let (payload, _) = authorize_inspection_update(db, &user, payload, None)?;
        db.begin_immediate()?;
        for key in ["outlet", "auditDate", "auditTime", "auditType"] {
            if !payload.at(key).truthy() {
                return Err(bad("Outlet, audit date, time, and type are required"));
            }
        }
        validate_metadata(db, &payload, None)?;
        let unit = or_default(&payload, "businessUnit", "Ottotree");
        let remarks = or_default(&payload, "remarks", "");
        let schedule_id = insert_record(db, "schedules", &[
            ("business_unit", unit.clone()),
            ("outlet", payload.at("outlet").clone()),
            ("zone", "All Locations".into()),
            ("scheduled_date", payload.at("auditDate").clone()),
            ("auditor", user.at("name").clone()),
            ("remarks", remarks.clone()),
            ("status", "In Progress".into()),
            ("created_at", Value::Int(now)),
        ])?;
        let reference = allocate_reference(db, payload.at("auditDate"), &payload.at("outlet").text())?;
        let session_id = insert_record(db, "inspection_sessions", &[
            ("business_unit", unit.clone()),
            ("outlet", payload.at("outlet").clone()),
            ("zone", "All Locations".into()),
            ("audit_date", payload.at("auditDate").clone()),
            ("audit_time", payload.at("auditTime").clone()),
            ("audit_type", payload.at("auditType").clone()),
            ("remarks", remarks),
            ("auditor", user.at("name").clone()),
            ("audit_ref", Value::str(&reference)),
            ("items_data_id", Value::Int(save_value(db, &Value::array())?)),
            ("signatures_data_id", Value::Int(save_value(db, &Value::object())?)),
            ("progress", Value::Int(0)),
            ("status", "Draft".into()),
            ("created_at", Value::Int(now)),
            ("updated_at", Value::Int(now)),
            ("owner_user_id", user.at("id").clone()),
            ("schedule_id", Value::Int(schedule_id)),
        ])?;
        db.exec("UPDATE inspection_sessions SET inspection_name = ? WHERE id = ?", params![reference, session_id])?;
        Entry::new(&user, "audit_started", "inspection", session_id).reference(reference.clone()).outlet(payload.at("outlet").text()).unit(unit.text()).at(now).log(db)?;
        ok(obj([("ok", true.into()), ("id", Value::Int(session_id)), ("auditRef", Value::str(reference)), ("scheduleId", Value::Int(schedule_id))]))
    })
}

pub fn patch_inspection(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let session_id = crate::id_or_reply!(ctx);
    let now = clock::now_ms();
    let user = ctx.user()?;
    with_db(|db| {
        // Serialize the status check and update to prevent duplicate finalization.
        db.begin_immediate()?;
        let Some(row) = db.row("SELECT * FROM inspection_sessions WHERE id = ?", params![session_id])? else { return error_json(404, "Inspection not found") };
        if row.get("closed_at").truthy() {
            return error_json(409, "Closed audits cannot be changed");
        }
        let mut existing = row.to_object();
        existing.set("signatures_data_id", load_or(db, row.get("signatures_data_id"), Value::object())?);
        let (mut payload, changed) = authorize_inspection_update(db, &user, payload, Some(&existing))?;
        let previous_signatures = existing.at("signatures_data_id").clone();
        if row.str("status") == "Completed" {
            if changed || payload.at("complete").as_bool() == Some(false) {
                return error_json(409, "Completed inspection items cannot be changed");
            }
            let signatures = payload.get("signatures").cloned().unwrap_or_else(Value::object);
            if signatures.py_eq(&previous_signatures) {
                return error_json(409, "Inspection is already completed");
            }
            db.exec("UPDATE inspection_sessions SET signatures_data_id = ?, updated_at = ? WHERE id = ?", params![save_value(db, &signatures)?, now, session_id])?;
            let signed: Vec<&str> = [("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by")]
                .iter()
                .filter(|(key, _)| signatures.at(key).at("url").truthy() && !signatures.at(key).py_eq(previous_signatures.at(key)))
                .map(|(_, label)| *label)
                .collect();
            if !signed.is_empty() {
                Entry::new(&user, "audit_signed", "inspection", session_id)
                    .reference(row.str("audit_ref"))
                    .outlet(row.str("outlet"))
                    .started(last_time(db, "audit_completed", "inspection", session_id)?)
                    .detail(signed.join(", "))
                    .unit(row.str("business_unit"))
                    .at(now)
                    .log(db)?;
            }
            notify_inspection(db, session_id, &user, &payload, Some(&existing), false)?;
            return ok(obj([("ok", true.into()), ("id", Value::Int(session_id)), ("status", "Completed".into()), ("auditId", row.value("audit_id"))]));
        }
        // An inspection keeps the outlet it was scheduled or started for.
        if payload.at("outlet").truthy() && !payload.at("outlet").py_eq(&row.value("outlet")) {
            return error_json(409, "An inspection's outlet is set by its schedule and cannot be changed");
        }
        payload.set("outlet", row.value("outlet"));
        let (items, _) = complete_item_details(db, payload.at("items").or(&Value::array()))?;
        payload.set("items", items.clone());
        let progress = inspection_progress(&items);
        let complete = payload.at("complete").truthy();
        if complete && progress < 100 {
            return error_json(409, "Inspection is incomplete");
        }
        let outlet = if payload.at("outlet").truthy() { payload.at("outlet").text() } else { first_outlet(db)? };
        let audit_date = normalize_audit_date(payload.at("auditDate"));
        // The audit's code carries its date, so it follows a draft's date when that changes.
        let session_name = inspection_name(&row.str("audit_ref"), &outlet, &Value::str(&audit_date));
        let changed_rows = db.exec(
            "UPDATE inspection_sessions SET inspection_name = ?, audit_ref = ?, business_unit = ?, outlet = ?, zone = ?, audit_date = ?, auditor = ?, items_data_id = ?, progress = ?, status = ?, signatures_data_id = ?, updated_at = ? WHERE id = ?",
            params![
                session_name,
                session_name,
                or_default(&payload, "businessUnit", "Ottotree"),
                outlet,
                or_default(&payload, "zone", "Unassigned"),
                audit_date,
                or_default(&payload, "auditor", "Unnamed Inspector"),
                save_value(db, &items)?,
                progress,
                if complete { "Completed" } else { "Draft" },
                save_value(db, payload.at("signatures").or(&Value::object()))?,
                now,
                session_id
            ],
        )?;
        if changed_rows == 0 {
            return send_error(404);
        }
        let mut sent = Value::object();
        for (key, value) in payload.entries() {
            if !value.is_null() {
                sent.set(key, value.clone());
            }
        }
        validate_metadata(db, &sent, Some(&existing))?;
        save_session_metadata(db, session_id, &payload)?;
        append_inspection_photos(db, &items, now)?;
        let mut audit_id = Value::Null;
        if complete {
            audit_id = Value::Int(finalize_inspection(db, session_id, &payload, now)?);
            Entry::new(&user, "audit_completed", "inspection", session_id)
                .reference(session_name.clone())
                .outlet(row.str("outlet"))
                .started(row.opt_int("created_at"))
                .unit(row.str("business_unit"))
                .at(now)
                .log(db)?;
        }
        notify_inspection(db, session_id, &user, &payload, Some(&existing), changed)?;
        ok(obj([
            ("ok", true.into()),
            ("id", Value::Int(session_id)),
            ("inspectionName", Value::str(session_name)),
            ("progress", Value::Int(progress)),
            ("status", if complete { "Completed" } else { "Draft" }.into()),
            ("auditId", audit_id),
        ]))
    })
}

pub fn patch_schedule(ctx: &Ctx, payload: &Value) -> R<Reply> {
    if !ctx.path.starts_with("/api/schedules/") {
        return send_error(404);
    }
    let schedule_id = crate::id_or_reply!(ctx);
    let user = ctx.user()?;
    with_db(|db| {
        let Some(existing) = db.row("SELECT status, auditor, assignees_data_id, business_unit FROM schedules WHERE id = ?", params![schedule_id])? else { return send_error(404) };
        let outlet = if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(first_outlet(db)?) };
        let (locations, scope, label) = visit_scope(db, &outlet, payload)?;
        let (priority, due_date) = visit_terms(db, payload)?;
        let audit_type = visit_audit_type(db, payload)?;
        let scheduled = or_default(payload, "scheduledDate", "Today");
        let auditor = if payload.at("auditor").truthy() { payload.at("auditor").clone() } else if existing.get("auditor").truthy() { existing.value("auditor") } else { "Unassigned".into() };
        // Status follows the audit's progress; an edit keeps it unless a status is sent.
        let schedule_status = if payload.at("status").truthy() { payload.at("status").clone() } else { existing.value("status") };
        db.exec(
            "UPDATE schedules SET outlet = ?, zone = ?, locations_data_id = ?, scope_data_id = ?, priority = ?, due_date = ?, audit_type = ?, scheduled_date = ?, auditor = ?, remarks = ?, status = ? WHERE id = ?",
            params![outlet, label, save_value(db, &locations)?, save_value(db, &scope)?, priority, due_date, audit_type, scheduled, auditor, or_default(payload, "remarks", ""), schedule_status, schedule_id],
        )?;
        if payload.has("assignees") {
            let people = validate_assignees(db, &outlet.text(), payload.at("assignees"))?;
            let previous = assignees_of(db, existing.get("assignees_data_id"))?;
            save_assignment(db, &user, schedule_id, &outlet.text(), &scheduled, &people, &previous, &existing.str("business_unit"))?;
        }
        ok_true()
    })
}

pub fn delete_inspection(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let session_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        db.begin_immediate()?;
        let session = db.row("SELECT status, schedule_id FROM inspection_sessions WHERE id = ?", params![session_id])?;
        if session.as_ref().is_some_and(|row| row.str("status") == "Completed") {
            return error_json(409, "Completed inspections must be retained");
        }
        if db.exec("DELETE FROM inspection_sessions WHERE id = ?", params![session_id])? == 0 {
            return send_error(404);
        }
        if let Some(schedule) = session.and_then(|row| row.opt_int("schedule_id")).filter(|id| *id != 0) {
            db.exec("UPDATE schedules SET status = 'Pending' WHERE id = ?", params![schedule])?;
        }
        ok_true()
    })
}

pub fn delete_schedule(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    if !ctx.path.starts_with("/api/schedules/") {
        return send_error(404);
    }
    let schedule_id = crate::id_or_reply!(ctx);
    let user = ctx.user()?;
    with_db(|db| {
        db.begin_immediate()?;
        let session = db.row("SELECT id, status, closed_at, audit_ref FROM inspection_sessions WHERE schedule_id = ?", params![schedule_id])?;
        if let Some(session) = &session {
            if session.str("status") == "Completed" || session.get("closed_at").truthy() {
                let message = format!("This visit's audit {} is completed and is kept as an audit record, so the visit cannot be deleted.", session.str("audit_ref")).replace("  ", " ");
                return error_json(409, &message);
            }
            // Opening a visit starts a draft at once; deleting the visit deletes that unfinished draft with it.
            if !list_has(user.at("inspectionPermissions"), "auditor") {
                return Err(forbidden("This visit has a draft audit; deleting it needs auditor permission"));
            }
            db.exec("DELETE FROM inspection_sessions WHERE id = ?", params![session.int("id")])?;
        }
        if db.exec("DELETE FROM schedules WHERE id = ?", params![schedule_id])? == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

/// Close a completed audit once it is signed and its findings and work orders are closed.
pub fn close_audit(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    if !list_has(user.at("inspectionPermissions"), "verifier") {
        return Err(forbidden("Verifier permission is required to close an audit"));
    }
    let session_id = payload.at("id").or(&Value::Int(0)).py_int().map_err(bad)?;
    with_db(|db| {
        db.begin_immediate()?;
        let session = db.row("SELECT * FROM inspection_sessions WHERE id = ?", params![session_id])?.ok_or_else(|| status(404, "Inspection not found"))?;
        if session.get("closed_at").truthy() {
            return ok(obj([("ok", true.into()), ("id", Value::Int(session_id)), ("closedAt", session.value("closed_at"))]));
        }
        if session.str("status") != "Completed" || !session.get("audit_id").truthy() {
            return Err(status(409, "Complete the inspection before closing the audit"));
        }
        let signatures = if session.get("signatures_data_id").is_null() { Value::Null } else { load_value(db, session.get("signatures_data_id"))? };
        if ["auditedBy", "verifiedBy", "acknowledgedBy"].iter().any(|key| !signatures.at(key).at("url").truthy()) {
            return Err(status(409, "Auditor, verifier, and acknowledger signatures are required before closure"));
        }
        let audit_id = session.int("audit_id");
        let pending = db.exists("SELECT 1 FROM findings WHERE audit_id = ? AND status != 'Closed' LIMIT 1", params![audit_id])?;
        let pending_order = db.exists("SELECT 1 FROM work_orders WHERE source_audit_id = ? AND status != 'Closed' LIMIT 1", params![audit_id])?;
        if pending || pending_order {
            return Err(status(409, "Close all linked findings and work orders before closing the audit"));
        }
        let now = clock::now_ms();
        db.exec("UPDATE inspection_sessions SET closed_at = ?, closed_by = ?, updated_at = ? WHERE id = ?", params![now, user.at("name"), now, session_id])?;
        Entry::new(&user, "audit_closed", "inspection", session_id)
            .reference(session.str("audit_ref"))
            .outlet(session.str("outlet"))
            .started(last_time(db, "audit_completed", "inspection", session_id)?)
            .unit(session.str("business_unit"))
            .at(now)
            .log(db)?;
        db.exec("INSERT INTO comments(record_type,record_id,comment,author,created_at,system_generated) VALUES ('inspection',?,'Audit closed',?,?,1)", params![session_id, user.at("name"), now])?;
        ok(obj([("ok", true.into()), ("id", Value::Int(session_id)), ("closedAt", Value::Int(now))]))
    })
}
