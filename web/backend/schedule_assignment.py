"""Who a scheduled visit is assigned to: people who can audit, at the visit's outlet.

A superior picks one or more people; each newly assigned person is notified, the visit waits
for them under Waiting on you, and the assignment is logged.
"""
import time

from backend import activity
from backend.outlet_access import role_scope
from backend.permissions import resolve_permissions
from backend.relational_values import load_value, save_value


def assignable_people(db, outlet):
    """Active people who may audit (Inspections page and auditor permission) and cover the outlet."""
    people = []
    for row in db.execute("SELECT id, name, role, title, outlets_data_id, permission_overrides_data_id, profile_photo_data_id FROM users "
                          "WHERE active = 1 ORDER BY name"):
        overrides = load_value(row["permission_overrides_data_id"]) if row["permission_overrides_data_id"] is not None else None
        pages, capabilities = resolve_permissions(db, row["role"], overrides)
        if "inspections" not in pages or "auditor" not in capabilities:
            continue
        outlets = (load_value(row["outlets_data_id"]) or []) if row["outlets_data_id"] else []
        if outlet and role_scope(db, row["role"]) != "all" and outlet not in outlets:
            continue
        photo = (load_value(row["profile_photo_data_id"]) or {}) if row["profile_photo_data_id"] else {}
        people.append({"id": row["id"], "name": row["name"], "role": row["role"], "title": row["title"] or "",
                       "profilePhoto": {"url": photo["url"]} if isinstance(photo, dict) and photo.get("url") else {}})
    return people


def validate_assignees(db, outlet, values):
    if values in (None, ""):
        return []
    if not isinstance(values, list):
        raise ValueError("Assignees must be a list of people")
    try:
        ids = list(dict.fromkeys(int(value) for value in values))
    except (TypeError, ValueError):
        raise ValueError("Assignees must be a list of people") from None
    allowed = {person["id"]: person for person in assignable_people(db, outlet)}
    missing = [value for value in ids if value not in allowed]
    if missing:
        raise ValueError("Assign only people who can audit and cover this outlet")
    return [allowed[value] for value in ids]


def assignees_of(reference):
    return (load_value(reference) or []) if reference else []


def save_assignment(db, user, schedule_id, outlet, scheduled_date, people, previous_ids=(), business_unit="Ottotree"):
    """Store the people, notify those newly assigned, and log who assigned them."""
    ids = [person["id"] for person in people]
    names = ", ".join(person["name"] for person in people) or "Unassigned"
    db.execute("UPDATE schedules SET assignees_data_id = ?, auditor = ? WHERE id = ?", (save_value(db, ids), names, schedule_id))
    added = [person for person in people if person["id"] not in set(previous_ids)]
    if not added:
        return
    now = int(time.time() * 1000)
    reference = f"SCH-{int(schedule_id):05d}"
    db.executemany(
        "INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) "
        "VALUES (?,?,'In-App','Unread','schedule',?,?,?)",
        [("Scheduled audit assigned to you", f"{reference}: {outlet} on {scheduled_date}, assigned by {user.get('name') or 'a supervisor'}",
          schedule_id, now, person["id"]) for person in added])
    activity.log(db, user, "audit_assigned", "schedule", schedule_id, reference, outlet,
                 detail=", ".join(person["name"] for person in added), business_unit=business_unit, at=now)
