"""Who did what, and how long it took: the activity behind the KPI section of Reports.

Each step of an audit, work request, and work order is logged with the person, the time, and,
where a step answers an earlier one, the time it took to act (duration_ms).
"""
import time

ACTIONS = {
    "audit_assigned": "Assigned scheduled audit",
    "audit_started": "Started audit",
    "audit_completed": "Completed audit",
    "audit_signed": "Signed audit",
    "audit_closed": "Closed audit",
    "request_raised": "Raised work request",
    "request_declined": "Declined work request",
    "order_created": "Created work order",
    "order_status": "Changed work order status",
    "order_closed": "Closed work order",
}


def log(db, user, action, record_type, record_id, record_ref="", outlet="", started_at=None, detail="", business_unit="Ottotree", at=None):
    """Record one step. started_at (ms) is when the thing being answered began; the gap is its time to act."""
    at = at or int(time.time() * 1000)
    duration = at - int(started_at) if started_at else None
    db.execute(
        "INSERT INTO activity_log (created_at, user_id, user_name, action, record_type, record_id, record_ref, outlet, business_unit, duration_ms, detail) "
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (at, (user or {}).get("id"), (user or {}).get("name") or "Unknown", action, record_type, record_id, record_ref or "",
         outlet or "", business_unit or "Ottotree", duration if duration is None or duration >= 0 else None, detail or ""))


def last_time(db, action, record_type, record_id):
    row = db.execute("SELECT MAX(created_at) FROM activity_log WHERE action = ? AND record_type = ? AND record_id = ?",
                     (action, record_type, record_id)).fetchone()
    return row[0] if row else None


def backfill(db):
    """Once per database: rebuild what can be known of earlier activity from the records themselves."""
    if db.execute("SELECT 1 FROM app_settings WHERE key = 'system.activityBackfilled'").fetchone():
        return
    from backend.relational_values import save_value
    from datetime import datetime
    names = {row["id"]: row["name"] for row in db.execute("SELECT id, name FROM users")}

    def ms(day):
        try:
            return int(datetime.fromisoformat(str(day)[:10]).timestamp() * 1000)
        except ValueError:
            return None

    for row in db.execute("SELECT * FROM inspection_sessions").fetchall():
        owner = {"id": row["owner_user_id"], "name": names.get(row["owner_user_id"]) or row["auditor"]}
        common = {"record_ref": row["audit_ref"], "outlet": row["outlet"], "business_unit": row["business_unit"]}
        log(db, owner, "audit_started", "inspection", row["id"], at=row["created_at"], **common)
        if row["status"] == "Completed":
            log(db, owner, "audit_completed", "inspection", row["id"], started_at=row["created_at"], at=row["updated_at"], **common)
        if row["closed_at"]:
            log(db, {"name": row["closed_by"]}, "audit_closed", "inspection", row["id"], started_at=row["updated_at"] if row["updated_at"] <= row["closed_at"] else None,
                at=row["closed_at"], **common)
    for row in db.execute("SELECT * FROM work_requests").fetchall():
        common = {"record_ref": row["request_ref"], "outlet": row["outlet"], "business_unit": row["business_unit"]}
        log(db, {"id": row["requested_by_user_id"], "name": row["requested_by"]}, "request_raised", "work_request", row["id"], at=row["created_at"], **common)
        if row["status"] == "Declined":
            log(db, {"name": row["reviewed_by"]}, "request_declined", "work_request", row["id"], started_at=row["created_at"], at=row["updated_at"],
                detail=row["decline_remark"], **common)
    requests = {row["id"]: row for row in db.execute("SELECT id, created_at, request_ref FROM work_requests")}
    for row in db.execute("SELECT * FROM work_orders").fetchall():
        common = {"record_ref": row["work_order_ref"], "outlet": row["outlet"], "business_unit": row["business_unit"]}
        request = requests.get(row["work_request_id"])
        log(db, {"name": None}, "order_created", "work_order", row["id"], started_at=request["created_at"] if request else None,
            at=row["created_at"], detail=request["request_ref"] if request else "", **common)
        # Status changes are kept as system comments with their author and time.
        for change in db.execute("SELECT comment, author, created_at FROM comments WHERE record_type = 'work_order' AND record_id = ? AND system_generated = 1 "
                                 "AND comment LIKE 'Status changed from %' ORDER BY created_at", (row["id"],)):
            closed = change["comment"].rstrip().endswith(" to Closed")
            log(db, {"name": change["author"]}, "order_closed" if closed else "order_status", "work_order", row["id"],
                started_at=row["created_at"] if closed else None, at=change["created_at"], detail=change["comment"], **common)
        if row["status"] == "Closed" and not db.execute("SELECT 1 FROM activity_log WHERE action = 'order_closed' AND record_id = ?", (row["id"],)).fetchone():
            closed_at = ms(row["closed_at"]) if row["closed_at"] else None
            if closed_at:
                log(db, {"name": row["pic"] or row["assignee"]}, "order_closed", "work_order", row["id"], started_at=row["created_at"], at=closed_at, **common)
    db.execute("INSERT INTO app_settings (key, value_data_id) VALUES ('system.activityBackfilled', ?)", (save_value(db, True),))


