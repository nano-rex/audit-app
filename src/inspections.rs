//! Inspections: their checklists, completion into an audit with findings, and how they are listed.

use std::collections::{BTreeSet, HashMap};

use crate::audits::{allocate_reference, summarize};
use crate::clock;
use crate::common::{finding_ref, image_labels, normalize_audit_date, normalized_inspection_name, priority_due_date, value_is_digits};
use crate::db::{all_settings, first_department, first_outlet, hydrate_many, insert_record, load_or, load_value, marks, save_value, with_db, Db};
use crate::error::{bad, status, R};
use crate::json::{obj, strings, Value};
use crate::params;
use crate::sqlite::{Row, Sql, ToSql};

fn item_asset_id(item: &Value) -> Option<i64> {
    let value = item.at("equipmentId");
    if value.truthy() && value_is_digits(value) {
        value.py_str().parse().ok()
    } else {
        None
    }
}

/// Name, location, and category of these assets, by id.
pub fn asset_details(db: &Db, ids: &[i64]) -> R<HashMap<i64, Row>> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut found = HashMap::new();
    for chunk in ids.chunks(500) {
        let params: Vec<&dyn ToSql> = chunk.iter().map(|id| id as &dyn ToSql).collect();
        for row in db.rows(&format!("SELECT id, name, asset_id, location, zone, category FROM equipment WHERE id IN ({})", marks(chunk.len())), &params)? {
            found.insert(row.int("id"), row);
        }
    }
    Ok(found)
}

fn has_place(item: &Value) -> bool {
    item.at("section").truthy() && item.at("location").truthy()
}

fn first_text(values: &[String]) -> String {
    values.iter().find(|value| !value.is_empty()).cloned().unwrap_or_default()
}

/// Every check carries its asset's name, location, and category. A page could save checks
/// without them (those ticked on a location page left before saving), which printed as "Item" in
/// "All Locations"; they are filled in from the asset.
pub fn complete_item_details(db: &Db, items: &Value) -> R<(Value, bool)> {
    let list = items.items();
    let missing: Vec<i64> = list.iter().filter(|item| !has_place(item)).filter_map(item_asset_id).collect();
    if missing.is_empty() {
        return Ok((items.clone(), false));
    }
    let assets = asset_details(db, &missing)?;
    let mut completed = Vec::with_capacity(list.len());
    for item in list {
        let asset = if has_place(item) { None } else { item_asset_id(item).and_then(|id| assets.get(&id)) };
        match asset {
            Some(asset) => {
                let mut item = item.clone();
                if !item.at("section").truthy() {
                    let name = first_text(&[asset.str("name"), asset.str("asset_id"), "Fixed Asset".into()]);
                    item.set("section", name);
                }
                if !item.at("location").truthy() {
                    item.set("location", first_text(&[asset.str("location"), asset.str("zone")]));
                }
                if !item.at("category").truthy() {
                    item.set("category", asset.str("category"));
                }
                item.set_default("notApplicable", false);
                let score = if item.at("passed").truthy() { 100 } else { 0 };
                item.set_default("score", Value::Int(score));
                completed.push(item);
            }
            None => completed.push(item.clone()),
        }
    }
    Ok((Value::Array(completed), true))
}

/// Once: fill the asset name and location of checks (and their findings) saved without them.
pub fn repair_item_details(db: &Db) -> R<()> {
    if db.exists("SELECT 1 FROM app_settings WHERE key = 'system.itemDetailsRepaired'", params![])? {
        return Ok(());
    }
    for row in db.rows("SELECT id, items_data_id FROM inspection_sessions", params![])? {
        let items = load_or(db, row.get("items_data_id"), Value::array())?;
        let (items, changed) = complete_item_details(db, &items)?;
        if changed {
            db.exec("UPDATE inspection_sessions SET items_data_id = ? WHERE id = ?", params![save_value(db, &items)?, row.int("id")])?;
        }
    }
    let findings = db.rows(
        "SELECT id, equipment_id, item_name, location FROM findings WHERE equipment_id IS NOT NULL AND (COALESCE(item_name, '') = '' OR COALESCE(location, '') IN ('', 'All Locations'))",
        params![],
    )?;
    let assets = asset_details(db, &findings.iter().map(|row| row.int("equipment_id")).collect::<Vec<_>>())?;
    for row in findings {
        if let Some(asset) = assets.get(&row.int("equipment_id")) {
            let mut location = row.value("location");
            if matches!(location.text().as_str(), "" | "All Locations") {
                let from_asset = first_text(&[asset.str("location"), asset.str("zone")]);
                if !from_asset.is_empty() {
                    location = Value::str(from_asset);
                }
            }
            let name = first_text(&[row.str("item_name"), asset.str("name"), asset.str("asset_id")]);
            db.exec("UPDATE findings SET item_name = ?, location = ? WHERE id = ?", params![name, location, row.int("id")])?;
        }
    }
    db.exec("INSERT INTO app_settings (key, value_data_id) VALUES ('system.itemDetailsRepaired', ?)", params![save_value(db, &Value::Bool(true))?])?;
    Ok(())
}

