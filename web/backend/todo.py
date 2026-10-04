"""What is waiting on the signed-in user: drafts, signatures, corrective actions, verification, closure."""
from backend.common import sla_status
from backend.database import connect
from backend.relational_values import load_values
from backend.workflow import assigned_to, can_verify

SIGNATURES = (("auditedBy", "auditor", "auditor"), ("verifiedBy", "verifier", "verifier"),
              ("acknowledgedBy", "acknowledger", "acknowledger"))
ACTIVE_ORDERS = ("Open", "Assigned", "In Progress", "Pending")
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
                    items.append(inspection_item(row, "Close audit", "Signed and all corrective actions closed", "signoff"))
        if "work-orders" in permissions:
            for row in db.execute("SELECT * FROM work_orders WHERE status NOT IN ('Verified', 'Closed') ORDER BY CASE WHEN due_date IS NULL OR due_date = '' THEN 1 ELSE 0 END, due_date, id LIMIT 500"):
                order = dict(row)
                if order["status"] in ACTIVE_ORDERS and assigned_to(user, order):
                    items.append(order_item(order, "Complete corrective action"))
                elif order["status"] == "Completed" and can_verify(user):
                    items.append(order_item(order, "Verify corrective action"))
    return {"items": items[:LIMIT], "total": len(items), "unreadNotifications": unread}


def inspection_item(row, action, detail, view="checklist"):
    return {"type": "inspection", "id": row["id"], "action": action, "view": view,
            "title": f"{row['audit_ref'] or ''} {row['inspection_name'] or row['outlet']}".strip(),
            "detail": f"{row['outlet']} | {row['audit_date']} | {detail}"}


def order_item(order, action):
    due = f"Due {order['due_date']}" if order.get("due_date") else "No due date"
    return {"type": "work_order", "id": order["id"], "action": action,
            "title": f"{order.get('work_order_ref') or '#' + str(order['id'])} {order['title']}",
            "detail": f"{order['outlet']} | {order.get('zone') or 'No location'} | {order['priority']} | {due}",
            "overdue": sla_status(order["status"], order.get("due_date")) == "Overdue"}
