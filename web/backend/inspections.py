"""Inspections for the audit application."""
from backend.relational_values import load_value, save_value, hydrate_many
from datetime import datetime
from backend.scoring import summarize as summarize_score
from backend.audit_metadata import allocate_reference
from backend.common import finding_ref, image_labels, normalize_audit_date, normalized_inspection_name, priority_due_date
from backend.workflow import WorkflowError
from backend.database import connect, first_category, first_department, first_outlet, insert_record


def asset_details(db, ids):
    """Name, location, and category of these assets, by id."""
    ids, found = sorted(set(ids)), {}
    for start in range(0, len(ids), 500):
        chunk = ids[start:start + 500]
        for row in db.execute(f"SELECT id, name, asset_id, location, zone, category FROM equipment WHERE id IN ({','.join('?' for _ in chunk)})", chunk):
            found[row["id"]] = row
    return found


def complete_item_details(db, items):
    """Every check carries its asset's name, location, and category. A page could save checks
    without them (those ticked on a location page left before saving), which printed as "Item" in
    "All Locations"; they are filled in from the asset."""
    def asset_id(item):
        value = str(item.get("equipmentId") or "")
        return int(value) if value.isdigit() else None
    missing = [asset_id(item) for item in items if asset_id(item) and not (item.get("section") and item.get("location"))]
    if not missing:
        return items, False
    assets = asset_details(db, missing)
    completed = []
    for item in items:
        asset = assets.get(asset_id(item)) if not (item.get("section") and item.get("location")) else None
        if asset:
            item = dict(item)
            item["section"] = item.get("section") or asset["name"] or asset["asset_id"] or "Fixed Asset"
            item["location"] = item.get("location") or asset["location"] or asset["zone"] or ""
            item["category"] = item.get("category") or asset["category"] or ""
            item.setdefault("notApplicable", False)
            item.setdefault("score", 100 if item.get("passed") else 0)
        completed.append(item)
    return completed, True


def repair_item_details(db):
    """Once: fill the asset name and location of checks (and their findings) saved without them."""
    if db.execute("SELECT 1 FROM app_settings WHERE key = 'system.itemDetailsRepaired'").fetchone():
        return
    for row in db.execute("SELECT id, items_data_id FROM inspection_sessions").fetchall():
        items, changed = complete_item_details(db, load_value(row["items_data_id"] or "[]") or [])
        if changed:
            db.execute("UPDATE inspection_sessions SET items_data_id = ? WHERE id = ?", (save_value(db, items), row["id"]))
    findings = db.execute("SELECT id, equipment_id, item_name, location FROM findings WHERE equipment_id IS NOT NULL "
                          "AND (COALESCE(item_name, '') = '' OR COALESCE(location, '') IN ('', 'All Locations'))").fetchall()
    assets = asset_details(db, [row["equipment_id"] for row in findings])
    for row in findings:
        asset = assets.get(row["equipment_id"])
        if asset:
            location = row["location"]
            if (location or "") in ("", "All Locations"):
                location = asset["location"] or asset["zone"] or location
            db.execute("UPDATE findings SET item_name = ?, location = ? WHERE id = ?",
                       (row["item_name"] or asset["name"] or asset["asset_id"] or "", location, row["id"]))
    db.execute("INSERT INTO app_settings (key, value_data_id) VALUES ('system.itemDetailsRepaired', ?)", (save_value(db, True),))


def require_photo_evidence(items, settings):
    """A failed check always needs a photo; a passed one only when every inspected asset must have one."""
    every_asset = settings.get("system.requirePhotoEveryAsset") is not False
    for item in items:
        if item.get("notApplicable") or item.get("images"):
            continue
        if every_asset or not item.get("passed"):
            name = item.get("section") or item.get("item") or "each inspected asset"
            raise WorkflowError(f"Add a photo for {name} before completing the inspection")