/// A failed check always needs a photo; a passed one only when every inspected asset must have one.
fn require_photo_evidence(items: &[Value], settings: &Value) -> R<()> {
    let every_asset = settings.at("system.requirePhotoEveryAsset").as_bool() != Some(false);
    for item in items {
        if item.at("notApplicable").truthy() || item.at("images").truthy() {
            continue;
        }
        if every_asset || !item.at("passed").truthy() {
            let name = item.at("section").or(item.at("item"));
            let name = if name.truthy() { name.py_str() } else { "each inspected asset".into() };
            return Err(status(409, format!("Add a photo for {name} before completing the inspection")));
        }
    }
    Ok(())
}

fn default_priority(db: &Db) -> R<String> {
    let first = db.scalar("SELECT name FROM priority_levels WHERE active = 1 ORDER BY classification = 'Priority' DESC, due_days, name LIMIT 1", params![])?;
    let row = if first.is_null() { db.scalar("SELECT name FROM priority_levels ORDER BY due_days LIMIT 1", params![])? } else { first };
    Ok(if row.is_null() { "Priority".into() } else { row.text() })
}

fn as_id(value: &Value) -> Option<i64> {
    if value.truthy() && value_is_digits(value) {
        value.py_str().parse().ok()
    } else {
        None
    }
}

/// The assets a failed check is a finding for, as (asset id, name). A casual audit's group check
/// is a finding for each asset the auditor said has the issue (named "Downlight · AST-00012"); a
/// group without that choice, from before it existed, is one finding for the whole group.
fn finding_targets(item: &Value) -> R<Vec<(Option<i64>, String)>> {
    let section = item.at("section").text();
    let group = item.at("groupCount").or(&Value::Int(1)).py_int().map_err(bad)?;
    if group > 1 {
        let members: BTreeSet<i64> = item.at("groupIds").items().iter().filter_map(as_id).collect();
        let affected: Vec<i64> = item.at("affectedIds").items().iter().filter_map(as_id).filter(|id| members.contains(id) || members.is_empty()).collect();
        let labels = item.at("affectedLabels");
        if !affected.is_empty() {
            return Ok(crate::common::unique(affected)
                .into_iter()
                .map(|asset| {
                    let label = labels.entries().iter().find(|(key, _)| *key == asset.to_string()).map(|(_, value)| value).filter(|value| value.truthy());
                    let label = label.map(Value::py_str).unwrap_or_else(|| format!("#{asset}"));
                    (Some(asset), format!("{section} · {label}"))
                })
                .collect());
        }
        return Ok(vec![(as_id(item.at("equipmentId")), format!("{section} ×{group}"))]);
    }
    Ok(vec![(as_id(item.at("equipmentId")), section)])
}

fn text_or(value: &Value, fallback: &str) -> Value {
    if value.truthy() { value.clone() } else { Value::str(fallback) }
}

