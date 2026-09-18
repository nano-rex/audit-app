"""Inspections for the audit application."""
from relational_values import load_value, save_value, hydrate
from datetime import datetime
from scoring import summarize as summarize_score
from common import audit_ref, create_notification, finding_ref, image_labels, normalize_audit_date, normalized_inspection_name, priority_due_date, sla_status, work_order_ref
from database import connect, first_category, first_department, first_outlet, insert_record


def finalize_inspection(db, session_id, payload, now):
    items = payload.get("items") or []
    settings = {row["key"]: load_value(row["value_data_id"]) for row in db.execute("SELECT key, value_data_id FROM app_settings")}
    summary = summarize_score(items, settings)
    audit_date = normalize_audit_date(payload.get("auditDate"))
    outlet = payload.get("outlet") or first_outlet(db)
    unit = payload.get("businessUnit", "Ottotree")
    audit_id = insert_record(db, "audits", {
        "business_unit": unit, "outlet": outlet, "branch": payload.get("zone", "All Locations"),
        "audit_date": audit_date, "audit_time": payload.get("auditTime") or datetime.now().strftime("%H:%M"),
        "auditor": payload.get("auditor", "Unnamed Auditor"), "audit_type": payload.get("auditType") or "Standard",
        "remarks": payload.get("remarks", ""), "score": summary["score"], "scoring_data_id": save_value(db, summary), "created_at": now,
    })
    reference = audit_ref(audit_id, audit_date)
    db.execute("UPDATE audits SET audit_ref = ? WHERE id = ?", (reference, audit_id))
    for item in items:
        item_id = insert_record(db, "inspection_items", {
            "audit_id": audit_id, "section": item.get("section", "Fixed Asset"), "item": item.get("item", "Checklist item"),
            "score": 100 if item.get("passed") or item.get("notApplicable") else 0,
            "notes": item.get("notes", ""), "evidence_status": ", ".join(image_labels(item.get("images"))),
        })
        if item.get("passed") or item.get("notApplicable"):
            continue
        priority = item.get("priority") or "Priority"
        priority_row = db.execute("SELECT classification FROM priority_levels WHERE name = ?", (priority,)).fetchone()
        if not priority_row:
            raise ValueError("Select a configured priority for every finding")
        department = item.get("assignedDepartment") or item.get("department") or first_department(db)
        category = item.get("category") or first_category(db)
        pic = item.get("pic", "")
        location = item.get("location") or payload.get("zone", "Unassigned")
        comment = item.get("notes") or item.get("item", "Inspection finding")
        due_date = priority_due_date(db, priority, now)
        images = save_value(db, item.get("images") or [])
        finding_id = insert_record(db, "findings", {
            "audit_id": audit_id, "audit_ref": reference, "business_unit": unit, "outlet": outlet, "location": location,
            "category": category, "priority": priority, "priority_classification": priority_row["classification"],
            "assigned_department": department, "pic": pic, "comment": comment, "status": "Assigned",
            "cause": item.get("cause", ""), "recommendation": item.get("recommendation", ""),
            "required_action": item.get("requiredAction", ""), "images_data_id": images, "due_date": due_date,
            "source_item_id": item_id, "created_at": now, "updated_at": now,
        })
        finding_reference = finding_ref(finding_id, audit_date)
        db.execute("UPDATE findings SET finding_ref = ? WHERE id = ?", (finding_reference, finding_id))
        db.execute("UPDATE inspection_items SET finding_id = ? WHERE id = ?", (finding_id, item_id))
        order_id = insert_record(db, "work_orders", {
            "business_unit": unit, "outlet": outlet, "zone": location, "request_type": department, "category": category,
            "priority": priority, "title": f"{finding_reference} - {item.get('section', 'Fixed Asset')} - {item.get('item', 'Finding')}",
            "description": comment, "assignee": pic or department, "pic": pic, "status": "Assigned",
            "cause": item.get("cause", ""), "recommendation": item.get("recommendation", ""),
            "required_action": item.get("requiredAction", ""), "images_data_id": images,
            "due_date": due_date, "sla_status": sla_status("Assigned", due_date),
            "source_audit_id": audit_id, "source_item_id": item_id, "source_finding_id": finding_id,
            "outlet_confirmed": 0, "created_at": now,
        })
        db.execute("UPDATE work_orders SET work_order_ref = ? WHERE id = ?", (work_order_ref(order_id, audit_date), order_id))
        create_notification(db, "Finding assigned", f"{finding_reference}: {department} / {pic or 'PIC unassigned'}", related_type="work-order", related_id=order_id)
    db.execute("UPDATE inspection_sessions SET status = 'Completed', progress = 100, audit_id = ?, updated_at = ? WHERE id = ?", (audit_id, now, session_id))
    db.execute("UPDATE schedules SET status = 'Completed' WHERE id = (SELECT schedule_id FROM inspection_sessions WHERE id = ?)", (session_id,))
    return audit_id


