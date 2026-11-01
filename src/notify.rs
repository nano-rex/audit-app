//! In-app notices: audit progress and verification, work-order assignment, daily due-date
//! reminders, and scheduled audits assigned to people.

use std::collections::BTreeSet;

use crate::activity::Entry;
use crate::accounts::person_outlets;
use crate::audits::assigned_to;
use crate::clock::{self, Date};
use crate::config::SUPER_ROLE;
use crate::db::{connect, load_or, load_value, save_value, Db};
use crate::error::{bad, R};
use crate::json::{obj, Value};
use crate::params;
use crate::permissions::{list_has, resolve_permissions, INSPECTION_PERMISSIONS};

fn insert_notice(db: &Db, title: &str, message: &str, related_type: &str, related_id: i64, now: i64, recipient: i64) -> R<()> {
    db.exec(
        "INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread',?,?,?,?)",
        params![title, message, related_type, related_id, now, recipient],
    )?;
    Ok(())
}

/// Tell reviewers an audit moved on, and its owner when it is verified.
pub fn notify_inspection(db: &Db, session_id: i64, actor: &Value, payload: &Value, previous: Option<&Value>, changed: bool) -> R<()> {
    let Some(session) = db.row("SELECT * FROM inspection_sessions WHERE id = ?", params![session_id])? else { return Ok(()) };
    let now = clock::now_ms();
    let actor_id = actor.at("id").as_i64();
    if changed && session.int("progress") > 0 {
        let mut roles = Vec::new();
        for row in db.rows("SELECT name, inspection_permissions_data_id FROM roles", params![])? {
            roles.push((row.str("name"), load_or(db, row.get("inspection_permissions_data_id"), Value::array())?));
        }
        let mut recipients = BTreeSet::new();
        for user in db.rows("SELECT id, role, permission_overrides_data_id FROM users WHERE active = 1", params![])? {
            if Some(user.int("id")) == actor_id {
                continue;
            }
            let overrides = if user.get("permission_overrides_data_id").is_null() { Value::Null } else { load_value(db, user.get("permission_overrides_data_id"))? };
            let capabilities = if user.str("role") == SUPER_ROLE {
                crate::json::strings(&INSPECTION_PERMISSIONS)
            } else if !overrides.is_null() {
                overrides.get("inspectionPermissions").cloned().unwrap_or_else(Value::array)
            } else {
                roles.iter().find(|(name, _)| *name == user.str("role")).map(|(_, caps)| caps.clone()).unwrap_or_else(Value::array)
            };
            if list_has(&capabilities, "verifier") || list_has(&capabilities, "acknowledger") {
                recipients.insert(user.int("id"));
            }
        }
        let message = format!("{} saved {}: {}% complete.", actor.at("name").py_str(), session.str("inspection_name"), session.int("progress"));
        // A draft is saved many times. Refresh the reviewer's unread notice instead of adding one per save.
        let waiting: BTreeSet<i64> = db
            .column("SELECT recipient_user_id FROM notifications WHERE related_type = 'inspection' AND related_id = ? AND title = 'Audit progress updated' AND status = 'Unread'", params![session_id])?
            .iter()
            .filter_map(|value| value.as_i64())
            .collect();
        for recipient in recipients.intersection(&waiting) {
            db.exec(
                "UPDATE notifications SET message = ?, created_at = ? WHERE related_type = 'inspection' AND related_id = ? AND title = 'Audit progress updated' AND status = 'Unread' AND recipient_user_id = ?",
                params![message, now, session_id, recipient],
            )?;
        }
        for recipient in recipients.difference(&waiting) {
            insert_notice(db, "Audit progress updated", &message, "inspection", session_id, now, *recipient)?;
        }
    }
    let prior = match previous {
        Some(previous) => {
            let value = previous.at("signatures_data_id").clone();
            if value.truthy() { value } else { Value::object() }
        }
        None => Value::object(),
    };
    let verified = payload.at("signatures").at("verifiedBy");
    let mut owner_id = session.opt_int("owner_user_id").filter(|id| *id != 0);
    if owner_id.is_none() && session.get("auditor").truthy() {
        owner_id = db.scalar("SELECT id FROM users WHERE name = ? AND active = 1 ORDER BY id LIMIT 1", params![session.str("auditor")])?.as_i64();
    }
    if let Some(owner) = owner_id {
        if verified.truthy() && !verified.py_eq(prior.at("verifiedBy")) && Some(owner) != actor_id {
            let message = format!("{} verified {}.", actor.at("name").py_str(), session.str("inspection_name"));
            insert_notice(db, "Audit verified", &message, "inspection", session_id, now, owner)?;
        }
    }
    Ok(())
}

