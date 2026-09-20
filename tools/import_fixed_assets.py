#!/usr/bin/env python3
import argparse
import importlib.util
import re
import sqlite3
import time
from pathlib import Path

try:
    import openpyxl
except ImportError as exc:
    raise SystemExit("openpyxl is required: python3 -m pip install openpyxl") from exc

ROOT = Path(__file__).resolve().parents[1]
DB_PATH = ROOT / "web" / "data" / "ottotree_audit_web.db"
DEFAULT_ASSET_FOLDER = ROOT / "Fixed_Assets"
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


def ensure_schema(db_path):
    server_path = ROOT / "web" / "server.py"
    spec = importlib.util.spec_from_file_location("audit_server", server_path)
    server = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(server)
    server.configure_data_directory(Path(db_path).parent)
    from backend import config
    config.DB_PATH = Path(db_path)
    server.init_db()


def connect(db_path):
    db_path.parent.mkdir(parents=True, exist_ok=True)
    from backend.database import DatabaseConnection
    conn = sqlite3.connect(db_path, factory=DatabaseConnection)
    conn.row_factory = sqlite3.Row
    return conn


def location_qr_code(outlet, name):
    outlet_code = re.sub(r"[^A-Z0-9]+", "-", (outlet or "OUTLET").upper()).strip("-") or "OUTLET"
    location_code = re.sub(r"[^A-Z0-9]+", "-", (name or "LOCATION").upper()).strip("-") or "LOCATION"
    return f"LOC-{outlet_code}-{location_code}"


def sync_default_zone(db, outlet, now):
    locations = [row["name"] for row in db.execute(
        "SELECT name FROM locations WHERE outlet_code = ? ORDER BY name",
        (outlet,),
    ).fetchall()]
    if not locations:
        return
    existing = db.execute(
        "SELECT id FROM zones WHERE outlet_code = ? AND lower(name) = 'zone-1'",
        (outlet,),
    ).fetchone()
    from backend.relational_values import save_value
    locations_json = save_value(db, locations)
    if existing:
        db.execute("UPDATE zones SET locations_data_id = ? WHERE id = ?", (locations_json, existing["id"]))
    else:
        db.execute(
            """
            INSERT INTO zones (outlet_code, name, locations_data_id, description, created_at)
            VALUES (?, 'Zone-1', ?, 'Default zone containing all locations', ?)
            """,
            (outlet, locations_json, now),
        )


def ensure_location(db, outlet, location, now):
    outlet = clean(outlet)
    location = clean(location) or "Unassigned"
    if not outlet:
        return False
    existing = db.execute(
        "SELECT id FROM locations WHERE outlet_code = ? AND name = ?",
        (outlet, location),
    ).fetchone()
    if existing:
        return False
    next_order = db.execute(
        "SELECT COALESCE(MAX(display_order), 0) + 1 FROM locations WHERE outlet_code = ?",
        (outlet,),
    ).fetchone()[0]
    db.execute(
        """
        INSERT INTO locations (outlet_code, name, floor, area, display_order, size, qr_code, created_at)
        VALUES (?, ?, '', '', ?, '', ?, ?)
        """,
        (outlet, location, next_order, location_qr_code(outlet, location), now),
    )
    sync_default_zone(db, outlet, now)
    return True


def fixed_asset_files(source):
    source = Path(source)
    if source.is_file():
        if source.suffix.lower() != ".xlsx":
            raise SystemExit(f"Expected an .xlsx file, got {source}")
        return [source]
    if source.is_dir():
        return sorted(source.glob("*.xlsx"))
    raise SystemExit(f"Fixed asset source does not exist: {source}")


def import_photos(media, workbook, value):
    if not value:
        return []
    photo = (workbook.parent / value).resolve()
    if not photo.is_file():
        raise ValueError(f"Photo file must exist beside the workbook: {value}")
    identifier, mime = media.put(photo.read_bytes())
    return [{"id": identifier, "url": "/api/media/" + identifier, "name": photo.name, "type": mime}]


def import_assets(folder, db_path, limit=0):
    files = fixed_asset_files(folder)
    if not files:
        raise SystemExit(f"No .xlsx files found in {folder}")
    ensure_schema(db_path)
    from backend.relational_values import save_value
    from backend.media_store import MediaStore
    media = MediaStore(db_path)
    count = 0
    created = 0
    updated = 0
    locations_created = 0
    with connect(db_path) as db:
        for path in files:
            for item in iter_assets(path):
                code = item["code"]
                exists = db.execute("SELECT 1 FROM equipment WHERE code = ?", (code,)).fetchone() is not None
                asset_type = item.get("type") or "Fixed Asset"
                status = item.get("operational_status") or "Active"
                location = item.get("location") or "Unassigned"
                description = item.get("description") or ""
                now_ms = int(time.time() * 1000)
                if ensure_location(db, item.get("outlet") or "", location, now_ms):
                    locations_created += 1
                db.execute(
                    """
                    INSERT INTO equipment
                    (asset_id, qr_code, business_unit, outlet, zone, equipment_type,
                     health_status, last_checked, replacement_flag, notes, name, description,
                     type, operational_status, code, model, serial_number, brand, location,
                     installation_date, temporary_relocation, warranty_date, calibration_date,
                     expiry_date, photos_data_id, inverter_model, motor_capacity, source_file, source_sheet,
                     inspection_criteria_data_id, created_at)
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
                        photos_data_id = excluded.photos_data_id,
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
                        save_value(db, import_photos(media, path, item.get("photos"))),
                        item.get("inverter_model") or "",
                        item.get("motor_capacity") or "",
                        item.get("source_file") or "",
                        item.get("source_sheet") or "",
                        save_value(db, DEFAULT_CRITERIA),
                    ),
                )
                count += 1
                if exists:
                    updated += 1
                else:
                    created += 1
                if limit and count >= limit:
                    return {"total": count, "created": created, "updated": updated, "locations_created": locations_created}
    return {"total": count, "created": created, "updated": updated, "locations_created": locations_created}


def main():
    parser = argparse.ArgumentParser(description="Import Ottotree fixed asset XLSX listings into the audit app SQLite database.")
    parser.add_argument("source", nargs="?", default=str(DEFAULT_ASSET_FOLDER), help="Fixed Asset Listing .xlsx file or folder; defaults to ./Fixed_Assets beside the repo")
    parser.add_argument("--db", default=str(DB_PATH), help="SQLite database path")
    parser.add_argument("--limit", type=int, default=0, help="Maximum rows to import; 0 imports all rows")
    args = parser.parse_args()
    result = import_assets(args.source, Path(args.db), args.limit)
    print(
        f"Synced {result['total']} fixed asset rows: "
        f"{result['created']} created, {result['updated']} updated, "
        f"{result['locations_created']} locations created"
    )


if __name__ == "__main__":
    main()
