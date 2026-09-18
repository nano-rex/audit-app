"""Routes work orders for the audit application."""
from relational_values import data_value
import time
from reminders import notify_work_order
from common import create_notification, priority_due_date, sla_status, work_order_ref
from database import connect, first_category, first_department, first_outlet
from work_orders import sync_finding_from_work_order
from workflow import WorkflowError, validate_update


def post_work_orders(self, parsed, payload=None):
    now = int(time.time() * 1000)
    payload = validate_update(payload, None, self.current_user())
    with connect() as db:
        default_outlet = first_outlet(db)
        default_department = first_department(db)
        default_category = first_category(db)
        status = payload.get("status", "Assigned")
        verified_at, closed_at = payload["verifiedAt"], payload["closedAt"]
        priority = payload.get("priority", "Medium")
        due_date = payload.get("dueDate") or priority_due_date(db, priority, now)
        current_sla_status = sla_status(status, due_date)
        cursor = db.execute(
            """
            INSERT INTO work_orders
            (business_unit, outlet, zone, request_type, category, priority, title, description,
             assignee, pic, status, action_taken, completion_date, completion_remark, completion_photo_data_id,
             verified_by, verified_at, verification_remark, closed_at, due_date, vendor, sla_status,
             cost, outlet_confirmed, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)
            """,
            (
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or default_outlet,
                payload.get("zone", "Unassigned"),
                payload.get("requestType") or default_department,
                payload.get("category") or default_category,
                priority,
                payload.get("title", "Work order"),
                payload.get("description", ""),
                payload.get("assignee", "Technical Support"),
                payload.get("pic", ""),
                status,
                payload.get("actionTaken", ""),
                payload.get("completionDate", ""),
                payload.get("completionRemark", ""),
                data_value(db, payload.get("completionPhoto"), []),
                payload.get("verifiedBy", ""),
                verified_at,
                payload.get("verificationRemark", ""),
                closed_at,
                due_date,
                payload.get("vendor", ""),
                current_sla_status,
                float(payload.get("cost") or 0),
                now,
            ),
        )
        db.execute(
            "UPDATE work_orders SET work_order_ref = ? WHERE id = ?",
            (work_order_ref(cursor.lastrowid), cursor.lastrowid),
        )
        save_finding_details(db, cursor.lastrowid, payload)
        notify_work_order(db, cursor.lastrowid, status)
    self.json({"ok": True})