// ---- Work orders ----

fn active_people(db: &Db) -> R<Vec<Value>> {
    Ok(db.rows("SELECT id, name, email, department, role, permission_overrides_data_id FROM users WHERE active = 1", params![])?.iter().map(|row| row.to_object()).collect())
}

/// Who hears about a work order: those it is assigned to, else the administrators.
fn recipients(order: &Value, people: &[Value]) -> BTreeSet<i64> {
    let selected: BTreeSet<i64> = people.iter().filter(|person| assigned_to(person, order)).filter_map(|person| person.at("id").as_i64()).collect();
    if !selected.is_empty() {
        return selected;
    }
    people.iter().filter(|person| matches!(person.at("role").as_str(), Some("Super" | "Admin"))).filter_map(|person| person.at("id").as_i64()).collect()
}

pub fn notify_work_order(db: &Db, work_order_id: i64, status: &str) -> R<()> {
    let Some(order) = db.row("SELECT * FROM work_orders WHERE id = ?", params![work_order_id])? else { return Ok(()) };
    let order = order.to_object();
    let now = clock::now_ms();
    let people = active_people(db)?;
    let title = format!("Work order {}", status.to_lowercase());
    let message = format!("{}: {}", order.at("work_order_ref").py_str(), order.at("title").py_str());
    for recipient in recipients(&order, &people) {
        insert_notice(db, &title, &message, "work_order", work_order_id, now, recipient)?;
    }
    Ok(())
}

const OPEN_ORDERS: &str = "status NOT IN ('Completed','Verified','Closed')";

/// Reminders owed today that have not been recorded; reads only.
fn pending_reminders(db: &Db, today: Date) -> R<Vec<(i64, Value, &'static str, Date)>> {
    let horizon = today.add_days(3).iso();
    let orders = db.rows(&format!("SELECT * FROM work_orders WHERE {OPEN_ORDERS} AND due_date IS NOT NULL AND due_date != '' AND due_date <= ?"), params![horizon])?;
    if orders.is_empty() {
        return Ok(Vec::new());
    }
    let sent: BTreeSet<(i64, i64, String, String)> = db
        .rows("SELECT user_id, work_order_id, kind, due_date FROM due_notification_events WHERE day = ?", params![today.iso()])?
        .iter()
        .map(|row| (row.int("user_id"), row.int("work_order_id"), row.str("kind"), row.str("due_date")))
        .collect();
    let people = active_people(db)?;
    let mut pending = Vec::new();
    for row in orders {
        let order = row.to_object();
        let Some(due) = Date::parse_iso(&order.at("due_date").py_str()) else { continue };
        let kind = if due < today { "Overdue" } else { "Due soon" };
        for recipient in recipients(&order, &people) {
            if !sent.contains(&(recipient, row.int("id"), kind.to_string(), due.iso())) {
                pending.push((recipient, order.clone(), kind, due));
            }
        }
    }
    Ok(pending)
}

pub fn deliver_due_reminders() -> R<usize> {
    let today = clock::today();
    let db = connect()?;
    // Every notification read calls this. Take the write lock only when a reminder is owed, so
    // polling clients do not queue behind, or delay, inspection saves.
    let pending = pending_reminders(&db, today)?;
    if pending.is_empty() {
        return Ok(0);
    }
    let result = (|| -> R<usize> {
        db.begin_immediate()?;
        let mut count = 0;
        for (recipient, order, kind, due) in pending {
            let id = order.at("id").as_i64().unwrap_or(0);
            if !db.exists(&format!("SELECT 1 FROM work_orders WHERE id = ? AND {OPEN_ORDERS}"), params![id])? {
                continue;
            }
            let added = db.exec(
                "INSERT OR IGNORE INTO due_notification_events(user_id,work_order_id,kind,day,due_date) VALUES (?,?,?,?,?)",
                params![recipient, id, kind, today.iso(), due.iso()],
            )?;
            if added > 0 {
                let reference = if order.at("work_order_ref").truthy() { order.at("work_order_ref").py_str() } else { id.to_string() };
                let message = format!("{reference}: {} — due {}", order.at("title").py_str(), due.iso());
                insert_notice(&db, kind, &message, "work_order", id, clock::now_ms(), recipient)?;
                count += 1;
            }
        }
        Ok(count)
    })();
    crate::db::finish(&db, result)
}

