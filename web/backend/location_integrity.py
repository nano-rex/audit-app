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
        for table in ("schedules", "inspection_sessions"):
            for row in db.execute(f"SELECT locations_data_id FROM {table} WHERE outlet = ? AND locations_data_id IS NOT NULL", (outlet,)):
                if name in (load_value(row["locations_data_id"]) or []):
                    raise WorkflowError("This location is chosen for scheduled work and must be retained")
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


ALL_LOCATIONS = "All Locations"


def visit_locations(db, outlet, payload):
    """The locations a scheduled visit covers; an empty list means every location of the outlet.

    Older clients send one location as "zone"; it is read as a selection of that one location.
    """
    values = payload.get("locations")
    if values is None:
        zone = payload.get("zone")
        found = zone and db.execute("SELECT 1 FROM locations WHERE outlet_code = ? AND name = ?", (outlet, zone)).fetchone()
        values = [zone] if found else []
    return zone_locations(db, outlet, values)


def locations_label(values):
    return ", ".join(values) if values else ALL_LOCATIONS


def visit_scope(db, outlet, payload):
    """What a visit covers, chosen on one of three tabs: locations (none ticked means all), zones
    (their locations), or particular assets (and so their locations). Returns the locations, the
    scope as chosen ({"by": ..., "zones": [...]} or {"by": "assets", "assets": [ids]}), and its label."""
    by = payload.get("scope") or "locations"
    if by == "zones":
        names = payload.get("zones")
        if not isinstance(names, list) or not names or any(not isinstance(name, str) for name in names):
            raise ValueError("Choose at least one zone")
        rows = {row["name"]: load_value(row["locations_data_id"]) or [] for row in
                db.execute("SELECT name, locations_data_id FROM zones WHERE outlet_code = ?", (outlet,))}
        names = list(dict.fromkeys(names))
        if any(name not in rows for name in names):
            raise ValueError("Every zone must belong to the selected outlet")
        locations = list(dict.fromkeys(location for name in names for location in rows[name]))
        if not locations:
            raise ValueError("The chosen zones have no locations yet")
        return locations, {"by": "zones", "zones": names}, f"Zone{'s' if len(names) > 1 else ''}: {', '.join(names)}"
    if by == "assets":
        ids = payload.get("assets")
        if not isinstance(ids, list) or not ids or not all(str(value).isdigit() for value in ids):
            raise ValueError("Choose at least one asset")
        ids = list(dict.fromkeys(int(value) for value in ids))
        rows = []
        for start in range(0, len(ids), 500):
            chunk = ids[start:start + 500]
            rows += db.execute(f"SELECT id, location, zone FROM equipment WHERE outlet = ? AND id IN ({','.join('?' for _ in chunk)})", (outlet, *chunk)).fetchall()
        if len(rows) != len(ids):
            raise ValueError("Every asset must belong to the selected outlet")
        locations = list(dict.fromkeys(row["location"] or row["zone"] or "Unassigned" for row in rows))
        return locations, {"by": "assets", "assets": ids}, f"{len(ids)} asset{'s' if len(ids) != 1 else ''} in {', '.join(locations)}"[:500]
    if by != "locations":
        raise ValueError("Choose locations, zones, or assets")
    locations = visit_locations(db, outlet, payload)
    return locations, {"by": "locations"}, locations_label(locations)


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
    chosen = {int(identifier) for identifier in identifiers}
    for identifier in chosen:
        db.execute("UPDATE equipment SET location = ?, zone = ? WHERE id = ?", (name, name, identifier))
    # The list is the location's full membership: an item left out is no longer here.
    for row in db.execute("SELECT id FROM equipment WHERE outlet = ? AND location = ?", (outlet, name)).fetchall():
        if row["id"] not in chosen:
            db.execute("UPDATE equipment SET location = '', zone = 'Unassigned' WHERE id = ?", (row["id"],))
