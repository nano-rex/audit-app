"""Work orders for the audit application."""
from backend.relational_values import hydrate_many
import time
from backend.common import sla_status
from backend.database import connect
from backend.report_filters import report_scope


def notifications(user_id):
    from backend.reminders import deliver_due_reminders
    deliver_due_reminders()
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
    return {"items": hydrate_many(rows)}


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
    return {"items": hydrate_many(rows)}


def work_order_items(user=None):
    where = ""
    params = ()
    if user and user.get("role") == "Department/PIC":
        name = str(user.get("name") or "").strip().casefold()
        email = str(user.get("email") or "").strip().casefold()
        department = str(user.get("department") or "").strip().casefold()
        where = "WHERE CASE WHEN TRIM(COALESCE(pic, '')) != '' " \
                "THEN LOWER(TRIM(pic)) IN (?, ?) " \
                "ELSE LOWER(TRIM(COALESCE(assignee, ''))) IN (?, ?) " \
                "OR LOWER(TRIM(COALESCE(request_type, ''))) = ? END"
        params = (name, email, name, email, department)
    with connect() as db:
        rows = db.execute(
            f"""
            SELECT id, work_order_ref, business_unit, outlet, zone, request_type, category, priority, title,
                   description, assignee, pic, status, closed_at, due_date, vendor, sla_status, cost,
                   outlet_confirmed, source_finding_id, cause, recommendation, required_action, images_data_id
            FROM work_orders
            {where}
            ORDER BY
                CASE priority WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END,
                created_at DESC, id DESC
            """, params
        ).fetchall()
    return {"items": with_current_sla(hydrate_many(rows))}


def with_current_sla(orders):
    """The stored SLA status is as of the last edit; an order becomes overdue without being edited."""
    for order in orders:
        order["sla_status"] = sla_status(order["status"], order.get("due_date"))
    return orders


def sync_finding_from_work_order(db, work_order_id):
    """Findings follow their work order: the same status, person in charge, and closing date.
    A closed work order closes the request it was made from."""
    row = db.execute("SELECT source_finding_id, work_request_id, status, pic, closed_at FROM work_orders WHERE id = ?", (work_order_id,)).fetchone()
    if not row:
        return
    now = int(time.time() * 1000)
    db.execute("UPDATE findings SET status = ?, pic = ?, closed_at = ?, updated_at = ? WHERE id = ? OR (work_request_id IS NOT NULL AND work_request_id = ?)",
               (row["status"], row["pic"] or "", row["closed_at"] or "", now, row["source_finding_id"], row["work_request_id"]))
    if row["work_request_id"] and row["status"] == "Closed":
        db.execute("UPDATE work_requests SET status = 'Closed', updated_at = ? WHERE id = ?", (now, row["work_request_id"]))


def finding_items(unit="Ottotree", filters=None, user=None):
    where, params = report_scope(unit, "findings", filters)
    if user and user.get("role") == "Department/PIC":
        name = str(user.get("name") or "").strip().casefold()
        email = str(user.get("email") or "").strip().casefold()
        department = str(user.get("department") or "").strip().casefold()
        where += " AND CASE WHEN TRIM(COALESCE(findings.pic, '')) != '' " \
                 "THEN LOWER(TRIM(findings.pic)) IN (?, ?) " \
                 "ELSE LOWER(TRIM(COALESCE(findings.assigned_department, ''))) = ? END"
        params = (*params, name, email, department)
    with connect() as db:
        rows = db.execute(
            f"""SELECT findings.*, audits.audit_date, audits.audit_time, audits.auditor,
                       COALESCE(NULLIF(findings.item_name, ''), inspection_items.section, 'Item') AS item_name,
                       COALESCE(NULLIF(findings.criterion, ''), inspection_items.item, '') AS criterion,
                       COALESCE(equipment.kind, 'asset') AS item_kind,
                       work_requests.request_ref, work_requests.status AS request_status,
                       (SELECT work_order_ref FROM work_orders WHERE work_orders.source_finding_id = findings.id) AS order_ref
                FROM findings LEFT JOIN audits ON audits.id = findings.audit_id
                LEFT JOIN inspection_items ON inspection_items.id = findings.source_item_id
                LEFT JOIN equipment ON equipment.id = findings.equipment_id
                LEFT JOIN work_requests ON work_requests.id = findings.work_request_id
                WHERE {where} ORDER BY findings.created_at DESC, findings.id DESC""", params
        ).fetchall()
    return {"items": hydrate_many(rows)}
