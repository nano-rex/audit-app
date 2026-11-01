//! Findings, work requests, work orders, notifications, and comments.
//!
//! A work request is what should be fixed, raised from an audit's findings or reported directly.
//! It is reviewed on Maintenance > Work Requests: approving it creates a work order; declining it
//! closes its findings with the reason. When the work order closes, the request and its findings
//! close with it.

use crate::activity::Entry;
use crate::clock;
use crate::common::{fold, record_year, sla_status, today_date};
use crate::db::{first_department, first_outlet, hydrate_many, insert_record, load_or, marks, save_value, update_record, with_db, Db};
use crate::error::{status, R};
use crate::json::{obj, Value};
use crate::params;
use crate::reports::report_scope;
use crate::sqlite::{Sql, ToSql};

fn bind(values: &[Value]) -> Vec<&dyn ToSql> {
    values.iter().map(|value| value as &dyn ToSql).collect()
}

pub fn notifications(user_id: i64) -> R<Value> {
    crate::notify::deliver_due_reminders()?;
    with_db(|db| {
        let rows = db.rows(
            "SELECT id, title, message, channel, status, related_type, related_id, created_at, read_at FROM notifications WHERE recipient_user_id = ? OR recipient_user_id IS NULL ORDER BY created_at DESC, id DESC LIMIT 100",
            params![user_id],
        )?;
        Ok(obj([("items", Value::Array(hydrate_many(db, &rows)?))]))
    })
}

pub fn comments(record_type: &str, record_id: &str) -> R<Value> {
    with_db(|db| {
        let id: i64 = if record_id.is_empty() { 0 } else { Value::str(record_id).py_int().map_err(crate::error::bad)? };
        let rows = if !record_type.is_empty() && id != 0 {
            db.rows("SELECT id, record_type, record_id, comment, author, created_at FROM comments WHERE record_type = ? AND record_id = ? ORDER BY created_at DESC, id DESC", params![record_type, id])?
        } else {
            db.rows("SELECT id, record_type, record_id, comment, author, created_at FROM comments ORDER BY created_at DESC, id DESC", params![])?
        };
        Ok(obj([("items", Value::Array(hydrate_many(db, &rows)?))]))
    })
}

fn is_department_pic(user: &Value) -> bool {
    user.at("role").as_str() == Some("Department/PIC")
}

pub fn work_order_items(user: &Value) -> R<Value> {
    with_db(|db| {
        let mut where_clause = String::new();
        let mut values = Vec::new();
        if is_department_pic(user) {
            let name = fold(&user.at("name").text());
            let email = fold(&user.at("email").text());
            let department = fold(&user.at("department").text());
            where_clause = "WHERE CASE WHEN TRIM(COALESCE(pic, '')) != '' THEN LOWER(TRIM(pic)) IN (?, ?) ELSE LOWER(TRIM(COALESCE(assignee, ''))) IN (?, ?) OR LOWER(TRIM(COALESCE(request_type, ''))) = ? END".into();
            values = vec![Value::str(&name), Value::str(&email), Value::str(&name), Value::str(&email), Value::str(department)];
        }
        let rows = db.rows(
            &format!(
                "SELECT id, work_order_ref, business_unit, outlet, zone, request_type, category, priority, title, description, assignee, pic, status, closed_at, due_date, vendor, sla_status, cost, outlet_confirmed, source_finding_id, cause, recommendation, required_action, images_data_id FROM work_orders {where_clause} ORDER BY CASE priority WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END, created_at DESC, id DESC"
            ),
            &bind(&values),
        )?;
        Ok(obj([("items", Value::Array(with_current_sla(hydrate_many(db, &rows)?)))]))
    })
}

/// The stored SLA status is as of the last edit; an order becomes overdue without being edited.
pub fn with_current_sla(mut orders: Vec<Value>) -> Vec<Value> {
    for order in &mut orders {
        let current = sla_status(&order.at("status").py_str(), order.at("due_date"));
        order.set("sla_status", current);
    }
    orders
}

