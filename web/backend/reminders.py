"""In-app assignment and daily due-date reminders, without external providers."""
from datetime import date, timedelta
import time

from backend.database import connect
from backend.workflow import assigned_to


def active_people(db):
    return [dict(row) for row in db.execute("SELECT id, name, email, department, role, permission_overrides_data_id FROM users WHERE active = 1")]


def recipients(db, order, people=None):
    people = active_people(db) if people is None else people
    selected = {person["id"] for person in people if assigned_to(person, order)}
    if not selected:
        selected = {person["id"] for person in people if person["role"] in {"Super", "Admin"}}
    return selected


def notify_work_order(db, work_order_id, status):
    order = dict(db.execute("SELECT * FROM work_orders WHERE id = ?", (work_order_id,)).fetchone())
    now = int(time.time() * 1000)
    for recipient in recipients(db, order):
        db.execute("INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread','work_order',?,?,?)",
                   (f"Work order {status.lower()}", f"{order['work_order_ref']}: {order['title']}", work_order_id, now, recipient))


OPEN_ORDERS = "status NOT IN ('Completed','Verified','Closed')"


def pending_reminders(db, today):
    """Reminders owed today that have not been recorded; reads only."""
    orders = db.execute(f"SELECT * FROM work_orders WHERE {OPEN_ORDERS} AND due_date IS NOT NULL AND due_date != '' AND due_date <= ?", ((today + timedelta(days=3)).isoformat(),)).fetchall()
    if not orders:
        return []
    sent = {tuple(row) for row in db.execute("SELECT user_id, work_order_id, kind, due_date FROM due_notification_events WHERE day = ?", (today.isoformat(),))}
    people = active_people(db)
    pending = []
    for row in orders:
        order = dict(row)
        try:
            due = date.fromisoformat(order["due_date"])
        except ValueError:
            continue
        kind = "Overdue" if due < today else "Due soon"
        for recipient in sorted(recipients(db, order, people=people)):
            if (recipient, order["id"], kind, due.isoformat()) not in sent:
                pending.append((recipient, order, kind, due))
    return pending


def deliver_due_reminders(today=None):
    today = today or date.today()
    count = 0
    with connect() as db:
        # Every notification read calls this. Take the write lock only when a reminder is owed,
        # so polling clients do not queue behind, or delay, inspection saves.
        pending = pending_reminders(db, today)
        if not pending:
            return 0
        db.execute("BEGIN IMMEDIATE")
        for recipient, order, kind, due in pending:
            if not db.execute(f"SELECT 1 FROM work_orders WHERE id = ? AND {OPEN_ORDERS}", (order["id"],)).fetchone():
                continue
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