/// Complete an inspection: the audit with its score, a row per check, and a finding per failure.
pub fn finalize_inspection(db: &Db, session_id: i64, payload: &Value, now: i64) -> R<i64> {
    let items = payload.at("items").items().to_vec();
    let settings = all_settings(db, None)?;
    require_photo_evidence(&items, &settings)?;
    let summary = summarize(&Value::Array(items.clone()), &settings)?;
    let audit_date = normalize_audit_date(payload.at("auditDate"));
    let outlet = if payload.at("outlet").truthy() { payload.at("outlet").clone() } else { Value::str(first_outlet(db)?) };
    let unit = payload.get("businessUnit").cloned().unwrap_or_else(|| "Ottotree".into());
    let audit_time = if payload.at("auditTime").truthy() { payload.at("auditTime").clone() } else { Value::str(clock::local_now().hm()) };
    let audit_id = insert_record(db, "audits", &[
        ("business_unit", unit.clone()),
        ("outlet", outlet.clone()),
        ("branch", payload.get("zone").cloned().unwrap_or_else(|| "All Locations".into())),
        ("audit_date", Value::str(&audit_date)),
        ("audit_time", audit_time),
        ("auditor", payload.get("auditor").cloned().unwrap_or_else(|| "Unnamed Auditor".into())),
        ("audit_type", text_or(payload.at("auditType"), "Standard")),
        ("remarks", payload.get("remarks").cloned().unwrap_or_else(|| "".into())),
        ("score", summary.at("score").clone()),
        ("scoring_data_id", Value::Int(save_value(db, &summary)?)),
        ("created_at", Value::Int(now)),
    ])?;
    let stored = db.scalar("SELECT audit_ref FROM inspection_sessions WHERE id = ?", params![session_id])?.text();
    let reference = if stored.is_empty() { allocate_reference(db, &Value::str(&audit_date), &outlet.text())? } else { stored };
    db.exec("UPDATE audits SET audit_ref = ? WHERE id = ?", params![reference, audit_id])?;
    for item in &items {
        let passed = item.at("passed").truthy() || item.at("notApplicable").truthy();
        let item_id = insert_record(db, "inspection_items", &[
            ("audit_id", Value::Int(audit_id)),
            ("section", item.get("section").cloned().unwrap_or_else(|| "Fixed Asset".into())),
            ("item", item.get("item").cloned().unwrap_or_else(|| "Checklist item".into())),
            ("score", Value::Int(if passed { 100 } else { 0 })),
            ("notes", item.get("notes").cloned().unwrap_or_else(|| "".into())),
            ("evidence_status", Value::str(image_labels(item.at("images")).join(", "))),
        ])?;
        if passed {
            continue;
        }
        // Details are set on the Findings page afterwards; until then a finding takes the most
        // urgent active priority level.
        let priority = if item.at("priority").truthy() { item.at("priority").clone() } else { Value::str(default_priority(db)?) };
        let classification = db.row("SELECT classification FROM priority_levels WHERE name = ?", params![priority])?
            .ok_or_else(|| bad("Select a configured priority for every finding"))?
            .value("classification");
        // A finding is filed under its asset's type (kept in the category column).
        let category = text_or(item.at("category"), "");
        let department = item.at("assignedDepartment").or(item.at("department"));
        let department = if department.truthy() { department.clone() } else { Value::str(first_department(db)?) };
        let pic = item.get("pic").cloned().unwrap_or_else(|| "".into());
        let location = if item.at("location").truthy() { item.at("location").clone() } else { payload.get("zone").cloned().unwrap_or_else(|| "Unassigned".into()) };
        let comment = if item.at("notes").truthy() { item.at("notes").clone() } else { item.get("item").cloned().unwrap_or_else(|| "Inspection finding".into()) };
        let due_date = priority_due_date(db, &priority, now)?;
        for (equipment_id, item_name) in finding_targets(item)? {
            let images = save_value(db, item.at("images").or(&Value::array()))?;
            let finding_id = insert_record(db, "findings", &[
                ("audit_id", Value::Int(audit_id)),
                ("audit_ref", Value::str(&reference)),
                ("business_unit", unit.clone()),
                ("outlet", outlet.clone()),
                ("location", location.clone()),
                ("category", category.clone()),
                ("priority", priority.clone()),
                ("priority_classification", classification.clone()),
                ("assigned_department", department.clone()),
                ("pic", pic.clone()),
                ("comment", comment.clone()),
                ("status", "Open".into()),
                ("cause", item.get("cause").cloned().unwrap_or_else(|| "".into())),
                ("recommendation", item.get("recommendation").cloned().unwrap_or_else(|| "".into())),
                ("required_action", item.get("requiredAction").cloned().unwrap_or_else(|| "".into())),
                ("images_data_id", Value::Int(images)),
                ("due_date", Value::str(&due_date)),
                ("source_item_id", Value::Int(item_id)),
                ("equipment_id", equipment_id.map(Value::Int).unwrap_or(Value::Null)),
                ("item_name", Value::str(item_name)),
                ("criterion", text_or(item.at("item"), "")),
                ("created_at", Value::Int(now)),
                ("updated_at", Value::Int(now)),
            ])?;
            db.exec("UPDATE findings SET finding_ref = ? WHERE id = ?", params![finding_ref(finding_id, &Value::str(&audit_date)), finding_id])?;
            db.exec("UPDATE inspection_items SET finding_id = ? WHERE id = ?", params![finding_id, item_id])?;
        }
    }
    db.exec("UPDATE inspection_sessions SET status = 'Completed', progress = 100, audit_id = ?, updated_at = ? WHERE id = ?", params![audit_id, now, session_id])?;
    db.exec("UPDATE schedules SET status = 'Completed' WHERE id = (SELECT schedule_id FROM inspection_sessions WHERE id = ?)", params![session_id])?;
    Ok(audit_id)
}