/// Findings follow their work order: the same status, person in charge, and closing date. A
/// closed work order closes the request it was made from.
pub fn sync_finding_from_work_order(db: &Db, work_order_id: i64) -> R<()> {
    let Some(row) = db.row("SELECT source_finding_id, work_request_id, status, pic, closed_at FROM work_orders WHERE id = ?", params![work_order_id])? else {
        return Ok(());
    };
    let now = clock::now_ms();
    db.exec(
        "UPDATE findings SET status = ?, pic = ?, closed_at = ?, updated_at = ? WHERE id = ? OR (work_request_id IS NOT NULL AND work_request_id = ?)",
        params![row.value("status"), row.str("pic"), row.str("closed_at"), now, row.value("source_finding_id"), row.value("work_request_id")],
    )?;
    if row.get("work_request_id").truthy() && row.str("status") == "Closed" {
        db.exec("UPDATE work_requests SET status = 'Closed', updated_at = ? WHERE id = ?", params![now, row.int("work_request_id")])?;
    }
    Ok(())
}

pub fn finding_items(unit: &str, filters: &Value, user: &Value) -> R<Value> {
    let (mut where_clause, mut values) = report_scope(unit, "findings", filters)?;
    if is_department_pic(user) {
        let name = fold(&user.at("name").text());
        let email = fold(&user.at("email").text());
        let department = fold(&user.at("department").text());
        where_clause.push_str(" AND CASE WHEN TRIM(COALESCE(findings.pic, '')) != '' THEN LOWER(TRIM(findings.pic)) IN (?, ?) ELSE LOWER(TRIM(COALESCE(findings.assigned_department, ''))) = ? END");
        values.extend([Value::str(name), Value::str(email), Value::str(department)]);
    }
    with_db(|db| {
        let rows = db.rows(
            &format!(
                "SELECT findings.*, audits.audit_date, audits.audit_time, audits.auditor,
                       COALESCE(NULLIF(findings.item_name, ''), inspection_items.section, 'Item') AS item_name,
                       COALESCE(NULLIF(findings.criterion, ''), inspection_items.item, '') AS criterion,
                       COALESCE(equipment.kind, 'asset') AS item_kind,
                       work_requests.request_ref, work_requests.status AS request_status,
                       (SELECT work_order_ref FROM work_orders WHERE work_orders.source_finding_id = findings.id) AS order_ref
                FROM findings LEFT JOIN audits ON audits.id = findings.audit_id
                LEFT JOIN inspection_items ON inspection_items.id = findings.source_item_id
                LEFT JOIN equipment ON equipment.id = findings.equipment_id
                LEFT JOIN work_requests ON work_requests.id = findings.work_request_id
                WHERE {where_clause} ORDER BY findings.created_at DESC, findings.id DESC"
            ),
            &bind(&values),
        )?;
        // item_name and criterion appear twice: the stored column, then its fallback. The fallback
        // is the one meant (the Python version's dict(row) kept the stored column instead).
        let rows: Vec<crate::sqlite::Row> = rows.into_iter().map(dedupe_columns).collect();
        Ok(obj([("items", Value::Array(hydrate_many(db, &rows)?))]))
    })
}

/// A row whose repeated column names keep only their last value, at the first one's place.
fn dedupe_columns(row: crate::sqlite::Row) -> crate::sqlite::Row {
    let mut columns: Vec<String> = Vec::new();
    let mut values: Vec<Sql> = Vec::new();
    for (column, value) in row.columns.iter().zip(row.values) {
        match columns.iter().position(|name| name == column) {
            Some(index) => values[index] = value,
            None => {
                columns.push(column.clone());
                values.push(value);
            }
        }
    }
    crate::sqlite::Row { columns: std::rc::Rc::new(columns), values }
}

// ---- Work requests ----

pub fn request_ref(record_id: i64) -> String {
    format!("WR-{}-{record_id:05}", record_year(&Value::Null))
}

/// Turning a request into work, or declining it, assigns work; a Department/PIC account does not.
pub fn reviews_requests(user: &Value) -> bool {
    !is_department_pic(user) && (user.at("role").as_str() == Some("Super") || crate::permissions::list_has(user.at("permissions"), "work-orders"))
}

