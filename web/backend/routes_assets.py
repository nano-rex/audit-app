"""Inspectable items: fixed assets, and fixtures & finishes (parts of the building itself)."""
import secrets
import time

from backend.config import DEFAULT_FIXTURE_CRITERIA, DEFAULT_INSPECTION_CRITERIA, ITEM_KINDS
from backend.database import connect, first_outlet, insert_record
from backend.relational_values import data_value, load_value, save_value


EDITABLE = {
    "name": "name", "category": "category", "businessUnit": "business_unit", "outlet": "outlet", "location": "location",
    "type": "type", "operationalStatus": "operational_status", "installationDate": "installation_date",
    "replacementFlag": "replacement_flag", "description": "description", "model": "model", "serialNumber": "serial_number",
    "brand": "brand", "temporaryRelocation": "temporary_relocation", "warrantyDate": "warranty_date",
    "calibrationDate": "calibration_date", "expiryDate": "expiry_date", "inverterModel": "inverter_model",
    "motorCapacity": "motor_capacity", "sourceFile": "source_file", "sourceSheet": "source_sheet",
}


def item_values(db, payload, existing=None):
    """Columns shared by create and edit. A fixture keeps only what describes a part of the building."""
    kind = payload.get("kind") or (existing["kind"] if existing else "asset")
    if kind not in ITEM_KINDS:
        raise ValueError("Choose Fixed Asset or Fixture & Finish")
    fixture = kind == "fixture"
    name = str(payload.get("name") or payload.get("assetId") or payload.get("code") or "").strip()
    if not name:
        raise ValueError("Enter a name")
    category = str(payload.get("category") or "").strip()
    if category and not db.execute("SELECT 1 FROM categories WHERE name = ?", (category,)).fetchone():
        raise ValueError("Select an existing category")
    item_type = str(payload.get("type") or payload.get("equipmentType") or ("Fixture & Finish" if fixture else "Fixed Asset"))
    status = payload.get("operationalStatus") or payload.get("healthStatus") or "Operational"
    location = payload.get("location") or payload.get("zone") or ""
    installed = payload.get("installationDate") or payload.get("lastChecked") or ""
    text = lambda key: "" if fixture else str(payload.get(key) or "")
    criteria = payload.get("inspectionCriteria") or (DEFAULT_FIXTURE_CRITERIA if fixture else DEFAULT_INSPECTION_CRITERIA)
    return {
        "kind": kind, "category": category, "name": name,
        "business_unit": payload.get("businessUnit", "Ottotree"), "outlet": payload.get("outlet") or first_outlet(db),
        "zone": location or "Unassigned", "location": location,
        "equipment_type": item_type, "type": item_type, "health_status": status, "operational_status": status,
        "last_checked": installed or "Today", "installation_date": "" if fixture else installed,
        "replacement_flag": 1 if payload.get("replacementFlag") else 0,
        "notes": payload.get("description") or payload.get("notes", ""), "description": payload.get("description", ""),
        "model": text("model"), "serial_number": text("serialNumber"), "brand": text("brand"),
        "temporary_relocation": text("temporaryRelocation"), "warranty_date": text("warrantyDate"),
        "calibration_date": text("calibrationDate"), "expiry_date": text("expiryDate"),
        "inverter_model": text("inverterModel"), "motor_capacity": text("motorCapacity"),
        "source_file": payload.get("sourceFile", ""), "source_sheet": payload.get("sourceSheet", ""),
        "photos_data_id": data_value(db, payload.get("photos"), []),
        "inspection_criteria_data_id": save_value(db, criteria),
    }


def item_code(payload, kind, record_id):
    """The code a user typed, or a generated one: every item needs a unique code for its QR label."""
    code = str(payload.get("code") or payload.get("assetId") or "").strip()
    return code or f"{'FXT' if kind == 'fixture' else 'AST'}-{record_id:05d}"


def post_equipment(self, parsed, payload=None):
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        values = item_values(db, payload)
        placeholder = "pending-" + secrets.token_hex(8)
        record_id = insert_record(db, "equipment", values | {
            "asset_id": placeholder, "qr_code": placeholder, "code": placeholder, "created_at": int(time.time() * 1000)})
        code = item_code(payload, values["kind"], record_id)
        # A code that is already in use is refused; it used to replace the other item silently.
        db.execute("UPDATE equipment SET asset_id = ?, qr_code = ?, code = ? WHERE id = ?", (code, code, code, record_id))
    self.json({"ok": True, "id": record_id, "code": code})


def patch_equipment(self, parsed, payload=None):
    item_id = parsed.path.rsplit("/", 1)[-1]
    if not item_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        existing = db.execute("SELECT * FROM equipment WHERE id = ?", (int(item_id),)).fetchone()
        if not existing:
            self.send_error(404)
            return
        # Fields that are not sent keep their stored values.
        payload = {key: existing[column] for key, column in EDITABLE.items()} | {
            "photos": load_value(existing["photos_data_id"]) if existing["photos_data_id"] is not None else [],
            "inspectionCriteria": load_value(existing["inspection_criteria_data_id"]) if existing["inspection_criteria_data_id"] is not None else None,
        } | payload
        values = item_values(db, payload, existing)
        code = str(payload.get("code") or payload.get("assetId") or "").strip() or existing["code"]
        values.update(asset_id=code, qr_code=code, code=code)
        db.execute(f"UPDATE equipment SET {', '.join(f'{column} = ?' for column in values)} WHERE id = ?", (*values.values(), int(item_id)))
    self.json({"ok": True})


def delete_equipment(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute("DELETE FROM equipment WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
