"""Routes assets for the audit application."""
from backend.relational_values import save_value
import time
from backend.relational_values import data_value
from backend.config import DEFAULT_INSPECTION_CRITERIA
from backend.database import connect, first_outlet


def post_equipment(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_outlet = first_outlet(db)
        db.execute(
            """
            INSERT OR REPLACE INTO equipment
            (asset_id, qr_code, business_unit, outlet, zone, equipment_type,
             health_status, last_checked, replacement_flag, notes, name, description,
             type, operational_status, code, model, serial_number, brand, location,
             installation_date, temporary_relocation, warranty_date, calibration_date,
             expiry_date, photos_data_id, inverter_model, motor_capacity, source_file, source_sheet,
             inspection_criteria_data_id, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                payload.get("code") or payload.get("assetId") or "EQ-NEW",
                payload.get("code") or payload.get("assetId") or "EQ-NEW",
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or default_outlet,
                payload.get("location") or payload.get("zone") or "Unassigned",
                payload.get("type") or payload.get("equipmentType", "Fixed Asset"),
                payload.get("operationalStatus") or payload.get("healthStatus", "Operational"),
                payload.get("installationDate") or payload.get("lastChecked", "Today"),
                1 if payload.get("replacementFlag") else 0,
                payload.get("description") or payload.get("notes", ""),
                payload.get("name") or payload.get("assetId") or payload.get("code") or "Fixed Asset",
                payload.get("description", ""),
                payload.get("type") or payload.get("equipmentType", "Fixed Asset"),
                payload.get("operationalStatus") or payload.get("healthStatus", "Operational"),
                payload.get("code") or payload.get("assetId") or "EQ-NEW",
                payload.get("model", ""),
                payload.get("serialNumber", ""),
                payload.get("brand", ""),
                payload.get("location") or payload.get("zone") or "",
                payload.get("installationDate") or payload.get("lastChecked", ""),
                payload.get("temporaryRelocation", ""),
                payload.get("warrantyDate", ""),
                payload.get("calibrationDate", ""),
                payload.get("expiryDate", ""),
                data_value(db, payload.get("photos"), []),
                payload.get("inverterModel", ""),
                payload.get("motorCapacity", ""),
                payload.get("sourceFile", ""),
                payload.get("sourceSheet", ""),
                save_value(db, payload.get("inspectionCriteria") or DEFAULT_INSPECTION_CRITERIA),
                now,
            ),
        )
    self.json({"ok": True})


def patch_equipment(self, parsed, payload=None):
    item_id = parsed.path.rsplit("/", 1)[-1]
    if not item_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute(
            """
            UPDATE equipment
            SET asset_id = ?, qr_code = ?, business_unit = ?, outlet = ?, zone = ?,
                equipment_type = ?, health_status = ?, last_checked = ?, replacement_flag = ?,
                notes = ?, name = ?, description = ?, type = ?, operational_status = ?,
                code = ?, model = ?, serial_number = ?, brand = ?, location = ?,
                installation_date = ?, temporary_relocation = ?, warranty_date = ?,
                calibration_date = ?, expiry_date = ?, photos_data_id = ?, inverter_model = ?,
                motor_capacity = ?, source_file = ?, source_sheet = ?, inspection_criteria_data_id = ?
            WHERE id = ?
            """,
            (
                payload.get("code") or payload.get("assetId") or "EQ-NEW",
                payload.get("code") or payload.get("assetId") or "EQ-NEW",
                payload.get("businessUnit", "Ottotree"),
                payload.get("outlet") or first_outlet(db),
                payload.get("location") or payload.get("zone") or "Unassigned",
                payload.get("type") or payload.get("equipmentType", "Fixed Asset"),
                payload.get("operationalStatus") or payload.get("healthStatus", "Operational"),
                payload.get("installationDate") or payload.get("lastChecked", "Today"),
                1 if payload.get("replacementFlag") else 0,
                payload.get("description") or payload.get("notes", ""),
                payload.get("name") or payload.get("assetId") or payload.get("code") or "Fixed Asset",
                payload.get("description", ""),
                payload.get("type") or payload.get("equipmentType", "Fixed Asset"),
                payload.get("operationalStatus") or payload.get("healthStatus", "Operational"),
                payload.get("code") or payload.get("assetId") or "EQ-NEW",
                payload.get("model", ""),
                payload.get("serialNumber", ""),
                payload.get("brand", ""),
                payload.get("location") or payload.get("zone") or "",
                payload.get("installationDate") or payload.get("lastChecked", ""),
                payload.get("temporaryRelocation", ""),
                payload.get("warrantyDate", ""),
                payload.get("calibrationDate", ""),
                payload.get("expiryDate", ""),
                data_value(db, payload.get("photos"), []),
                payload.get("inverterModel", ""),
                payload.get("motorCapacity", ""),
                payload.get("sourceFile", ""),
                payload.get("sourceSheet", ""),
                save_value(db, payload.get("inspectionCriteria") or DEFAULT_INSPECTION_CRITERIA),
                int(item_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


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
    return
