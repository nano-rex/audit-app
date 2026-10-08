"""Inspectable items: fixed assets, and variable assets (stored as kind "fixture"): the building and
everything else without a code label of its own. Both have the same attributes; a variable asset's
code is generated (VAR-00012) and not shown, since it carries no QR label."""
import secrets
import time

from backend import outlet_access
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
    """Columns shared by create and edit, the same for fixed and variable assets."""
    kind = payload.get("kind") or (existing["kind"] if existing else "asset")
    if kind not in ITEM_KINDS:
        raise ValueError("Choose Fixed Asset or Fixture & Finish")
    fixture = kind == "fixture"
    name = str(payload.get("name") or payload.get("assetId") or payload.get("code") or "").strip()
    if not name:
        raise ValueError("Enter a name")
    # Categories were retired: an item is described by its asset type.
    item_type = str(payload.get("type") or payload.get("equipmentType") or ("Others" if fixture else "Fixed Asset")).strip()
    status = payload.get("operationalStatus") or payload.get("healthStatus") or "Operational"
    location = payload.get("location") or payload.get("zone") or ""
    installed = payload.get("installationDate") or payload.get("lastChecked") or ""
    text = lambda key: str(payload.get(key) or "")
    criteria = payload.get("inspectionCriteria") or (DEFAULT_FIXTURE_CRITERIA if fixture else DEFAULT_INSPECTION_CRITERIA)
    return {
        "kind": kind, "category": "", "name": name,
        "business_unit": payload.get("businessUnit", "Ottotree"), "outlet": payload.get("outlet") or first_outlet(db),
        "zone": location or "Unassigned", "location": location,
        "equipment_type": item_type, "type": item_type, "health_status": status, "operational_status": status,
        "last_checked": installed or "Today", "installation_date": installed,
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
    return code or f"{'VAR' if kind == 'fixture' else 'AST'}-{record_id:05d}"


def post_equipment(self, parsed, payload=None):
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        values = item_values(db, payload)
        if serial_clash(db, [values["serial_number"]]):
            raise ValueError(f"Serial number {values['serial_number']} is already used by another item")
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
        # Serial numbers repeated in imported records are left alone until one of them is changed.
        if values["serial_number"] != (existing["serial_number"] or "") and serial_clash(db, [values["serial_number"]], [int(item_id)]):
            raise ValueError(f"Serial number {values['serial_number']} is already used by another item")
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


# ---- Several items at once. ----

# Set on each item by itself in a bulk edit: every item needs its own code (its QR label), and
# a serial number belongs to one item.
OWN_FIELDS = {"code", "assetId", "qrCode", "serialNumber"}
# Where an item is stays with each item in a bulk edit.
PLACE_FIELDS = {"outlet", "location", "zone", "kind"}
BULK_LIMIT = 2000


def serial_clash(db, serials, skip_ids=()):
    """The first serial number among these that another item already has, if any."""
    serials = [serial for serial in serials if serial]
    if len(set(serials)) != len(serials):
        return next(serial for serial in serials if serials.count(serial) > 1)
    skip = list(skip_ids) or [0]
    for serial in serials:
        marks = ",".join("?" for _ in skip)
        if db.execute(f"SELECT 1 FROM equipment WHERE serial_number = ? AND id NOT IN ({marks})", (serial, *skip)).fetchone():
            return serial
    return None


def bulk_targets(db, user, outlets, locations):
    """Each (outlet, location) a bulk add makes an item at. "all" outlets are the ones the person
    covers; "all" locations are every location of each outlet; a named location is used at the
    outlets that have it; no location makes one item per outlet."""
    scope = outlet_access.allowed(user)
    known = [row[0] for row in db.execute("SELECT code FROM outlets ORDER BY code")]
    if outlets == "all":
        codes = [code for code in known if scope is None or code in scope]
    elif isinstance(outlets, list) and outlets and all(isinstance(code, str) for code in outlets):
        codes = list(dict.fromkeys(outlets))
        if any(code not in known for code in codes):
            raise ValueError("Select existing outlets")
        if scope is not None and any(code not in scope for code in codes):
            raise PermissionError(outlet_access.DENIED)
    else:
        raise ValueError("Choose an outlet, or all outlets")
    if locations not in ("all", None) and not (isinstance(locations, list) and all(isinstance(name, str) for name in locations)):
        raise ValueError("Choose a location, or all locations")
    targets = []
    for code in codes:
        names = [row[0] for row in db.execute("SELECT name FROM locations WHERE outlet_code = ? ORDER BY name", (code,))]
        if locations == "all":
            targets += [(code, name) for name in names]
        elif locations:
            targets += [(code, name) for name in dict.fromkeys(locations) if name in names]
        else:
            targets.append((code, ""))
    if not targets:
        raise ValueError("None of the chosen outlets has that location")
    if len(targets) > BULK_LIMIT:
        raise ValueError(f"That would add {len(targets)} items; add at most {BULK_LIMIT} at a time")
    return targets


def post_equipment_bulk(self, parsed, payload=None):
    """The same item at every chosen outlet and location, each with its own generated code."""
    payload = dict(payload or {})
    shared = {key: value for key, value in payload.items() if key not in OWN_FIELDS | {"outlets", "locations"}}
    created = []
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        for outlet, location in bulk_targets(db, self.current_user(), payload.get("outlets"), payload.get("locations")):
            values = item_values(db, shared | {"outlet": outlet, "location": location})
            placeholder = "pending-" + secrets.token_hex(8)
            record_id = insert_record(db, "equipment", values | {
                "asset_id": placeholder, "qr_code": placeholder, "code": placeholder, "created_at": int(time.time() * 1000)})
            code = item_code({}, values["kind"], record_id)
            db.execute("UPDATE equipment SET asset_id = ?, qr_code = ?, code = ? WHERE id = ?", (code, code, code, record_id))
            created.append({"id": record_id, "code": code, "outlet": outlet, "location": location})
    self.json({"ok": True, "count": len(created), "items": created})


def patch_equipment_bulk(self, parsed, payload=None):
    """One change to several items: the shared fields sent apply to all of them, while each keeps
    its own outlet and location, and its own code and serial number (sent per item)."""
    payload = payload or {}
    ids = payload.get("ids")
    if not isinstance(ids, list) or not ids or not all(str(value).isdigit() for value in ids):
        raise ValueError("Choose the items to edit")
    ids = list(dict.fromkeys(int(value) for value in ids))
    fields = {key: value for key, value in (payload.get("fields") or {}).items() if key not in OWN_FIELDS | PLACE_FIELDS}
    own = {}
    for item in payload.get("items") or []:
        if isinstance(item, dict) and str(item.get("id", "")).isdigit():
            own[int(item["id"])] = {key: str(item[key]).strip() for key in ("code", "serialNumber") if key in item and item[key] is not None}
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        marks = ",".join("?" for _ in ids)
        rows = {row["id"]: row for row in db.execute(f"SELECT * FROM equipment WHERE id IN ({marks})", ids)}
        if len(rows) != len(ids):
            raise ValueError("Some of these items no longer exist")
        updates = {}
        for item_id in ids:
            existing = rows[item_id]
            stored = {key: existing[column] for key, column in EDITABLE.items()} | {
                "photos": load_value(existing["photos_data_id"]) if existing["photos_data_id"] is not None else [],
                "inspectionCriteria": load_value(existing["inspection_criteria_data_id"]) if existing["inspection_criteria_data_id"] is not None else None,
            }
            values = item_values(db, stored | fields | own.get(item_id, {}), existing)
            values["code"] = own.get(item_id, {}).get("code") or existing["code"]
            updates[item_id] = values
        codes = [values["code"] for values in updates.values()]
        repeated = next((code for code in codes if codes.count(code) > 1), None)
        if repeated:
            raise ValueError(f"Code {repeated} is given to more than one item; each item needs its own code")
        taken = db.execute(f"SELECT code FROM equipment WHERE code IN ({','.join('?' for _ in codes)}) AND id NOT IN ({marks})",
                           (*codes, *ids)).fetchone()
        if taken:
            raise ValueError(f"Code {taken[0]} is already used by another item")
        changed = [values["serial_number"] for item_id, values in updates.items() if values["serial_number"] != (rows[item_id]["serial_number"] or "")]
        kept = [values["serial_number"] for item_id, values in updates.items() if values["serial_number"] == (rows[item_id]["serial_number"] or "")]
        clash = serial_clash(db, changed, ids) or next((serial for serial in changed if serial and serial in kept), None)
        if clash:
            raise ValueError(f"Serial number {clash} is already used by another item")
        # Codes may be swapped between the items, so each first gives its code up.
        db.executemany("UPDATE equipment SET code = ?, asset_id = ?, qr_code = ? WHERE id = ?",
                       [(f"pending-{secrets.token_hex(8)}",) * 3 + (item_id,) for item_id in ids])
        for item_id, values in updates.items():
            values.update(asset_id=values["code"], qr_code=values["code"])
            db.execute(f"UPDATE equipment SET {', '.join(f'{column} = ?' for column in values)} WHERE id = ?", (*values.values(), item_id))
    self.json({"ok": True, "count": len(ids)})
