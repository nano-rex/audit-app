"""Master-data checks that preserve audit snapshots and asset placement."""
from backend.relational_values import load_value, save_value
from backend.workflow import WorkflowError


def outlet_exists(db, code):
    if not db.execute("SELECT 1 FROM outlets WHERE code = ?", (code,)).fetchone():
        raise ValueError("Select an existing outlet")


def required_name(value):
    value = str(value or "").strip()
    if not value or len(value) > 150:
        raise ValueError("Enter a name or code of 1–150 characters")
    return value


def record(db, table, record_id):
    row = db.execute(f"SELECT * FROM {table} WHERE id = ?", (record_id,)).fetchone()
    if not row:
        raise WorkflowError("Record not found", 404)
    return dict(row)


def protect_history(db, outlet, name=None):
    references = (("audits", "branch"), ("inspection_sessions", "zone"),
                  ("schedules", "zone"), ("findings", "location"), ("work_orders", "zone"))
    for table, column in references:
        where = "outlet = ?"
        values = [outlet]
        if name is not None:
            where += f" AND {column} = ?"
            values.append(name)
        if db.execute(f"SELECT 1 FROM {table} WHERE {where} LIMIT 1", values).fetchone():
            raise WorkflowError("This name is used by audit records or scheduled work. Keep it and edit its other details, or create a new entry.")
    if name is not None:
        for session in db.execute("SELECT items_data_id FROM inspection_sessions WHERE outlet = ?", (outlet,)):
            if any(item.get("location") == name for item in load_value(session["items_data_id"]) or []):
                raise WorkflowError("This location is used by saved inspection items and must be retained")


def zone_locations(db, outlet, values):
    if not isinstance(values, list) or any(not isinstance(value, str) for value in values):
        raise ValueError("Zone locations must be a list of location names")
    available = {row[0] for row in db.execute("SELECT name FROM locations WHERE outlet_code = ?", (outlet,))}
    if any(value not in available for value in values):
        raise ValueError("Every zone location must belong to the selected outlet")
    return list(dict.fromkeys(values))


def update_membership(db, outlet, old_name, new_name=None):
    for row in db.execute("SELECT id, locations_data_id FROM zones WHERE outlet_code = ?", (outlet,)).fetchall():
        previous = load_value(row["locations_data_id"]) or []
        if old_name in previous:
            values = [new_name if name == old_name else name for name in previous if name != old_name or new_name]
            db.execute("UPDATE zones SET locations_data_id = ? WHERE id = ?", (save_value(db, list(dict.fromkeys(values))), row["id"]))


def assign_equipment(db, outlet, name, identifiers):
    if not isinstance(identifiers, list):
        raise ValueError("Equipment IDs must be a list")
    for identifier in identifiers:
        row = db.execute("SELECT outlet FROM equipment WHERE id = ?", (int(identifier),)).fetchone()
        if not row or row["outlet"] != outlet:
            raise ValueError("Select equipment from this outlet; relocate other equipment in the asset editor first")
    for identifier in identifiers:
        db.execute("UPDATE equipment SET location = ?, zone = ? WHERE id = ?", (name, name, int(identifier)))