pub fn work_request_items(user: &Value) -> R<Value> {
    let (where_clause, values) = if is_department_pic(user) {
        ("WHERE LOWER(TRIM(COALESCE(work_requests.department, ''))) = ?".to_string(), vec![Value::str(fold(&user.at("department").text()))])
    } else {
        (String::new(), Vec::new())
    };
    with_db(|db| {
        let rows = db.rows(
            &format!(
                "SELECT work_requests.*, work_orders.work_order_ref, work_orders.status AS work_order_status,
                       (SELECT COUNT(*) FROM findings WHERE findings.work_request_id = work_requests.id) AS findings_count,
                       (SELECT GROUP_CONCAT(finding_ref, ', ') FROM findings WHERE findings.work_request_id = work_requests.id) AS finding_refs
                FROM work_requests
                LEFT JOIN work_orders ON work_orders.id = work_requests.work_order_id
                {where_clause}
                ORDER BY CASE work_requests.status WHEN 'Open' THEN 0 WHEN 'Ordered' THEN 1 ELSE 2 END,
                         work_requests.created_at DESC, work_requests.id DESC"
            ),
            &bind(&values),
        )?;
        Ok(obj([("items", Value::Array(hydrate_many(db, &rows)?))]))
    })
}

fn int_list(values: &Value) -> R<Vec<i64>> {
    values.items().iter().map(|value| value.py_int().map_err(crate::error::bad)).collect()
}

pub fn create_work_request(user: &Value, payload: &Value) -> R<Value> {
    let now = clock::now_ms();
    let finding_ids = int_list(payload.at("findingIds").or(&Value::array()))?;
    let description = payload.at("description").text().trim().to_string();
    if description.is_empty() {
        return Err(status(400, "Describe the work that is needed"));
    }
    with_db(|db| {
        db.begin_immediate()?;
        let values: Vec<(&str, Value)> = if !finding_ids.is_empty() {
            let params: Vec<&dyn ToSql> = finding_ids.iter().map(|id| id as &dyn ToSql).collect();
            let findings: Vec<Value> = db.rows(&format!("SELECT * FROM findings WHERE id IN ({})", marks(finding_ids.len())), &params)?.iter().map(|row| row.to_object()).collect();
            let distinct: std::collections::BTreeSet<i64> = finding_ids.iter().copied().collect();
            if findings.len() != distinct.len() {
                return Err(status(404, "A finding in this request no longer exists"));
            }
            if findings.iter().any(|row| row.at("work_request_id").truthy() || row.at("status").as_str() == Some("Closed")) {
                return Err(status(409, "A work request has already been raised for this item"));
            }
            let places: std::collections::BTreeSet<(String, String)> = findings.iter().map(|row| (row.at("outlet").py_str(), row.at("location").py_str())).collect();
            if places.len() > 1 {
                return Err(status(400, "A work request covers one item at one location"));
            }
            let first = &findings[0];
            let mut images: Vec<Value> = Vec::new();
            let mut seen: Vec<Value> = Vec::new();
            for row in &findings {
                let stored = load_or(db, &Sql::from_value(row.at("images_data_id")), Value::Null)?;
                for image in stored.items() {
                    let key = image.at("url");
                    if key.truthy() {
                        if seen.iter().any(|known| known.py_eq(key)) {
                            continue;
                        }
                        seen.push(key.clone());
                    }
                    images.push(image.clone());
                }
            }
            let chosen = if payload.at("images").truthy() { payload.at("images").clone() } else { Value::Array(images) };
            vec![
                ("business_unit", first.at("business_unit").clone()),
                ("outlet", first.at("outlet").clone()),
                ("location", first.at("location").clone()),
                ("equipment_id", first.at("equipment_id").clone()),
                ("item_name", first.at("item_name").or(payload.at("itemName")).or(&Value::str("")).clone()),
                ("category", payload.at("category").or(first.at("category")).clone()),
                ("priority", payload.at("priority").or(first.at("priority")).clone()),
                ("department", payload.at("department").or(first.at("assigned_department")).clone()),
                ("audit_id", first.at("audit_id").clone()),
                ("audit_ref", first.at("audit_ref").or(&Value::str("")).clone()),
                ("images_data_id", Value::Int(save_value(db, &chosen)?)),
            ]
        } else {
            for (key, label) in [("outlet", "Outlet"), ("location", "Location"), ("itemName", "Item")] {
                if payload.at(key).text().trim().is_empty() {
                    return Err(status(400, format!("{label} is required")));
                }
            }
            let outlet = if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(first_outlet(db)?) };
            let department = if payload.at("department").truthy() { payload.at("department").clone() } else { Value::str(first_department(db)?) };
            vec![
                ("business_unit", payload.get("businessUnit").cloned().unwrap_or_else(|| "Ottotree".into())),
                ("outlet", outlet),
                ("location", payload.at("location").clone()),
                ("equipment_id", payload.at("equipmentId").clone()),
                ("item_name", Value::str(payload.at("itemName").py_str().trim())),
                ("category", payload.at("category").or(&Value::str("")).clone()),
                ("priority", payload.at("priority").or(&Value::str("Medium")).clone()),
                ("department", department),
                ("audit_id", Value::Null),
                ("audit_ref", "".into()),
                ("images_data_id", Value::Int(save_value(db, payload.at("images").or(&Value::array()))?)),
            ]
        };
        let outlet = values.iter().find(|(key, _)| *key == "outlet").map(|(_, value)| value.text()).unwrap_or_default();
        let unit = values.iter().find(|(key, _)| *key == "business_unit").map(|(_, value)| value.text()).unwrap_or_default();
        let mut record = values;
        record.extend([
            ("description", Value::str(&description)),
            ("status", "Open".into()),
            ("requested_by", user.get("name").cloned().unwrap_or_else(|| "".into())),
            ("requested_by_user_id", user.at("id").clone()),
            ("created_at", Value::Int(now)),
            ("updated_at", Value::Int(now)),
        ]);
        let request_id = insert_record(db, "work_requests", &record)?;
        db.exec("UPDATE work_requests SET request_ref = ? WHERE id = ?", params![request_ref(request_id), request_id])?;
        for finding_id in &finding_ids {
            db.exec("UPDATE findings SET work_request_id = ?, status = 'Requested', updated_at = ? WHERE id = ?", params![request_id, now, finding_id])?;
        }
        Entry::new(user, "request_raised", "work_request", request_id).reference(request_ref(request_id)).outlet(outlet).unit(unit).at(now).log(db)?;
        Ok(obj([("ok", true.into()), ("id", Value::Int(request_id)), ("requestRef", Value::str(request_ref(request_id)))]))
    })
}