def finalize_inspection(db, session_id, payload, now):
    items = payload.get("items") or []
    settings = {row["key"]: load_value(row["value_data_id"]) for row in db.execute("SELECT key, value_data_id FROM app_settings")}
    require_photo_evidence(items, settings)
    category_departments = {row["name"]: row["department"] for row in db.execute("SELECT name, department FROM categories WHERE COALESCE(department, '') != ''")}
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
    reference = db.execute("SELECT audit_ref FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()[0] or allocate_reference(db, audit_date, outlet)
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
        category = item.get("category") or first_category(db)
        # Without an explicit choice, a finding goes to the department responsible for its category.
        department = item.get("assignedDepartment") or item.get("department") or category_departments.get(category) or first_department(db)
        pic = item.get("pic", "")
        location = item.get("location") or payload.get("zone", "Unassigned")
        comment = item.get("notes") or item.get("item", "Inspection finding")
        due_date = priority_due_date(db, priority, now)
        images = save_value(db, item.get("images") or [])
        finding_id = insert_record(db, "findings", {
            "audit_id": audit_id, "audit_ref": reference, "business_unit": unit, "outlet": outlet, "location": location,
            "category": category, "priority": priority, "priority_classification": priority_row["classification"],
            "assigned_department": department, "pic": pic, "comment": comment, "status": "Open",
            "cause": item.get("cause", ""), "recommendation": item.get("recommendation", ""),
            "required_action": item.get("requiredAction", ""), "images_data_id": images, "due_date": due_date,
            "source_item_id": item_id, "equipment_id": int(item["equipmentId"]) if str(item.get("equipmentId") or "").isdigit() else None,
            # A casual audit's group is one finding for all the assets in it.
            "item_name": (f"{item.get('section') or ''} ×{item['groupCount']}" if int(item.get("groupCount") or 1) > 1 else item.get("section") or ""),
            "criterion": item.get("item") or "", "created_at": now, "updated_at": now,
        })
        finding_reference = finding_ref(finding_id, audit_date)
        db.execute("UPDATE findings SET finding_ref = ? WHERE id = ?", (finding_reference, finding_id))
        db.execute("UPDATE inspection_items SET finding_id = ? WHERE id = ?", (finding_id, item_id))
    db.execute("UPDATE inspection_sessions SET status = 'Completed', progress = 100, audit_id = ?, updated_at = ? WHERE id = ?", (audit_id, now, session_id))
    db.execute("UPDATE schedules SET status = 'Completed' WHERE id = (SELECT schedule_id FROM inspection_sessions WHERE id = ?)", (session_id,))
    return audit_id


def visit_locations_of(reference):
    """The locations chosen for a visit or its audit; empty means every location."""
    return (load_value(reference) or []) if reference else []


def visit_scope_of(reference):
    """How a visit's scope was chosen: by locations (the default), zones, or particular assets."""
    return (load_value(reference) or {"by": "locations"}) if reference else {"by": "locations"}


def inspection_sessions():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, closed_at, closed_by, audit_ref, inspection_name, business_unit, outlet, zone, audit_date, auditor, progress, status, audit_id, items_data_id, signatures_data_id, owner_user_id, created_at, updated_at, schedule_id
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
        signatures = load_value(item.pop("signatures_data_id") or "{}") or {}
        # Which sign-off roles are done, so lists can show what is still missing without loading each audit.
        item["signed"] = [key for key in ("auditedBy", "verifiedBy", "acknowledgedBy") if (signatures.get(key) or {}).get("url")]
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
                       status, closed_at
                FROM findings
                WHERE audit_id = ?
                ORDER BY id
                """,
                (row["audit_id"],),
            ).fetchall()
    if not row:
        return None
    data = dict(row)
    with connect() as db:
        data["items"], _ = complete_item_details(db, load_value(data.pop("items_data_id") or "[]") or [])
    data["signatures"] = load_value(data.pop("signatures_data_id") or "{}")
    data["visit_locations"] = visit_locations_of(data.pop("locations_data_id", None))
    data["visit_scope"] = visit_scope_of(data.pop("scope_data_id", None))
    with connect() as db:
        style = db.execute("SELECT style FROM audit_types WHERE name = ?", (data.get("audit_type") or "",)).fetchone()
    data["audit_style"] = style[0] if style and style[0] else "Detailed"
    data["inspection_name"] = normalized_inspection_name(data)
    data["audit_ref"] = audit["audit_ref"] if audit else (data.get("audit_ref") or "")
    data["scoring"] = load_value(audit["scoring_data_id"]) if audit and audit["scoring_data_id"] else None
    data["findings"] = hydrate_many(findings)
    return data


def schedule_items():
    with connect() as db:
        rows = db.execute("SELECT schedules.*, inspection_sessions.id AS inspection_id, inspection_sessions.audit_ref, inspection_sessions.inspection_name, inspection_sessions.progress AS progress, inspection_sessions.status AS inspection_status FROM schedules LEFT JOIN inspection_sessions ON inspection_sessions.schedule_id = schedules.id ORDER BY schedules.status IN ('Completed', 'Cancelled'), schedules.scheduled_date, schedules.id DESC").fetchall()
    return {"items": [dict(row) | {"schedule_ref": f"SCH-{row['id']:05d}", "visit_locations": visit_locations_of(row["locations_data_id"]),
                                    "visit_scope": visit_scope_of(row["scope_data_id"]),
                                    "assignees": visit_locations_of(row["assignees_data_id"])} for row in rows]}
