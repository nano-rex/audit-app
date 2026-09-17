"""Routes locations for the audit application."""
import json
import time
from common import location_qr_code
from database import connect, first_outlet
from seed_data import add_locations_to_default_zone


def post_locations(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        outlet = payload.get("outlet") or default_outlet
        location_name = payload.get("name", "New Location")
        db.execute(
            """
            INSERT OR REPLACE INTO locations (outlet_code, name, floor, area, display_order, size, qr_code, created_at)
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
        for item_id in payload.get("equipmentIds") or []:
            db.execute(
                "UPDATE equipment SET outlet = ?, location = ?, zone = ? WHERE id = ?",
                (outlet, location_name, location_name, int(item_id)),
            )
    self.json({"ok": True})


def post_zones(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        db.execute(
            """
            INSERT OR REPLACE INTO zones (outlet_code, name, locations_json, description, created_at)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                payload.get("outlet") or default_outlet,
                payload.get("name", "Zone-1"),
                json.dumps(payload.get("locations") or []),
                payload.get("description", ""),
                now,
            ),
        )
    self.json({"ok": True})


def post_setup_outlets(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute(
            """
            INSERT OR REPLACE INTO outlets (code, location, description, created_at)
            VALUES (?, ?, ?, ?)
            """,
            (
                (payload.get("code") or "Outlet").upper(),
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
        cursor = db.execute(
            """
            UPDATE outlets
            SET code = ?, location = ?, description = ?
            WHERE id = ?
            """,
            (
                (payload.get("code") or "Outlet").upper(),
                payload.get("location", ""),
                payload.get("description", ""),
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
        outlet = payload.get("outlet") or first_outlet(db)
        location_name = payload.get("name", "New Location")
        cursor = db.execute(
            """
            UPDATE locations
            SET outlet_code = ?, name = ?, floor = ?, area = ?, display_order = ?, size = ?, qr_code = ?
            WHERE id = ?
            """,
            (
                outlet,
                location_name,
                payload.get("floor", ""),
                payload.get("area", ""),
                int(payload.get("displayOrder") or 0),
                payload.get("size", ""),
                payload.get("qrCode") or location_qr_code(outlet, location_name),
                int(location_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
        add_locations_to_default_zone(db, outlet)
        for item_id in payload.get("equipmentIds") or []:
            db.execute(
                "UPDATE equipment SET outlet = ?, location = ?, zone = ? WHERE id = ?",
                (outlet, location_name, location_name, int(item_id)),
            )
    self.json({"ok": True})
    return


def patch_zones(self, parsed, payload=None):
    zone_id = parsed.path.rsplit("/", 1)[-1]
    if not zone_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute(
            """
            UPDATE zones
            SET outlet_code = ?, name = ?, locations_json = ?, description = ?
            WHERE id = ?
            """,
            (
                payload.get("outlet") or first_outlet(db),
                payload.get("name", "Zone-1"),
                json.dumps(payload.get("locations") or []),
                payload.get("description", ""),
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
        row = db.execute("SELECT outlet_code, name FROM locations WHERE id = ?", (int(record_id),)).fetchone()
        cursor = db.execute("DELETE FROM locations WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
        if row:
            db.execute(
                "UPDATE equipment SET location = '', zone = 'Unassigned' WHERE outlet = ? AND location = ?",
                (row["outlet_code"], row["name"]),
            )
    self.json({"ok": True})
    return


def delete_zones(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute("DELETE FROM zones WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return
