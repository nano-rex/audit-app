"""Role inheritance and explicit per-user permission overrides."""
from backend.relational_values import load_value
from datetime import datetime, timezone

from backend.config import APP_TABS, SUPER_ROLE

INSPECTION_PERMISSIONS = ("auditor", "verifier", "acknowledger")

# Records whose changes are permitted per person: adding, editing and deleting them ("manage"),
# and approving changes made by those who may manage but not approve ("approve").
# Each entry: label, the page that shows them, and the API routes that change them.
CHANGE_RECORDS = {
    "assets": ("Assets (fixed assets, fixtures & finishes)", "equipment", ("equipment",)),
    "users": ("Users", "users", ("users",)),
    "outlets": ("Outlets", "outlets", ("setup/outlets",)),
    "zones": ("Zones", "outlets", ("zones",)),
    "locations": ("Locations", "outlets", ("locations",)),
    "departments": ("Departments", "departments", ("setup/departments",)),
    "roles": ("Roles", "roles", ("roles",)),
}
ACTION_PERMISSIONS = tuple(f"{record}.{level}" for record in CHANGE_RECORDS for level in ("manage", "approve"))


def validate_list(value, allowed):
    if not isinstance(value, list) or any(not isinstance(item, str) or item not in allowed for item in value):
        raise ValueError("Select valid permissions")
    return list(dict.fromkeys(value))


def validate_overrides(value):
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError("Permission overrides must be an object or null")
    validated = {
        "permissions": validate_list(value.get("permissions", []), {tab[0] for tab in APP_TABS}),
        "inspectionPermissions": validate_list(value.get("inspectionPermissions", []), INSPECTION_PERMISSIONS),
    }
    # Personal change permissions are optional; without them the role's apply.
    if "actions" in value:
        validated["actions"] = validate_list(value["actions"], set(ACTION_PERMISSIONS))
    return validated


def resolve_permissions(db, role_name, overrides=None):
    if role_name == SUPER_ROLE:
        return [tab[0] for tab in APP_TABS], list(INSPECTION_PERMISSIONS)
    if overrides is not None:
        value = validate_overrides(overrides)
        return value["permissions"], value["inspectionPermissions"]
    role = db.execute("SELECT permissions_data_id, inspection_permissions_data_id FROM roles WHERE name = ?", (role_name,)).fetchone()
    if not role:
        return [], []
    return load_value(role["permissions_data_id"] or "[]"), load_value(role["inspection_permissions_data_id"] or "[]")


def resolve_actions(db, role_name, overrides=None):
    """The change permissions a person has: their own if set, otherwise their role's."""
    if role_name == SUPER_ROLE:
        return list(ACTION_PERMISSIONS)
    if overrides is not None and "actions" in overrides:
        return validate_overrides(overrides)["actions"]
    role = db.execute("SELECT action_permissions_data_id FROM roles WHERE name = ?", (role_name,)).fetchone()
    return (load_value(role["action_permissions_data_id"]) or []) if role and role["action_permissions_data_id"] else []


def authorize_inspection_update(user, payload, existing=None):
    payload = dict(payload)
    payload["auditor"] = existing["auditor"] if existing else user["name"]
    capabilities = set(user.get("inspectionPermissions", []))
    if existing is None:
        if "auditor" not in capabilities:
            raise PermissionError("Auditor permission is required to create an inspection")
        previous_signatures = {}
        merged = dict(payload)
        changed = True
    else:
        fields = {"businessUnit": "business_unit", "outlet": "outlet", "zone": "zone", "auditDate": "audit_date",
                  "auditor": "auditor", "auditTime": "audit_time", "auditType": "audit_type", "remarks": "remarks"}
        merged = {key: existing[column] for key, column in fields.items()}
        merged["items"] = load_value(existing["items_data_id"] or "[]")
        previous_signatures = load_value(existing["signatures_data_id"] or "{}")
        merged["signatures"] = previous_signatures
        changed = any(key in payload and payload[key] != value for key, value in merged.items() if key != "signatures")
        if "auditor" not in capabilities and (changed or payload.get("complete") and existing["status"] != "Completed"):
            raise PermissionError("Auditor permission is required to edit or complete inspection items")
        merged.update(payload)
    signatures = merged.get("signatures") or {}
    if not isinstance(signatures, dict):
        raise ValueError("Signatures must be an object")
    required = {"auditedBy": "auditor", "verifiedBy": "verifier", "acknowledgedBy": "acknowledger"}
    for key in set(signatures) | set(previous_signatures):
        if key not in required:
            raise ValueError("Unknown inspection signature")
        if signatures.get(key) != previous_signatures.get(key) and required[key] not in capabilities:
            raise PermissionError(f"{required[key].capitalize()} permission is required for this signature")
        if signatures.get(key) and signatures.get(key) != previous_signatures.get(key):
            if not isinstance(signatures[key], dict) or not signatures[key].get("url", "").startswith("/api/media/"):
                raise ValueError("Upload or draw a signature image before signing")
            signatures[key] = {**signatures[key], "name": user["name"], "signedByUserId": user["id"],
                               "signedAt": datetime.now(timezone.utc).isoformat()}
    merged["signatures"] = signatures
    return merged, changed