/// An open request can be corrected by whoever raised it or whoever reviews requests. One raised
/// from findings keeps its item and place; one reported directly can change them too.
pub fn edit_work_request(user: &Value, request_id: i64, payload: &Value) -> R<Value> {
    let now = clock::now_ms();
    with_db(|db| {
        db.begin_immediate()?;
        let row = db.row("SELECT * FROM work_requests WHERE id = ?", params![request_id])?.ok_or_else(|| status(404, "Work request not found"))?;
        if row.str("status") != "Open" {
            return Err(status(409, "Only an open request can be edited; edit its work order instead"));
        }
        if row.value("requested_by_user_id").py_ne_id(user.at("id")) && !reviews_requests(user) {
            return Err(status(403, "Only whoever raised this request, or someone who assigns work orders, can edit it"));
        }
        let description = payload.get("description").cloned().unwrap_or_else(|| row.value("description")).text().trim().to_string();
        if description.is_empty() {
            return Err(status(400, "Describe the work that is needed"));
        }
        let mut values: Vec<(String, Value)> = vec![("description".into(), Value::str(&description)), ("updated_at".into(), Value::Int(now))];
        for key in ["department", "category", "priority"] {
            if payload.at(key).truthy() {
                values.push((key.into(), Value::str(payload.at(key).py_str())));
            }
        }
        if payload.has("images") {
            values.push(("images_data_id".into(), Value::Int(save_value(db, payload.at("images").or(&Value::array()))?)));
        }
        let from_findings = db.exists("SELECT 1 FROM findings WHERE work_request_id = ?", params![request_id])?;
        if !from_findings {
            for (key, column, label) in [("outlet", "outlet", "Outlet"), ("location", "location", "Location"), ("itemName", "item_name", "Item")] {
                if payload.has(key) {
                    let value = payload.at(key).text().trim().to_string();
                    if value.is_empty() {
                        return Err(status(400, format!("{label} is required")));
                    }
                    values.push((column.into(), Value::str(value)));
                }
            }
            if let Some((_, outlet)) = values.iter().find(|(column, _)| column == "outlet") {
                if !db.exists("SELECT 1 FROM outlets WHERE code = ?", params![outlet])? {
                    return Err(status(400, "Select an existing outlet"));
                }
            }
        }
        update_record(db, "work_requests", &values, request_id)?;
        let get = |column: &str| values.iter().find(|(name, _)| name == column).map(|(_, value)| value.clone()).unwrap_or(Value::Null);
        if from_findings && ["department", "category", "priority"].iter().any(|column| values.iter().any(|(name, _)| name == column)) {
            // The findings it covers follow the request.
            db.exec(
                "UPDATE findings SET assigned_department = COALESCE(?, assigned_department), category = COALESCE(?, category), priority = COALESCE(?, priority), updated_at = ? WHERE work_request_id = ?",
                params![get("department"), get("category"), get("priority"), now, request_id],
            )?;
            if let Some(level) = db.row("SELECT classification FROM priority_levels WHERE name = ?", params![get("priority")])? {
                db.exec("UPDATE findings SET priority_classification = ? WHERE work_request_id = ?", params![level.value("classification"), request_id])?;
            }
        }
        Ok(obj([("ok", true.into())]))
    })
}