# Time to act: which step answers which, in the reports' words.
TIMINGS = (
    ("audit", "Start to completion of an audit", ("audit_completed",)),
    ("request", "Work request raised to acted on (order created or declined)", ("order_created", "request_declined")),
    ("order", "Work order opened to closed", ("order_closed",)),
    ("closure", "Audit completed to closed", ("audit_closed",)),
)
COUNTED = ("audit_started", "audit_completed", "audit_signed", "audit_closed", "request_raised", "request_declined", "order_created", "order_closed")


def kpi_report(db, where, params, limit=200):
    """Per-person counts and average time to act, overall averages, and the latest activity."""
    rows = [dict(row) for row in db.execute(f"SELECT * FROM activity_log WHERE {where} ORDER BY created_at DESC, id DESC", params)]
    people = {}
    for row in rows:
        person = people.setdefault(row["user_name"] or "Unknown", {"name": row["user_name"] or "Unknown", **{action: 0 for action in COUNTED}, "_times": {}})
        if row["action"] in COUNTED:
            person[row["action"]] += 1
        for key, _, actions in TIMINGS:
            if row["action"] in actions and row["duration_ms"] is not None:
                person["_times"].setdefault(key, []).append(row["duration_ms"])

    def average(values):
        return round(sum(values) / len(values) / 1000) if values else None

    for person in people.values():
        times = person.pop("_times")
        person.update({f"{key}Seconds": average(times.get(key, [])) for key, _, _ in TIMINGS})
    overall = []
    for key, label, actions in TIMINGS:
        values = [row["duration_ms"] for row in rows if row["action"] in actions and row["duration_ms"] is not None]
        overall.append({"key": key, "label": label, "count": len(values), "averageSeconds": average(values),
                        "longestSeconds": round(max(values) / 1000) if values else None})
    activity = [{**row, "label": ACTIONS.get(row["action"], row["action"])} for row in rows[:limit]]
    ranked = sorted(people.values(), key=lambda person: (-sum(person[action] for action in COUNTED), person["name"]))
    return {"people": ranked, "timeToAct": overall, "activity": activity, "activityTotal": len(rows)}


def duration_text(seconds):
    """3 d 4 h, 2 h 15 m, 12 m, or under a minute."""
    if seconds is None:
        return "—"
    minutes, _ = divmod(int(seconds), 60)
    hours, minutes = divmod(minutes, 60)
    days, hours = divmod(hours, 24)
    if days:
        return f"{days} d {hours} h"
    if hours:
        return f"{hours} h {minutes} m"
    return f"{minutes} m" if minutes else "under a minute"