def post_comments(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute(
            """
            INSERT INTO comments (record_type, record_id, comment, author, created_at)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                payload.get("recordType", "general"),
                int(payload.get("recordId") or 0),
                payload.get("comment", ""),
                self.current_user().get("name", ""),
                now,
            ),
        )
    self.json({"ok": True})


def post_notifications(self, parsed, payload=None):
    with connect() as db:
        create_notification(
            db,
            payload.get("title", "Notification"),
            payload.get("message", ""),
            payload.get("channel", "In-App"),
            payload.get("relatedType", ""),
            int(payload.get("relatedId") or 0),
        )
    self.json({"ok": True})


def patch_notifications(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute(
            "UPDATE notifications SET status = 'Read', read_at = ? WHERE id = ? AND (recipient_user_id IS NULL OR recipient_user_id = ?)",
            (int(time.time() * 1000), int(record_id), self.current_user()["id"]),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def patch_work_orders(self, parsed, payload=None):
    item_id = parsed.path.rsplit("/", 1)[-1]
    if not item_id.isdigit():
        self.send_error(400)
        return
    user = self.current_user()
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        existing = db.execute("SELECT * FROM work_orders WHERE id = ?", (int(item_id),)).fetchone()
        if not existing:
            self.json({"error": "Work order not found"}, 404)
            return
        payload = validate_update(payload, existing, user)
        status = payload["status"]
        verified_at, closed_at = payload["verifiedAt"], payload["closedAt"]
        priority = payload.get("priority", "Medium")
        due_date = payload.get("dueDate") or priority_due_date(db, priority, int(time.time() * 1000))
        current_sla_status = sla_status(status, due_date)
        cursor = db.execute(
            """
            UPDATE work_orders
            SET business_unit = ?, outlet = ?, zone = ?, request_type = ?, category = ?,
                priority = ?, title = ?, description = ?, assignee = ?, pic = ?, status = ?,
                action_taken = ?, completion_date = ?, completion_remark = ?, completion_photo_data_id = ?,
                verified_by = ?, verified_at = ?, verification_remark = ?, closed_at = ?,
                due_date = ?, vendor = ?, sla_status = ?, cost = ?
            WHERE id = ?
            """,
            (
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or first_outlet(db),
                payload.get("zone", "Unassigned"),
                payload.get("requestType") or first_department(db),
                payload.get("category") or first_category(db),
                priority,
                payload.get("title", "Work order"),
                payload.get("description", ""),
                payload.get("assignee", "Technical Support"),
                payload.get("pic", ""),
                status,
                payload.get("actionTaken", ""),
                payload.get("completionDate", ""),
                payload.get("completionRemark", ""),
                data_value(db, payload.get("completionPhoto"), []),
                payload.get("verifiedBy", ""),
                verified_at,
                payload.get("verificationRemark", ""),
                closed_at,
                due_date,
                payload.get("vendor", ""),
                current_sla_status,
                float(payload.get("cost") or 0),
                int(item_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
        save_finding_details(db, int(item_id), payload)
        sync_finding_from_work_order(db, int(item_id))
        if status != existing["status"]:
            message = f"Status changed from {existing['status']} to {status}"
            if payload.get("verificationRemark"):
                message += f": {payload['verificationRemark']}"
            db.execute("INSERT INTO comments(record_type, record_id, comment, author, created_at, system_generated) VALUES ('work_order', ?, ?, ?, ?, 1)",
                       (int(item_id), message, user["name"], int(time.time() * 1000)))
            notify_work_order(db, int(item_id), status)
        elif any(payload.get(key, "") != (existing[column] or "") for key, column in (("pic", "pic"), ("assignee", "assignee"), ("requestType", "request_type"))):
            notify_work_order(db, int(item_id), "Assigned")
    self.json({"ok": True})
    return


def save_finding_details(db, work_order_id, payload):
    """Retain evidence and diagnostic fields when updating a work order."""
    fields = {"cause": "cause", "recommendation": "recommendation",
              "requiredAction": "required_action", "images": "images_data_id"}
    for key, column in fields.items():
        if key in payload:
            value = data_value(db, payload[key], []) if key == "images" else payload[key]
            db.execute(f"UPDATE work_orders SET {column} = ? WHERE id = ?", (value, work_order_id))
            db.execute(f"UPDATE findings SET {column} = ? WHERE id = "
                       "(SELECT source_finding_id FROM work_orders WHERE id = ?)", (value, work_order_id))


def delete_notifications(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute("DELETE FROM notifications WHERE id = ? AND (recipient_user_id IS NULL OR recipient_user_id = ?)", (int(record_id), self.current_user()["id"]))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_comments(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        event = db.execute("SELECT system_generated FROM comments WHERE id = ?", (int(record_id),)).fetchone()
        if event and event["system_generated"]:
            raise WorkflowError("Workflow history cannot be deleted")
        cursor = db.execute("DELETE FROM comments WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_work_orders(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    if self.current_user().get("role") == "Department/PIC":
        raise WorkflowError("Department/PIC users cannot delete work orders", 403)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        row = db.execute("SELECT source_finding_id, status FROM work_orders WHERE id = ?", (int(record_id),)).fetchone()
        if row and (row["source_finding_id"] or row["status"] in {"Completed", "Verified", "Closed"}):
            raise WorkflowError("Audit-linked or completed work orders must be retained")
        cursor = db.execute("DELETE FROM work_orders WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return
