"""Editing and removing an audit's findings on the Findings page.

The details once recorded during the inspection (asset type, priority, department, PIC, due date,
description, cause, recommendation, required action, photos) are set here, after the audit.
"""
import time
from datetime import date

from backend import activity
from backend.database import connect
from backend.relational_values import data_value
from backend.workflow import WorkflowError

TEXT_FIELDS = {"category": "category", "pic": "pic", "comment": "comment", "cause": "cause",
               "recommendation": "recommendation", "requiredAction": "required_action"}


def finding_id(parsed):
    value = parsed.path.rsplit("/", 1)[-1]
    if not value.isdigit():
        raise WorkflowError("Finding not found", 404)
    return int(value)


def editable(user):
    """A Department/PIC account follows up findings but does not change or remove them."""
    if (user or {}).get("role") == "Department/PIC":
        raise PermissionError("Only an auditor or supervisor can change findings")


def patch_findings(self, parsed, payload=None):
    user = self.current_user()
    editable(user)
    payload = payload or {}
    record_id = finding_id(parsed)
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        row = db.execute("SELECT * FROM findings WHERE id = ?", (record_id,)).fetchone()
        if not row:
            raise WorkflowError("Finding not found", 404)
        if row["status"] == "Closed":
            raise WorkflowError("A closed finding cannot be changed")
        values = {column: str(payload[key]).strip() for key, column in TEXT_FIELDS.items() if key in payload and payload[key] is not None}
        if "comment" in values and not values["comment"]:
            raise ValueError("Describe what is wrong")
        if payload.get("priority"):
            level = db.execute("SELECT classification FROM priority_levels WHERE name = ?", (payload["priority"],)).fetchone()
            if not level:
                raise ValueError("Choose one of the priority levels")
            values.update(priority=payload["priority"], priority_classification=level[0])
        if payload.get("department"):
            if not db.execute("SELECT 1 FROM departments WHERE code = ?", (payload["department"],)).fetchone():
                raise ValueError("Choose one of the departments")
            values["assigned_department"] = payload["department"]
        if "dueDate" in payload:
            due = str(payload.get("dueDate") or "").strip()
            if due:
                try:
                    date.fromisoformat(due)
                except ValueError:
                    raise ValueError("Enter a valid due date") from None
            values["due_date"] = due
        if "images" in payload:
            values["images_data_id"] = data_value(db, payload.get("images") or [], [])
        if not values:
            raise ValueError("Nothing to change")
        values["updated_at"] = now
        db.execute(f"UPDATE findings SET {', '.join(f'{column} = ?' for column in values)} WHERE id = ?", (*values.values(), record_id))
        activity.log(db, user, "finding_edited", "finding", record_id, row["finding_ref"], row["outlet"],
                     business_unit=row["business_unit"], at=now)
    self.json({"ok": True})


def delete_findings(self, parsed, payload=None):
    user = self.current_user()
    editable(user)
    record_id = finding_id(parsed)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        row = db.execute("SELECT * FROM findings WHERE id = ?", (record_id,)).fetchone()
        if not row:
            raise WorkflowError("Finding not found", 404)
        if row["work_request_id"] or db.execute("SELECT 1 FROM work_orders WHERE source_finding_id = ?", (record_id,)).fetchone():
            raise WorkflowError("This finding has a work request or work order; decline or close that instead")
        if row["status"] == "Closed":
            raise WorkflowError("A closed finding is kept with its audit")
        db.execute("UPDATE inspection_items SET finding_id = NULL WHERE finding_id = ?", (record_id,))
        db.execute("DELETE FROM findings WHERE id = ?", (record_id,))
        activity.log(db, user, "finding_deleted", "finding", record_id, row["finding_ref"], row["outlet"],
                     detail=f"{row['item_name'] or ''} · {row['criterion'] or ''}".strip(" ·"), business_unit=row["business_unit"])
    self.json({"ok": True})
