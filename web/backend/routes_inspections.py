"""Routes inspections for the audit application."""
from backend.relational_values import load_value, save_value
from backend.permissions import authorize_inspection_update
from backend.inspection_notifications import notify_inspection
import time
from datetime import datetime
from backend.audit_metadata import allocate_reference, validate_metadata
from backend.common import finding_ref, inspection_name, inspection_progress, normalize_audit_date, work_order_ref
from backend.database import connect, first_category, first_department, first_outlet, insert_record
from backend.inspections import finalize_inspection


def append_inspection_photos(db, items, now):
    """Add newly uploaded inspection evidence to each asset's photo library."""
    for item in items:
        try:
            equipment_id = int(item.get("equipmentId") or 0)
        except (TypeError, ValueError):
            continue
        images = item.get("images") or []
        if not equipment_id or not isinstance(images, list) or not images:
            continue
        row = db.execute("SELECT photos_data_id FROM equipment WHERE id = ?", (equipment_id,)).fetchone()
        if not row:
            continue
        existing = load_value(row["photos_data_id"] or "[]") or []
        if not isinstance(existing, list):
            existing = []
        known = {image.get("id") or image.get("url") for image in existing if isinstance(image, dict)}
        additions = []
        for image in images:
            if not isinstance(image, dict):
                continue
            key = image.get("id") or image.get("url")
            if not key or key in known:
                continue
            additions.append({**image, "uploadedAt": image.get("uploadedAt") or datetime.fromtimestamp(now / 1000).isoformat()})
            known.add(key)
        if additions:
            db.execute("UPDATE equipment SET photos_data_id = ? WHERE id = ?", (save_value(db, existing + additions), equipment_id))


