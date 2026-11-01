//! What is waiting on the signed-in user: drafts, signatures, assigned work orders, closure.

use crate::accounts::{is_company_admin_user, keep};
use crate::audits::assigned_to;
use crate::common::sla_status;
use crate::db::{load_values, read_setting, with_db, Db};
use crate::error::R;
use crate::json::{obj, Value};
use crate::notify::assignees_of;
use crate::params;
use crate::permissions::{list_has, CHANGE_RECORDS};
use crate::sqlite::{Row, Sql, ToSql};
use crate::work::reviews_requests;

const SIGNATURES: [(&str, &str, &str); 3] = [("auditedBy", "auditor", "auditor"), ("verifiedBy", "verifier", "verifier"), ("acknowledgedBy", "acknowledger", "acknowledger")];
const LIMIT: usize = 50;

fn inspection_item(row: &Row, action: &str, detail: &str, view: &str) -> Value {
    let title = [row.str("audit_ref"), row.str("inspection_name"), row.str("outlet")].into_iter().find(|text| !text.is_empty()).unwrap_or_default();
    obj([
        ("type", "inspection".into()),
        ("id", row.value("id")),
        ("action", action.into()),
        ("view", view.into()),
        ("outlet", row.value("outlet")),
        ("title", Value::str(title)),
        ("detail", Value::str(format!("{} | {} | {detail}", row.str("outlet"), row.str("audit_date")))),
    ])
}

fn order_item(order: &Value, action: &str) -> Value {
    let due = if order.at("due_date").truthy() { format!("Due {}", order.at("due_date").py_str()) } else { "No due date".into() };
    let reference = if order.at("work_order_ref").truthy() { order.at("work_order_ref").py_str() } else { format!("#{}", order.at("id").py_str()) };
    let zone = if order.at("zone").truthy() { order.at("zone").py_str() } else { "No location".into() };
    obj([
        ("type", "work_order".into()),
        ("id", order.at("id").clone()),
        ("action", action.into()),
        ("outlet", order.at("outlet").clone()),
        ("title", Value::str(format!("{reference} {}", order.at("title").py_str()))),
        ("detail", Value::str(format!("{} | {zone} | {} | {due}", order.at("outlet").py_str(), order.at("priority").py_str()))),
        ("overdue", Value::Bool(sla_status(&order.at("status").py_str(), order.at("due_date")) == "Overdue")),
    ])
}

