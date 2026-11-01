//! Editing and removing an audit's findings on the Findings page.
//!
//! The details once recorded during the inspection (asset type, priority, department, PIC, due
//! date, description, cause, recommendation, required action, photos) are set here, after the audit.

use crate::activity::Entry;
use crate::api::{ok_true, Ctx, Reply};
use crate::clock::{self, Date};
use crate::db::{data_value, update_record, with_db};
use crate::error::{bad, forbidden, status, R};
use crate::json::Value;
use crate::params;

const TEXT_FIELDS: [(&str, &str); 6] = [("category", "category"), ("pic", "pic"), ("comment", "comment"), ("cause", "cause"), ("recommendation", "recommendation"), ("requiredAction", "required_action")];

fn finding_id(ctx: &Ctx) -> R<i64> {
    let tail = ctx.tail();
    if !crate::common::is_digits(tail) {
        return Err(status(404, "Finding not found"));
    }
    tail.parse().map_err(|_| status(404, "Finding not found"))
}

/// A Department/PIC account follows up findings but does not change or remove them.
fn editable(user: &Value) -> R<()> {
    if user.at("role").as_str() == Some("Department/PIC") {
        return Err(forbidden("Only an auditor or supervisor can change findings"));
    }
    Ok(())
}

pub fn patch_finding(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    editable(&user)?;
    let record_id = finding_id(ctx)?;
    let now = clock::now_ms();
    with_db(|db| {
        db.begin_immediate()?;
        let row = db.row("SELECT * FROM findings WHERE id = ?", params![record_id])?.ok_or_else(|| status(404, "Finding not found"))?;
        if row.str("status") == "Closed" {
            return Err(status(409, "A closed finding cannot be changed"));
        }
        let mut values: Vec<(String, Value)> = Vec::new();
        for (key, column) in TEXT_FIELDS {
            if let Some(value) = payload.get(key).filter(|value| !value.is_null()) {
                values.push((column.into(), Value::str(value.py_str().trim())));
            }
        }
        if values.iter().any(|(column, value)| column == "comment" && !value.truthy()) {
            return Err(bad("Describe what is wrong"));
        }
        if payload.at("priority").truthy() {
            let level = db.row("SELECT classification FROM priority_levels WHERE name = ?", params![payload.at("priority")])?.ok_or_else(|| bad("Choose one of the priority levels"))?;
            values.push(("priority".into(), payload.at("priority").clone()));
            values.push(("priority_classification".into(), level.value("classification")));
        }
        if payload.at("department").truthy() {
            if !db.exists("SELECT 1 FROM departments WHERE code = ?", params![payload.at("department")])? {
                return Err(bad("Choose one of the departments"));
            }
            values.push(("assigned_department".into(), payload.at("department").clone()));
        }
        if payload.has("dueDate") {
            let due = payload.at("dueDate").text().trim().to_string();
            if !due.is_empty() && Date::parse_iso(&due).is_none() {
                return Err(bad("Enter a valid due date"));
            }
            values.push(("due_date".into(), Value::str(due)));
        }
        if payload.has("images") {
            values.push(("images_data_id".into(), Value::Int(data_value(db, payload.at("images").or(&Value::array()), Value::array())?)));
        }
        if values.is_empty() {
            return Err(bad("Nothing to change"));
        }
        values.push(("updated_at".into(), Value::Int(now)));
        update_record(db, "findings", &values, record_id)?;
        Entry::new(&user, "finding_edited", "finding", record_id).reference(row.str("finding_ref")).outlet(row.str("outlet")).unit(row.str("business_unit")).at(now).log(db)?;
        ok_true()
    })
}

pub fn delete_finding(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    editable(&user)?;
    let record_id = finding_id(ctx)?;
    with_db(|db| {
        db.begin_immediate()?;
        let row = db.row("SELECT * FROM findings WHERE id = ?", params![record_id])?.ok_or_else(|| status(404, "Finding not found"))?;
        if row.get("work_request_id").truthy() || db.exists("SELECT 1 FROM work_orders WHERE source_finding_id = ?", params![record_id])? {
            return Err(status(409, "This finding has a work request or work order; decline or close that instead"));
        }
        if row.str("status") == "Closed" {
            return Err(status(409, "A closed finding is kept with its audit"));
        }
        db.exec("UPDATE inspection_items SET finding_id = NULL WHERE finding_id = ?", params![record_id])?;
        db.exec("DELETE FROM findings WHERE id = ?", params![record_id])?;
        let detail = format!("{} · {}", row.str("item_name"), row.str("criterion"));
        let detail = detail.trim_matches(|c| c == ' ' || c == '·').to_string();
        Entry::new(&user, "finding_deleted", "finding", record_id).reference(row.str("finding_ref")).outlet(row.str("outlet")).detail(detail).unit(row.str("business_unit")).log(db)?;
        ok_true()
    })
}
