"""Routes locations for the audit application."""
from relational_values import load_value, save_value
from location_integrity import assign_equipment, outlet_exists, protect_history, record, required_name, update_membership, zone_locations
from workflow import WorkflowError
import time
from common import location_qr_code
from database import connect, first_outlet
from seed_data import add_locations_to_default_zone


def post_locations(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        default_outlet = first_outlet(db)
        outlet = payload.get("outlet") or default_outlet
        outlet_exists(db, outlet)
        location_name = required_name(payload.get("name"))
        db.execute(
            """
            INSERT INTO locations (outlet_code, name, floor, area, display_order, size, qr_code, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                outlet,
                location_name,
                payload.get("floor", ""),
                payload.get("area", ""),
                int(payload.get("displayOrder") or 0),
                payload.get("size", ""),
                payload.get("qrCode") or location_qr_code(outlet, location_name),
                now,
            ),
        )
        add_locations_to_default_zone(db, outlet, now)
        assign_equipment(db, outlet, location_name, payload.get("equipmentIds") or [])
    self.json({"ok": True})


def post_zones(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        default_outlet = first_outlet(db)
        outlet = payload.get("outlet") or default_outlet
        outlet_exists(db, outlet)
        members = zone_locations(db, outlet, payload.get("locations") or [])
        db.execute(
            """
            INSERT INTO zones (outlet_code, name, locations_data_id, description, created_at)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                outlet,
                required_name(payload.get("name")),
                save_value(db, members),
                payload.get("description", ""),
                now,
            ),
        )
    self.json({"ok": True})


def post_setup_outlets(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        db.execute(
            """
            INSERT INTO outlets (code, location, description, created_at)
            VALUES (?, ?, ?, ?)
            """,
            (
                required_name(payload.get("code")).upper(),
                payload.get("location", ""),
                payload.get("description", ""),
                now,
            ),
        )
    self.json({"ok": True})


def patch_setup_outlets(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        existing = record(db, "outlets", int(record_id))
        code = required_name(payload.get("code", existing["code"])).upper()
        if code != existing["code"]:
            protect_history(db, existing["code"])
            for room in db.execute("SELECT id, name, qr_code FROM locations WHERE outlet_code = ?", (existing["code"],)).fetchall():
                if room["qr_code"] == location_qr_code(existing["code"], room["name"]):
                    db.execute("UPDATE locations SET qr_code = ? WHERE id = ?", (location_qr_code(code, room["name"]), room["id"]))
            for table, column in (("locations", "outlet_code"), ("zones", "outlet_code"), ("equipment", "outlet")):
                db.execute(f"UPDATE {table} SET {column} = ? WHERE {column} = ?", (code, existing["code"]))
        cursor = db.execute(
            """
            UPDATE outlets
            SET code = ?, location = ?, description = ?
            WHERE id = ?
            """,
            (
                code,
                payload.get("location", existing["location"]),
                payload.get("description", existing["description"]),
                int(record_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def patch_locations(self, parsed, payload=None):
    location_id = parsed.path.rsplit("/", 1)[-1]
    if not location_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        existing = record(db, "locations", int(location_id))
        outlet = payload.get("outlet") or existing["outlet_code"]
        if outlet != existing["outlet_code"]:
            raise WorkflowError("Create a new location to move it to a different outlet")
        outlet_exists(db, outlet)
        location_name = required_name(payload.get("name", existing["name"]))
        if location_name != existing["name"]:
            protect_history(db, outlet, existing["name"])
            update_membership(db, outlet, existing["name"], location_name)
            db.execute("UPDATE equipment SET location = ? WHERE outlet = ? AND location = ?", (location_name, outlet, existing["name"]))
            db.execute("UPDATE equipment SET zone = ? WHERE outlet = ? AND zone = ?", (location_name, outlet, existing["name"]))
        cursor = db.execute(
            """
            UPDATE locations
            SET outlet_code = ?, name = ?, floor = ?, area = ?, display_order = ?, size = ?, qr_code = ?
            WHERE id = ?
            """,
            (
                outlet,
                location_name,
                payload.get("floor", existing["floor"]),
                payload.get("area", existing["area"]),
                int(payload.get("displayOrder", existing["display_order"]) or 0),
                payload.get("size", existing["size"]),
                payload.get("qrCode") or (location_qr_code(outlet, location_name) if existing["qr_code"] == location_qr_code(outlet, existing["name"]) else existing["qr_code"]),
                int(location_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
        add_locations_to_default_zone(db, outlet)
        assign_equipment(db, outlet, location_name, payload.get("equipmentIds") or [])
    self.json({"ok": True})
    return


def patch_zones(self, parsed, payload=None):
    zone_id = parsed.path.rsplit("/", 1)[-1]
    if not zone_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        existing = record(db, "zones", int(zone_id))
        outlet = payload.get("outlet") or existing["outlet_code"]
        if outlet != existing["outlet_code"]:
            raise WorkflowError("Create a new zone to move it to a different outlet")
        name = required_name(payload.get("name", existing["name"]))
        if name != existing["name"]:
            protect_history(db, outlet, existing["name"])
            db.execute("UPDATE equipment SET zone = ? WHERE outlet = ? AND zone = ?", (name, outlet, existing["name"]))
        members = zone_locations(db, outlet, payload.get("locations", load_value(existing["locations_data_id"]) or []))
        cursor = db.execute(
            """
            UPDATE zones
            SET outlet_code = ?, name = ?, locations_data_id = ?, description = ?
            WHERE id = ?
            """,
            (
                outlet,
                name,
                save_value(db, members),
                payload.get("description", existing["description"]),
                int(zone_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_setup_outlets(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        existing = record(db, "outlets", int(record_id))
        protect_history(db, existing["code"])
        for table, column in (("locations", "outlet_code"), ("zones", "outlet_code"), ("equipment", "outlet")):
            if db.execute(f"SELECT 1 FROM {table} WHERE {column} = ? LIMIT 1", (existing["code"],)).fetchone():
                raise WorkflowError("Remove this outlet's locations, zones, and equipment before deleting it")
        cursor = db.execute("DELETE FROM outlets WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_locations(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        row = record(db, "locations", int(record_id))
        protect_history(db, row["outlet_code"], row["name"])
        if db.execute("SELECT 1 FROM equipment WHERE outlet = ? AND (location = ? OR zone = ?) LIMIT 1", (row["outlet_code"], row["name"], row["name"])).fetchone():
            raise WorkflowError("Relocate this location's equipment before deleting it")
        update_membership(db, row["outlet_code"], row["name"])
        db.execute("DELETE FROM locations WHERE id = ?", (int(record_id),))
    self.json({"ok": True})
    return


def delete_zones(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        row = record(db, "zones", int(record_id))
        protect_history(db, row["outlet_code"], row["name"])
        if db.execute("SELECT 1 FROM equipment WHERE outlet = ? AND zone = ? LIMIT 1", (row["outlet_code"], row["name"])).fetchone():
            raise WorkflowError("Relocate this zone's equipment before deleting it")
        cursor = db.execute("DELETE FROM zones WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return
