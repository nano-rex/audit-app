"""Changes that wait for approval.

Someone allowed to add, edit, and delete a kind of record ("<record>.manage") but not to approve
its changes ("<record>.approve") does not change it directly: the request is kept here, the
approvers are told, and an approver applies or rejects it. Applying replays the original
request as the approver, so it goes through exactly the same checks as a direct change.
"""
import time

from backend import activity
from backend.config import ADMIN_ROLE
from backend.database import connect
from backend.permissions import CHANGE_RECORDS, resolve_actions, resolve_permissions
from backend.relational_values import load_value, save_value
from backend.workflow import WorkflowError

TABLES = {"assets": "equipment", "users": "users", "outlets": "outlets", "zones": "zones", "locations": "locations",
          "departments": "departments", "roles": "roles"}
# Never kept in a request or shown to an approver.
HIDDEN = {"password_hash", "password"}


def record_for(method, path):
    """(record kind, record id or None) for a change to one of the managed records, else (None, None)."""
    if method not in ("POST", "PATCH", "DELETE"):
        return None, None
    rest = path.removeprefix("/api/")
    for record, (_, _, routes) in CHANGE_RECORDS.items():
        for route in routes:
            if rest == route:
                return record, None
            if rest.startswith(route + "/"):
                tail = rest[len(route) + 1:]
                if tail.isdigit():
                    return record, int(tail)
    return None, None


def allowed(user, record, level):
    return f"{record}.{level}" in (user or {}).get("actions", [])


def describe(payload, before):
    """A short name for the record: what the request or the record calls it."""
    for source in (payload or {}, before or {}):
        for key in ("name", "code", "title", "email", "assetId", "asset_id"):
            if source.get(key):
                return str(source[key])
    return ""


def snapshot(db, record, record_id):
    if record_id is None:
        return None
    row = db.execute(f"SELECT * FROM {TABLES[record]} WHERE id = ?", (record_id,)).fetchone()
    if not row:
        return None
    return {key: row[key] for key in row.keys() if key not in HIDDEN and not key.endswith("_data_id")}


def approvers(db, record, requester_id):
    """Active people whose role or own permissions let them approve this kind of change and open its page."""
    page = CHANGE_RECORDS[record][1]
    people = []
    for row in db.execute("SELECT id, role, permission_overrides_data_id FROM users WHERE active = 1"):
        overrides = load_value(row["permission_overrides_data_id"]) if row["permission_overrides_data_id"] is not None else None
        pages, _ = resolve_permissions(db, row["role"], overrides)
        if row["id"] != requester_id and page in pages and f"{record}.approve" in resolve_actions(db, row["role"], overrides):
            people.append(row["id"])
    return people


def hold(user, method, path, payload, record, record_id):
    """Keep the change for approval instead of making it."""
    payload = {key: value for key, value in (payload or {}).items() if key not in HIDDEN}
    action = {"POST": "add", "PATCH": "edit", "DELETE": "delete"}[method]
    now = int(time.time() * 1000)
    label = CHANGE_RECORDS[record][0]
    with connect() as db:
        before = snapshot(db, record, record_id)
        if record_id is not None and before is None:
            raise WorkflowError("That record no longer exists", 404)
        name = describe(payload, before)
        change_id = db.execute(
            "INSERT INTO change_requests (record, action, record_id, method, path, payload_data_id, before_data_id, summary, "
            "status, requested_by, requested_by_user_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'Pending', ?, ?, ?)",
            (record, action, record_id, method, path, save_value(db, payload), save_value(db, before or {}),
             f"{action.capitalize()} {label.split(' (')[0].rstrip('s').lower()} {name}".strip(), user.get("name", ""), user.get("id"), now)).lastrowid
        summary = db.execute("SELECT summary FROM change_requests WHERE id = ?", (change_id,)).fetchone()[0]
        db.executemany(
            "INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) "
            "VALUES ('Change waiting for your approval', ?, 'In-App', 'Unread', 'change', ?, ?, ?)",
            [(f"{summary}, requested by {user.get('name') or 'someone'}", change_id, now, approver) for approver in approvers(db, record, user.get("id"))])
        activity.log(db, user, "change_requested", "change", change_id, f"CHG-{change_id:05d}", detail=summary, at=now)
    return {"ok": True, "pending": True, "changeId": change_id,
            "message": f"Sent for approval: {summary}. It is applied once an approver accepts it; you can follow it under Approvals."}


def change_items(user):
    """Changes this person may decide, and the ones they asked for."""
    records = {record for record in CHANGE_RECORDS if allowed(user, record, "approve")}
    with connect() as db:
        rows = [dict(row) for row in db.execute("SELECT * FROM change_requests ORDER BY CASE status WHEN 'Pending' THEN 0 ELSE 1 END, created_at DESC LIMIT 300")]
        for row in rows:
            row["payload"] = load_value(row.pop("payload_data_id")) or {}
            row["before"] = load_value(row.pop("before_data_id")) or {}
    items = []
    for row in rows:
        mine = row["requested_by_user_id"] == user.get("id")
        decides = row["record"] in records and not mine
        if mine or decides:
            items.append(row | {"mine": mine, "canDecide": decides and row["status"] == "Pending",
                                "recordLabel": CHANGE_RECORDS[row["record"]][0]})
    return {"items": items}


