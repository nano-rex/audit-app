#!/usr/bin/env python3
import json
import mimetypes
import re
import sqlite3
import time
from io import StringIO
import csv
import html
import hashlib
import os
import secrets
from datetime import datetime
from http import cookies
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

ROOT = Path(__file__).resolve().parent
DATA_DIR = ROOT / "data"
DB_PATH = DATA_DIR / "ottotree_audit_web.db"
LOUDSPEAKER_OUTLETS = ("STP", "SBA", "TPG", "AQP", "CCS", "SPK", "BSP", "MYT", "DJM", "KPG", "TSU", "TMA", "PGA", "PSC", "PWS")
DEFAULT_CATEGORIES = (
    "AV Equipment",
    "COM Equipment",
    "Facility",
    "F&B Equipment",
    "Electrical",
    "Plumbing",
    "Air Conditioning",
    "Lighting",
    "Furniture",
    "Building",
    "Safety",
    "Cleanliness",
    "IT / Network",
    "KTV Equipment",
    "Others",
)
DEFAULT_INSPECTION_CRITERIA = [
    "Present and correctly placed",
    "Clean and free from visible damage",
    "Operational during inspection",
    "Label, cable, or accessory is complete",
]
DEFAULT_PRIORITY_LEVELS = [
    {"name": "Priority", "classification": "Priority", "dueDays": 3},
    {"name": "Non-Priority", "classification": "Non-Priority", "dueDays": 14},
    {"name": "High", "classification": "Priority", "dueDays": 3},
    {"name": "Medium", "classification": "Non-Priority", "dueDays": 14},
    {"name": "Low", "classification": "Non-Priority", "dueDays": 14},
]
DEFAULT_AUDIT_TYPES = [
    {"name": "Standard", "description": "Full outlet inspection", "active": True},
    {"name": "Quick", "description": "Short follow-up inspection", "active": True},
]
DEFAULT_SCORING_SETTINGS = {
    "passMark": 70,
    "weighting": "Equal",
    "excellentBand": 90,
    "goodBand": 70,
    "belowBand": 60,
}
DEFAULT_REPORT_SETTINGS = {
    "companyName": "Ottotree",
    "departmentHeader": "Facilities Department",
    "logoText": "OTTOTREE",
    "appTitle": "Ottotree Audit",
    "appSubtitle": "Loudspeaker & Mini Studio operations",
    "businessUnitLabel": "Ottotree",
    "todayHeading": "inspections for today",
    "reportHeading": "monthly audit report",
    "loginTitle": "Ottotree Audit",
}
DEFAULT_SYSTEM_SETTINGS = {
    "emailEnabled": False,
    "whatsappEnabled": False,
    "pushEnabled": False,
    "cmmsEnabled": False,
    "preventiveMaintenanceEnabled": False,
    "aiPhotoDetectionEnabled": False,
    "aiSummaryEnabled": False,
    "aiRecommendationEnabled": False,
}
APP_TABS = (
    ("today", "To-do"),
    ("inspections", "Inspections"),
    ("findings", "Findings"),
    ("work-orders", "Work Orders"),
    ("equipment", "Equipment"),
    ("reports", "Reports"),
    ("categories", "Categories"),
    ("departments", "Departments"),
    ("outlets", "Outlets"),
    ("users", "Users"),
    ("roles", "Roles"),
    ("corrective-actions", "Corrective Actions"),
    ("notifications", "Notifications"),
    ("settings", "Settings"),
)
INCLUDE_PATTERN = re.compile(r"<!--\s*include:\s*([a-zA-Z0-9_./-]+)\s*-->")
SESSION_TOKENS = {}
DEFAULT_PASSWORD = "password123"
SUPER_ROLE = "Super"
ADMIN_ROLE = "Admin"


def today_date():
    return datetime.now().strftime("%Y-%m-%d")


def normalize_audit_date(value):
    return today_date() if not value or value == "Today" else value


def parse_date(value):
    try:
        return datetime.strptime(normalize_audit_date(value), "%Y-%m-%d")
    except (TypeError, ValueError):
        return datetime.now()


def record_year(value=None):
    return normalize_audit_date(value).split("-", 1)[0]


def audit_ref(record_id, audit_date=None):
    return f"AUD-{record_year(audit_date)}-{int(record_id):04d}"


def finding_ref(record_id, audit_date=None):
    return f"F-{record_year(audit_date)}-{int(record_id):05d}"


def work_order_ref(record_id, audit_date=None):
    return f"WO-{record_year(audit_date)}-{int(record_id):05d}"


def location_qr_code(outlet, name):
    outlet_code = re.sub(r"[^A-Z0-9]+", "-", (outlet or "OUTLET").upper()).strip("-") or "OUTLET"
    location_code = re.sub(r"[^A-Z0-9]+", "-", (name or "LOCATION").upper()).strip("-") or "LOCATION"
    return f"LOC-{outlet_code}-{location_code}"


def hash_password(password, salt=None):
    salt = salt or secrets.token_hex(16)
    digest = hashlib.sha256(f"{salt}:{password}".encode("utf-8")).hexdigest()
    return f"{salt}${digest}"


def verify_password(password, stored_hash):
    if not stored_hash or "$" not in stored_hash:
        return False
    salt, digest = stored_hash.split("$", 1)
    return secrets.compare_digest(hash_password(password, salt).split("$", 1)[1], digest)


def public_user(row):
    if not row:
        return None
    permissions = []
    if row["role"] == SUPER_ROLE:
        permissions = [tab[0] for tab in APP_TABS]
    else:
        with connect() as db:
            role = db.execute("SELECT permissions_json FROM roles WHERE name = ?", (row["role"],)).fetchone()
            if role:
                permissions = json.loads(role["permissions_json"] or "[]")
    return {
        "id": row["id"],
        "name": row["name"],
        "role": row["role"],
        "email": row["email"],
        "department": row["department"] or "",
        "title": row["title"] or "",
        "responsibilities": row["responsibilities"] or "",
        "active": bool(row["active"]),
        "lastLoginAt": row["last_login_at"] or "",
        "resetRequired": bool(row["reset_required"]),
        "permissions": permissions,
    }


def is_super_user(user):
    return bool(user and user.get("role") == SUPER_ROLE)


def is_company_admin_user(user):
    return bool(user and user.get("role") in (SUPER_ROLE, ADMIN_ROLE))


def workflow_dates(payload):
    status = payload.get("status", "Assigned")
    verified_at = payload.get("verifiedAt", "")
    closed_at = payload.get("closedAt", "")
    if status in ("Verified", "Closed") and not verified_at:
        verified_at = today_date()
    if status == "Closed" and not closed_at:
        closed_at = today_date()
    if status not in ("Verified", "Closed"):
        closed_at = ""
    return status, verified_at, closed_at


def read_setting(db, key, fallback=None):
    row = db.execute("SELECT value FROM app_settings WHERE key = ?", (key,)).fetchone()
    if not row:
        return fallback
    try:
        return json.loads(row["value"])
    except json.JSONDecodeError:
        return row["value"]


def branding_settings():
    with connect() as db:
        return {
            "appTitle": read_setting(db, "report.appTitle", DEFAULT_REPORT_SETTINGS["appTitle"]),
            "appSubtitle": read_setting(db, "report.appSubtitle", DEFAULT_REPORT_SETTINGS["appSubtitle"]),
            "companyName": read_setting(db, "report.companyName", DEFAULT_REPORT_SETTINGS["companyName"]),
            "departmentHeader": read_setting(db, "report.departmentHeader", DEFAULT_REPORT_SETTINGS["departmentHeader"]),
            "logoText": read_setting(db, "report.logoText", DEFAULT_REPORT_SETTINGS["logoText"]),
            "logoUrl": read_setting(db, "report.logoUrl", ""),
            "businessUnitLabel": read_setting(db, "report.businessUnitLabel", DEFAULT_REPORT_SETTINGS["businessUnitLabel"]),
            "todayHeading": read_setting(db, "report.todayHeading", DEFAULT_REPORT_SETTINGS["todayHeading"]),
            "reportHeading": read_setting(db, "report.reportHeading", DEFAULT_REPORT_SETTINGS["reportHeading"]),
            "loginTitle": read_setting(db, "report.loginTitle", DEFAULT_REPORT_SETTINGS["loginTitle"]),
        }


def calculate_due_date(created_at, due_days):
    base = datetime.fromtimestamp((created_at or int(time.time() * 1000)) / 1000)
    return datetime.fromtimestamp(base.timestamp() + int(due_days) * 86400).strftime("%Y-%m-%d")


def priority_due_date(db, priority, created_at):
    row = db.execute("SELECT due_days FROM priority_levels WHERE name = ? AND active = 1", (priority,)).fetchone()
    days = int(row["due_days"]) if row else (3 if priority in ("High", "Priority") else 14)
    return calculate_due_date(created_at, days)


def sla_status(status, due_date):
    if status in ("Completed", "Verified", "Closed"):
        return "Completed"
    if not due_date:
        return "No due date"
    days = (parse_date(due_date) - parse_date(today_date())).days
    if days < 0:
        return "Overdue"
    if days <= 3:
        return "Due Soon"
    return "On Track"


def create_notification(db, title, message, channel="In-App", related_type=None, related_id=None):
    db.execute(
        """
        INSERT INTO notifications (title, message, channel, status, related_type, related_id, created_at)
        VALUES (?, ?, ?, 'Unread', ?, ?, ?)
        """,
        (title, message, channel, related_type, related_id, int(time.time() * 1000)),
    )


def image_label(image):
    if isinstance(image, dict):
        return image.get("markedName") or image.get("name") or "Image"
    return str(image)


def image_labels(images):
    return [image_label(image) for image in (images or [])]


def parse_image_list(value):
    if not value:
        return []
    if isinstance(value, list):
        return value
    try:
        images = json.loads(value)
        return images if isinstance(images, list) else [images]
    except (TypeError, json.JSONDecodeError):
        return [value]


def json_text(value, fallback=None):
    if value is None:
        return json.dumps(fallback if fallback is not None else [])
    if isinstance(value, str):
        return value
    return json.dumps(value)


