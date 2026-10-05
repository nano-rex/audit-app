"""Routes inspections for the audit application."""
from backend.relational_values import load_value, save_value
from backend.permissions import authorize_inspection_update
from backend.inspection_notifications import notify_inspection
import time
from backend import activity
from datetime import datetime
from backend.audit_metadata import allocate_reference, validate_metadata
from backend.common import inspection_name, inspection_progress, normalize_audit_date
from backend.database import connect, first_outlet, insert_record
from backend.inspections import finalize_inspection, visit_locations_of
from backend.location_integrity import locations_label, visit_locations


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
        reference = db.execute("SELECT audit_ref FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()[0]
        record = {"record_ref": reference, "outlet": payload.get("outlet") or default_outlet, "business_unit": payload.get("businessUnit", "Ottotree")}
        activity.log(db, user, "audit_started", "inspection", session_id, at=now, **record)
        if status == "Completed":
            audit_id = finalize_inspection(db, session_id, payload, now)
            activity.log(db, user, "audit_completed", "inspection", session_id, started_at=now, at=now, **record)
        notify_inspection(db, session_id, user, payload, changed=True)
        db.commit()
        self.json({"ok": True, "id": session_id, "inspectionName": session_name, "progress": progress, "status": status, "auditId": audit_id})
        return
    self.json({"ok": True})


def post_schedules(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        outlet = payload.get("outlet") or first_outlet(db)
        locations = visit_locations(db, outlet, payload)
        # A new visit always starts as Pending; starting and completing the audit move it on.
        cursor = db.execute(
            """
            INSERT INTO schedules
            (business_unit, outlet, zone, locations_data_id, scheduled_date, auditor, remarks, status, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, 'Pending', ?)
            """,
            (
                payload.get("businessUnit", "Ottotree"),
                outlet,
                locations_label(locations),
                save_value(db, locations),
                payload.get("scheduledDate", "Today"),
                payload.get("auditor", "Unassigned"),
                payload.get("remarks", ""),
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
                # The audit covers the locations the visit was scheduled for, and no others.
                "locations_data_id": save_value(db, visit_locations_of(schedule["locations_data_id"])),
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
            started = db.execute("SELECT audit_ref FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()[0]
            activity.log(db, user, "audit_started", "inspection", session_id, started, schedule["outlet"], business_unit=schedule["business_unit"], at=now)
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
        activity.log(db, user, "audit_started", "inspection", session_id, reference, payload["outlet"],
                     business_unit=payload.get("businessUnit", "Ottotree"), at=now)
    self.json({"ok": True, "id": session_id, "auditRef": reference, "scheduleId": schedule_id})


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
            before = existing["signatures_data_id"] or {}
            signed = [label for key, label in (("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by"))
                      if (payload["signatures"].get(key) or {}).get("url") and payload["signatures"].get(key) != before.get(key)]
            if signed:
                activity.log(db, user, "audit_signed", "inspection", int(session_id), existing["audit_ref"], existing["outlet"],
                             started_at=activity.last_time(db, "audit_completed", "inspection", int(session_id)), detail=", ".join(signed),
                             business_unit=existing["business_unit"], at=now)
            notify_inspection(db, int(session_id), user, payload, existing)
            db.commit()
            self.json({"ok": True, "id": int(session_id), "status": "Completed", "auditId": existing["audit_id"]})
            return
        # An inspection keeps the outlet it was scheduled or started for.
        if payload.get("outlet") and payload["outlet"] != existing["outlet"]:
            self.json({"error": "An inspection's outlet is set by its schedule and cannot be changed"}, 409)
            return
        payload["outlet"] = existing["outlet"]
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
            activity.log(db, user, "audit_completed", "inspection", int(session_id), existing["audit_ref"], existing["outlet"],
                         started_at=existing["created_at"], business_unit=existing["business_unit"], at=now)
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
        existing = db.execute("SELECT status FROM schedules WHERE id = ?", (int(schedule_id),)).fetchone()
        if not existing:
            self.send_error(404)
            return
        outlet = payload.get("outlet") or first_outlet(db)
        locations = visit_locations(db, outlet, payload)
        db.execute(
            """
            UPDATE schedules
            SET outlet = ?, zone = ?, locations_data_id = ?, scheduled_date = ?, auditor = ?, remarks = ?, status = ?
            WHERE id = ?
            """,
            (
                outlet,
                locations_label(locations),
                save_value(db, locations),
                payload.get("scheduledDate", "Today"),
                payload.get("auditor", "Unassigned"),
                payload.get("remarks", ""),
                # Status follows the audit's progress; an edit keeps it unless a status is sent.
                payload.get("status") or existing["status"],
                int(schedule_id),
            ),
        )
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
