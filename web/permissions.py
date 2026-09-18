"""Role inheritance and explicit per-user permission overrides."""
import json
from datetime import datetime, timezone

from config import APP_TABS, SUPER_ROLE

INSPECTION_PERMISSIONS = ("auditor", "verifier", "acknowledger")


def validate_list(value, allowed):
    if not isinstance(value, list) or any(not isinstance(item, str) or item not in allowed for item in value):
        raise ValueError("Select valid permissions")
    return list(dict.fromkeys(value))


def validate_overrides(value):
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError("Permission overrides must be an object or null")
    return {
        "permissions": validate_list(value.get("permissions", []), {tab[0] for tab in APP_TABS}),
        "inspectionPermissions": validate_list(value.get("inspectionPermissions", []), INSPECTION_PERMISSIONS),
    }


def resolve_permissions(db, role_name, overrides=None):
    if role_name == SUPER_ROLE:
        return [tab[0] for tab in APP_TABS], list(INSPECTION_PERMISSIONS)
    if overrides is not None:
        value = validate_overrides(overrides)
        return value["permissions"], value["inspectionPermissions"]
    role = db.execute("SELECT permissions_json, inspection_permissions FROM roles WHERE name = ?", (role_name,)).fetchone()
    if not role:
        return [], []
    return json.loads(role["permissions_json"] or "[]"), json.loads(role["inspection_permissions"] or "[]")


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
        merged["items"] = json.loads(existing["items_json"] or "[]")
        previous_signatures = json.loads(existing["signatures_json"] or "{}")
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
