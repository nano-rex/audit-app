"""Work orders for the audit application."""
import json
import time
from database import connect


def notifications(user_id):
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, title, message, channel, status, related_type, related_id, created_at, read_at
            FROM notifications
            WHERE recipient_user_id = ? OR recipient_user_id IS NULL
            ORDER BY created_at DESC, id DESC
            LIMIT 100
            """, (user_id,)
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def comments(record_type="", record_id=0):
    where = ""
    params = ()
    if record_type and record_id:
        where = "WHERE record_type = ? AND record_id = ?"
        params = (record_type, int(record_id))
    with connect() as db:
        rows = db.execute(
            f"""
            SELECT id, record_type, record_id, comment, author, created_at
            FROM comments
            {where}
            ORDER BY created_at DESC, id DESC
            """,
            params,
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def work_order_items():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, work_order_ref, business_unit, outlet, zone, request_type, category, priority, title,
                   description, assignee, pic, status, action_taken, completion_date,
                   completion_remark, completion_photo, verified_by, verified_at,
                   verification_remark, closed_at, due_date, vendor, sla_status, cost,
                   outlet_confirmed, source_finding_id, cause, recommendation, required_action, images_json
            FROM work_orders
            ORDER BY
                CASE priority WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END,
                created_at DESC, id DESC
            """
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def sync_finding_from_work_order(db, work_order_id):
    row = db.execute(
        """
        SELECT source_finding_id, status, pic, action_taken, completion_date,
               completion_photo, completion_remark, verified_by, verified_at,
               verification_remark, closed_at
        FROM work_orders
        WHERE id = ?
        """,
        (work_order_id,),
    ).fetchone()
    if not row or not row["source_finding_id"]:
        return
    db.execute(
        """
        UPDATE findings
        SET status = ?, pic = ?, corrective_action = ?, completion_date = ?,
            completion_photo = ?, completion_remark = ?, verified_by = ?,
            verified_at = ?, verification_remark = ?, closed_at = ?, updated_at = ?
        WHERE id = ?
        """,
        (
            row["status"],
            row["pic"] or "",
            row["action_taken"] or "",
            row["completion_date"] or "",
            row["completion_photo"] or json.dumps([]),
            row["completion_remark"] or "",
            row["verified_by"] or "",
            row["verified_at"] or "",
            row["verification_remark"] or "",
            row["closed_at"] or "",
            int(time.time() * 1000),
            row["source_finding_id"],
        ),
    )


def finding_items():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, finding_ref, audit_id, audit_ref, business_unit, outlet, location,
                   category, priority, assigned_department, pic, comment, status,
                   priority_classification, cause, recommendation, required_action, images_json, due_date,
                   corrective_action, completion_date, completion_photo, completion_remark,
                   verified_by, verified_at, verification_remark, closed_at, source_item_id,
                   created_at, updated_at
            FROM findings
            ORDER BY created_at DESC, id DESC
            """
        ).fetchall()
    return {"items": [dict(row) for row in rows]}
