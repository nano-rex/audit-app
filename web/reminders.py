"""In-app assignment and daily due-date reminders, without external providers."""
from datetime import date, timedelta
import time

from database import connect
from permissions import resolve_permissions
from relational_values import load_value
from workflow import assigned_to


def recipients(db, order, include_reviewers=False):
    people = [dict(row) for row in db.execute("SELECT id, name, email, department, role, permission_overrides_data_id FROM users WHERE active = 1")]
    selected = {person["id"] for person in people if assigned_to(person, order)}
    if include_reviewers:
        roles = {}
        for person in people:
            key = (person["role"], person["permission_overrides_data_id"])
            if key not in roles:
                override = load_value(key[1]) if key[1] is not None else None
                roles[key] = resolve_permissions(db, person["role"], override)[1]
            if "verifier" in roles[key]:
                selected.add(person["id"])
    if not selected:
        selected = {person["id"] for person in people if person["role"] in {"Super", "Admin"}}
    return selected


def notify_work_order(db, work_order_id, status):
    order = dict(db.execute("SELECT * FROM work_orders WHERE id = ?", (work_order_id,)).fetchone())
    now = int(time.time() * 1000)
    for recipient in recipients(db, order, include_reviewers=status == "Completed"):
        db.execute("INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread','work_order',?,?,?)",
                   (f"Work order {status.lower()}", f"{order['work_order_ref']}: {order['title']}", work_order_id, now, recipient))


def deliver_due_reminders(today=None):
    today = today or date.today()
    count = 0
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        orders = db.execute("SELECT * FROM work_orders WHERE status NOT IN ('Completed','Verified','Closed') AND due_date IS NOT NULL AND due_date != '' AND due_date <= ?", ((today + timedelta(days=3)).isoformat(),)).fetchall()
        for row in orders:
            order = dict(row)
            try:
                due = date.fromisoformat(order["due_date"])
            except ValueError:
                continue
            kind = "Overdue" if due < today else "Due soon"
            for recipient in recipients(db, order):
                event = db.execute("INSERT OR IGNORE INTO due_notification_events(user_id,work_order_id,kind,day,due_date) VALUES (?,?,?,?,?)", (recipient, order["id"], kind, today.isoformat(), due.isoformat()))
                if event.rowcount:
                    db.execute("INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread','work_order',?,?,?)",
                               (kind, f"{order['work_order_ref'] or order['id']}: {order['title']} — due {due.isoformat()}", order["id"], int(time.time() * 1000), recipient))
                    count += 1
    return count


def reminder_loop(stop):
    import logging
    while not stop.is_set():
        try:
            deliver_due_reminders()
        except Exception:
            logging.exception("Unable to deliver audit reminders")
        stop.wait(60)