pub fn todo_items(user: &Value) -> R<Value> {
    let capabilities = user.at("inspectionPermissions");
    let can = |name: &str| list_has(capabilities, name);
    let permissions = user.at("permissions");
    let has_page = |name: &str| list_has(permissions, name);
    let user_id = user.at("id").as_i64().unwrap_or(0);
    with_db(|db| {
        let mut items = Vec::new();
        let unread = db.count("SELECT count(*) FROM notifications WHERE status = 'Unread' AND (recipient_user_id = ? OR recipient_user_id IS NULL)", params![user_id])?;
        if has_page("inspections") && capabilities.truthy() {
            if can("auditor") {
                for row in db.rows(
                    "SELECT id, audit_ref, inspection_name, outlet, audit_date, progress FROM inspection_sessions WHERE status = 'Draft' AND closed_at IS NULL AND (owner_user_id = ? OR owner_user_id IS NULL) ORDER BY updated_at DESC LIMIT ?",
                    params![user_id, LIMIT as i64],
                )? {
                    items.push(inspection_item(&row, "Continue inspection", &format!("{}% complete", row.int("progress")), "checklist"));
                }
            }
            let completed = db.rows(
                "SELECT id, audit_ref, inspection_name, outlet, audit_date, audit_id, owner_user_id, signatures_data_id FROM inspection_sessions WHERE status = 'Completed' AND closed_at IS NULL ORDER BY updated_at DESC LIMIT ?",
                params![LIMIT as i64],
            )?;
            let signatures = load_values(db, &completed.iter().map(|row| row.get("signatures_data_id").clone()).collect::<Vec<_>>())?;
            for (row, signed) in completed.iter().zip(signatures) {
                let signed = if signed.truthy() { signed } else { Value::object() };
                let mut missing = Vec::new();
                for (key, capability, label) in SIGNATURES {
                    if !can(capability) || signed.at(key).at("url").truthy() {
                        continue;
                    }
                    // The auditor's signature belongs to whoever ran the inspection.
                    if key == "auditedBy" && row.opt_int("owner_user_id").is_some_and(|owner| owner != user_id) {
                        continue;
                    }
                    missing.push(label);
                }
                if !missing.is_empty() {
                    items.push(inspection_item(row, &format!("Sign as {}", missing.join(", ")), "Completed, awaiting signature", "signoff"));
                }
                if can("verifier")
                    && SIGNATURES.iter().all(|(key, ..)| signed.at(key).at("url").truthy())
                    && !db.exists(
                        "SELECT 1 FROM work_orders WHERE source_audit_id = ? AND status != 'Closed' UNION ALL SELECT 1 FROM findings WHERE audit_id = ? AND status != 'Closed' LIMIT 1",
                        params![row.value("audit_id"), row.value("audit_id")],
                    )?
                {
                    items.push(inspection_item(row, "Close audit", "Signed and all work orders closed", "signoff"));
                }
            }
        }
        if has_page("inspections") && can("auditor") {
            // Visits a superior assigned to this person that nobody has started yet.
            for row in db.rows(
                "SELECT * FROM schedules WHERE status = 'Pending' AND assignees_data_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM inspection_sessions WHERE schedule_id = schedules.id) ORDER BY scheduled_date, id LIMIT 200",
                params![],
            )? {
                if assignees_of(db, row.get("assignees_data_id"))?.contains(&user_id) {
                    let zone = if row.get("zone").truthy() { row.str("zone") } else { "All Locations".into() };
                    items.push(obj([
                        ("type", "schedule".into()),
                        ("id", row.value("id")),
                        ("action", "Start assigned audit".into()),
                        ("view", "checklist".into()),
                        ("outlet", row.value("outlet")),
                        ("title", Value::str(format!("SCH-{:05} {}", row.int("id"), row.str("outlet")))),
                        ("detail", Value::str(format!("{} | {zone} | {}", row.str("outlet"), row.str("scheduled_date")))),
                    ]));
                }
            }
        }
        if reviews_requests(user) {
            for row in db.rows("SELECT * FROM work_requests WHERE status = 'Open' ORDER BY created_at LIMIT ?", params![LIMIT as i64])? {
                let by = if row.get("requested_by").truthy() { row.str("requested_by") } else { "someone".into() };
                items.push(obj([
                    ("type", "work_request".into()),
                    ("id", row.value("id")),
                    ("action", "Review work request".into()),
                    ("outlet", row.value("outlet")),
                    ("title", Value::str(format!("{} {}", row.str("request_ref"), row.str("item_name")).trim().to_string())),
                    ("detail", Value::str(format!("{} | {} | Requested by {by}", row.str("outlet"), row.str("location")))),
                ]));
            }
        }
        if has_page("work-orders") {
            for row in db.rows("SELECT * FROM work_orders WHERE status != 'Closed' ORDER BY CASE WHEN due_date IS NULL OR due_date = '' THEN 1 ELSE 0 END, due_date, id LIMIT 500", params![])? {
                let order = row.to_object();
                if assigned_to(user, &order) {
                    items.push(order_item(&order, "Resolve work order"));
                }
            }
        }
        let items = keep(user, items, "outlet"); // Only tasks at the account's outlets.
        // Changes waiting for this person's approval (they are not tied to one outlet).
        let approvable: Vec<Value> = CHANGE_RECORDS.iter().filter(|(record, ..)| list_has(user.at("actions"), &format!("{record}.approve"))).map(|(record, ..)| Value::str(*record)).collect();
        let mut params: Vec<&dyn ToSql> = vec![user.at("id")];
        params.extend(approvable.iter().map(|value| value as &dyn ToSql));
        let marks = if approvable.is_empty() { "NULL".to_string() } else { crate::db::marks(approvable.len()) };
        let approvals = db.count(&format!("SELECT count(*) FROM change_requests WHERE status = 'Pending' AND requested_by_user_id IS NOT ? AND record IN ({marks})"), &params)?;
        let findings = items_needing_request(db, user)?;
        let resets = if is_company_admin_user(user) { db.count("SELECT count(*) FROM password_reset_requests WHERE resolved_at IS NULL", params![])? } else { 0 };
        // How many things wait in each section, shown as a number beside its tab.
        let count = |test: &dyn Fn(&Value) -> bool| Value::Int(items.iter().filter(|item| test(item)).count() as i64);
        let counts = obj([
            ("guided", count(&|item| matches!(item.at("type").as_str(), Some("inspection" | "schedule")) && item.at("view").as_str() == Some("checklist"))),
            ("signoff", count(&|item| item.at("type").as_str() == Some("inspection") && item.at("view").as_str() == Some("signoff"))),
            ("findings", Value::Int(findings)),
            ("requests", count(&|item| item.at("type").as_str() == Some("work_request"))),
            ("orders", count(&|item| item.at("type").as_str() == Some("work_order"))),
            ("resets", Value::Int(resets)),
            ("notifications", Value::Int(unread)),
            ("approvals", Value::Int(approvals)),
        ]);
        let total = items.len();
        Ok(obj([
            ("items", Value::Array(items.into_iter().take(LIMIT).collect())),
            ("total", Value::Int(total as i64)),
            ("unreadNotifications", Value::Int(unread)),
            ("counts", counts),
            ("serverOutdated", Value::Bool(crate::api::server_outdated())),
        ]))
    })
}

/// Failed items (one per audit, item, and location) that nobody has requested work for yet.
fn items_needing_request(db: &Db, user: &Value) -> R<i64> {
    let permissions = user.at("permissions");
    let role = user.at("role").as_str().unwrap_or("");
    let relevant = ["findings", "inspections", "work-orders"].iter().any(|page| list_has(permissions, page));
    if role == "Department/PIC" || (!relevant && role != "Super") {
        return Ok(0);
    }
    if read_setting(db, "system.findingsEnabled", Value::Bool(true))?.as_bool() == Some(false) && role != "Super" {
        return Ok(0);
    }
    let rows = db.column(
        "SELECT findings.outlet FROM findings LEFT JOIN inspection_items ON inspection_items.id = findings.source_item_id
         GROUP BY findings.audit_id, COALESCE(findings.equipment_id, NULLIF(findings.item_name, ''), inspection_items.section), findings.location
         HAVING SUM(findings.status != 'Open') = 0 AND SUM(findings.work_request_id IS NOT NULL) = 0
            AND SUM(EXISTS(SELECT 1 FROM work_orders WHERE work_orders.source_finding_id = findings.id)) = 0",
        params![],
    )?;
    let outlets: Vec<Value> = rows.iter().map(|outlet: &Sql| obj([("outlet", outlet.to_value())])).collect();
    Ok(keep(user, outlets, "outlet").len() as i64)
}