/// Deliver reminders every minute until the process ends.
pub fn reminder_loop() {
    loop {
        if let Err(error) = deliver_due_reminders() {
            crate::error::log_error(&format!("Unable to deliver audit reminders: {error}"));
        }
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

// ---- Who a scheduled visit is assigned to ----
//
// A superior picks one or more people who can audit at the visit's outlet; each newly assigned
// person is notified, the visit waits for them in the Dashboard's Scheduled audits, and the
// assignment is logged.

/// Active people who may audit (Inspections page and auditor permission) and cover the outlet.
pub fn assignable_people(db: &Db, outlet: &str) -> R<Vec<Value>> {
    let mut people = Vec::new();
    for row in db.rows("SELECT id, name, role, title, outlets_data_id, permission_overrides_data_id, profile_photo_data_id FROM users WHERE active = 1 ORDER BY name", params![])? {
        let overrides = if row.get("permission_overrides_data_id").is_null() { Value::Null } else { load_value(db, row.get("permission_overrides_data_id"))? };
        let (pages, capabilities) = resolve_permissions(db, &row.str("role"), &overrides)?;
        if !list_has(&pages, "inspections") || !list_has(&capabilities, "auditor") {
            continue;
        }
        if let Some(outlets) = person_outlets(db, row.get("outlets_data_id"))? {
            if !outlet.is_empty() && !outlets.iter().any(|code| code.as_str() == Some(outlet)) {
                continue;
            }
        }
        let photo = if row.get("profile_photo_data_id").truthy() { load_value(db, row.get("profile_photo_data_id"))? } else { Value::object() };
        let picture = if photo.is_object() && photo.at("url").truthy() { obj([("url", photo.at("url").clone())]) } else { Value::object() };
        people.push(obj([
            ("id", row.value("id")),
            ("name", row.value("name")),
            ("role", row.value("role")),
            ("title", Value::str(row.str("title"))),
            ("profilePhoto", picture),
        ]));
    }
    Ok(people)
}

pub fn validate_assignees(db: &Db, outlet: &str, values: &Value) -> R<Vec<Value>> {
    if values.is_null() || values.as_str() == Some("") {
        return Ok(Vec::new());
    }
    let Value::Array(items) = values else { return Err(bad("Assignees must be a list of people")) };
    let mut ids = Vec::new();
    for item in items {
        let id = item.py_int().map_err(|_| bad("Assignees must be a list of people"))?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let allowed = assignable_people(db, outlet)?;
    let mut chosen = Vec::new();
    for id in ids {
        match allowed.iter().find(|person| person.at("id").as_i64() == Some(id)) {
            Some(person) => chosen.push(person.clone()),
            None => return Err(bad("Assign only people who can audit and cover this outlet")),
        }
    }
    Ok(chosen)
}

pub fn assignees_of(db: &Db, reference: &crate::sqlite::Sql) -> R<Vec<i64>> {
    if !reference.truthy() {
        return Ok(Vec::new());
    }
    Ok(load_value(db, reference)?.items().iter().filter_map(Value::as_i64).collect())
}

/// Store the people, notify those newly assigned, and log who assigned them.
pub fn save_assignment(db: &Db, user: &Value, schedule_id: i64, outlet: &str, scheduled_date: &Value, people: &[Value], previous: &[i64], business_unit: &str) -> R<()> {
    let ids: Vec<Value> = people.iter().map(|person| person.at("id").clone()).collect();
    let names = people.iter().map(|person| person.at("name").py_str()).collect::<Vec<_>>().join(", ");
    let names = if names.is_empty() { "Unassigned".to_string() } else { names };
    db.exec("UPDATE schedules SET assignees_data_id = ?, auditor = ? WHERE id = ?", params![save_value(db, &Value::Array(ids))?, names, schedule_id])?;
    let added: Vec<&Value> = people.iter().filter(|person| !person.at("id").as_i64().is_some_and(|id| previous.contains(&id))).collect();
    if added.is_empty() {
        return Ok(());
    }
    let now = clock::now_ms();
    let reference = format!("SCH-{schedule_id:05}");
    let by = if user.at("name").truthy() { user.at("name").py_str() } else { "a supervisor".into() };
    let message = format!("{reference}: {outlet} on {}, assigned by {by}", scheduled_date.py_str());
    for person in &added {
        insert_notice(db, "Scheduled audit assigned to you", &message, "schedule", schedule_id, now, person.at("id").as_i64().unwrap_or(0))?;
    }
    Entry::new(user, "audit_assigned", "schedule", schedule_id)
        .reference(reference)
        .outlet(outlet)
        .detail(added.iter().map(|person| person.at("name").py_str()).collect::<Vec<_>>().join(", "))
        .unit(business_unit)
        .at(now)
        .log(db)
}
