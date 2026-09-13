#!/usr/bin/env python3
import argparse
import json
import re
import sqlite3
import sys
from pathlib import Path

try:
    import openpyxl
except ImportError as exc:
    raise SystemExit("openpyxl is required: python3 -m pip install openpyxl") from exc

ROOT = Path(__file__).resolve().parents[1]
DB_PATH = ROOT / "web" / "data" / "ottotree_audit_web.db"
DEFAULT_CRITERIA = [
    "Present and correctly placed",
    "Clean and free from visible damage",
    "Operational during inspection",
    "Label, cable, or accessory is complete",
]
HEADER_ALIASES = {
    "name": "name",
    "description": "description",
    "asset type": "type",
    "operational status": "operational_status",
    "temporary relocation": "temporary_relocation",
    "code": "code",
    "location": "location",
    "model": "model",
    "serial no.": "serial_number",
    "serial no": "serial_number",
    "serial number": "serial_number",
    "inverter model (if any)": "inverter_model",
    "inverter model": "inverter_model",
    "motor capacity": "motor_capacity",
    "brand": "brand",
    "installation date": "installation_date",
    "warranty date": "warranty_date",
    "calibration date": "calibration_date",
    "expiry date": "expiry_date",
    "photos": "photos",
}
DATE_FIELDS = {"installation_date", "warranty_date", "calibration_date", "expiry_date"}


def clean(value):
    if value is None:
        return ""
    text = str(value).strip()
    return "" if text in {"-", "N/A", "n/a", "None"} else text


def clean_date(value):
    if value is None:
        return ""
    if hasattr(value, "strftime"):
        return value.strftime("%Y-%m-%d")
    return clean(value)


def outlet_from_file(path):
    match = re.match(r"([A-Za-z0-9]+)\s*-", path.name)
    return match.group(1).upper() if match else ""


def find_header(ws):
    for row_index, row in enumerate(ws.iter_rows(min_row=1, max_row=min(ws.max_row, 20), values_only=True), 1):
        labels = [clean(cell).lower() for cell in row]
        if "name" in labels and "code" in labels and "asset type" in labels:
            mapping = {}
            for col_index, label in enumerate(labels):
                field = HEADER_ALIASES.get(label)
                if field:
                    mapping[field] = col_index
            return row_index, mapping
    return None, {}


def iter_assets(path):
    outlet = outlet_from_file(path)
    wb = openpyxl.load_workbook(path, read_only=True, data_only=True)
    for ws in wb.worksheets:
        if ws.title.lower() in {"example", "format"}:
            continue
        header_row, mapping = find_header(ws)
        if not mapping:
            continue
        for row in ws.iter_rows(min_row=header_row + 1, values_only=True):
            code = clean(row[mapping["code"]]) if mapping.get("code") is not None and mapping["code"] < len(row) else ""
            name = clean(row[mapping["name"]]) if mapping.get("name") is not None and mapping["name"] < len(row) else ""
            if not code or not name:
                continue
            item = {"outlet": outlet, "source_file": path.name, "source_sheet": ws.title}
            for field, col_index in mapping.items():
                if col_index >= len(row):
                    item[field] = ""
                elif field in DATE_FIELDS:
                    item[field] = clean_date(row[col_index])
                else:
                    item[field] = clean(row[col_index])
            yield item


def connect(db_path):
    db_path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    return conn


def import_assets(folder, db_path, limit=0):
    files = sorted(Path(folder).glob("*.xlsx"))
    if not files:
        raise SystemExit(f"No .xlsx files found in {folder}")
    count = 0
    with connect(db_path) as db:
        for path in files:
            for item in iter_assets(path):
                code = item["code"]
                asset_type = item.get("type") or "Fixed Asset"
                status = item.get("operational_status") or "Active"
                location = item.get("location") or "Unassigned"
                description = item.get("description") or ""
                db.execute(
                    """
                    INSERT INTO equipment
                    (asset_id, qr_code, business_unit, outlet, zone, equipment_type,
                     health_status, last_checked, replacement_flag, notes, name, description,
                     type, operational_status, code, model, serial_number, brand, location,
                     installation_date, temporary_relocation, warranty_date, calibration_date,
                     expiry_date, photos, inverter_model, motor_capacity, source_file, source_sheet,
                     inspection_criteria, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%s','now') * 1000)
                    ON CONFLICT(code) DO UPDATE SET
                        asset_id = excluded.code,
                        qr_code = excluded.code,
                        outlet = excluded.outlet,
                        zone = excluded.zone,
                        equipment_type = excluded.equipment_type,
                        health_status = excluded.health_status,
                        last_checked = excluded.last_checked,
                        notes = excluded.notes,
                        name = excluded.name,
                        description = excluded.description,
                        type = excluded.type,
                        operational_status = excluded.operational_status,
                        code = excluded.code,
                        model = excluded.model,
                        serial_number = excluded.serial_number,
                        brand = excluded.brand,
                        location = excluded.location,
                        installation_date = excluded.installation_date,
                        temporary_relocation = excluded.temporary_relocation,
                        warranty_date = excluded.warranty_date,
                        calibration_date = excluded.calibration_date,
                        expiry_date = excluded.expiry_date,
                        photos = excluded.photos,
                        inverter_model = excluded.inverter_model,
                        motor_capacity = excluded.motor_capacity,
                        source_file = excluded.source_file,
                        source_sheet = excluded.source_sheet
                    """,
                    (
                        code,
                        code,
                        "Ottotree",
                        item.get("outlet") or "",
                        location,
                        asset_type,
                        status,
                        item.get("installation_date") or "",
                        0,
                        description,
                        item.get("name") or code,
                        description,
                        asset_type,
                        status,
                        code,
                        item.get("model") or "",
                        item.get("serial_number") or "",
                        item.get("brand") or "",
                        location,
                        item.get("installation_date") or "",
                        item.get("temporary_relocation") or "",
                        item.get("warranty_date") or "",
                        item.get("calibration_date") or "",
                        item.get("expiry_date") or "",
                        json.dumps([item.get("photos")]) if item.get("photos") else json.dumps([]),
                        item.get("inverter_model") or "",
                        item.get("motor_capacity") or "",
                        item.get("source_file") or "",
                        item.get("source_sheet") or "",
                        json.dumps(DEFAULT_CRITERIA),
                    ),
                )
                count += 1
                if limit and count >= limit:
                    return count
    return count


def main():
    parser = argparse.ArgumentParser(description="Import Ottotree fixed asset XLSX listings into the audit app SQLite database.")
    parser.add_argument("folder", nargs="?", default="/home/user/codex/Fixed_Assets", help="Folder containing Fixed Asset Listing .xlsx files")
    parser.add_argument("--db", default=str(DB_PATH), help="SQLite database path")
    parser.add_argument("--limit", type=int, default=0, help="Maximum rows to import; 0 imports all rows")
    args = parser.parse_args()
    count = import_assets(args.folder, Path(args.db), args.limit)
    print(f"Imported {count} fixed asset rows")


if __name__ == "__main__":
    main()