def connect():
    DATA_DIR.mkdir(exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    return conn


def ensure_column(db, table, column, definition):
    columns = [row["name"] for row in db.execute(f"PRAGMA table_info({table})").fetchall()]
    if column not in columns:
        db.execute(f"ALTER TABLE {table} ADD COLUMN {column} {definition}")


def init_db():
    with connect() as db:
        db.executescript(
            """
            CREATE TABLE IF NOT EXISTS audits (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                audit_ref TEXT,
                business_unit TEXT NOT NULL,
                outlet TEXT NOT NULL,
                branch TEXT NOT NULL,
                audit_date TEXT NOT NULL,
                auditor TEXT NOT NULL,
                audit_type TEXT NOT NULL,
                score INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS schedules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                business_unit TEXT NOT NULL,
                outlet TEXT NOT NULL,
                zone TEXT,
                scheduled_date TEXT NOT NULL,
                auditor TEXT NOT NULL,
                remarks TEXT,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS captain_logins (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                outlet TEXT NOT NULL,
                captain_name TEXT NOT NULL,
                logged_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS inspection_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                audit_id INTEGER NOT NULL,
                finding_id INTEGER,
                section TEXT NOT NULL,
                item TEXT NOT NULL,
                score INTEGER NOT NULL,
                notes TEXT,
                evidence_status TEXT NOT NULL,
                FOREIGN KEY(audit_id) REFERENCES audits(id)
            );

            CREATE TABLE IF NOT EXISTS inspection_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                inspection_name TEXT,
                business_unit TEXT NOT NULL,
                outlet TEXT NOT NULL,
                zone TEXT NOT NULL,
                audit_date TEXT NOT NULL,
                auditor TEXT NOT NULL,
                items_json TEXT NOT NULL,
                progress INTEGER NOT NULL,
                status TEXT NOT NULL,
                audit_id INTEGER,
                signatures_json TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS work_orders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                work_order_ref TEXT,
                business_unit TEXT NOT NULL,
                outlet TEXT NOT NULL,
                zone TEXT NOT NULL,
                request_type TEXT NOT NULL,
                category TEXT,
                priority TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                assignee TEXT NOT NULL,
                pic TEXT,
                status TEXT NOT NULL,
                action_taken TEXT,
                completion_date TEXT,
                completion_remark TEXT,
                completion_photo TEXT,
                verified_by TEXT,
                verified_at TEXT,
                verification_remark TEXT,
                closed_at TEXT,
                outlet_confirmed INTEGER NOT NULL DEFAULT 0,
                source_audit_id INTEGER,
                source_item_id INTEGER,
                source_finding_id INTEGER,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS findings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                finding_ref TEXT,
                audit_id INTEGER NOT NULL,
                audit_ref TEXT,
                business_unit TEXT NOT NULL,
                outlet TEXT NOT NULL,
                location TEXT NOT NULL,
                category TEXT,
                priority TEXT NOT NULL,
                assigned_department TEXT,
                pic TEXT,
                comment TEXT,
                status TEXT NOT NULL,
                corrective_action TEXT,
                completion_date TEXT,
                completion_photo TEXT,
                completion_remark TEXT,
                verified_by TEXT,
                verified_at TEXT,
                verification_remark TEXT,
                closed_at TEXT,
                source_item_id INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY(audit_id) REFERENCES audits(id)
            );

            CREATE TABLE IF NOT EXISTS equipment (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                asset_id TEXT NOT NULL UNIQUE,
                qr_code TEXT NOT NULL,
                business_unit TEXT NOT NULL,
                outlet TEXT NOT NULL,
                zone TEXT NOT NULL,
                equipment_type TEXT NOT NULL,
                health_status TEXT NOT NULL,
                last_checked TEXT NOT NULL,
                replacement_flag INTEGER NOT NULL DEFAULT 0,
                notes TEXT,
                name TEXT,
                description TEXT,
                type TEXT,
                operational_status TEXT,
                code TEXT,
                model TEXT,
                serial_number TEXT,
                brand TEXT,
                location TEXT,
                installation_date TEXT,
                inspection_criteria TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS admin_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                record_type TEXT NOT NULL,
                name TEXT NOT NULL,
                parent TEXT,
                detail TEXT,
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS departments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                code TEXT NOT NULL UNIQUE,
                description TEXT,
                responsibilities TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS categories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                sequence INTEGER NOT NULL DEFAULT 0,
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS outlets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                code TEXT NOT NULL UNIQUE,
                location TEXT,
                description TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS locations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                outlet_code TEXT NOT NULL,
                name TEXT NOT NULL,
                floor TEXT,
                area TEXT,
                display_order INTEGER NOT NULL DEFAULT 0,
                size TEXT,
                qr_code TEXT,
                created_at INTEGER NOT NULL,
                UNIQUE(outlet_code, name)
            );

            CREATE TABLE IF NOT EXISTS zones (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                outlet_code TEXT NOT NULL,
                name TEXT NOT NULL,
                locations_json TEXT NOT NULL,
                description TEXT,
                created_at INTEGER NOT NULL,
                UNIQUE(outlet_code, name)
            );

            CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                role TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE,
                department TEXT,
                password_hash TEXT,
                active INTEGER NOT NULL DEFAULT 1,
                reset_required INTEGER NOT NULL DEFAULT 0,
                last_login_at TEXT,
                login_count INTEGER NOT NULL DEFAULT 0,
                title TEXT,
                responsibilities TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS roles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                permissions_json TEXT NOT NULL,
                protected INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS priority_levels (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                classification TEXT NOT NULL,
                due_days INTEGER NOT NULL,
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS audit_types (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS comments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                record_type TEXT NOT NULL,
                record_id INTEGER NOT NULL,
                comment TEXT NOT NULL,
                author TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS notifications (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                message TEXT NOT NULL,
                channel TEXT NOT NULL,
                status TEXT NOT NULL,
                related_type TEXT,
                related_id INTEGER,
                created_at INTEGER NOT NULL,
                read_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS user_login_activity (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                email TEXT NOT NULL,
                logged_at TEXT NOT NULL,
                remember_me INTEGER NOT NULL DEFAULT 0,
                user_agent TEXT,
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            """
        )
        ensure_column(db, "audits", "audit_ref", "TEXT")
        ensure_column(db, "inspection_items", "finding_id", "INTEGER")
        ensure_column(db, "work_orders", "work_order_ref", "TEXT")
        ensure_column(db, "work_orders", "source_finding_id", "INTEGER")
        ensure_column(db, "work_orders", "category", "TEXT")
        ensure_column(db, "work_orders", "pic", "TEXT")
        ensure_column(db, "work_orders", "action_taken", "TEXT")
        ensure_column(db, "work_orders", "completion_date", "TEXT")
        ensure_column(db, "work_orders", "completion_remark", "TEXT")
        ensure_column(db, "work_orders", "completion_photo", "TEXT")
        ensure_column(db, "work_orders", "verified_by", "TEXT")
        ensure_column(db, "work_orders", "verified_at", "TEXT")
        ensure_column(db, "work_orders", "verification_remark", "TEXT")
        ensure_column(db, "work_orders", "closed_at", "TEXT")
        ensure_column(db, "findings", "completion_photo", "TEXT")
        ensure_column(db, "findings", "verified_by", "TEXT")
        ensure_column(db, "findings", "verified_at", "TEXT")
        ensure_column(db, "findings", "verification_remark", "TEXT")
        ensure_column(db, "findings", "closed_at", "TEXT")
        ensure_column(db, "inspection_sessions", "inspection_name", "TEXT")
        ensure_column(db, "inspection_sessions", "signatures_json", "TEXT")
        ensure_column(db, "users", "department", "TEXT")
        ensure_column(db, "users", "password_hash", "TEXT")
        ensure_column(db, "users", "active", "INTEGER NOT NULL DEFAULT 1")
        ensure_column(db, "users", "reset_required", "INTEGER NOT NULL DEFAULT 0")
        ensure_column(db, "users", "last_login_at", "TEXT")
        ensure_column(db, "users", "login_count", "INTEGER NOT NULL DEFAULT 0")
        ensure_column(db, "roles", "description", "TEXT")
        ensure_column(db, "roles", "permissions_json", "TEXT")
        ensure_column(db, "roles", "protected", "INTEGER NOT NULL DEFAULT 0")
        ensure_column(db, "locations", "floor", "TEXT")
        ensure_column(db, "locations", "area", "TEXT")
        ensure_column(db, "locations", "display_order", "INTEGER NOT NULL DEFAULT 0")
        ensure_column(db, "locations", "qr_code", "TEXT")
        ensure_column(db, "work_orders", "due_date", "TEXT")
        ensure_column(db, "work_orders", "vendor", "TEXT")
        ensure_column(db, "work_orders", "sla_status", "TEXT")
        ensure_column(db, "work_orders", "cost", "REAL NOT NULL DEFAULT 0")
        ensure_column(db, "findings", "cause", "TEXT")
        ensure_column(db, "findings", "recommendation", "TEXT")
        ensure_column(db, "findings", "required_action", "TEXT")
        ensure_column(db, "schedules", "zone", "TEXT")
        ensure_column(db, "equipment", "name", "TEXT")
        ensure_column(db, "equipment", "description", "TEXT")
        ensure_column(db, "equipment", "type", "TEXT")
        ensure_column(db, "equipment", "operational_status", "TEXT")
        ensure_column(db, "equipment", "code", "TEXT")
        ensure_column(db, "equipment", "model", "TEXT")
        ensure_column(db, "equipment", "serial_number", "TEXT")
        ensure_column(db, "equipment", "brand", "TEXT")
        ensure_column(db, "equipment", "location", "TEXT")
        ensure_column(db, "equipment", "installation_date", "TEXT")
        ensure_column(db, "equipment", "inspection_criteria", "TEXT")
        db.execute(
            "UPDATE inspection_sessions SET audit_date = ? WHERE audit_date IS NULL OR audit_date = '' OR audit_date = 'Today'",
            (today_date(),),
        )
        for row in db.execute("SELECT id, audit_date FROM audits WHERE audit_ref IS NULL OR audit_ref = ''").fetchall():
            db.execute("UPDATE audits SET audit_ref = ? WHERE id = ?", (audit_ref(row["id"], row["audit_date"]), row["id"]))
        for row in db.execute("SELECT id, created_at FROM work_orders WHERE work_order_ref IS NULL OR work_order_ref = ''").fetchall():
            year = datetime.fromtimestamp((row["created_at"] or int(time.time() * 1000)) / 1000).strftime("%Y")
            db.execute("UPDATE work_orders SET work_order_ref = ? WHERE id = ?", (f"WO-{year}-{int(row['id']):05d}", row["id"]))
        for row in db.execute("SELECT id, created_at FROM findings WHERE finding_ref IS NULL OR finding_ref = ''").fetchall():
            year = datetime.fromtimestamp((row["created_at"] or int(time.time() * 1000)) / 1000).strftime("%Y")
            db.execute("UPDATE findings SET finding_ref = ? WHERE id = ?", (f"F-{year}-{int(row['id']):05d}", row["id"]))
        db.execute("UPDATE inspection_sessions SET inspection_name = outlet || '_' || audit_date || '_' || id WHERE inspection_name IS NULL OR inspection_name = ''")
        db.execute("UPDATE inspection_sessions SET inspection_name = outlet || '_' || audit_date || '_' || id WHERE inspection_name LIKE '%_Today_%'")
        for row in db.execute("SELECT id, items_json FROM inspection_sessions").fetchall():
            db.execute(
                "UPDATE inspection_sessions SET progress = ? WHERE id = ?",
                (inspection_progress(json.loads(row["items_json"] or "[]")), row["id"]),
            )
        db.execute(
            "UPDATE equipment SET inspection_criteria = ? WHERE inspection_criteria IS NULL OR inspection_criteria = ''",
            (json.dumps(DEFAULT_INSPECTION_CRITERIA),),
        )
        for row in db.execute("SELECT id, outlet_code, name FROM locations WHERE qr_code IS NULL OR qr_code = ''").fetchall():
            db.execute("UPDATE locations SET qr_code = ? WHERE id = ?", (location_qr_code(row["outlet_code"], row["name"]), row["id"]))
        db.execute(
            """
            UPDATE schedules
            SET zone = COALESCE(
                (SELECT name FROM locations WHERE locations.outlet_code = schedules.outlet ORDER BY name LIMIT 1),
                'Unassigned'
            )
            WHERE zone IS NULL OR zone = ''
            """
        )
        db.execute("UPDATE equipment SET name = asset_id WHERE name IS NULL OR name = ''")
        db.execute("UPDATE equipment SET description = notes WHERE description IS NULL")
        db.execute("UPDATE equipment SET type = equipment_type WHERE type IS NULL OR type = ''")
        db.execute("UPDATE equipment SET operational_status = health_status WHERE operational_status IS NULL OR operational_status = ''")
        db.execute("UPDATE equipment SET code = asset_id WHERE code IS NULL OR code = ''")
        db.execute("UPDATE equipment SET location = zone WHERE location IS NULL OR location = ''")
        db.execute("UPDATE equipment SET installation_date = last_checked WHERE installation_date IS NULL OR installation_date = ''")
        db.execute("DELETE FROM audits WHERE auditor = 'Sample Auditor'")
        schedules = db.execute("SELECT COUNT(*) FROM schedules").fetchone()[0]
        if schedules == 0:
            seed_schedules(db)
        equipment = db.execute("SELECT COUNT(*) FROM equipment").fetchone()[0]
        if equipment == 0:
            seed_equipment(db)
        admin = db.execute("SELECT COUNT(*) FROM admin_records").fetchone()[0]
        if admin == 0:
            seed_admin(db)
        normalize_loudspeaker_outlets(db)
        db.execute("UPDATE admin_records SET record_type = 'Audit Area' WHERE record_type = 'Business Unit'")
        db.execute("DELETE FROM admin_records WHERE record_type = 'Audit Area'")
        seed_setup_records(db)
        seed_categories(db)
        seed_locations(db)
        seed_zones(db)
        seed_roles(db)
        seed_priority_levels(db)
        seed_audit_types(db)
        seed_settings(db)
        seed_users(db)
        default_department = first_department(db)
        if default_department:
            db.execute("UPDATE users SET department = ? WHERE department IS NULL OR department = ''", (default_department,))
        db.execute(
            "UPDATE users SET password_hash = ? WHERE password_hash IS NULL OR password_hash = ''",
            (hash_password(DEFAULT_PASSWORD),),
        )


def seed_schedules(db):
    now = int(time.time() * 1000)
    rows = [
        ("Mini Studio", "MST", "Server Room", "2026-09-01", "Ah Fai", "Room checklist before peak hours"),
        ("Mini Studio", "MQS", "Entrance", "2026-09-02", "Ah Fan", "Photo evidence follow-up"),
        ("Loudspeaker", "STP", "Display Zone", "2026-09-01", "Gavin", "Speaker display readiness"),
    ]
    for index, row in enumerate(rows):
        db.execute(
            """
            INSERT INTO schedules
            (business_unit, outlet, zone, scheduled_date, auditor, remarks, status, created_at)
            VALUES (?, ?, ?, ?, ?, ?, 'Pending', ?)
            """,
            (*row, now - index * 100000),
        )


def seed_equipment(db):
    now = int(time.time() * 1000)
    rows = [
        ("EQ-MST-UPS-001", "QR-MST-UPS-001", "Mini Studio", "MST", "Server Room", "UPS", "Monitor", "2026-08-20", 1, "Battery age needs budget review"),
        ("EQ-MQS-CCTV-002", "QR-MQS-CCTV-002", "Mini Studio", "MQS", "Entrance", "CCTV", "Normal", "2026-08-18", 0, "Camera view clear"),
        ("EQ-STP-SPK-001", "QR-STP-SPK-001", "Loudspeaker", "STP", "Display Zone", "Speaker Display", "Normal", "2026-08-22", 0, "Demo unit working"),
        ("EQ-MDP-SRV-001", "QR-MDP-SRV-001", "Mini Studio", "MDP", "Server Room", "Server", "Replace", "2026-08-15", 1, "Old hard disk health warning"),
    ]
    for row in rows:
        db.execute(
            """
            INSERT OR IGNORE INTO equipment
            (asset_id, qr_code, business_unit, outlet, zone, equipment_type,
             health_status, last_checked, replacement_flag, notes, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (*row, now),
        )


def seed_admin(db):
    now = int(time.time() * 1000)
    rows = [
        ("Outlet", "MAM", "LONG", "Active outlet"),
        ("Outlet", "MQS", "LONG", "Active outlet"),
        ("Outlet", "MDP", "LONG", "Active outlet"),
        ("Outlet", "MST", "LONG", "Active outlet"),
        ("Zone", "Server Room", "All outlets", "Servers, UPS, network hardware"),
        ("Zone", "Entrance", "All outlets", "Front entrance and display area"),
        ("Zone", "Display Zone", "All outlets", "Speaker/headphone display fixtures"),
        ("Department", "SSD", "Work Orders", "SSD department"),
        ("Department", "FMS", "Work Orders", "FMS department"),
        ("Department", "AVC", "Work Orders", "AVC department"),
        ("Department", "CLD", "Work Orders", "CLD department"),
        ("Department", "MD", "Work Orders", "MD department"),
        ("Captain PIN", "MST -> 1113", "Captain Login", "Outlet captain access"),
        ("Captain PIN", "MAM -> 1213", "Captain Login", "Outlet captain access"),
        ("Captain PIN", "MQS -> 1220", "Captain Login", "Outlet captain access"),
        ("Captain PIN", "MDP -> 0201", "Captain Login", "Outlet captain access"),
    ]
    for row in rows:
        db.execute(
            """
            INSERT INTO admin_records (record_type, name, parent, detail, active, created_at)
            VALUES (?, ?, ?, ?, 1, ?)
            """,
            (*row, now),
        )


def normalize_loudspeaker_outlets(db):
    legacy_map = {"MAM": "STP", "MQS": "SBA", "MDP": "TPG", "MST": "AQP"}
    for old, new in legacy_map.items():
        for table in ("audits", "schedules", "work_orders", "equipment"):
            db.execute(
                f"UPDATE {table} SET outlet = ? WHERE business_unit = 'Loudspeaker' AND outlet = ?",
                (new, old),
            )


def seed_setup_records(db):
    now = int(time.time() * 1000)
    if db.execute("SELECT COUNT(*) FROM departments").fetchone()[0] == 0:
        department_defaults = {
            "SSD": ("SSD department", "Store support and display issue ownership"),
            "FMS": ("FMS department", "Facilities, fixtures, maintenance, and site readiness"),
            "AVC": ("AVC department", "Audio visual, cabling, playback, and demo equipment"),
            "CLD": ("CLD department", "Cloud, system access, and connected workflow support"),
            "MD": ("MD department", "Management decisions, approvals, and escalation ownership"),
        }
        for code, (description, responsibilities) in department_defaults.items():
            db.execute(
                """
                INSERT INTO departments (code, description, responsibilities, created_at)
                VALUES (?, ?, ?, ?)
                """,
                (code, description, responsibilities, now),
            )
    if db.execute("SELECT COUNT(*) FROM outlets").fetchone()[0] == 0:
        for code in ("MAM", "MQS", "MDP", "MST", *LOUDSPEAKER_OUTLETS):
            db.execute(
                """
                INSERT INTO outlets (code, location, description, created_at)
                VALUES (?, '', 'Ottotree outlet', ?)
                """,
                (code, now),
            )


def seed_categories(db):
    now = int(time.time() * 1000)
    if db.execute("SELECT COUNT(*) FROM categories").fetchone()[0] != 0:
        return
    for index, name in enumerate(DEFAULT_CATEGORIES, 1):
        db.execute(
            """
            INSERT INTO categories (name, description, sequence, active, created_at)
            VALUES (?, '', ?, 1, ?)
            """,
            (name, index, now),
        )


def seed_locations(db):
    now = int(time.time() * 1000)
    outlets = [row["code"] for row in db.execute("SELECT code FROM outlets ORDER BY code").fetchall()]
    for outlet in outlets:
        for name, size in (("R-01", "Room 01"), ("R-02", "Room 02"), ("R-03", "Room 03")):
            db.execute(
                """
                INSERT OR IGNORE INTO locations (outlet_code, name, size, created_at)
                VALUES (?, ?, ?, ?)
                """,
                (outlet, name, size, now),
            )


def seed_zones(db):
    now = int(time.time() * 1000)
    outlets = [row["code"] for row in db.execute("SELECT code FROM outlets ORDER BY code").fetchall()]
    for outlet in outlets:
        add_locations_to_default_zone(db, outlet, now)


def add_locations_to_default_zone(db, outlet, now=None):
    now = now or int(time.time() * 1000)
    locations = [row["name"] for row in db.execute(
        "SELECT name FROM locations WHERE outlet_code = ? ORDER BY name",
        (outlet,),
    ).fetchall()]
    if not locations:
        return
    existing = db.execute(
        "SELECT id, locations_json FROM zones WHERE outlet_code = ? AND lower(name) = 'zone-1'",
        (outlet,),
    ).fetchone()
    if existing:
        db.execute(
            "UPDATE zones SET locations_json = ? WHERE id = ?",
            (json.dumps(locations), existing["id"]),
        )
    else:
        db.execute(
            """
            INSERT OR IGNORE INTO zones (outlet_code, name, locations_json, description, created_at)
            VALUES (?, 'Zone-1', ?, 'Default inspection zone', ?)
            """,
            (outlet, json.dumps(locations), now),
        )


def seed_users(db):
    now = int(time.time() * 1000)
    db.execute("UPDATE OR IGNORE users SET email = 'super@sudo' WHERE lower(email) = 'super@audit-app.local'")
    db.execute("DELETE FROM users WHERE lower(email) = 'super@audit-app.local'")
    rows = [
        ("Super User", SUPER_ROLE, "super@sudo", "SSD", "Super", "Full app control", "doas"),
        ("Ottotree System Administrator", ADMIN_ROLE, "admin@ottotree.local", "SSD", "System Administrator", "Ottotree system administrator staff", DEFAULT_PASSWORD),
    ]
    for row in rows:
        db.execute(
            """
            INSERT INTO users (name, role, email, department, password_hash, active, reset_required, title, responsibilities, created_at)
            VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?, ?)
            ON CONFLICT(email) DO UPDATE SET
                name = excluded.name,
                role = excluded.role,
                department = excluded.department,
                password_hash = excluded.password_hash,
                active = 1,
                reset_required = 0,
                title = excluded.title,
                responsibilities = excluded.responsibilities
            """,
            (row[0], row[1], row[2], row[3], hash_password(row[6]), row[4], row[5], now),
        )


def seed_roles(db):
    now = int(time.time() * 1000)
    full_permissions = json.dumps([tab[0] for tab in APP_TABS])
    admin_permissions = [tab[0] for tab in APP_TABS if tab[0] not in ("roles", "settings")]
    role_rows = [
        (ADMIN_ROLE, "Company administrator access", admin_permissions),
        ("Auditor", "Field inspection access", ["today", "inspections", "equipment", "reports"]),
        ("Department/PIC", "Corrective action ownership", ["today", "findings", "work-orders", "corrective-actions", "notifications", "reports"]),
        ("Management", "Management reporting access", ["reports", "findings", "notifications"]),
    ]
    if not db.execute("SELECT id FROM roles WHERE name = ?", (SUPER_ROLE,)).fetchone():
        db.execute("UPDATE roles SET name = ?, description = 'Built-in full access role' WHERE name = 'Admin'", (SUPER_ROLE,))
    db.execute(
        """
        INSERT OR IGNORE INTO roles (name, description, permissions_json, protected, created_at)
        VALUES (?, 'Built-in full access role', ?, 1, ?)
        """,
        (SUPER_ROLE, full_permissions, now),
    )
    db.execute(
        "UPDATE roles SET permissions_json = ?, protected = 1 WHERE name = ?",
        (full_permissions, SUPER_ROLE),
    )
    for name, description, permissions in role_rows:
        db.execute(
            """
            INSERT OR IGNORE INTO roles (name, description, permissions_json, protected, created_at)
            VALUES (?, ?, ?, 0, ?)
            """,
            (name, description, json.dumps(permissions), now),
        )


def seed_priority_levels(db):
    now = int(time.time() * 1000)
    for row in DEFAULT_PRIORITY_LEVELS:
        db.execute(
            """
            INSERT OR IGNORE INTO priority_levels (name, classification, due_days, active, created_at)
            VALUES (?, ?, ?, 1, ?)
            """,
            (row["name"], row["classification"], row["dueDays"], now),
        )


def seed_audit_types(db):
    now = int(time.time() * 1000)
    for row in DEFAULT_AUDIT_TYPES:
        db.execute(
            """
            INSERT OR IGNORE INTO audit_types (name, description, active, created_at)
            VALUES (?, ?, ?, ?)
            """,
            (row["name"], row["description"], 1 if row["active"] else 0, now),
        )


def seed_settings(db):
    settings = {
        **{f"scoring.{key}": value for key, value in DEFAULT_SCORING_SETTINGS.items()},
        **{f"report.{key}": value for key, value in DEFAULT_REPORT_SETTINGS.items()},
        **{f"system.{key}": value for key, value in DEFAULT_SYSTEM_SETTINGS.items()},
    }
    for key, value in settings.items():
        db.execute(
            "INSERT OR IGNORE INTO app_settings (key, value) VALUES (?, ?)",
            (key, json.dumps(value)),
        )
    db.execute(
        "UPDATE app_settings SET value = ? WHERE key = 'report.companyName' AND value = ?",
        (json.dumps(DEFAULT_REPORT_SETTINGS["companyName"]), json.dumps("Audit App")),
    )
    db.execute(
        "UPDATE app_settings SET value = ? WHERE key = 'report.logoText' AND value = ?",
        (json.dumps(DEFAULT_REPORT_SETTINGS["logoText"]), json.dumps("AUDIT")),
    )
    for key, previous in {
        "appTitle": "Audit App",
        "appSubtitle": "Facilities audit workspace",
        "businessUnitLabel": "Facilities",
        "loginTitle": "Audit App",
    }.items():
        db.execute(
            "UPDATE app_settings SET value = ? WHERE key = ? AND value = ?",
            (json.dumps(DEFAULT_REPORT_SETTINGS[key]), f"report.{key}", json.dumps(previous)),
        )


def rating(score):
    if score >= 90:
        return "Excellent"
    if score >= 70:
        return "Good"
    if score >= 60:
        return "Below Expectation"
    return "Critical"


def checklist(unit):
    loudspeaker = [
        {"section": "Loudspeaker Display", "item": "Main speaker display is present, clean, and powered"},
        {"section": "Loudspeaker Display", "item": "Price tags and product cards are accurate"},
        {"section": "Loudspeaker Demo", "item": "Demo audio source and cables are working"},
        {"section": "Loudspeaker Safety", "item": "Power socket, cable routing, and fixture are safe"},
    ]
    mini_studio = [
        {"section": "Main Entrance", "item": "Big headphone display is present and in good condition"},
        {"section": "Studio Area", "item": "Demo headphones are clean, working, and correctly placed"},
        {"section": "Counter", "item": "F&B counter cabinet and cashier drawer area are clean"},
        {"section": "Safety", "item": "Emergency exit and walkway are clear and usable"},
    ]
    if unit == "Loudspeaker":
        return loudspeaker
    if unit == "Mini Studio":
        return mini_studio
    return mini_studio + loudspeaker


def scope(unit, table_name):
    if unit in ("Mini Studio", "Loudspeaker"):
        return f"{table_name}.business_unit = ?", (unit,)
    return "1 = 1", ()


def dashboard(unit):
    audit_where, audit_params = scope(unit, "audits")
    audit_where = f"{audit_where} AND audits.id IN (SELECT audit_id FROM inspection_sessions WHERE status = 'Completed' AND audit_id IS NOT NULL)"
    schedule_where, schedule_params = scope(unit, "schedules")
    work_order_where, work_order_params = scope(unit, "work_orders")
    equipment_where, equipment_params = scope(unit, "equipment")
    with connect() as db:
        stats = db.execute(
            f"""
            SELECT COUNT(*) total,
                   COALESCE(ROUND(AVG(score)), 0) average,
                   SUM(CASE WHEN score >= 90 THEN 1 ELSE 0 END) excellent,
                   SUM(CASE WHEN score < 60 THEN 1 ELSE 0 END) below60
            FROM audits WHERE {audit_where}
            """,
            audit_params,
        ).fetchone()
        outlets = db.execute(
            f"""
            SELECT outlet, branch,
                   COALESCE(ROUND(AVG(score)), 0) average,
                   COUNT(*) audit_count,
                   (SELECT score FROM audits latest
                    WHERE latest.business_unit = audits.business_unit
                      AND latest.outlet = audits.outlet
                    ORDER BY created_at DESC, id DESC LIMIT 1) latest,
                   (SELECT audit_date FROM audits latest
                    WHERE latest.business_unit = audits.business_unit
                      AND latest.outlet = audits.outlet
                    ORDER BY created_at DESC, id DESC LIMIT 1) audit_date
            FROM audits
            WHERE {audit_where}
            GROUP BY outlet
            ORDER BY outlet
            """,
            audit_params,
        ).fetchall()
        recent = db.execute(
            f"""
            SELECT outlet, branch, audit_date, score
            FROM audits
            WHERE {audit_where}
            ORDER BY created_at DESC, id DESC
            LIMIT 5
            """,
            audit_params,
        ).fetchall()
        kpi = db.execute(
            f"""
            SELECT COUNT(*) assigned,
                   SUM(CASE WHEN status = 'Completed' THEN 1 ELSE 0 END) completed,
                   SUM(CASE WHEN status = 'Pending' THEN 1 ELSE 0 END) pending
            FROM schedules
            WHERE {schedule_where}
            """,
            schedule_params,
        ).fetchone()
        schedules = db.execute(
            f"""
            SELECT id, outlet, zone, scheduled_date, auditor, remarks, status, created_at
            FROM schedules
            WHERE {schedule_where} AND status != 'Completed'
            ORDER BY scheduled_date ASC, created_at DESC, id DESC
            LIMIT 5
            """,
            schedule_params,
        ).fetchall()
        work_orders = db.execute(
            f"""
            SELECT id, work_order_ref, outlet, zone, request_type, category, priority, title,
                   description, assignee, pic, status, action_taken, completion_date,
                   completion_remark, completion_photo, verified_by, verified_at,
                   verification_remark, closed_at, due_date, vendor, sla_status, cost,
                   outlet_confirmed, created_at
            FROM work_orders
            WHERE {work_order_where}
            ORDER BY
                CASE priority WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END,
                created_at DESC
            LIMIT 8
            """,
            work_order_params,
        ).fetchall()
        equipment_rows = db.execute(
            f"""
            SELECT id, asset_id, qr_code, outlet, zone, equipment_type, health_status,
                   last_checked, replacement_flag, notes, name, description, type,
                   operational_status, code, model, serial_number, brand, location,
                   installation_date, inspection_criteria
            FROM equipment
            WHERE {equipment_where}
            ORDER BY
                CASE health_status WHEN 'Replace' THEN 1 WHEN 'Monitor' THEN 2 ELSE 3 END,
                last_checked DESC
            LIMIT 12
            """,
            equipment_params,
        ).fetchall()
        all_work_orders = [dict(row) for row in db.execute(
            f"""
            SELECT id, outlet, zone, request_type, category, priority, status, due_date, created_at
            FROM work_orders
            WHERE {work_order_where}
            """,
            work_order_params,
        ).fetchall()]
        findings = [dict(row) for row in db.execute(
            """
            SELECT outlet, location, category, priority, assigned_department, pic, status, created_at
            FROM findings
            ORDER BY created_at DESC
            """
        ).fetchall()]
        monthly_trend = [dict(row) for row in db.execute(
            f"""
            SELECT substr(audit_date, 1, 7) month, COUNT(*) audits, COALESCE(ROUND(AVG(score)), 0) average_score
            FROM audits
            WHERE {audit_where}
            GROUP BY substr(audit_date, 1, 7)
            ORDER BY month
            """,
            audit_params,
        ).fetchall()]

    outlet_rows = [dict(row) for row in outlets]
    recent_rows = [dict(row) | {"rating": rating(row["score"])} for row in recent]
    assigned = kpi["assigned"] or 0
    completed = kpi["completed"] or 0
    response_rate = round((completed * 100 / assigned) if assigned else 0)
    completed_audits = int(stats["total"] or 0)
    pending_audits = sum(1 for row in inspection_sessions()["items"] if row["status"] != "Completed")
    closed_statuses = {"Completed", "Verified", "Closed"}
    open_work_orders = [row for row in all_work_orders if row["status"] not in closed_statuses]
    completed_work_orders = [row for row in all_work_orders if row["status"] in closed_statuses]
    priority_findings = [row for row in findings if row["priority"] in ("High", "Priority")]
    non_priority_findings = [row for row in findings if row["priority"] not in ("High", "Priority")]
    overdue_orders = [row for row in all_work_orders if sla_status(row["status"], row.get("due_date")) == "Overdue"]
    due_soon_orders = [row for row in all_work_orders if sla_status(row["status"], row.get("due_date")) == "Due Soon"]
    def grouped(rows, key):
        counts = {}
        for row in rows:
            label = row.get(key) or "Unassigned"
            counts[label] = counts.get(label, 0) + 1
        return [{"label": label, "count": count} for label, count in sorted(counts.items())]
    return {
        "stats": {
            "total": stats["total"] or 0,
            "average": stats["average"] or 0,
            "excellent": stats["excellent"] or 0,
            "below60": stats["below60"] or 0,
            "auditsCompleted": completed_audits,
            "auditsPending": pending_audits,
            "priorityIssues": len(priority_findings),
            "nonPriorityIssues": len(non_priority_findings),
            "outstandingIssues": len(open_work_orders),
            "completedCorrectiveActions": len(completed_work_orders),
            "overdueFindings": len(overdue_orders),
            "completionRate": round((len(completed_work_orders) * 100 / len(all_work_orders)) if all_work_orders else 0),
        },
        "outlets": outlet_rows,
        "recent": recent_rows,
        "rankings": sorted(outlet_rows, key=lambda item: item["latest"], reverse=True),
        "kpi": {
            "assigned": assigned,
            "completed": completed,
            "pending": kpi["pending"] or 0,
            "responseRate": response_rate,
        },
        "today": {
            "scheduled": [dict(row) for row in schedules],
            "pendingUploads": 0,
            "followUps": len(work_orders),
            "dueSoon": len(due_soon_orders),
            "overdue": len(overdue_orders),
        },
        "workOrders": [dict(row) for row in work_orders],
        "equipment": [dict(row) for row in equipment_rows],
        "charts": {
            "priorityVsNonPriority": [
                {"label": "Priority", "count": len(priority_findings)},
                {"label": "Non-Priority", "count": len(non_priority_findings)},
            ],
            "findingsByDepartment": grouped(findings, "assigned_department"),
            "findingsByArea": grouped(findings, "location"),
            "findingsByCategory": grouped(findings, "category"),
            "findingsByPriority": grouped(findings, "priority"),
            "monthlyAuditTrend": monthly_trend,
            "findingsTrend": grouped(
                [{"month": datetime.fromtimestamp((row.get("created_at") or 0) / 1000).strftime("%Y-%m")} for row in findings],
                "month",
            ),
            "departmentPerformance": grouped(all_work_orders, "request_type"),
            "locationPerformance": grouped(all_work_orders, "zone"),
            "categoryPerformance": grouped(all_work_orders, "category"),
        },
    }


def report(unit):
    data = dashboard(unit)
    critical_orders = [
        item for item in data["workOrders"]
        if item["priority"] == "High"
    ]
    return {
        "unit": unit,
        "monthlySummary": {
            "audits": data["stats"]["total"],
            "auditsCompleted": data["stats"]["auditsCompleted"],
            "auditsPending": data["stats"]["auditsPending"],
            "averageScore": data["stats"]["average"],
            "openWorkOrders": len(data["workOrders"]),
            "totalFindings": data["stats"]["priorityIssues"] + data["stats"]["nonPriorityIssues"],
            "priorityFindings": data["stats"]["priorityIssues"],
            "nonPriorityFindings": data["stats"]["nonPriorityIssues"],
            "completedCorrectiveActions": data["stats"]["completedCorrectiveActions"],
            "outstandingFindings": data["stats"]["outstandingIssues"],
            "overdueFindings": data["stats"]["overdueFindings"],
            "completionRate": data["stats"]["completionRate"],
        },
        "rankings": data["rankings"],
        "criticalIssues": critical_orders,
        "kpi": data["kpi"],
        "recent": data["recent"],
        "charts": data["charts"],
    }


def report_csv(unit):
    data = report(unit)
    brand = branding_settings()
    out = StringIO()
    writer = csv.writer(out)
    writer.writerow([f"{brand['appTitle']} Report", unit])
    writer.writerow([])
    writer.writerow(["Audits", "Completed", "Pending", "Average Score", "Open Work Orders", "Total Findings", "Priority", "Non-Priority", "Completion Rate"])
    summary = data["monthlySummary"]
    writer.writerow([summary["audits"], summary["auditsCompleted"], summary["auditsPending"], summary["averageScore"], summary["openWorkOrders"], summary["totalFindings"], summary["priorityFindings"], summary["nonPriorityFindings"], str(summary["completionRate"]) + "%"])
    writer.writerow([])
    writer.writerow(["KPI"])
    writer.writerow(["Assigned Tasks", "Completed", "Pending", "Response Rate"])
    kpi = data["kpi"]
    writer.writerow([kpi["assigned"], kpi["completed"], kpi["pending"], str(kpi["responseRate"]) + "%"])
    writer.writerow([])
    writer.writerow(["Outlet Rankings"])
    writer.writerow(["Outlet", "Average", "Latest", "Audit Count", "Last Audit Date"])
    for row in data["rankings"]:
        writer.writerow([row["outlet"], row["average"], row["latest"], row["audit_count"], row["audit_date"]])
    writer.writerow([])
    writer.writerow(["Critical Issues"])
    writer.writerow(["ID", "Reference", "Outlet", "Zone", "Category", "Priority", "Title", "Assignee", "Status"])
    for row in data["criticalIssues"]:
        writer.writerow([row["id"], row.get("work_order_ref", ""), row["outlet"], row["zone"], row.get("category", ""), row["priority"], row["title"], row["assignee"], row["status"]])
    writer.writerow([])
    writer.writerow(["Detailed Findings"])
    writer.writerow(["Reference", "Audit", "Outlet", "Location", "Category", "Priority", "Department", "PIC", "Status", "Comment"])
    for row in finding_items()["items"]:
        writer.writerow([row.get("finding_ref", ""), row.get("audit_ref", ""), row.get("outlet", ""), row.get("location", ""), row.get("category", ""), row.get("priority", ""), row.get("assigned_department", ""), row.get("pic", ""), row.get("status", ""), row.get("comment", "")])
    return out.getvalue().encode("utf-8")


def admin_records():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, record_type, name, parent, detail, active
            FROM admin_records
            ORDER BY record_type, name
            """
        ).fetchall()
    grouped = {}
    for row in rows:
        grouped.setdefault(row["record_type"], []).append(dict(row))
    return grouped


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


