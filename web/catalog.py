"""Catalog for the audit application."""
import json
from config import APP_TABS
from database import connect
from response_cache import cached_response


def setup_records():
    with connect() as db:
        departments = [dict(row) for row in db.execute(
            "SELECT id, code, description, responsibilities FROM departments ORDER BY code"
        ).fetchall()]
        outlets = [dict(row) for row in db.execute(
            "SELECT id, code, location, description FROM outlets ORDER BY code"
        ).fetchall()]
        zones = [dict(row) for row in db.execute(
            "SELECT id, outlet_code, name, locations_json, description FROM zones ORDER BY outlet_code, name"
        ).fetchall()]
        categories = [dict(row) for row in db.execute(
            "SELECT id, name, description, sequence, active FROM categories ORDER BY sequence, name"
        ).fetchall()]
        roles = [dict(row) for row in db.execute(
            "SELECT id, name, description, permissions_json, protected FROM roles ORDER BY protected DESC, name"
        ).fetchall()]
        priorities = [dict(row) for row in db.execute(
            "SELECT id, name, classification, due_days, active FROM priority_levels ORDER BY due_days, name"
        ).fetchall()]
        audit_types = [dict(row) for row in db.execute(
            "SELECT id, name, description, active FROM audit_types ORDER BY name"
        ).fetchall()]
        settings = {row["key"]: json.loads(row["value"]) for row in db.execute("SELECT key, value FROM app_settings ORDER BY key").fetchall()}
    for zone in zones:
        zone["locations"] = json.loads(zone.pop("locations_json") or "[]")
    for role in roles:
        role["permissions"] = json.loads(role.pop("permissions_json") or "[]")
    return {
        "departments": departments,
        "outlets": outlets,
        "zones": zones,
        "categories": categories,
        "roles": roles,
        "priorities": priorities,
        "auditTypes": audit_types,
        "settings": settings,
        "tabs": [{"id": tab[0], "label": tab[1]} for tab in APP_TABS],
    }


def role_items():
    with connect() as db:
        rows = db.execute(
            "SELECT id, name, description, permissions_json, protected FROM roles ORDER BY protected DESC, name"
        ).fetchall()
    items = []
    for row in rows:
        item = dict(row)
        item["permissions"] = json.loads(item.pop("permissions_json") or "[]")
        items.append(item)
    return {"items": items, "tabs": [{"id": tab[0], "label": tab[1]} for tab in APP_TABS]}


def users():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, name, role, email, department, active, reset_required,
                   last_login_at, login_count, title, responsibilities
            FROM users
            ORDER BY role, name
            """
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def locations(outlet):
    with connect() as db:
        rows = db.execute(
            """
            SELECT locations.id, locations.outlet_code, locations.name, locations.floor,
                   locations.area, locations.display_order, locations.size, locations.qr_code,
                   GROUP_CONCAT(COALESCE(equipment.name, equipment.asset_id), ', ') equipment
            FROM locations
            LEFT JOIN equipment
              ON equipment.outlet = locations.outlet_code
             AND equipment.location = locations.name
            WHERE locations.outlet_code = ?
            GROUP BY locations.id
            ORDER BY locations.display_order, locations.floor, locations.area, locations.name
            """,
            (outlet,),
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def zones(outlet=""):
    with connect() as db:
        if outlet:
            rows = db.execute(
                """
                SELECT id, outlet_code, name, locations_json, description
                FROM zones
                WHERE outlet_code = ?
                ORDER BY name
                """,
                (outlet,),
            ).fetchall()
        else:
            rows = db.execute(
                """
                SELECT id, outlet_code, name, locations_json, description
                FROM zones
                ORDER BY outlet_code, name
                """
            ).fetchall()
    items = []
    for row in rows:
        item = dict(row)
        item["locations"] = json.loads(item.pop("locations_json") or "[]")
        items.append(item)
    return {"items": items}


def equipment_items(outlet=None):
    return cached_response(("equipment", outlet), lambda: query_equipment_items(outlet))


def query_equipment_items(outlet=None):
    where = ""
    params = ()
    if outlet:
        where = "WHERE outlet = ?"
        params = (outlet,)
    with connect() as db:
        rows = db.execute(
            f"""
            SELECT id, asset_id, qr_code, outlet, zone, equipment_type, health_status,
                   last_checked, replacement_flag, notes, name, description, type,
                   operational_status, code, model, serial_number, brand, location,
                   installation_date, temporary_relocation, warranty_date, calibration_date,
                   expiry_date, photos, inverter_model, motor_capacity, source_file,
                   source_sheet, inspection_criteria
            FROM equipment
            {where}
            ORDER BY COALESCE(name, asset_id), id
            """,
            params,
        ).fetchall()
    return {"items": [dict(row) for row in rows]}
