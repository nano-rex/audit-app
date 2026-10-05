"""What is waiting on the signed-in user: drafts, signatures, assigned work orders, closure."""
from backend.common import read_setting, sla_status
from backend.accounts import is_company_admin_user
from backend.database import connect
from backend.relational_values import load_values
from backend.workflow import assigned_to
from backend.outlet_access import keep
from backend.work_requests import reviews_requests

SIGNATURES = (("auditedBy", "auditor", "auditor"), ("verifiedBy", "verifier", "verifier"),
              ("acknowledgedBy", "acknowledger", "acknowledger"))
LIMIT = 50


def todo_items(user):
    capabilities = set(user.get("inspectionPermissions", []))
    permissions = set(user.get("permissions", []))
    items = []
    with connect() as db:
        unread = db.execute("SELECT count(*) FROM notifications WHERE status = 'Unread' AND (recipient_user_id = ? OR recipient_user_id IS NULL)", (user["id"],)).fetchone()[0]
        if "inspections" in permissions and capabilities:
            if "auditor" in capabilities:
                for row in db.execute("SELECT id, audit_ref, inspection_name, outlet, audit_date, progress FROM inspection_sessions WHERE status = 'Draft' AND closed_at IS NULL "
                                  "AND (owner_user_id = ? OR owner_user_id IS NULL) ORDER BY updated_at DESC LIMIT ?", (user["id"], LIMIT)):
                    items.append(inspection_item(row, "Continue inspection", f"{row['progress']}% complete"))
            completed = db.execute("SELECT id, audit_ref, inspection_name, outlet, audit_date, audit_id, owner_user_id, signatures_data_id FROM inspection_sessions WHERE status = 'Completed' AND closed_at IS NULL ORDER BY updated_at DESC LIMIT ?", (LIMIT,)).fetchall()
            signatures = load_values([row["signatures_data_id"] for row in completed])
            for row, signed in zip(completed, signatures):
                signed = signed or {}
                missing = []
                for key, capability, label in SIGNATURES:
                    if capability not in capabilities or (signed.get(key) or {}).get("url"):
                        continue
                    if key == "auditedBy" and row["owner_user_id"] not in (None, user["id"]):
                        continue  # The auditor's signature belongs to whoever ran the inspection.
                    missing.append(label)
                if missing:
                    items.append(inspection_item(row, "Sign as " + ", ".join(missing), "Completed, awaiting signature", "signoff"))
                if "verifier" in capabilities and all((signed.get(key) or {}).get("url") for key, _, _ in SIGNATURES) and not db.execute(
                        "SELECT 1 FROM work_orders WHERE source_audit_id = ? AND status != 'Closed' UNION ALL SELECT 1 FROM findings WHERE audit_id = ? AND status != 'Closed' LIMIT 1",
                        (row["audit_id"], row["audit_id"])).fetchone():
                    items.append(inspection_item(row, "Close audit", "Signed and all work orders closed", "signoff"))
        if reviews_requests(user):
            for row in db.execute("SELECT * FROM work_requests WHERE status = 'Open' ORDER BY created_at LIMIT ?", (LIMIT,)):
                items.append({"type": "work_request", "id": row["id"], "action": "Review work request", "outlet": row["outlet"],
                              "title": f"{row['request_ref']} {row['item_name'] or ''}".strip(),
                              "detail": f"{row['outlet']} | {row['location']} | Requested by {row['requested_by'] or 'someone'}"})
        if "work-orders" in permissions:
            for row in db.execute("SELECT * FROM work_orders WHERE status != 'Closed' ORDER BY CASE WHEN due_date IS NULL OR due_date = '' THEN 1 ELSE 0 END, due_date, id LIMIT 500"):
                order = dict(row)
                if assigned_to(user, order):
                    items.append(order_item(order, "Resolve work order"))
        items = keep(user, items)  # Only tasks at the account's outlets.
        findings = items_needing_request(db, user)
        resets = db.execute("SELECT count(*) FROM password_reset_requests WHERE resolved_at IS NULL").fetchone()[0] if is_company_admin_user(user) else 0
    # How many things wait in each section, shown as a number beside its tab.
    counts = {
        "guided": sum(item["type"] == "inspection" and item["view"] == "checklist" for item in items),
        "signoff": sum(item["type"] == "inspection" and item["view"] == "signoff" for item in items),
        "findings": findings,
        "requests": sum(item["type"] == "work_request" for item in items),
        "orders": sum(item["type"] == "work_order" for item in items),
        "resets": resets,
        "notifications": unread,
    }
    return {"items": items[:LIMIT], "total": len(items), "unreadNotifications": unread, "counts": counts}


def items_needing_request(db, user):
    """Failed items (one per audit, item, and location) that nobody has requested work for yet."""
    permissions = set(user.get("permissions", []))
    if user.get("role") == "Department/PIC" or not permissions & {"findings", "inspections", "work-orders"} and user.get("role") != "Super":
        return 0
    if read_setting(db, "system.findingsEnabled", True) is False and user.get("role") != "Super":
        return 0
    rows = db.execute(
        """
        SELECT findings.outlet
        FROM findings LEFT JOIN inspection_items ON inspection_items.id = findings.source_item_id
        GROUP BY findings.audit_id, COALESCE(findings.equipment_id, NULLIF(findings.item_name, ''), inspection_items.section), findings.location
        HAVING SUM(findings.status != 'Open') = 0 AND SUM(findings.work_request_id IS NOT NULL) = 0
           AND SUM(EXISTS(SELECT 1 FROM work_orders WHERE work_orders.source_finding_id = findings.id)) = 0
        """).fetchall()
    return len(keep(user, [{"outlet": row[0]} for row in rows]))


def inspection_item(row, action, detail, view="checklist"):
    return {"type": "inspection", "id": row["id"], "action": action, "view": view, "outlet": row["outlet"],
            "title": f"{row['audit_ref'] or ''} {row['inspection_name'] or row['outlet']}".strip(),
            "detail": f"{row['outlet']} | {row['audit_date']} | {detail}"}


def order_item(order, action):
    due = f"Due {order['due_date']}" if order.get("due_date") else "No due date"
    return {"type": "work_order", "id": order["id"], "action": action, "outlet": order["outlet"],
            "title": f"{order.get('work_order_ref') or '#' + str(order['id'])} {order['title']}",
            "detail": f"{order['outlet']} | {order.get('zone') or 'No location'} | {order['priority']} | {due}",
            "overdue": sla_status(order["status"], order.get("due_date")) == "Overdue"}