/// The locations chosen for a visit or its audit; empty means every location.
pub fn visit_locations_of(db: &Db, reference: &Sql) -> R<Value> {
    let value = load_or(db, reference, Value::array())?;
    Ok(if value.truthy() { value } else { Value::array() })
}

/// How a visit's scope was chosen: by locations (the default), zones, or particular assets.
pub fn visit_scope_of(db: &Db, reference: &Sql) -> R<Value> {
    let value = load_or(db, reference, Value::Null)?;
    Ok(if value.truthy() { value } else { obj([("by", "locations".into())]) })
}

fn sorted_set(values: impl Iterator<Item = String>) -> Value {
    let set: BTreeSet<String> = values.filter(|value| !value.is_empty()).collect();
    strings(&set.into_iter().collect::<Vec<_>>())
}

pub fn inspection_sessions() -> R<Value> {
    with_db(|db| {
        let rows = db.rows(
            "SELECT id, closed_at, closed_by, audit_ref, inspection_name, business_unit, outlet, zone, audit_date, auditor, progress, status, audit_id, items_data_id, signatures_data_id, owner_user_id, created_at, updated_at, schedule_id FROM inspection_sessions ORDER BY updated_at DESC, id DESC",
            params![],
        )?;
        let items = crate::db::load_values(db, &rows.iter().map(|row| if row.get("items_data_id").truthy() { row.get("items_data_id").clone() } else { Sql::Text("[]".into()) }).collect::<Vec<_>>())?;
        let signatures = crate::db::load_values(db, &rows.iter().map(|row| if row.get("signatures_data_id").truthy() { row.get("signatures_data_id").clone() } else { Sql::Text("{}".into()) }).collect::<Vec<_>>())?;
        let mut out = Vec::with_capacity(rows.len());
        for ((row, session_items), signed) in rows.iter().zip(items).zip(signatures) {
            let mut item = row.to_object();
            item.remove("items_data_id");
            item.remove("signatures_data_id");
            let signed = if signed.truthy() { signed } else { Value::object() };
            // Which sign-off roles are done, so lists can show what is still missing without loading each audit.
            item.set("signed", strings(&["auditedBy", "verifiedBy", "acknowledgedBy"].iter().filter(|key| signed.at(key).at("url").truthy()).copied().collect::<Vec<_>>()));
            item.set("inspection_name", normalized_inspection_name(&item));
            let entries = session_items.items();
            item.set("locations", sorted_set(entries.iter().filter(|entry| entry.at("location").truthy()).map(|entry| entry.at("location").py_str())));
            item.set("categories", sorted_set(entries.iter().filter(|entry| entry.at("category").truthy()).map(|entry| entry.at("category").py_str())));
            item.set("priorities", sorted_set(entries.iter().filter(|entry| entry.at("priority").truthy()).map(|entry| entry.at("priority").py_str())));
            item.set("departments", sorted_set(entries.iter().map(|entry| entry.at("assignedDepartment").or(entry.at("department")).text())));
            item.set("pics", sorted_set(entries.iter().filter(|entry| entry.at("pic").truthy()).map(|entry| entry.at("pic").py_str())));
            item.set("findings_count", Value::Int(entries.iter().filter(|entry| !entry.at("passed").truthy() && !entry.at("notApplicable").truthy()).count() as i64));
            out.push(item);
        }
        Ok(obj([("items", Value::Array(out))]))
    })
}