pub fn decline_work_request(user: &Value, request_id: i64, remark: &Value) -> R<Value> {
    if !reviews_requests(user) {
        return Err(status(403, "Only someone who assigns work orders can decline a request"));
    }
    let remark = remark.text().trim().to_string();
    if remark.is_empty() {
        return Err(status(400, "Say why no work is needed"));
    }
    let now = clock::now_ms();
    with_db(|db| {
        db.begin_immediate()?;
        let row = db
            .row("SELECT status, request_ref, outlet, business_unit, created_at FROM work_requests WHERE id = ?", params![request_id])?
            .ok_or_else(|| status(404, "Work request not found"))?;
        if row.str("status") != "Open" {
            return Err(status(409, "Only an open request can be declined"));
        }
        db.exec(
            "UPDATE work_requests SET status = 'Declined', decline_remark = ?, reviewed_by = ?, updated_at = ? WHERE id = ?",
            params![remark, user.get("name").cloned().unwrap_or_else(|| "".into()), now, request_id],
        )?;
        // No work is needed, so the findings it covered are settled.
        db.exec("UPDATE findings SET status = 'Closed', closed_at = ?, updated_at = ? WHERE work_request_id = ?", params![today_date(), now, request_id])?;
        Entry::new(user, "request_declined", "work_request", request_id)
            .reference(row.str("request_ref"))
            .outlet(row.str("outlet"))
            .started(row.opt_int("created_at"))
            .detail(remark.clone())
            .unit(row.str("business_unit"))
            .at(now)
            .log(db)?;
        Ok(obj([("ok", true.into())]))
    })
}

/// Check a request may become a work order; returns it for prefilling.
pub fn claim_for_work_order(db: &Db, user: &Value, request_id: i64) -> R<Value> {
    if !reviews_requests(user) {
        return Err(status(403, "Only someone who assigns work orders can create one from a request"));
    }
    let row = db.row("SELECT * FROM work_requests WHERE id = ?", params![request_id])?.ok_or_else(|| status(404, "Work request not found"))?;
    if row.str("status") != "Open" {
        return Err(status(409, "A work order has already been created or the request was declined"));
    }
    Ok(row.to_object())
}

pub fn link_work_order(db: &Db, request_id: i64, order_id: i64, order_status: &Value, pic: &Value) -> R<()> {
    let now = clock::now_ms();
    db.exec("UPDATE work_orders SET work_request_id = ? WHERE id = ?", params![request_id, order_id])?;
    db.exec("UPDATE work_requests SET status = 'Ordered', work_order_id = ?, updated_at = ? WHERE id = ?", params![order_id, now, request_id])?;
    db.exec("UPDATE findings SET status = ?, pic = ?, updated_at = ? WHERE work_request_id = ?", params![order_status, pic.or(&Value::str("")), now, request_id])?;
    Ok(())
}

trait IdCompare {
    fn py_ne_id(&self, other: &Value) -> bool;
}

impl IdCompare for Value {
    fn py_ne_id(&self, other: &Value) -> bool {
        !self.py_eq(other)
    }
}
