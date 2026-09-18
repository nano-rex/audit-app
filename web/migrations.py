"""Migrations for the audit application."""
import json
import time
from datetime import datetime
from common import audit_ref, hash_password, inspection_progress, location_qr_code, today_date
from config import DEFAULT_INSPECTION_CRITERIA, DEFAULT_PASSWORD
from database import connect, first_department
from seed_data import normalize_loudspeaker_outlets, seed_audit_types, seed_categories, seed_equipment, seed_locations, seed_priority_levels, seed_roles, seed_schedules, seed_settings, seed_setup_records, seed_users, seed_zones


def ensure_column(db, table, column, definition):
    columns = [row["name"] for row in db.execute(f"PRAGMA table_info({table})").fetchall()]
    if column not in columns:
        db.execute(f"ALTER TABLE {table} ADD COLUMN {column} {definition}")


def init_db():
    with connect() as db:
        db.execute("PRAGMA journal_mode=WAL")
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
                asset_id TEXT NOT NULL,
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
                code TEXT NOT NULL UNIQUE,
                model TEXT,
                serial_number TEXT,
                brand TEXT,
                location TEXT,
                installation_date TEXT,
                temporary_relocation TEXT,
                warranty_date TEXT,
                calibration_date TEXT,
                expiry_date TEXT,
                photos TEXT,
                inverter_model TEXT,
                motor_capacity TEXT,
                source_file TEXT,
                source_sheet TEXT,
                inspection_criteria TEXT,
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
        ensure_column(db, "audits", "scoring_json", "TEXT")
        for table in ("audits", "inspection_sessions"):
            for column in ("audit_time", "remarks"):
                ensure_column(db, table, column, "TEXT")
        ensure_column(db, "inspection_sessions", "audit_type", "TEXT")
        ensure_column(db, "findings", "images_json", "TEXT")
        ensure_column(db, "findings", "due_date", "TEXT")
        ensure_column(db, "findings", "priority_classification", "TEXT")
        for column in ("cause", "recommendation", "required_action", "images_json"):
            ensure_column(db, "work_orders", column, "TEXT")
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
        ensure_column(db, "equipment", "temporary_relocation", "TEXT")
        ensure_column(db, "equipment", "warranty_date", "TEXT")
        ensure_column(db, "equipment", "calibration_date", "TEXT")
        ensure_column(db, "equipment", "expiry_date", "TEXT")
        ensure_column(db, "equipment", "photos", "TEXT")
        ensure_column(db, "equipment", "inverter_model", "TEXT")
        ensure_column(db, "equipment", "motor_capacity", "TEXT")
        ensure_column(db, "equipment", "source_file", "TEXT")
        ensure_column(db, "equipment", "source_sheet", "TEXT")
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
        db.execute("UPDATE equipment SET asset_id = code WHERE code IS NOT NULL AND code != ''")
        db.execute("UPDATE equipment SET qr_code = code WHERE code IS NOT NULL AND code != ''")
        db.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_equipment_code ON equipment(code)")
        for statement in (
            "CREATE INDEX IF NOT EXISTS idx_equipment_outlet_name ON equipment(outlet, COALESCE(name, asset_id), id)",
            "CREATE INDEX IF NOT EXISTS idx_equipment_name ON equipment(COALESCE(name, asset_id), id)",
            "CREATE INDEX IF NOT EXISTS idx_audits_outlet_recent ON audits(business_unit, outlet, created_at DESC, id DESC)",
            "CREATE INDEX IF NOT EXISTS idx_sessions_completed ON inspection_sessions(status, audit_id)",
            "CREATE INDEX IF NOT EXISTS idx_sessions_updated ON inspection_sessions(updated_at DESC, id DESC)",
            "CREATE INDEX IF NOT EXISTS idx_findings_audit ON findings(audit_id)",
            "CREATE INDEX IF NOT EXISTS idx_locations_outlet ON locations(outlet_code, name)",
            "CREATE INDEX IF NOT EXISTS idx_zones_outlet ON zones(outlet_code, name)",
            "CREATE INDEX IF NOT EXISTS idx_users_email_lower ON users(lower(email))",
        ):
            db.execute(statement)
        ensure_column(db, "comments", "system_generated", "INTEGER NOT NULL DEFAULT 0")
        ensure_column(db, "users", "profile_photo", "TEXT NOT NULL DEFAULT '{}'")
        ensure_column(db, "users", "permission_overrides", "TEXT")
        ensure_column(db, "roles", "inspection_permissions", "TEXT")
        ensure_column(db, "users", "signature_image", "TEXT NOT NULL DEFAULT '{}'")
        ensure_column(db, "inspection_sessions", "owner_user_id", "INTEGER")
        ensure_column(db, "inspection_sessions", "schedule_id", "INTEGER")
        db.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_schedule ON inspection_sessions(schedule_id) WHERE schedule_id IS NOT NULL")
        ensure_column(db, "notifications", "recipient_user_id", "INTEGER")
        db.execute("CREATE INDEX IF NOT EXISTS idx_notifications_recipient ON notifications(recipient_user_id, created_at DESC)")
        db.execute("UPDATE equipment SET location = zone WHERE location IS NULL OR location = ''")
        db.execute("UPDATE equipment SET installation_date = last_checked WHERE installation_date IS NULL OR installation_date = ''")
        db.execute("DELETE FROM audits WHERE auditor = 'Sample Auditor'")
        schedules = db.execute("SELECT COUNT(*) FROM schedules").fetchone()[0]
        if schedules == 0:
            seed_schedules(db)
        equipment = db.execute("SELECT COUNT(*) FROM equipment").fetchone()[0]
        if equipment == 0:
            seed_equipment(db)
        normalize_loudspeaker_outlets(db)
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
