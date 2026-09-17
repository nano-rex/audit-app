"""Routes work orders for the audit application."""
import time
from common import create_notification, json_text, priority_due_date, sla_status, work_order_ref, workflow_dates
from database import connect, first_category, first_department, first_outlet
from work_orders import sync_finding_from_work_order


def post_work_orders(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        default_department = first_department(db)
        default_category = first_category(db)
        status, verified_at, closed_at = workflow_dates(payload)
        priority = payload.get("priority", "Medium")
        due_date = payload.get("dueDate") or priority_due_date(db, priority, now)
        current_sla_status = payload.get("slaStatus") or sla_status(status, due_date)
        cursor = db.execute(
            """
            INSERT INTO work_orders
            (business_unit, outlet, zone, request_type, category, priority, title, description,
             assignee, pic, status, action_taken, completion_date, completion_remark, completion_photo,
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
                json_text(payload.get("completionPhoto"), []),
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
        create_notification(db, "Work order assigned", payload.get("title", "Work order"), "In-App", "work_order", cursor.lastrowid)
        if current_sla_status == "Due Soon":
            create_notification(db, "Work order due soon", f"{payload.get('title', 'Work order')} is due on {due_date}", "In-App", "work_order", cursor.lastrowid)
        if current_sla_status == "Overdue":
            create_notification(db, "Work order overdue", f"{payload.get('title', 'Work order')} passed its due date {due_date}", "In-App", "work_order", cursor.lastrowid)
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
                payload.get("author", self.current_user().get("name", "")),
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
            "UPDATE notifications SET status = 'Read', read_at = ? WHERE id = ?",
            (int(time.time() * 1000), int(record_id)),
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
    with connect() as db:
        status, verified_at, closed_at = workflow_dates(payload)
        priority = payload.get("priority", "Medium")
        due_date = payload.get("dueDate") or priority_due_date(db, priority, int(time.time() * 1000))
        current_sla_status = payload.get("slaStatus") or sla_status(status, due_date)
        cursor = db.execute(
            """
            UPDATE work_orders
            SET business_unit = ?, outlet = ?, zone = ?, request_type = ?, category = ?,
                priority = ?, title = ?, description = ?, assignee = ?, pic = ?, status = ?,
                action_taken = ?, completion_date = ?, completion_remark = ?, completion_photo = ?,
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
                json_text(payload.get("completionPhoto"), []),
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
        if status in ("Completed", "Verified", "Closed"):
            create_notification(db, "Corrective action completed", payload.get("title", "Work order"), "In-App", "work_order", int(item_id))
    self.json({"ok": True})
    return


def save_finding_details(db, work_order_id, payload):
    """Retain evidence and diagnostic fields when updating a work order."""
    fields = {"cause": "cause", "recommendation": "recommendation",
              "requiredAction": "required_action", "images": "images_json"}
    for key, column in fields.items():
        if key in payload:
            value = json_text(payload[key], []) if key == "images" else payload[key]
            db.execute(f"UPDATE work_orders SET {column} = ? WHERE id = ?", (value, work_order_id))
            db.execute(f"UPDATE findings SET {column} = ? WHERE id = "
                       "(SELECT source_finding_id FROM work_orders WHERE id = ?)", (value, work_order_id))


def delete_notifications(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute("DELETE FROM notifications WHERE id = ?", (int(record_id),))
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
    with connect() as db:
        cursor = db.execute("DELETE FROM work_orders WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return