class ReplayHandler:
    """Runs a held request through its route as the approver, keeping what the route answers."""

    def __init__(self, user):
        self.user = user
        self.status = 200
        self.body = None

    def current_user(self):
        return self.user

    def json(self, data, status=200):
        self.status, self.body = status, data

    def send_error(self, status, message=None):
        self.status, self.body = status, {"error": message or "The change could not be applied"}


def decide(user, change_id, decision, remark=""):
    from urllib.parse import urlparse
    from backend.routes import dispatch
    from backend import outlet_access
    if decision not in ("approve", "reject"):
        raise ValueError("Choose approve or reject")
    with connect() as db:
        row = db.execute("SELECT * FROM change_requests WHERE id = ?", (change_id,)).fetchone()
        if not row:
            raise WorkflowError("Change not found", 404)
        row = dict(row)
        payload = load_value(row["payload_data_id"]) or {}
    if row["status"] != "Pending":
        raise WorkflowError("This change has already been decided")
    if row["requested_by_user_id"] == user.get("id"):
        raise PermissionError("Someone else must decide on your own change")
    if not allowed(user, row["record"], "approve"):
        raise PermissionError(f"You cannot approve {CHANGE_RECORDS[row['record']][0].lower()} changes")
    now = int(time.time() * 1000)
    if decision == "approve":
        parsed = urlparse(row["path"])
        with connect() as db:
            outlet_access.guard_mutation(db, user, row["method"], parsed.path, payload)
        replay = ReplayHandler(user)
        if not dispatch(row["method"], replay, parsed, payload):
            raise WorkflowError("This change can no longer be applied", 410)
        if replay.status >= 400:
            # Left pending, so the approver can see why and reject it instead.
            raise WorkflowError((replay.body or {}).get("error") or "The change could not be applied", replay.status)
    status = "Approved" if decision == "approve" else "Rejected"
    with connect() as db:
        db.execute("UPDATE change_requests SET status = ?, decided_by = ?, decided_at = ?, remark = ? WHERE id = ? AND status = 'Pending'",
                   (status, user.get("name", ""), now, str(remark or "").strip(), change_id))
        if row["requested_by_user_id"]:
            db.execute("INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) "
                       "VALUES (?, ?, 'In-App', 'Unread', 'change', ?, ?, ?)",
                       (f"Your change was {status.lower()}", f"{row['summary']}: {status.lower()} by {user.get('name')}"
                        + (f" ({remark})" if remark else ""), change_id, now, row["requested_by_user_id"]))
        activity.log(db, user, f"change_{status.lower()}", "change", change_id, f"CHG-{change_id:05d}", detail=row["summary"],
                     started_at=row["created_at"], at=now)
    return {"ok": True, "status": status}


def grant_existing_access(db):
    """Once: everyone keeps what they can do today. A role (or a person's own permissions) that
    opens a page may add, edit, delete, and approve the records on it."""
    if db.execute("SELECT 1 FROM app_settings WHERE key = 'system.changePermissions'").fetchone():
        return

    def actions_for(pages):
        return [f"{record}.{level}" for record, (_, page, _) in CHANGE_RECORDS.items() if page in pages for level in ("manage", "approve")]

    for row in db.execute("SELECT id, name, permissions_data_id FROM roles").fetchall():
        pages = load_value(row["permissions_data_id"]) or [] if row["permissions_data_id"] else []
        # Users and roles were changed only by administrators before.
        if row["name"] != ADMIN_ROLE:
            pages = [page for page in pages if page not in ("users", "roles")]
        else:
            pages = [*pages, "users", "roles", "departments", "outlets", "equipment"]
        db.execute("UPDATE roles SET action_permissions_data_id = ? WHERE id = ?", (save_value(db, actions_for(pages)), row["id"]))
    for row in db.execute("SELECT id, role, permission_overrides_data_id FROM users WHERE permission_overrides_data_id IS NOT NULL").fetchall():
        overrides = load_value(row["permission_overrides_data_id"]) or {}
        pages = [page for page in overrides.get("permissions") or [] if row["role"] == ADMIN_ROLE or page not in ("users", "roles")]
        db.execute("UPDATE users SET permission_overrides_data_id = ? WHERE id = ?",
                   (save_value(db, {**overrides, "actions": actions_for(pages)}), row["id"]))
    db.execute("INSERT INTO app_settings (key, value_data_id) VALUES ('system.changePermissions', ?)", (save_value(db, True),))