/// One inspection in full: checks, signatures, visit scope, score, and findings.
pub fn inspection_session(session_id: i64) -> R<Option<Value>> {
    with_db(|db| inspection_session_in(db, session_id))
}

pub fn inspection_session_in(db: &Db, session_id: i64) -> R<Option<Value>> {
    let Some(row) = db.row("SELECT * FROM inspection_sessions WHERE id = ?", params![session_id])? else { return Ok(None) };
    let mut audit = None;
    let mut findings = Vec::new();
    if row.get("audit_id").truthy() {
        audit = db.row("SELECT audit_ref, scoring_data_id FROM audits WHERE id = ?", params![row.int("audit_id")])?;
        findings = db.rows(
            "SELECT finding_ref, location, category, priority, priority_classification, assigned_department, pic, comment, cause, recommendation, required_action, images_data_id, due_date, status, closed_at FROM findings WHERE audit_id = ? ORDER BY id",
            params![row.int("audit_id")],
        )?;
    }
    let mut data = row.to_object();
    data.remove("items_data_id");
    let (items, _) = complete_item_details(db, &load_or(db, row.get("items_data_id"), Value::array())?)?;
    data.set("items", if items.is_null() { Value::array() } else { items });
    data.remove("signatures_data_id");
    data.set("signatures", load_or(db, row.get("signatures_data_id"), Value::object())?);
    data.remove("locations_data_id");
    data.set("visit_locations", visit_locations_of(db, row.get("locations_data_id"))?);
    data.remove("scope_data_id");
    data.set("visit_scope", visit_scope_of(db, row.get("scope_data_id"))?);
    let style = db.scalar("SELECT style FROM audit_types WHERE name = ?", params![row.str("audit_type")])?;
    data.set("audit_style", if style.truthy() { style.to_value() } else { "Detailed".into() });
    data.set("inspection_name", normalized_inspection_name(&data));
    let audit_ref = match &audit {
        Some(audit) => audit.value("audit_ref"),
        None => Value::str(row.str("audit_ref")),
    };
    data.set("audit_ref", audit_ref);
    let scoring = match &audit {
        Some(audit) if audit.get("scoring_data_id").truthy() => load_value(db, audit.get("scoring_data_id"))?,
        _ => Value::Null,
    };
    data.set("scoring", scoring);
    data.set("findings", Value::Array(hydrate_many(db, &findings)?));
    Ok(Some(data))
}

/// A schedule row as the lists show it.
pub fn schedule_view(db: &Db, row: &Row) -> R<Value> {
    let mut item = row.to_object();
    item.set("schedule_ref", format!("SCH-{:05}", row.int("id")));
    item.set("visit_locations", visit_locations_of(db, row.get("locations_data_id"))?);
    item.set("visit_scope", visit_scope_of(db, row.get("scope_data_id"))?);
    item.set("assignees", visit_locations_of(db, row.get("assignees_data_id"))?);
    Ok(item)
}

pub fn schedule_items() -> R<Value> {
    with_db(|db| {
        let rows = db.rows(
            "SELECT schedules.*, inspection_sessions.id AS inspection_id, inspection_sessions.audit_ref, inspection_sessions.inspection_name, inspection_sessions.progress AS progress, inspection_sessions.status AS inspection_status FROM schedules LEFT JOIN inspection_sessions ON inspection_sessions.schedule_id = schedules.id ORDER BY schedules.status IN ('Completed', 'Cancelled'), schedules.scheduled_date, schedules.id DESC",
            params![],
        )?;
        let items = rows.iter().map(|row| schedule_view(db, row)).collect::<R<Vec<_>>>()?;
        Ok(obj([("items", Value::Array(items))]))
    })
}