def post_audits(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        cursor = db.execute(
            """
            INSERT INTO audits
            (business_unit, outlet, branch, audit_date, auditor, audit_type, score, created_at)
            VALUES (?, ?, 'LONG', ?, ?, ?, ?, ?)
            """,
            (
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or default_outlet,
                normalize_audit_date(payload.get("auditDate")),
                payload.get("auditor", "Unnamed Auditor"),
                payload.get("auditType", "Standard"),
                max(0, min(100, int(payload.get("score") or 0))),
                now,
            ),
        )
        db.execute(
            "UPDATE audits SET audit_ref = ? WHERE id = ?",
            (allocate_reference(db, normalize_audit_date(payload.get("auditDate"))), cursor.lastrowid),
        )
    self.json({"ok": True})


def post_inspections(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        default_department = first_department(db)
        default_category = first_category(db)
        items = payload.get("items") or []
        score_values = [max(0, min(100, int(item.get("score") or 0))) for item in items]
        total_score = round(sum(score_values) / len(score_values)) if score_values else 0
        cursor = db.execute(
            """
            INSERT INTO audits
            (business_unit, outlet, branch, audit_date, auditor, audit_type, score, created_at)
            VALUES (?, ?, ?, ?, ?, 'Inspection', ?, ?)
            """,
            (
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or default_outlet,
                payload.get("zone", "Unassigned"),
                normalize_audit_date(payload.get("auditDate")),
                payload.get("auditor", "Unnamed Auditor"),
                total_score,
                now,
            ),
        )
        audit_id = cursor.lastrowid
        audit_reference = allocate_reference(db, normalize_audit_date(payload.get("auditDate")))
        db.execute("UPDATE audits SET audit_ref = ? WHERE id = ?", (audit_reference, audit_id))
        for item in items:
            cursor = db.execute(
                """
                INSERT INTO inspection_items
                (audit_id, section, item, score, notes, evidence_status)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                (
                    audit_id,
                    item.get("section", "General"),
                    item.get("item", "Checklist item"),
                    max(0, min(100, int(item.get("score") or 0))),
                    item.get("notes", ""),
                    item.get("evidenceStatus", "Missing"),
                ),
            )
            item_score = max(0, min(100, int(item.get("score") or 0)))
            if item_score < 70:
                finding_cursor = db.execute(
                    """
                    INSERT INTO findings
                    (audit_id, audit_ref, business_unit, outlet, location, category, priority,
                     assigned_department, pic, comment, status, source_item_id, created_at, updated_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, '', ?, 'Assigned', ?, ?, ?)
                    """,
                    (
                        audit_id,
                        audit_reference,
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or default_outlet,
                        payload.get("zone", "Unassigned"),
                        item.get("category") or default_category,
                        "High" if item_score < 60 else "Medium",
                        default_department,
                        item.get("notes", "") or item.get("item", "Inspection finding"),
                        cursor.lastrowid,
                        now,
                        now,
                    ),
                )
                finding_id = finding_cursor.lastrowid
                finding_reference = finding_ref(finding_id, normalize_audit_date(payload.get("auditDate")))
                db.execute("UPDATE findings SET finding_ref = ? WHERE id = ?", (finding_reference, finding_id))
                db.execute("UPDATE inspection_items SET finding_id = ? WHERE id = ?", (finding_id, cursor.lastrowid))
            if item_score < 70 and not item.get("workOrderRequested"):
                work_order_cursor = db.execute(
                    """
                    INSERT INTO work_orders
                    (business_unit, outlet, zone, request_type, category, priority, title, description,
                     assignee, pic, status, action_taken, completion_date, completion_remark, completion_photo_data_id,
                     verified_by, verified_at, verification_remark, closed_at, outlet_confirmed,
                     source_audit_id, source_item_id, source_finding_id, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, '', 'Assigned', '', '', '', ?, '', '', '', '', 0, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or default_outlet,
                        payload.get("zone", "Unassigned"),
                        default_department,
                        item.get("category") or default_category,
                        "High" if item_score < 60 else "Medium",
                        item.get("item", "Inspection issue"),
                        item.get("notes", "") or "Created from low inspection score",
                        "Technical Support",
                        save_value(db, []),
                        audit_id,
                        cursor.lastrowid,
                        finding_id,
                        now,
                    ),
                )
                db.execute(
                    "UPDATE work_orders SET work_order_ref = ? WHERE id = ?",
                    (work_order_ref(work_order_cursor.lastrowid, normalize_audit_date(payload.get("auditDate"))), work_order_cursor.lastrowid),
                )
        append_inspection_photos(db, items, now)
    self.json({"ok": True})


def post_inspection_sessions(self, parsed, payload=None):
    user = self.current_user()
    payload, _ = authorize_inspection_update(user, payload)
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        items = payload.get("items") or []
        progress = inspection_progress(items)
        if payload.get("complete") and progress < 100:
            self.json({"error": "Inspection is incomplete"}, 409)
            return
        status = "Completed" if payload.get("complete") and progress == 100 else "Draft"
        cursor = db.execute(
            """
            INSERT INTO inspection_sessions
            (business_unit, outlet, zone, audit_date, auditor, items_data_id,
             progress, status, signatures_data_id, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or default_outlet,
                payload.get("zone", "Unassigned"),
                normalize_audit_date(payload.get("auditDate")),
                payload.get("auditor", "Unnamed Inspector"),
                save_value(db, items),
                progress,
                status,
                save_value(db, payload.get("signatures") or {}),
                now,
                now,
            ),
        )
        session_id = cursor.lastrowid
        session_name = inspection_name({
            "id": session_id,
            "outlet": payload.get("outlet") or default_outlet,
            "audit_date": normalize_audit_date(payload.get("auditDate")),
        })
        db.execute("UPDATE inspection_sessions SET inspection_name = ? WHERE id = ?", (session_name, session_id))
        db.execute("UPDATE inspection_sessions SET owner_user_id = ? WHERE id = ?", (user["id"], session_id))
        db.execute("UPDATE inspection_sessions SET audit_ref = ? WHERE id = ?", (allocate_reference(db, normalize_audit_date(payload.get("auditDate"))), session_id))
        save_session_metadata(db, session_id, payload)
        append_inspection_photos(db, items, now)
        audit_id = None
        if status == "Completed":
            audit_id = finalize_inspection(db, session_id, payload, now)
        notify_inspection(db, session_id, user, payload, changed=True)
        db.commit()
        self.json({"ok": True, "id": session_id, "inspectionName": session_name, "progress": progress, "status": status, "auditId": audit_id})
        return
    self.json({"ok": True})


def post_schedules(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        cursor = db.execute(
            """
            INSERT INTO schedules
            (business_unit, outlet, zone, scheduled_date, auditor, remarks, status, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or default_outlet,
                payload.get("zone", "Unassigned"),
                payload.get("scheduledDate", "Today"),
                payload.get("auditor", "Unassigned"),
                payload.get("remarks", ""),
                payload.get("status", "Pending"),
                now,
            ),
        )
    self.json({"ok": True, "id": cursor.lastrowid, "scheduleRef": f"SCH-{cursor.lastrowid:05d}"})


def start_schedule(self, parsed, payload=None):
    user = self.current_user()
    schedule_id = int(payload.get("scheduleId") or 0)
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        schedule = db.execute("SELECT * FROM schedules WHERE id = ?", (schedule_id,)).fetchone()
        if not schedule:
            self.json({"error": "Schedule not found"}, 404)
            return
        existing = db.execute("SELECT id FROM inspection_sessions WHERE schedule_id = ?", (schedule_id,)).fetchone()
        if existing:
            session_id = existing["id"]
        else:
            if "auditor" not in user.get("inspectionPermissions", []):
                raise PermissionError("An auditor must start this scheduled inspection first")
            session_id = insert_record(db, "inspection_sessions", {
                "business_unit": schedule["business_unit"], "outlet": schedule["outlet"], "zone": schedule["zone"],
                "audit_date": normalize_audit_date(schedule["scheduled_date"]), "auditor": user["name"],
                "items_data_id": save_value(db, []), "signatures_data_id": save_value(db, {}), "progress": 0, "status": "Draft",
                "created_at": now, "updated_at": now, "owner_user_id": user["id"], "schedule_id": schedule_id,
                "audit_ref": allocate_reference(db, schedule["scheduled_date"]), "audit_time": datetime.now().strftime("%H:%M"),
                "audit_type": (db.execute("SELECT name FROM audit_types WHERE active = 1 ORDER BY id LIMIT 1").fetchone() or [""])[0],
                "remarks": schedule["remarks"] or "",
            })
            name = inspection_name({"id": session_id, "outlet": schedule["outlet"], "audit_date": normalize_audit_date(schedule["scheduled_date"])})
            db.execute("UPDATE inspection_sessions SET inspection_name = ? WHERE id = ?", (name, session_id))
            db.execute("UPDATE schedules SET status = 'In Progress' WHERE id = ?", (schedule_id,))
    self.json({"ok": True, "id": session_id, "scheduleId": schedule_id, "scheduleRef": f"SCH-{schedule_id:05d}"})


def start_audit(self, parsed, payload=None):
    """Create an immediately available scheduled draft without fabricating a result."""
    user = self.current_user()
    payload, _ = authorize_inspection_update(user, payload)
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        for key in ("outlet", "auditDate", "auditTime", "auditType"):
            if not payload.get(key):
                raise ValueError("Outlet, audit date, time, and type are required")
        validate_metadata(db, payload)
        schedule_id = insert_record(db, "schedules", {
            "business_unit": payload.get("businessUnit", "Ottotree"), "outlet": payload["outlet"],
            "zone": "All Locations", "scheduled_date": payload["auditDate"], "auditor": user["name"],
            "remarks": payload.get("remarks", ""), "status": "In Progress", "created_at": now,
        })
        reference = allocate_reference(db, payload["auditDate"])
        session_id = insert_record(db, "inspection_sessions", {
            "business_unit": payload.get("businessUnit", "Ottotree"), "outlet": payload["outlet"],
            "zone": "All Locations", "audit_date": payload["auditDate"], "audit_time": payload["auditTime"],
            "audit_type": payload["auditType"], "remarks": payload.get("remarks", ""), "auditor": user["name"],
            "audit_ref": reference, "items_data_id": save_value(db, []), "signatures_data_id": save_value(db, {}), "progress": 0, "status": "Draft",
            "created_at": now, "updated_at": now, "owner_user_id": user["id"], "schedule_id": schedule_id,
        })
        name = inspection_name({"id": session_id, "outlet": payload["outlet"], "audit_date": payload["auditDate"]})
        db.execute("UPDATE inspection_sessions SET inspection_name = ? WHERE id = ?", (name, session_id))
    self.json({"ok": True, "id": session_id, "auditRef": reference, "scheduleId": schedule_id})


def post_captain_logins(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        db.execute(
            """
            INSERT INTO captain_logins (outlet, captain_name, logged_at)
            VALUES (?, ?, ?)
            """,
            (
                payload.get("outlet") or default_outlet,
                payload.get("captainName", "Unnamed Captain"),
                now,
            ),
        )
    self.json({"ok": True})


def patch_inspection_sessions(self, parsed, payload=None):
    session_id = parsed.path.rsplit("/", 1)[-1]
    if not session_id.isdigit():
        self.send_error(400)
        return
    now = int(time.time() * 1000)
    user = self.current_user()
    with connect() as db:
        # Serialize the status check and update to prevent duplicate finalization.
        db.execute("BEGIN IMMEDIATE")
        existing = db.execute("SELECT * FROM inspection_sessions WHERE id = ?", (int(session_id),)).fetchone()
        if not existing:
            self.json({"error": "Inspection not found"}, 404)
            return
        if existing["closed_at"]:
            self.json({"error": "Closed audits cannot be changed"}, 409)
            return
        existing = dict(existing)
        existing["signatures_data_id"] = load_value(existing["signatures_data_id"] or "{}")
        payload, changed = authorize_inspection_update(user, payload, existing)
        if existing["status"] == "Completed":
            if changed or payload.get("complete") is False:
                self.json({"error": "Completed inspection items cannot be changed"}, 409)
                return
            if payload.get("signatures", {}) == load_value(existing["signatures_data_id"] or "{}"):
                self.json({"error": "Inspection is already completed"}, 409)
                return
            db.execute("UPDATE inspection_sessions SET signatures_data_id = ?, updated_at = ? WHERE id = ?", (save_value(db, payload["signatures"]), now, int(session_id)))
            notify_inspection(db, int(session_id), user, payload, existing)
            db.commit()
            self.json({"ok": True, "id": int(session_id), "status": "Completed", "auditId": existing["audit_id"]})
            return
        items = payload.get("items") or []
        progress = inspection_progress(items)
        complete = bool(payload.get("complete"))
        if complete and progress < 100:
            self.json({"error": "Inspection is incomplete"}, 409)
            return
        session_name = inspection_name({
            "id": int(session_id),
            "outlet": payload.get("outlet") or first_outlet(db),
            "audit_date": normalize_audit_date(payload.get("auditDate")),
        })
        cursor = db.execute(
            """
            UPDATE inspection_sessions
            SET inspection_name = ?, business_unit = ?, outlet = ?, zone = ?, audit_date = ?, auditor = ?,
                items_data_id = ?, progress = ?, status = ?, signatures_data_id = ?, updated_at = ?
            WHERE id = ?
            """,
            (
                session_name,
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or first_outlet(db),
                payload.get("zone", "Unassigned"),
                normalize_audit_date(payload.get("auditDate")),
                payload.get("auditor", "Unnamed Inspector"),
                save_value(db, items),
                progress,
                "Completed" if complete else "Draft",
                save_value(db, payload.get("signatures") or {}),
                now,
                int(session_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
        validate_metadata(db, {key: value for key, value in payload.items() if value is not None}, existing)
        save_session_metadata(db, int(session_id), payload)
        append_inspection_photos(db, items, now)
        audit_id = None
        if complete:
            audit_id = finalize_inspection(db, int(session_id), payload, now)
        notify_inspection(db, int(session_id), user, payload, existing, changed)
    self.json({"ok": True, "id": int(session_id), "inspectionName": session_name, "progress": progress, "status": "Completed" if complete else "Draft", "auditId": audit_id})
    return


def save_session_metadata(db, session_id, payload):
    for key, column in (("auditTime", "audit_time"), ("auditType", "audit_type"), ("remarks", "remarks")):
        if key in payload:
            db.execute(f"UPDATE inspection_sessions SET {column} = ? WHERE id = ?", (payload[key], session_id))


def patch_schedules(self, parsed, payload=None):
    if not parsed.path.startswith("/api/schedules/"):
        self.send_error(404)
        return
    schedule_id = parsed.path.rsplit("/", 1)[-1]
    if not schedule_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute(
            """
            UPDATE schedules
            SET outlet = ?, zone = ?, scheduled_date = ?, auditor = ?, remarks = ?, status = ?
            WHERE id = ?
            """,
            (
                payload.get("outlet") or first_outlet(db),
                payload.get("zone", "Unassigned"),
                payload.get("scheduledDate", "Today"),
                payload.get("auditor", "Unassigned"),
                payload.get("remarks", ""),
                payload.get("status", "Pending"),
                int(schedule_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})


def delete_inspection_sessions(self, parsed, payload=None):
    session_id = parsed.path.rsplit("/", 1)[-1]
    if not session_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        session = db.execute("SELECT status, schedule_id FROM inspection_sessions WHERE id = ?", (int(session_id),)).fetchone()
        if session and session["status"] == "Completed":
            self.json({"error": "Completed inspections must be retained"}, 409)
            return
        cursor = db.execute("DELETE FROM inspection_sessions WHERE id = ?", (int(session_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
        if session and session["schedule_id"]:
            db.execute("UPDATE schedules SET status = 'Pending' WHERE id = ?", (session["schedule_id"],))
    self.json({"ok": True})
    return


def delete_schedules(self, parsed, payload=None):
    if not parsed.path.startswith("/api/schedules/"):
        self.send_error(404)
        return
    schedule_id = parsed.path.rsplit("/", 1)[-1]
    if not schedule_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        if db.execute("SELECT 1 FROM inspection_sessions WHERE schedule_id = ?", (int(schedule_id),)).fetchone():
            self.json({"error": "A schedule linked to an inspection must be retained"}, 409)
            return
        cursor = db.execute("DELETE FROM schedules WHERE id = ?", (int(schedule_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