def notifications():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, title, message, channel, status, related_type, related_id, created_at, read_at
            FROM notifications
            ORDER BY created_at DESC, id DESC
            LIMIT 100
            """
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def comments(record_type="", record_id=0):
    where = ""
    params = ()
    if record_type and record_id:
        where = "WHERE record_type = ? AND record_id = ?"
        params = (record_type, int(record_id))
    with connect() as db:
        rows = db.execute(
            f"""
            SELECT id, record_type, record_id, comment, author, created_at
            FROM comments
            {where}
            ORDER BY created_at DESC, id DESC
            """,
            params,
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def report_xls(unit):
    data = report(unit)
    brand = branding_settings()
    rows = [
        "<table>",
        f"<tr><th colspan='2'>{html.escape(brand['appTitle'])} Report</th></tr>",
    ]
    for key, value in data["monthlySummary"].items():
        rows.append(f"<tr><td>{key}</td><td>{value}</td></tr>")
    rows.append("</table>")
    return "\n".join(rows).encode("utf-8")


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
                   installation_date, inspection_criteria
            FROM equipment
            {where}
            ORDER BY COALESCE(name, asset_id), id
            """,
            params,
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def work_order_items():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, work_order_ref, business_unit, outlet, zone, request_type, category, priority, title,
                   description, assignee, pic, status, action_taken, completion_date,
                   completion_remark, completion_photo, verified_by, verified_at,
                   verification_remark, closed_at, due_date, vendor, sla_status, cost,
                   outlet_confirmed, source_finding_id
            FROM work_orders
            ORDER BY
                CASE priority WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END,
                created_at DESC, id DESC
            """
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def sync_finding_from_work_order(db, work_order_id):
    row = db.execute(
        """
        SELECT source_finding_id, status, pic, action_taken, completion_date,
               completion_photo, completion_remark, verified_by, verified_at,
               verification_remark, closed_at
        FROM work_orders
        WHERE id = ?
        """,
        (work_order_id,),
    ).fetchone()
    if not row or not row["source_finding_id"]:
        return
    db.execute(
        """
        UPDATE findings
        SET status = ?, pic = ?, corrective_action = ?, completion_date = ?,
            completion_photo = ?, completion_remark = ?, verified_by = ?,
            verified_at = ?, verification_remark = ?, closed_at = ?, updated_at = ?
        WHERE id = ?
        """,
        (
            row["status"],
            row["pic"] or "",
            row["action_taken"] or "",
            row["completion_date"] or "",
            row["completion_photo"] or json.dumps([]),
            row["completion_remark"] or "",
            row["verified_by"] or "",
            row["verified_at"] or "",
            row["verification_remark"] or "",
            row["closed_at"] or "",
            int(time.time() * 1000),
            row["source_finding_id"],
        ),
    )


def finding_items():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, finding_ref, audit_id, audit_ref, business_unit, outlet, location,
                   category, priority, assigned_department, pic, comment, status,
                   corrective_action, completion_date, completion_photo, completion_remark,
                   verified_by, verified_at, verification_remark, closed_at, source_item_id,
                   created_at, updated_at
            FROM findings
            ORDER BY created_at DESC, id DESC
            """
        ).fetchall()
    return {"items": [dict(row) for row in rows]}


