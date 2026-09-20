"""Notifications addressed to inspection participants using effective permissions."""
from backend.relational_values import load_value
import time

from backend.config import SUPER_ROLE
from backend.permissions import INSPECTION_PERMISSIONS


def notify_inspection(db, session_id, actor, payload, previous=None, changed=False):
    session = db.execute("SELECT * FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()
    now = int(time.time() * 1000)
    recipients = set()
    if changed and session["progress"] > 0:
        roles = {row["name"]: set(load_value(row["inspection_permissions_data_id"] or "[]")) for row in db.execute("SELECT name, inspection_permissions_data_id FROM roles")}
        for user in db.execute("SELECT id, role, permission_overrides_data_id FROM users WHERE active = 1"):
            if user["id"] == actor["id"]:
                continue
            overrides = load_value(user["permission_overrides_data_id"]) if user["permission_overrides_data_id"] is not None else None
            caps = set(INSPECTION_PERMISSIONS) if user["role"] == SUPER_ROLE else set(overrides.get("inspectionPermissions", [])) if overrides is not None else roles.get(user["role"], set())
            if caps.intersection({"verifier", "acknowledger"}):
                recipients.add(user["id"])
        db.executemany("INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread','inspection',?,?,?)",
                       [("Audit progress updated", f"{actor['name']} saved {session['inspection_name']}: {session['progress']}% complete.", session_id, now, recipient) for recipient in recipients])
    prior = load_value(previous["signatures_data_id"] or "{}") if previous else {}
    verified = (payload.get("signatures") or {}).get("verifiedBy")
    owner_id = session["owner_user_id"]
    if not owner_id and session["auditor"]:
        owner = db.execute("SELECT id FROM users WHERE name = ? AND active = 1 ORDER BY id LIMIT 1", (session["auditor"],)).fetchone()
        owner_id = owner["id"] if owner else None
    if verified and verified != prior.get("verifiedBy") and owner_id and owner_id != actor["id"]:
        db.execute("INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread','inspection',?,?,?)",
                   ("Audit verified", f"{actor['name']} verified {session['inspection_name']}.", session_id, now, owner_id))
