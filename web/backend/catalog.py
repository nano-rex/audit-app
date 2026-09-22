"""Catalog for the audit application."""
from backend.relational_values import load_value, hydrate_many
from backend.config import APP_TABS, SUPER_ROLE
from backend.database import connect
from backend.response_cache import cached_response


def setup_records(include_super=False):
    with connect() as db:
        departments = [dict(row) for row in db.execute(
            "SELECT id, code, description, responsibilities FROM departments ORDER BY code"
        ).fetchall()]
        outlets = [dict(row) for row in db.execute(
            "SELECT id, code, location, description FROM outlets ORDER BY code"
        ).fetchall()]
        zones = [dict(row) for row in db.execute(
            "SELECT id, outlet_code, name, locations_data_id, description FROM zones ORDER BY outlet_code, name"
        ).fetchall()]
        categories = [dict(row) for row in db.execute(
            "SELECT id, name, description, sequence, active FROM categories ORDER BY sequence, name"
        ).fetchall()]
        role_query = "SELECT id, name, description, permissions_data_id, inspection_permissions_data_id, protected FROM roles"
        role_query += " WHERE name = ?" if include_super else " WHERE name != ?"
        roles = [dict(row) for row in db.execute(
            role_query + " ORDER BY protected DESC, name", (SUPER_ROLE,)
        ).fetchall()]
        priorities = [dict(row) for row in db.execute(
            "SELECT id, name, classification, due_days, active FROM priority_levels ORDER BY due_days, name"
        ).fetchall()]
        audit_types = [dict(row) for row in db.execute(
            "SELECT id, name, description, active FROM audit_types ORDER BY name"
        ).fetchall()]
        settings = {row["key"]: load_value(row["value_data_id"]) for row in db.execute("SELECT key, value_data_id FROM app_settings ORDER BY key").fetchall()}
    for zone in zones:
        zone["locations"] = load_value(zone.pop("locations_data_id") or "[]")
    for role in roles:
        role["inspectionPermissions"] = load_value(role.pop("inspection_permissions_data_id") or "[]")
        role["permissions"] = load_value(role.pop("permissions_data_id") or "[]")
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


def role_items(include_super=False):
    with connect() as db:
        role_query = "SELECT id, name, description, permissions_data_id, inspection_permissions_data_id, protected FROM roles"
        role_query += " WHERE name = ?" if include_super else " WHERE name != ?"
        rows = db.execute(role_query + " ORDER BY protected DESC, name", (SUPER_ROLE,)).fetchall()
    items = []
    for row in rows:
        item = dict(row)
        item["permissions"] = load_value(item.pop("permissions_data_id") or "[]")
        item["inspectionPermissions"] = load_value(item.pop("inspection_permissions_data_id") or "[]")
        items.append(item)
    return {"items": items, "tabs": [{"id": tab[0], "label": tab[1]} for tab in APP_TABS]}


def users(include_super=False):
    with connect() as db:
        role_filter = "" if include_super else "WHERE role != ?"
        rows = db.execute(
            f"""
            SELECT id, name, username, role, email, department, active, reset_required,
                   last_login_at, login_count, title, responsibilities, permission_overrides_data_id,
                   EXISTS(SELECT 1 FROM password_reset_requests WHERE user_id = users.id AND resolved_at IS NULL) AS reset_requested
            FROM users
            {role_filter}
            ORDER BY role, name
            """, () if include_super else (SUPER_ROLE,)
        ).fetchall()
    items = [dict(row) for row in rows]
    for item in items:
        raw = item.pop("permission_overrides_data_id")
        item["permissionOverrides"] = load_value(raw) if raw is not None else None
    return {"items": items}


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
    return {"items": hydrate_many(rows)}


def zones(outlet=""):
    with connect() as db:
        if outlet:
            rows = db.execute(
                """
                SELECT id, outlet_code, name, locations_data_id, description
                FROM zones
                WHERE outlet_code = ?
                ORDER BY name
                """,
                (outlet,),
            ).fetchall()
        else:
            rows = db.execute(
                """
                SELECT id, outlet_code, name, locations_data_id, description
                FROM zones
                ORDER BY outlet_code, name
                """
            ).fetchall()
    items = []
    for row in rows:
        item = dict(row)
        item["locations"] = load_value(item.pop("locations_data_id") or "[]")
        items.append(item)
    return {"items": items}


def equipment_items(outlet=None, compact=False):
    return cached_response(("equipment", outlet, compact), lambda: query_equipment_items(outlet, compact))


def query_equipment_items(outlet=None, compact=False):
    where = ""
    params = ()
    if outlet:
        where = "WHERE outlet = ?"
        params = (outlet,)
    columns = ("id, asset_id, outlet, zone, equipment_type, name, type, code, location, "
               "inspection_criteria_data_id") if compact else ("id, asset_id, qr_code, outlet, zone, equipment_type, health_status, "
               "last_checked, replacement_flag, notes, name, description, type, operational_status, code, model, serial_number, brand, location, "
               "installation_date, temporary_relocation, warranty_date, calibration_date, expiry_date, photos_data_id, inverter_model, motor_capacity, source_file, source_sheet, inspection_criteria_data_id")
    with connect() as db:
        rows = db.execute(
            f"""
            SELECT {columns}
            FROM equipment
            {where}
            ORDER BY COALESCE(name, asset_id), id
            """,
            params,
        ).fetchall()
    return {"items": hydrate_many(rows)}


def user_login_activity(user_id, offset=0):
    offset = max(0, int(offset))
    with connect() as db:
        user = db.execute("SELECT name FROM users WHERE id = ?", (user_id,)).fetchone()
        if not user:
            from backend.workflow import WorkflowError
            raise WorkflowError("User not found", 404)
        rows = db.execute("SELECT id, email, logged_at, remember_me, user_agent FROM user_login_activity WHERE user_id = ? ORDER BY logged_at DESC, id DESC LIMIT 51 OFFSET ?", (user_id, offset)).fetchall()
    return {"name": user["name"], "items": [dict(row) for row in rows[:50]], "offset": offset, "hasMore": len(rows) > 50}