def first_department(db):
    row = db.execute("SELECT code FROM departments ORDER BY code LIMIT 1").fetchone()
    return row["code"] if row else ""


def first_category(db):
    row = db.execute("SELECT name FROM categories WHERE active = 1 ORDER BY sequence, name LIMIT 1").fetchone()
    return row["name"] if row else "Others"


def first_outlet(db):
    row = db.execute("SELECT code FROM outlets ORDER BY code LIMIT 1").fetchone()
    return row["code"] if row else ""


def inspection_progress(items):
    if not items:
        return 0
    complete = 0
    for item in items:
        passed = bool(item.get("passed"))
        not_applicable = bool(item.get("notApplicable"))
        notes = (item.get("notes") or "").strip()
        if passed or not_applicable or notes:
            complete += 1
    return round(complete * 100 / len(items))


def finalize_inspection(db, session_id, payload, now):
    items = payload.get("items") or []
    score_values = [100 if item.get("passed") else 0 for item in items if not item.get("notApplicable")]
    total_score = round(sum(score_values) / len(score_values)) if score_values else 0
    cursor = db.execute(
        """
        INSERT INTO audits
        (business_unit, outlet, branch, audit_date, auditor, audit_type, score, created_at)
        VALUES (?, ?, ?, ?, ?, 'Inspection', ?, ?)
        """,
        (
            payload.get("businessUnit", "Ottotree"),
            payload.get("outlet") or first_outlet(db),
            payload.get("zone", "Unassigned"),
            normalize_audit_date(payload.get("auditDate")),
            payload.get("auditor", "Unnamed Auditor"),
            total_score,
            now,
        ),
    )
    audit_id = cursor.lastrowid
    audit_reference = audit_ref(audit_id, normalize_audit_date(payload.get("auditDate")))
    db.execute("UPDATE audits SET audit_ref = ? WHERE id = ?", (audit_reference, audit_id))
    default_department = first_department(db)
    default_category = first_category(db)
    for item in items:
        cursor = db.execute(
            """
            INSERT INTO inspection_items
            (audit_id, section, item, score, notes, evidence_status)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            (
                audit_id,
                item.get("section", "Equipment"),
                item.get("item", "Checklist item"),
                100 if item.get("passed") or item.get("notApplicable") else 0,
                item.get("notes", ""),
                ", ".join(image_labels(item.get("images"))),
            ),
        )
        inspection_item_id = cursor.lastrowid
        if not item.get("passed") and not item.get("notApplicable"):
            finding_cursor = db.execute(
                """
                INSERT INTO findings
                (audit_id, audit_ref, business_unit, outlet, location, category, priority,
                 assigned_department, pic, comment, status, source_item_id, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, 'High', ?, '', ?, 'Assigned', ?, ?, ?)
                """,
                (
                    audit_id,
                    audit_reference,
                    payload.get("businessUnit", "Ottotree"),
                    payload.get("outlet") or first_outlet(db),
                    item.get("location") or payload.get("zone", "Unassigned"),
                    item.get("category") or default_category,
                    default_department,
                    item.get("notes", "") or item.get("item", "Inspection finding"),
                    inspection_item_id,
                    now,
                    now,
                ),
            )
            finding_id = finding_cursor.lastrowid
            finding_reference = finding_ref(finding_id, normalize_audit_date(payload.get("auditDate")))
            db.execute("UPDATE findings SET finding_ref = ? WHERE id = ?", (finding_reference, finding_id))
            db.execute("UPDATE inspection_items SET finding_id = ? WHERE id = ?", (finding_id, inspection_item_id))
            if item.get("workOrderRequested"):
                continue
            work_order_cursor = db.execute(
                """
                INSERT INTO work_orders
                (business_unit, outlet, zone, request_type, category, priority, title, description,
                 assignee, pic, status, action_taken, completion_date, completion_remark, completion_photo,
                 verified_by, verified_at, verification_remark, closed_at, outlet_confirmed,
                 source_audit_id, source_item_id, source_finding_id, created_at)
                VALUES (?, ?, ?, ?, ?, 'High', ?, ?, 'Technical Support', '', 'Assigned', '', '', '', ?, '', '', '', '', 0, ?, ?, ?, ?)
                """,
                (
                    payload.get("businessUnit", "Ottotree"),
                    payload.get("outlet") or first_outlet(db),
                    item.get("location") or payload.get("zone", "Unassigned"),
                    default_department,
                    item.get("category") or default_category,
                    f"{finding_reference} - {item.get('section', 'Equipment')} - {item.get('item', 'Checklist item')}",
                    item.get("notes", "") or "Created from incomplete inspection criterion",
                    json.dumps([]),
                    audit_id,
                    inspection_item_id,
                    finding_id,
                    now,
                ),
            )
            db.execute("UPDATE work_orders SET work_order_ref = ? WHERE id = ?", (work_order_ref(work_order_cursor.lastrowid, normalize_audit_date(payload.get("auditDate"))), work_order_cursor.lastrowid))
    db.execute("UPDATE inspection_sessions SET status = 'Completed', progress = 100, audit_id = ?, updated_at = ? WHERE id = ?", (audit_id, now, session_id))
    return audit_id


def inspection_sessions():
    with connect() as db:
        rows = db.execute(
            """
            SELECT id, inspection_name, business_unit, outlet, zone, audit_date, auditor, progress, status, audit_id, items_json, created_at, updated_at
            FROM inspection_sessions
            ORDER BY updated_at DESC, id DESC
            """
        ).fetchall()
    items = []
    for row in rows:
        item = dict(row)
        session_items = json.loads(item.pop("items_json") or "[]")
        item["inspection_name"] = normalized_inspection_name(item)
        item["locations"] = sorted({entry.get("location", "") for entry in session_items if entry.get("location")})
        item["categories"] = sorted({entry.get("category", "") for entry in session_items if entry.get("category")})
        item["priorities"] = sorted({entry.get("priority", "") for entry in session_items if entry.get("priority")})
        item["departments"] = sorted({entry.get("assignedDepartment", "") or entry.get("department", "") for entry in session_items if entry.get("assignedDepartment") or entry.get("department")})
        item["pics"] = sorted({entry.get("pic", "") for entry in session_items if entry.get("pic")})
        item["findings_count"] = sum(1 for entry in session_items if not entry.get("passed") and not entry.get("notApplicable"))
        items.append(item)
    return {"items": items}


def inspection_session(session_id):
    with connect() as db:
        row = db.execute("SELECT * FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()
        audit = None
        findings = []
        if row and row["audit_id"]:
            audit = db.execute("SELECT audit_ref FROM audits WHERE id = ?", (row["audit_id"],)).fetchone()
            findings = db.execute(
                """
                SELECT finding_ref, location, category, priority, assigned_department, pic, comment,
                       status, corrective_action, completion_date, completion_photo,
                       completion_remark, verified_by, verified_at, verification_remark, closed_at
                FROM findings
                WHERE audit_id = ?
                ORDER BY id
                """,
                (row["audit_id"],),
            ).fetchall()
    if not row:
        return None
    data = dict(row)
    data["items"] = json.loads(data.pop("items_json") or "[]")
    data["signatures"] = json.loads(data.pop("signatures_json") or "{}")
    data["inspection_name"] = normalized_inspection_name(data)
    data["audit_ref"] = audit["audit_ref"] if audit else ""
    data["findings"] = [dict(item) for item in findings]
    return data


def inspection_name(session):
    outlet = session.get("outlet") or "Outlet"
    date = normalize_audit_date(session.get("audit_date"))
    return f"{outlet}_{date}_{session.get('id')}"


def normalized_inspection_name(session):
    name = session.get("inspection_name") or inspection_name(session)
    if "_Today_" in name:
        return inspection_name(session)
    return name


def inspection_pdf(session):
    brand = branding_settings()
    items = session["items"]
    total_items = len(items)
    passed_items = sum(1 for item in items if item.get("passed"))
    na_items = sum(1 for item in items if item.get("notApplicable"))
    failed_items = total_items - passed_items - na_items
    score = round((passed_items * 100 / total_items) if total_items else 0)
    findings = session.get("findings") or []
    priority_findings = sum(1 for item in findings if item.get("priority") == "High")
    completed_findings = sum(1 for item in findings if item.get("status") in ("Completed", "Verified", "Closed"))
    outstanding_findings = max(0, len(findings) - completed_findings)
    lines = [
        f"{brand['appTitle'].upper()} REPORT",
        brand["departmentHeader"],
        f"Inspection: {session.get('inspection_name') or inspection_name(session)}",
        f"Inspection ID: {session['id']}",
        f"Audit Reference: {session.get('audit_ref') or 'Draft'}",
        f"Date: {session['audit_date']}",
        f"Outlet: {session['outlet']}",
        f"Location: {session['zone']}",
        f"Inspector: {session['auditor']}",
        f"Progress: {session['progress']}% | Score: {score}/100 | Rating: {rating(score)}",
        "",
        "SUMMARY",
        f"Total checklist items: {total_items}",
        f"Passed: {passed_items}",
        f"N/A: {na_items}",
        f"Failed: {failed_items}",
        f"Total findings: {len(findings)}",
        f"Priority findings: {priority_findings}",
        f"Completed corrective actions: {completed_findings}",
        f"Outstanding findings: {outstanding_findings}",
        "",
        "CHECKLIST",
    ]
    for item in items:
        status = "N/A" if item.get("notApplicable") else ("PASS" if item.get("passed") else "FAIL")
        location = item.get("location") or session["zone"]
        category = item.get("category") or "No category"
        lines.append(f"{status} - {location} - {category} - {item.get('section', 'Equipment')} - {item.get('item', '')}")
        if item.get("notes"):
            lines.append(f"Remark: {item['notes']}")
        if item.get("images"):
            lines.append("Original photos: " + ", ".join(image_labels(item["images"])))
            marked = [image.get("markedName") or image.get("markedDataUrl") for image in item["images"] if isinstance(image, dict) and image.get("markedDataUrl")]
            if marked:
                lines.append("Marked photos: " + ", ".join(marked))
    if findings:
        lines.extend(["", "FINDINGS AND CORRECTIVE ACTIONS"])
    for item in findings:
        completion_photos = image_labels(parse_image_list(item.get("completion_photo")))
        lines.append(f"{item.get('finding_ref', 'Finding')} - {item.get('location', '')} - {item.get('category') or 'No category'} - {item.get('priority', '')} - {item.get('status', '')}")
        lines.append(f"Department: {item.get('assigned_department') or 'Unassigned'} | PIC: {item.get('pic') or 'No PIC'}")
        if item.get("comment"):
            lines.append(f"Finding note: {item['comment']}")
        if item.get("corrective_action"):
            lines.append(f"Action taken: {item['corrective_action']}")
        if item.get("completion_date") or item.get("completion_remark"):
            lines.append(f"Completion: {item.get('completion_date') or 'No date'} | {item.get('completion_remark') or 'No remark'}")
        if completion_photos:
            lines.append("Completion photos: " + ", ".join(completion_photos))
        if item.get("verified_at") or item.get("closed_at"):
            lines.append(f"Verification: {item.get('verified_by') or 'No verifier'} | {item.get('verified_at') or 'No date'} | Closed: {item.get('closed_at') or 'No'}")
        if item.get("verification_remark"):
            lines.append(f"Verification remark: {item['verification_remark']}")
    signatures = session.get("signatures") or {}
    if signatures:
        lines.extend(["", "SIGNATURES"])
        for key, label in (("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by")):
            signature = signatures.get(key) or {}
            if signature.get("dataUrl"):
                lines.append(f"{label}: {signature.get('name') or 'Signed'}")
    def pdf_text(value):
        return str(value).replace("\\", "\\\\").replace("(", "[").replace(")", "]")

    commands = ["BT /F1 11 Tf 50 780 Td 13 TL"]
    for line in lines[:52]:
        commands.append(f"({pdf_text(line)}) Tj T*")
    commands.append("ET")
    stream = "\n".join(commands)
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        f"<< /Length {len(stream.encode('latin-1', 'replace'))} >>\nstream\n{stream}\nendstream".encode("latin-1", "replace"),
    ]
    pdf = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for index, obj in enumerate(objects, 1):
        offsets.append(len(pdf))
        pdf.extend(f"{index} 0 obj\n".encode())
        pdf.extend(obj)
        pdf.extend(b"\nendobj\n")
    xref = len(pdf)
    pdf.extend(f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode())
    for offset in offsets[1:]:
        pdf.extend(f"{offset:010d} 00000 n \n".encode())
    pdf.extend(f"trailer << /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF".encode())
    return bytes(pdf)


class Handler(BaseHTTPRequestHandler):
    def session_token(self):
        header = self.headers.get("Cookie", "")
        jar = cookies.SimpleCookie()
        try:
            jar.load(header)
        except cookies.CookieError:
            return ""
        return jar.get("ottotree_session").value if jar.get("ottotree_session") else ""

    def current_user(self):
        token = self.session_token()
        session = SESSION_TOKENS.get(token)
        if not session or session["expires_at"] < time.time():
            if token:
                SESSION_TOKENS.pop(token, None)
            return None
        with connect() as db:
            row = db.execute(
                """
                SELECT id, name, role, email, department, active, reset_required,
                       last_login_at, login_count, title, responsibilities
                FROM users
                WHERE id = ? AND active = 1
                """,
                (session["user_id"],),
            ).fetchone()
        return public_user(row)

    def require_auth(self, parsed):
        if not parsed.path.startswith("/api/"):
            return True
        if parsed.path == "/api/branding":
            return True
        if parsed.path.startswith("/api/auth/"):
            return True
        if self.current_user():
            return True
        self.json({"ok": False, "error": "Login required"}, status=401)
        return False

    def do_GET(self):
        parsed = urlparse(self.path)
        if not self.require_auth(parsed):
            return
        if parsed.path == "/api/auth/me":
            user = self.current_user()
            if not user:
                self.json({"ok": False, "error": "Login required"}, status=401)
                return
            self.json({"ok": True, "user": user})
            return
        if parsed.path == "/api/branding":
            self.json(branding_settings())
            return
        if parsed.path == "/api/dashboard":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.json(dashboard(unit))
            return
        if parsed.path == "/api/checklist":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.json({"items": checklist(unit)})
            return
        if parsed.path == "/api/work-orders":
            self.json(work_order_items())
            return
        if parsed.path == "/api/findings":
            self.json(finding_items())
            return
        if parsed.path == "/api/equipment":
            outlet = parse_qs(parsed.query).get("outlet", [None])[0]
            self.json(equipment_items(outlet))
            return
        if parsed.path == "/api/inspection-sessions":
            self.json(inspection_sessions())
            return
        if parsed.path.startswith("/api/inspection-sessions/"):
            suffix = parsed.path.rsplit("/", 1)[-1]
            if suffix == "export.pdf":
                session_id = parsed.path.split("/")[-2]
                if not session_id.isdigit():
                    self.send_error(400)
                    return
                session = inspection_session(int(session_id))
                if not session:
                    self.send_error(404)
                    return
                filename = f"{session.get('inspection_name') or inspection_name(session)}.pdf"
                self.download(inspection_pdf(session), "application/pdf", filename)
                return
            if not suffix.isdigit():
                self.send_error(400)
                return
            session = inspection_session(int(suffix))
            if not session:
                self.send_error(404)
                return
            self.json(session)
            return
        if parsed.path == "/api/locations":
            outlet = parse_qs(parsed.query).get("outlet", [""])[0]
            self.json(locations(outlet))
            return
        if parsed.path == "/api/zones":
            outlet = parse_qs(parsed.query).get("outlet", [""])[0]
            self.json(zones(outlet))
            return
        if parsed.path == "/api/reports":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.json(report(unit))
            return
        if parsed.path == "/api/reports/export.json":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.download(json.dumps(report(unit), indent=2).encode("utf-8"), "application/json", "audit-report.json")
            return
        if parsed.path == "/api/reports/export.csv":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.download(report_csv(unit), "text/csv", "audit-report.csv")
            return
        if parsed.path == "/api/reports/export.xls":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.download(report_xls(unit), "application/vnd.ms-excel", "audit-report.xls")
            return
        if parsed.path == "/api/admin":
            self.json(admin_records())
            return
        if parsed.path == "/api/setup":
            self.json(setup_records())
            return
        if parsed.path == "/api/roles":
            self.json(role_items())
            return
        if parsed.path == "/api/users":
            self.json(users())
            return
        if parsed.path == "/api/notifications":
            self.json(notifications())
            return
        if parsed.path == "/api/comments":
            params = parse_qs(parsed.query)
            self.json(comments(params.get("type", [""])[0], params.get("id", ["0"])[0]))
            return
        self.static_file(parsed.path)

    def do_POST(self):
        parsed = urlparse(self.path)
        length = int(self.headers.get("Content-Length", "0"))
        try:
            payload = json.loads(self.rfile.read(length) or b"{}")
        except json.JSONDecodeError:
            self.send_error(400, "Invalid JSON body")
            return
        now = int(time.time() * 1000)
        if parsed.path == "/api/auth/login":
            email = (payload.get("email") or "").strip().lower()
            password = payload.get("password") or ""
            remember = bool(payload.get("remember"))
            with connect() as db:
                row = db.execute(
                    """
                    SELECT id, name, role, email, department, password_hash, active,
                           reset_required, last_login_at, login_count, title, responsibilities
                    FROM users
                    WHERE lower(email) = ?
                    """,
                    (email,),
                ).fetchone()
                if not row or not row["active"] or not verify_password(password, row["password_hash"]):
                    self.json({"ok": False, "error": "Invalid email or password"}, status=401)
                    return
                token = secrets.token_urlsafe(32)
                max_age = 60 * 60 * 24 * 30 if remember else 60 * 60 * 8
                SESSION_TOKENS[token] = {"user_id": row["id"], "expires_at": time.time() + max_age}
                logged_at = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
                db.execute(
                    "UPDATE users SET last_login_at = ?, login_count = COALESCE(login_count, 0) + 1 WHERE id = ?",
                    (logged_at, row["id"]),
                )
                db.execute(
                    """
                    INSERT INTO user_login_activity (user_id, email, logged_at, remember_me, user_agent)
                    VALUES (?, ?, ?, ?, ?)
                    """,
                    (row["id"], row["email"], logged_at, 1 if remember else 0, self.headers.get("User-Agent", "")),
                )
                refreshed = db.execute(
                    """
                    SELECT id, name, role, email, department, active, reset_required,
                           last_login_at, login_count, title, responsibilities
                    FROM users
                    WHERE id = ?
                    """,
                    (row["id"],),
                ).fetchone()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Set-Cookie", f"ottotree_session={token}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax")
            self.end_headers()
            self.wfile.write(json.dumps({"ok": True, "user": public_user(refreshed)}).encode("utf-8"))
            return
        if parsed.path == "/api/auth/logout":
            SESSION_TOKENS.pop(self.session_token(), None)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Set-Cookie", "ottotree_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
            self.end_headers()
            self.wfile.write(json.dumps({"ok": True}).encode("utf-8"))
            return
        if parsed.path == "/api/auth/forgot-password":
            self.json({"ok": True, "message": "Ask a Super user to reset this user's password from Users setup."})
            return
        if parsed.path == "/api/auth/register":
            name = (payload.get("name") or "").strip()
            email = (payload.get("email") or "").strip().lower()
            password = payload.get("password") or ""
            if not name or not email or len(password) < 8:
                self.json({"ok": False, "error": "Name, email, and an 8-character password are required"}, status=400)
                return
            with connect() as db:
                try:
                    db.execute(
                        """
                        INSERT INTO users
                        (name, role, email, department, password_hash, active, reset_required, title, responsibilities, created_at)
                        VALUES (?, '', ?, '', ?, 0, 0, '', '', ?)
                        """,
                        (name, email, hash_password(password), now),
                    )
                except sqlite3.IntegrityError:
                    self.json({"ok": False, "error": "An account with this email already exists"}, status=409)
                    return
            self.json({"ok": True, "message": "Account registered. A Super user must activate it and assign a role before login."})
            return
        if parsed.path == "/api/auth/change-password":
            user = self.current_user()
            if not user:
                self.json({"ok": False, "error": "Login required"}, status=401)
                return
            old_password = payload.get("oldPassword") or ""
            new_password = payload.get("newPassword") or ""
            if len(new_password) < 8:
                self.json({"ok": False, "error": "New password must be at least 8 characters"}, status=400)
                return
            with connect() as db:
                row = db.execute("SELECT password_hash FROM users WHERE id = ?", (user["id"],)).fetchone()
                if not row or not verify_password(old_password, row["password_hash"]):
                    self.json({"ok": False, "error": "Current password is incorrect"}, status=400)
                    return
                db.execute(
                    "UPDATE users SET password_hash = ?, reset_required = 0 WHERE id = ?",
                    (hash_password(new_password), user["id"]),
                )
            self.json({"ok": True})
            return
        if not self.require_auth(parsed):
            return
        with connect() as db:
            default_outlet = first_outlet(db)
            default_department = first_department(db)
            default_category = first_category(db)
            if parsed.path == "/api/audits":
                cursor = db.execute(
                    """
                    INSERT INTO audits
                    (business_unit, outlet, branch, audit_date, auditor, audit_type, score, created_at)
                    VALUES (?, ?, 'LONG', ?, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or default_outlet,
                        normalize_audit_date(payload.get("auditDate")),
                        payload.get("auditor", "Unnamed Auditor"),
                        payload.get("auditType", "Standard"),
                        max(0, min(100, int(payload.get("score") or 0))),
                        now,
                    ),
                )
                db.execute(
                    "UPDATE audits SET audit_ref = ? WHERE id = ?",
                    (audit_ref(cursor.lastrowid, normalize_audit_date(payload.get("auditDate"))), cursor.lastrowid),
                )
            elif parsed.path == "/api/inspections":
                items = payload.get("items") or []
                score_values = [max(0, min(100, int(item.get("score") or 0))) for item in items]
                total_score = round(sum(score_values) / len(score_values)) if score_values else 0
                cursor = db.execute(
                    """
                    INSERT INTO audits
                    (business_unit, outlet, branch, audit_date, auditor, audit_type, score, created_at)
                    VALUES (?, ?, ?, ?, ?, 'Inspection', ?, ?)
                    """,
                    (
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or default_outlet,
                        payload.get("zone", "Unassigned"),
                        normalize_audit_date(payload.get("auditDate")),
                        payload.get("auditor", "Unnamed Auditor"),
                        total_score,
                        now,
                    ),
                )
                audit_id = cursor.lastrowid
                audit_reference = audit_ref(audit_id, normalize_audit_date(payload.get("auditDate")))
                db.execute("UPDATE audits SET audit_ref = ? WHERE id = ?", (audit_reference, audit_id))
                for item in items:
                    cursor = db.execute(
                        """
                        INSERT INTO inspection_items
                        (audit_id, section, item, score, notes, evidence_status)
                        VALUES (?, ?, ?, ?, ?, ?)
                        """,
                        (
                            audit_id,
                            item.get("section", "General"),
                            item.get("item", "Checklist item"),
                            max(0, min(100, int(item.get("score") or 0))),
                            item.get("notes", ""),
                            item.get("evidenceStatus", "Missing"),
                        ),
                    )
                    item_score = max(0, min(100, int(item.get("score") or 0)))
                    if item_score < 70:
                        finding_cursor = db.execute(
                            """
                            INSERT INTO findings
                            (audit_id, audit_ref, business_unit, outlet, location, category, priority,
                             assigned_department, pic, comment, status, source_item_id, created_at, updated_at)
                            VALUES (?, ?, ?, ?, ?, ?, ?, ?, '', ?, 'Assigned', ?, ?, ?)
                            """,
                            (
                                audit_id,
                                audit_reference,
                                payload.get("businessUnit", "Ottotree"),
                                payload.get("outlet") or default_outlet,
                                payload.get("zone", "Unassigned"),
                                item.get("category") or default_category,
                                "High" if item_score < 60 else "Medium",
                                default_department,
                                item.get("notes", "") or item.get("item", "Inspection finding"),
                                cursor.lastrowid,
                                now,
                                now,
                            ),
                        )
                        finding_id = finding_cursor.lastrowid
                        finding_reference = finding_ref(finding_id, normalize_audit_date(payload.get("auditDate")))
                        db.execute("UPDATE findings SET finding_ref = ? WHERE id = ?", (finding_reference, finding_id))
                        db.execute("UPDATE inspection_items SET finding_id = ? WHERE id = ?", (finding_id, cursor.lastrowid))
                    if item_score < 70 and not item.get("workOrderRequested"):
                        work_order_cursor = db.execute(
                            """
                            INSERT INTO work_orders
                            (business_unit, outlet, zone, request_type, category, priority, title, description,
                             assignee, pic, status, action_taken, completion_date, completion_remark, completion_photo,
                             verified_by, verified_at, verification_remark, closed_at, outlet_confirmed,
                             source_audit_id, source_item_id, source_finding_id, created_at)
                            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, '', 'Assigned', '', '', '', ?, '', '', '', '', 0, ?, ?, ?, ?)
                            """,
                            (
                                payload.get("businessUnit", "Ottotree"),
                                payload.get("outlet") or default_outlet,
                                payload.get("zone", "Unassigned"),
                                default_department,
                                item.get("category") or default_category,
                                "High" if item_score < 60 else "Medium",
                                item.get("item", "Inspection issue"),
                                item.get("notes", "") or "Created from low inspection score",
                                "Technical Support",
                                json.dumps([]),
                                audit_id,
                                cursor.lastrowid,
                                finding_id,
                                now,
                            ),
                        )
                        db.execute(
                            "UPDATE work_orders SET work_order_ref = ? WHERE id = ?",
                            (work_order_ref(work_order_cursor.lastrowid, normalize_audit_date(payload.get("auditDate"))), work_order_cursor.lastrowid),
                        )
            elif parsed.path == "/api/inspection-sessions":
                items = payload.get("items") or []
                progress = inspection_progress(items)
                status = "Completed" if payload.get("complete") and progress == 100 else "Draft"
                cursor = db.execute(
                    """
                    INSERT INTO inspection_sessions
                    (business_unit, outlet, zone, audit_date, auditor, items_json,
                     progress, status, signatures_json, created_at, updated_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or default_outlet,
                        payload.get("zone", "Unassigned"),
                        normalize_audit_date(payload.get("auditDate")),
                        payload.get("auditor", "Unnamed Inspector"),
                        json.dumps(items),
                        progress,
                        status,
                        json.dumps(payload.get("signatures") or {}),
                        now,
                        now,
                    ),
                )
                session_id = cursor.lastrowid
                session_name = inspection_name({
                    "id": session_id,
                    "outlet": payload.get("outlet") or default_outlet,
                    "audit_date": normalize_audit_date(payload.get("auditDate")),
                })
                db.execute("UPDATE inspection_sessions SET inspection_name = ? WHERE id = ?", (session_name, session_id))
                audit_id = None
                if status == "Completed":
                    audit_id = finalize_inspection(db, session_id, payload, now)
                self.json({"ok": True, "id": session_id, "inspectionName": session_name, "progress": progress, "status": status, "auditId": audit_id})
                return
            elif parsed.path == "/api/schedules":
                db.execute(
                    """
                    INSERT INTO schedules
                    (business_unit, outlet, zone, scheduled_date, auditor, remarks, status, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or default_outlet,
                        payload.get("zone", "Unassigned"),
                        payload.get("scheduledDate", "Today"),
                        payload.get("auditor", "Unassigned"),
                        payload.get("remarks", ""),
                        payload.get("status", "Pending"),
                        now,
                    ),
                )
            elif parsed.path == "/api/captain-logins":
                db.execute(
                    """
                    INSERT INTO captain_logins (outlet, captain_name, logged_at)
                    VALUES (?, ?, ?)
                    """,
                    (
                        payload.get("outlet") or default_outlet,
                        payload.get("captainName", "Unnamed Captain"),
                        now,
                    ),
                )
            elif parsed.path == "/api/work-orders":
                status, verified_at, closed_at = workflow_dates(payload)
                priority = payload.get("priority", "Medium")
                due_date = payload.get("dueDate") or priority_due_date(db, priority, now)
                current_sla_status = payload.get("slaStatus") or sla_status(status, due_date)
                cursor = db.execute(
                    """
                    INSERT INTO work_orders
                    (business_unit, outlet, zone, request_type, category, priority, title, description,
                     assignee, pic, status, action_taken, completion_date, completion_remark, completion_photo,
                     verified_by, verified_at, verification_remark, closed_at, due_date, vendor, sla_status,
                     cost, outlet_confirmed, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)
                    """,
                    (
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or default_outlet,
                        payload.get("zone", "Unassigned"),
                        payload.get("requestType") or default_department,
                        payload.get("category") or default_category,
                        priority,
                        payload.get("title", "Work order"),
                        payload.get("description", ""),
                        payload.get("assignee", "Technical Support"),
                        payload.get("pic", ""),
                        status,
                        payload.get("actionTaken", ""),
                        payload.get("completionDate", ""),
                        payload.get("completionRemark", ""),
                        json_text(payload.get("completionPhoto"), []),
                        payload.get("verifiedBy", ""),
                        verified_at,
                        payload.get("verificationRemark", ""),
                        closed_at,
                        due_date,
                        payload.get("vendor", ""),
                        current_sla_status,
                        float(payload.get("cost") or 0),
                        now,
                    ),
                )
                db.execute(
                    "UPDATE work_orders SET work_order_ref = ? WHERE id = ?",
                    (work_order_ref(cursor.lastrowid), cursor.lastrowid),
                )
                create_notification(db, "Work order assigned", payload.get("title", "Work order"), "In-App", "work_order", cursor.lastrowid)
                if current_sla_status == "Due Soon":
                    create_notification(db, "Work order due soon", f"{payload.get('title', 'Work order')} is due on {due_date}", "In-App", "work_order", cursor.lastrowid)
                if current_sla_status == "Overdue":
                    create_notification(db, "Work order overdue", f"{payload.get('title', 'Work order')} passed its due date {due_date}", "In-App", "work_order", cursor.lastrowid)
            elif parsed.path == "/api/equipment":
                db.execute(
                    """
                    INSERT OR REPLACE INTO equipment
                    (asset_id, qr_code, business_unit, outlet, zone, equipment_type,
                     health_status, last_checked, replacement_flag, notes, name, description,
                     type, operational_status, code, model, serial_number, brand, location,
                     installation_date, inspection_criteria, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("assetId") or payload.get("code") or "EQ-NEW",
                        payload.get("qrCode") or payload.get("code") or payload.get("assetId") or "EQ-NEW",
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or default_outlet,
                        payload.get("location") or payload.get("zone") or "Unassigned",
                        payload.get("type") or payload.get("equipmentType", "Equipment"),
                        payload.get("operationalStatus") or payload.get("healthStatus", "Operational"),
                        payload.get("installationDate") or payload.get("lastChecked", "Today"),
                        1 if payload.get("replacementFlag") else 0,
                        payload.get("description") or payload.get("notes", ""),
                        payload.get("name") or payload.get("assetId") or payload.get("code") or "Equipment",
                        payload.get("description", ""),
                        payload.get("type") or payload.get("equipmentType", "Equipment"),
                        payload.get("operationalStatus") or payload.get("healthStatus", "Operational"),
                        payload.get("code") or payload.get("assetId") or "EQ-NEW",
                        payload.get("model", ""),
                        payload.get("serialNumber", ""),
                        payload.get("brand", ""),
                        payload.get("location") or payload.get("zone") or "",
                        payload.get("installationDate") or payload.get("lastChecked", ""),
                        json.dumps(payload.get("inspectionCriteria") or DEFAULT_INSPECTION_CRITERIA),
                        now,
                    ),
                )
            elif parsed.path == "/api/locations":
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
            elif parsed.path == "/api/zones":
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
            elif parsed.path == "/api/admin":
                db.execute(
                    """
                    INSERT INTO admin_records (record_type, name, parent, detail, active, created_at)
                    VALUES (?, ?, ?, ?, 1, ?)
                    """,
                    (
                        payload.get("recordType", "Outlet"),
                        payload.get("name", "New record"),
                        payload.get("parent", ""),
                        payload.get("detail", ""),
                        now,
                    ),
                )
            elif parsed.path == "/api/setup/departments":
                db.execute(
                    """
                    INSERT OR REPLACE INTO departments (code, description, responsibilities, created_at)
                    VALUES (?, ?, ?, ?)
                    """,
                    (
                        (payload.get("code") or "Department").upper(),
                        payload.get("description", ""),
                        payload.get("responsibilities", ""),
                        now,
                    ),
                )
            elif parsed.path == "/api/setup/categories":
                db.execute(
                    """
                    INSERT OR REPLACE INTO categories (name, description, sequence, active, created_at)
                    VALUES (?, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("name", "New Category"),
                        payload.get("description", ""),
                        max(0, int(payload.get("sequence") or 0)),
                        1 if payload.get("active", True) else 0,
                        now,
                    ),
                )
            elif parsed.path == "/api/setup/outlets":
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
            elif parsed.path == "/api/users":
                if not is_company_admin_user(self.current_user()):
                    self.json({"ok": False, "error": "Admin access required"}, status=403)
                    return
                if not is_super_user(self.current_user()) and payload.get("role") == SUPER_ROLE:
                    self.json({"ok": False, "error": "Super role assignment requires Super access"}, status=403)
                    return
                password = payload.get("password") or DEFAULT_PASSWORD
                db.execute(
                    """
                    INSERT OR REPLACE INTO users
                    (name, role, email, department, password_hash, active, reset_required, title, responsibilities, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("name", "New User"),
                        payload.get("role", ""),
                        payload.get("email", "user@example.com"),
                        payload.get("department") or default_department,
                        hash_password(password),
                        1 if payload.get("active", True) else 0,
                        1 if payload.get("resetRequired", False) else 0,
                        payload.get("title", ""),
                        payload.get("responsibilities", ""),
                        now,
                    ),
                )
            elif parsed.path == "/api/roles":
                if not is_super_user(self.current_user()):
                    self.json({"ok": False, "error": "Super access required"}, status=403)
                    return
                name = (payload.get("name") or "New Role").strip()
                if name.lower() in ("admin", "super"):
                    self.json({"ok": False, "error": "The Super role is built in and cannot be recreated"}, status=400)
                    return
                db.execute(
                    """
                    INSERT OR REPLACE INTO roles (name, description, permissions_json, protected, created_at)
                    VALUES (?, ?, ?, 0, ?)
                    """,
                    (
                        name,
                        payload.get("description", ""),
                        json.dumps(payload.get("permissions") or []),
                        now,
                    ),
                )
            elif parsed.path == "/api/setup/priorities":
                db.execute(
                    """
                    INSERT OR REPLACE INTO priority_levels (name, classification, due_days, active, created_at)
                    VALUES (?, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("name", "Priority"),
                        payload.get("classification", "Priority"),
                        int(payload.get("dueDays") or 0),
                        1 if payload.get("active", True) else 0,
                        now,
                    ),
                )
            elif parsed.path == "/api/setup/audit-types":
                db.execute(
                    """
                    INSERT OR REPLACE INTO audit_types (name, description, active, created_at)
                    VALUES (?, ?, ?, ?)
                    """,
                    (
                        payload.get("name", "Standard"),
                        payload.get("description", ""),
                        1 if payload.get("active", True) else 0,
                        now,
                    ),
                )
            elif parsed.path == "/api/settings":
                if not is_super_user(self.current_user()):
                    self.json({"ok": False, "error": "Super access required"}, status=403)
                    return
                for key, value in (payload.get("settings") or {}).items():
                    db.execute("INSERT OR REPLACE INTO app_settings (key, value) VALUES (?, ?)", (key, json.dumps(value)))
            elif parsed.path == "/api/comments":
                db.execute(
                    """
                    INSERT INTO comments (record_type, record_id, comment, author, created_at)
                    VALUES (?, ?, ?, ?, ?)
                    """,
                    (
                        payload.get("recordType", "general"),
                        int(payload.get("recordId") or 0),
                        payload.get("comment", ""),
                        payload.get("author", self.current_user().get("name", "")),
                        now,
                    ),
                )
            elif parsed.path == "/api/notifications":
                create_notification(
                    db,
                    payload.get("title", "Notification"),
                    payload.get("message", ""),
                    payload.get("channel", "In-App"),
                    payload.get("relatedType", ""),
                    int(payload.get("relatedId") or 0),
                )
            else:
                self.send_error(404)
                return
        self.json({"ok": True})

    def do_PATCH(self):
        parsed = urlparse(self.path)
        length = int(self.headers.get("Content-Length", "0"))
        try:
            payload = json.loads(self.rfile.read(length) or b"{}")
        except json.JSONDecodeError:
            self.send_error(400, "Invalid JSON body")
            return
        if not self.require_auth(parsed):
            return

        if parsed.path.startswith("/api/inspection-sessions/"):
            session_id = parsed.path.rsplit("/", 1)[-1]
            if not session_id.isdigit():
                self.send_error(400)
                return
            now = int(time.time() * 1000)
            items = payload.get("items") or []
            progress = inspection_progress(items)
            complete = bool(payload.get("complete"))
            if complete and progress < 100:
                self.send_error(409, "Inspection is incomplete")
                return
            with connect() as db:
                session_name = inspection_name({
                    "id": int(session_id),
                    "outlet": payload.get("outlet") or first_outlet(db),
                    "audit_date": normalize_audit_date(payload.get("auditDate")),
                })
                cursor = db.execute(
                    """
                    UPDATE inspection_sessions
                    SET inspection_name = ?, business_unit = ?, outlet = ?, zone = ?, audit_date = ?, auditor = ?,
                        items_json = ?, progress = ?, status = ?, signatures_json = ?, updated_at = ?
                    WHERE id = ?
                    """,
                    (
                        session_name,
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or first_outlet(db),
                        payload.get("zone", "Unassigned"),
                        normalize_audit_date(payload.get("auditDate")),
                        payload.get("auditor", "Unnamed Inspector"),
                        json.dumps(items),
                        progress,
                        "Completed" if complete else "Draft",
                        json.dumps(payload.get("signatures") or {}),
                        now,
                        int(session_id),
                    ),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
                audit_id = None
                if complete:
                    audit_id = finalize_inspection(db, int(session_id), payload, now)
            self.json({"ok": True, "id": int(session_id), "inspectionName": session_name, "progress": progress, "status": "Completed" if complete else "Draft", "auditId": audit_id})
            return

        if parsed.path.startswith("/api/users/"):
            user_id = parsed.path.rsplit("/", 1)[-1]
            if not user_id.isdigit():
                self.send_error(400)
                return
            if not is_company_admin_user(self.current_user()):
                self.json({"ok": False, "error": "Admin access required"}, status=403)
                return
            with connect() as db:
                existing_user = db.execute("SELECT role FROM users WHERE id = ?", (int(user_id),)).fetchone()
                if not is_super_user(self.current_user()) and (payload.get("role") == SUPER_ROLE or (existing_user and existing_user["role"] == SUPER_ROLE)):
                    self.json({"ok": False, "error": "Super users require Super access"}, status=403)
                    return
                password = payload.get("password") or ""
                reset_password = bool(payload.get("resetPassword"))
                updates = [
                    payload.get("name", "New User"),
                    payload.get("role", ""),
                    payload.get("email", "user@example.com"),
                    payload.get("department") or first_department(db),
                    1 if payload.get("active", True) else 0,
                    1 if payload.get("resetRequired", False) else 0,
                    payload.get("title", ""),
                    payload.get("responsibilities", ""),
                ]
                password_sql = ""
                if reset_password or password:
                    password_sql = ", password_hash = ?"
                    updates.append(hash_password(password or DEFAULT_PASSWORD))
                    if reset_password:
                        updates[5] = 1
                updates.append(int(user_id))
                cursor = db.execute(
                    f"""
                    UPDATE users
                    SET name = ?, role = ?, email = ?, department = ?, active = ?, reset_required = ?,
                        title = ?, responsibilities = ?{password_sql}
                    WHERE id = ?
                    """,
                    tuple(updates),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/roles/"):
            role_id = parsed.path.rsplit("/", 1)[-1]
            if not role_id.isdigit():
                self.send_error(400)
                return
            if not is_super_user(self.current_user()):
                self.json({"ok": False, "error": "Super access required"}, status=403)
                return
            with connect() as db:
                role = db.execute("SELECT protected FROM roles WHERE id = ?", (int(role_id),)).fetchone()
                if not role:
                    self.send_error(404)
                    return
                if role["protected"]:
                    self.json({"ok": False, "error": "The Super role cannot be changed"}, status=400)
                    return
                name = (payload.get("name") or "New Role").strip()
                if name.lower() in ("admin", "super"):
                    self.json({"ok": False, "error": "Super is reserved"}, status=400)
                    return
                cursor = db.execute(
                    """
                    UPDATE roles
                    SET name = ?, description = ?, permissions_json = ?
                    WHERE id = ?
                    """,
                    (
                        name,
                        payload.get("description", ""),
                        json.dumps(payload.get("permissions") or []),
                        int(role_id),
                    ),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/setup/priorities/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute(
                    """
                    UPDATE priority_levels
                    SET name = ?, classification = ?, due_days = ?, active = ?
                    WHERE id = ?
                    """,
                    (
                        payload.get("name", "Priority"),
                        payload.get("classification", "Priority"),
                        int(payload.get("dueDays") or 0),
                        1 if payload.get("active", True) else 0,
                        int(record_id),
                    ),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/setup/audit-types/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute(
                    """
                    UPDATE audit_types
                    SET name = ?, description = ?, active = ?
                    WHERE id = ?
                    """,
                    (
                        payload.get("name", "Standard"),
                        payload.get("description", ""),
                        1 if payload.get("active", True) else 0,
                        int(record_id),
                    ),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/notifications/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute(
                    "UPDATE notifications SET status = 'Read', read_at = ? WHERE id = ?",
                    (int(time.time() * 1000), int(record_id)),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/setup/departments/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute(
                    """
                    UPDATE departments
                    SET code = ?, description = ?, responsibilities = ?
                    WHERE id = ?
                    """,
                    (
                        (payload.get("code") or "Department").upper(),
                        payload.get("description", ""),
                        payload.get("responsibilities", ""),
                        int(record_id),
                    ),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/setup/categories/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute(
                    """
                    UPDATE categories
                    SET name = ?, description = ?, sequence = ?, active = ?
                    WHERE id = ?
                    """,
                    (
                        payload.get("name", "New Category"),
                        payload.get("description", ""),
                        max(0, int(payload.get("sequence") or 0)),
                        1 if payload.get("active", True) else 0,
                        int(record_id),
                    ),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/setup/outlets/"):
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

        if parsed.path.startswith("/api/equipment/"):
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
                        installation_date = ?, inspection_criteria = ?
                    WHERE id = ?
                    """,
                    (
                        payload.get("assetId") or payload.get("code") or "EQ-NEW",
                        payload.get("qrCode") or payload.get("code") or payload.get("assetId") or "EQ-NEW",
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or first_outlet(db),
                        payload.get("location") or payload.get("zone") or "Unassigned",
                        payload.get("type") or payload.get("equipmentType", "Equipment"),
                        payload.get("operationalStatus") or payload.get("healthStatus", "Operational"),
                        payload.get("installationDate") or payload.get("lastChecked", "Today"),
                        1 if payload.get("replacementFlag") else 0,
                        payload.get("description") or payload.get("notes", ""),
                        payload.get("name") or payload.get("assetId") or payload.get("code") or "Equipment",
                        payload.get("description", ""),
                        payload.get("type") or payload.get("equipmentType", "Equipment"),
                        payload.get("operationalStatus") or payload.get("healthStatus", "Operational"),
                        payload.get("code") or payload.get("assetId") or "EQ-NEW",
                        payload.get("model", ""),
                        payload.get("serialNumber", ""),
                        payload.get("brand", ""),
                        payload.get("location") or payload.get("zone") or "",
                        payload.get("installationDate") or payload.get("lastChecked", ""),
                        json.dumps(payload.get("inspectionCriteria") or DEFAULT_INSPECTION_CRITERIA),
                        int(item_id),
                    ),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/work-orders/"):
            item_id = parsed.path.rsplit("/", 1)[-1]
            if not item_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                status, verified_at, closed_at = workflow_dates(payload)
                priority = payload.get("priority", "Medium")
                due_date = payload.get("dueDate") or priority_due_date(db, priority, int(time.time() * 1000))
                current_sla_status = payload.get("slaStatus") or sla_status(status, due_date)
                cursor = db.execute(
                    """
                    UPDATE work_orders
                    SET business_unit = ?, outlet = ?, zone = ?, request_type = ?, category = ?,
                        priority = ?, title = ?, description = ?, assignee = ?, pic = ?, status = ?,
                        action_taken = ?, completion_date = ?, completion_remark = ?, completion_photo = ?,
                        verified_by = ?, verified_at = ?, verification_remark = ?, closed_at = ?,
                        due_date = ?, vendor = ?, sla_status = ?, cost = ?
                    WHERE id = ?
                    """,
                    (
                        payload.get("businessUnit", "Ottotree"),
                        payload.get("outlet") or first_outlet(db),
                        payload.get("zone", "Unassigned"),
                        payload.get("requestType") or first_department(db),
                        payload.get("category") or first_category(db),
                        priority,
                        payload.get("title", "Work order"),
                        payload.get("description", ""),
                        payload.get("assignee", "Technical Support"),
                        payload.get("pic", ""),
                        status,
                        payload.get("actionTaken", ""),
                        payload.get("completionDate", ""),
                        payload.get("completionRemark", ""),
                        json_text(payload.get("completionPhoto"), []),
                        payload.get("verifiedBy", ""),
                        verified_at,
                        payload.get("verificationRemark", ""),
                        closed_at,
                        due_date,
                        payload.get("vendor", ""),
                        current_sla_status,
                        float(payload.get("cost") or 0),
                        int(item_id),
                    ),
                )
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
                sync_finding_from_work_order(db, int(item_id))
                if status in ("Completed", "Verified", "Closed"):
                    create_notification(db, "Corrective action completed", payload.get("title", "Work order"), "In-App", "work_order", int(item_id))
            self.json({"ok": True})
            return

        if parsed.path.startswith("/api/locations/"):
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

        if parsed.path.startswith("/api/zones/"):
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

        if not parsed.path.startswith("/api/schedules/"):
            self.send_error(404)
            return
        schedule_id = parsed.path.rsplit("/", 1)[-1]
        if not schedule_id.isdigit():
            self.send_error(400)
            return
        with connect() as db:
            cursor = db.execute(
                """
                UPDATE schedules
                SET outlet = ?, zone = ?, scheduled_date = ?, auditor = ?, remarks = ?, status = ?
                WHERE id = ?
                """,
                (
                    payload.get("outlet") or first_outlet(db),
                    payload.get("zone", "Unassigned"),
                    payload.get("scheduledDate", "Today"),
                    payload.get("auditor", "Unassigned"),
                    payload.get("remarks", ""),
                    payload.get("status", "Pending"),
                    int(schedule_id),
                ),
            )
            if cursor.rowcount == 0:
                self.send_error(404)
                return
        self.json({"ok": True})

    def do_DELETE(self):
        parsed = urlparse(self.path)
        if not self.require_auth(parsed):
            return
        if parsed.path.startswith("/api/inspection-sessions/"):
            session_id = parsed.path.rsplit("/", 1)[-1]
            if not session_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute("DELETE FROM inspection_sessions WHERE id = ?", (int(session_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/setup/departments/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute("DELETE FROM departments WHERE id = ?", (int(record_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/setup/categories/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute("DELETE FROM categories WHERE id = ?", (int(record_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/setup/priorities/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute("DELETE FROM priority_levels WHERE id = ?", (int(record_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/setup/audit-types/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute("DELETE FROM audit_types WHERE id = ?", (int(record_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/notifications/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute("DELETE FROM notifications WHERE id = ?", (int(record_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/comments/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute("DELETE FROM comments WHERE id = ?", (int(record_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/setup/outlets/"):
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
        if parsed.path.startswith("/api/users/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            if not is_company_admin_user(self.current_user()):
                self.json({"ok": False, "error": "Admin access required"}, status=403)
                return
            with connect() as db:
                existing_user = db.execute("SELECT role FROM users WHERE id = ?", (int(record_id),)).fetchone()
                if not is_super_user(self.current_user()) and existing_user and existing_user["role"] == SUPER_ROLE:
                    self.json({"ok": False, "error": "Super users require Super access"}, status=403)
                    return
                cursor = db.execute("DELETE FROM users WHERE id = ?", (int(record_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/roles/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            if not is_super_user(self.current_user()):
                self.json({"ok": False, "error": "Super access required"}, status=403)
                return
            with connect() as db:
                role = db.execute("SELECT name, protected FROM roles WHERE id = ?", (int(record_id),)).fetchone()
                if not role:
                    self.send_error(404)
                    return
                if role["protected"]:
                    self.json({"ok": False, "error": "The Super role cannot be deleted"}, status=400)
                    return
                cursor = db.execute("DELETE FROM roles WHERE id = ?", (int(record_id),))
                db.execute("UPDATE users SET role = '' WHERE role = ?", (role["name"],))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if parsed.path.startswith("/api/locations/"):
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
        if parsed.path.startswith("/api/zones/"):
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
        if parsed.path.startswith("/api/equipment/"):
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
        if parsed.path.startswith("/api/work-orders/"):
            record_id = parsed.path.rsplit("/", 1)[-1]
            if not record_id.isdigit():
                self.send_error(400)
                return
            with connect() as db:
                cursor = db.execute("DELETE FROM work_orders WHERE id = ?", (int(record_id),))
                if cursor.rowcount == 0:
                    self.send_error(404)
                    return
            self.json({"ok": True})
            return
        if not parsed.path.startswith("/api/schedules/"):
            self.send_error(404)
            return
        schedule_id = parsed.path.rsplit("/", 1)[-1]
        if not schedule_id.isdigit():
            self.send_error(400)
            return
        with connect() as db:
            cursor = db.execute("DELETE FROM schedules WHERE id = ?", (int(schedule_id),))
            if cursor.rowcount == 0:
                self.send_error(404)
                return
        self.json({"ok": True})

    def static_file(self, request_path):
        path = "index.html" if request_path in ("", "/") else request_path.lstrip("/")
        target = (ROOT / path).resolve()
        if not str(target).startswith(str(ROOT)) or not target.exists() or target.is_dir():
            self.send_error(404)
            return
        if target.name == "index.html":
            text = target.read_text(encoding="utf-8")
            def include(match):
                partial = (ROOT / match.group(1)).resolve()
                if not str(partial).startswith(str(ROOT)) or not partial.exists() or partial.is_dir():
                    return ""
                return partial.read_text(encoding="utf-8")
            body = INCLUDE_PATTERN.sub(include, text).encode("utf-8")
        else:
            body = target.read_bytes()
        mime = mimetypes.guess_type(target.name)[0] or "application/octet-stream"
        self.send_response(200)
        self.send_header("Content-Type", mime)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def json(self, payload, status=200):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def download(self, body, mime, filename):
        self.send_response(200)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Disposition", f"attachment; filename={filename}")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


if __name__ == "__main__":
    init_db()
    port = int(os.environ.get("PORT", "41883"))
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"Serving Ottotree Audit at http://127.0.0.1:{port}")
    print(f"SQLite database: {DB_PATH}")
    server.serve_forever()
