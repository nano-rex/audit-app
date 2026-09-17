"""Seed data for the audit application."""
import json
import time
from common import hash_password
from config import ADMIN_ROLE, APP_TABS, DEFAULT_AUDIT_TYPES, DEFAULT_CATEGORIES, DEFAULT_PASSWORD, DEFAULT_PRIORITY_LEVELS, DEFAULT_REPORT_SETTINGS, DEFAULT_SCORING_SETTINGS, DEFAULT_SYSTEM_SETTINGS, LOUDSPEAKER_OUTLETS, SUPER_ROLE


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
        if db.execute("SELECT 1 FROM users WHERE email = ?", (row[2],)).fetchone():
            continue
        db.execute(
            """
            INSERT INTO users (name, role, email, department, password_hash, active, reset_required, title, responsibilities, created_at)
            VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?, ?)
            ON CONFLICT(email) DO NOTHING
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