def inspection_sessions():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, inspection_name, business_unit, outlet, zone, audit_date, auditor, progress, status, audit_id, items_data_id, created_at, updated_at, schedule_id
            FROM inspection_sessions
            ORDER BY updated_at DESC, id DESC
            """
        ).fetchall()
        rows = [dict(row) for row in rows]
        for row in rows:
            row["items_data_id"] = load_value(row["items_data_id"] or "[]")
    items = []
    for row in rows:
        item = dict(row)
        session_items = load_value(item.pop("items_data_id") or "[]")
        item["inspection_name"] = normalized_inspection_name(item)
        item["locations"] = sorted({entry.get("location", "") for entry in session_items if entry.get("location")})
        item["categories"] = sorted({entry.get("category", "") for entry in session_items if entry.get("category")})
        item["priorities"] = sorted({entry.get("priority", "") for entry in session_items if entry.get("priority")})
        item["departments"] = sorted({entry.get("assignedDepartment", "") or entry.get("department", "") for entry in session_items if entry.get("assignedDepartment") or entry.get("department")})
        item["pics"] = sorted({entry.get("pic", "") for entry in session_items if entry.get("pic")})
        item["findings_count"] = sum(1 for entry in session_items if not entry.get("passed") and not entry.get("notApplicable"))
        items.append(item)
    return {"items": items}


def inspection_session(session_id):
    with connect() as db:
        row = db.execute("SELECT * FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()
        audit = None
        findings = []
        if row and row["audit_id"]:
            audit = db.execute("SELECT audit_ref, scoring_data_id FROM audits WHERE id = ?", (row["audit_id"],)).fetchone()
            findings = db.execute(
                """
                SELECT finding_ref, location, category, priority, priority_classification, assigned_department, pic, comment,
                       cause, recommendation, required_action, images_data_id, due_date,
                       status, corrective_action, completion_date, completion_photo_data_id,
                       completion_remark, verified_by, verified_at, verification_remark, closed_at
                FROM findings
                WHERE audit_id = ?
                ORDER BY id
                """,
                (row["audit_id"],),
            ).fetchall()
    if not row:
        return None
    data = dict(row)
    data["items"] = load_value(data.pop("items_data_id") or "[]")
    data["signatures"] = load_value(data.pop("signatures_data_id") or "{}")
    data["inspection_name"] = normalized_inspection_name(data)
    data["audit_ref"] = audit["audit_ref"] if audit else ""
    data["scoring"] = load_value(audit["scoring_data_id"]) if audit and audit["scoring_data_id"] else None
    data["findings"] = [hydrate(item) for item in findings]
    return data


def schedule_items():
    with connect() as db:
        rows = db.execute("SELECT schedules.*, inspection_sessions.id AS inspection_id, inspection_sessions.progress AS progress, inspection_sessions.status AS inspection_status FROM schedules LEFT JOIN inspection_sessions ON inspection_sessions.schedule_id = schedules.id ORDER BY schedules.scheduled_date, schedules.id DESC").fetchall()
    return {"items": [dict(row) | {"schedule_ref": f"SCH-{row['id']:05d}"} for row in rows]}
