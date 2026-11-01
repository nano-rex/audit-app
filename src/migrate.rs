//! Creating and upgrading an organization database, and the records a new one starts with.
//!
//! Every step is idempotent: a database written by any earlier release (of this program or the
//! Python one before it) is brought to the current schema, and each one-time conversion records
//! a flag in app_settings so it runs once.

use std::path::Path;

use crate::activity;
use crate::audits::{allocate_reference, convert_audit_codes};
use crate::clock;
use crate::common::{audit_ref, hash_password, inspection_progress, location_qr_code, today_date};
use crate::config::{self, ADMIN_ROLE, APP_TABS, DEFAULT_FIXTURE_CRITERIA, DEFAULT_INSPECTION_CRITERIA, DEFAULT_PASSWORD, LOUDSPEAKER_OUTLETS, VARIABLE_ASSET_TYPES};
use crate::control;
use crate::db::{self, first_department, load_or, load_value, read_setting, save_value, Db, FIELDS};
use crate::error::{internal, R};
use crate::json::{strings, Value};
use crate::locations::add_locations_to_default_zone;
use crate::params;
use crate::permissions::CHANGE_RECORDS;

fn ensure_column(db: &Db, table: &str, column: &str, definition: &str) -> R<()> {
    if !db.columns(table)?.iter().any(|name| name == column) {
        db.exec(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"), params![])?;
    }
    Ok(())
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS audits (
    id INTEGER PRIMARY KEY AUTOINCREMENT, audit_ref TEXT, business_unit TEXT NOT NULL, outlet TEXT NOT NULL, branch TEXT NOT NULL,
    audit_date TEXT NOT NULL, auditor TEXT NOT NULL, audit_type TEXT NOT NULL, score INTEGER NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS schedules (
    id INTEGER PRIMARY KEY AUTOINCREMENT, business_unit TEXT NOT NULL, outlet TEXT NOT NULL, zone TEXT, scheduled_date TEXT NOT NULL,
    auditor TEXT NOT NULL, remarks TEXT, status TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS captain_logins (
    id INTEGER PRIMARY KEY AUTOINCREMENT, outlet TEXT NOT NULL, captain_name TEXT NOT NULL, logged_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS inspection_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT, audit_id INTEGER NOT NULL, finding_id INTEGER, section TEXT NOT NULL, item TEXT NOT NULL,
    score INTEGER NOT NULL, notes TEXT, evidence_status TEXT NOT NULL, FOREIGN KEY(audit_id) REFERENCES audits(id)
);
CREATE TABLE IF NOT EXISTS inspection_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT, inspection_name TEXT, business_unit TEXT NOT NULL, outlet TEXT NOT NULL, zone TEXT NOT NULL,
    audit_date TEXT NOT NULL, auditor TEXT NOT NULL, items_data_id INTEGER REFERENCES value_sets(id), progress INTEGER NOT NULL,
    status TEXT NOT NULL, audit_id INTEGER, signatures_data_id INTEGER REFERENCES value_sets(id), created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS work_orders (
    id INTEGER PRIMARY KEY AUTOINCREMENT, work_order_ref TEXT, business_unit TEXT NOT NULL, outlet TEXT NOT NULL, zone TEXT NOT NULL,
    request_type TEXT NOT NULL, category TEXT, priority TEXT NOT NULL, title TEXT NOT NULL, description TEXT, assignee TEXT NOT NULL,
    pic TEXT, status TEXT NOT NULL, action_taken TEXT, completion_date TEXT, completion_remark TEXT,
    completion_photo_data_id INTEGER REFERENCES value_sets(id), verified_by TEXT, verified_at TEXT, verification_remark TEXT,
    closed_at TEXT, outlet_confirmed INTEGER NOT NULL DEFAULT 0, source_audit_id INTEGER, source_item_id INTEGER, source_finding_id INTEGER,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS findings (
    id INTEGER PRIMARY KEY AUTOINCREMENT, finding_ref TEXT, audit_id INTEGER NOT NULL, audit_ref TEXT, business_unit TEXT NOT NULL,
    outlet TEXT NOT NULL, location TEXT NOT NULL, category TEXT, priority TEXT NOT NULL, assigned_department TEXT, pic TEXT, comment TEXT,
    status TEXT NOT NULL, corrective_action TEXT, completion_date TEXT, completion_photo_data_id INTEGER REFERENCES value_sets(id),
    completion_remark TEXT, verified_by TEXT, verified_at TEXT, verification_remark TEXT, closed_at TEXT, source_item_id INTEGER,
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, FOREIGN KEY(audit_id) REFERENCES audits(id)
);
CREATE TABLE IF NOT EXISTS change_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT, record TEXT NOT NULL, action TEXT NOT NULL, record_id INTEGER, method TEXT NOT NULL,
    path TEXT NOT NULL, payload_data_id INTEGER REFERENCES value_sets(id), before_data_id INTEGER REFERENCES value_sets(id), summary TEXT,
    status TEXT NOT NULL, requested_by TEXT, requested_by_user_id INTEGER, decided_by TEXT, decided_at INTEGER, remark TEXT, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS activity_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT, created_at INTEGER NOT NULL, user_id INTEGER, user_name TEXT, action TEXT NOT NULL,
    record_type TEXT, record_id INTEGER, record_ref TEXT, outlet TEXT, business_unit TEXT, duration_ms INTEGER, detail TEXT
);
CREATE INDEX IF NOT EXISTS idx_activity_log_time ON activity_log(created_at);
CREATE INDEX IF NOT EXISTS idx_activity_log_record ON activity_log(record_type, record_id, action);
CREATE TABLE IF NOT EXISTS work_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT, request_ref TEXT, business_unit TEXT NOT NULL, outlet TEXT NOT NULL, location TEXT NOT NULL,
    equipment_id INTEGER, item_name TEXT, category TEXT, priority TEXT, department TEXT, description TEXT NOT NULL,
    images_data_id INTEGER REFERENCES value_sets(id), audit_id INTEGER, audit_ref TEXT, status TEXT NOT NULL, requested_by TEXT,
    requested_by_user_id INTEGER, reviewed_by TEXT, decline_remark TEXT, work_order_id INTEGER, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS equipment (
    id INTEGER PRIMARY KEY AUTOINCREMENT, asset_id TEXT NOT NULL, qr_code TEXT NOT NULL, business_unit TEXT NOT NULL, outlet TEXT NOT NULL,
    zone TEXT NOT NULL, equipment_type TEXT NOT NULL, health_status TEXT NOT NULL, last_checked TEXT NOT NULL,
    replacement_flag INTEGER NOT NULL DEFAULT 0, notes TEXT, name TEXT, description TEXT, type TEXT, operational_status TEXT,
    code TEXT NOT NULL UNIQUE, model TEXT, serial_number TEXT, brand TEXT, location TEXT, installation_date TEXT, temporary_relocation TEXT,
    warranty_date TEXT, calibration_date TEXT, expiry_date TEXT, photos_data_id INTEGER REFERENCES value_sets(id), inverter_model TEXT,
    motor_capacity TEXT, source_file TEXT, source_sheet TEXT, inspection_criteria_data_id INTEGER REFERENCES value_sets(id), created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS departments (
    id INTEGER PRIMARY KEY AUTOINCREMENT, code TEXT NOT NULL UNIQUE, description TEXT, responsibilities TEXT, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, description TEXT, sequence INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS outlets (
    id INTEGER PRIMARY KEY AUTOINCREMENT, code TEXT NOT NULL UNIQUE, location TEXT, description TEXT, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS locations (
    id INTEGER PRIMARY KEY AUTOINCREMENT, outlet_code TEXT NOT NULL, name TEXT NOT NULL, floor TEXT, area TEXT,
    display_order INTEGER NOT NULL DEFAULT 0, size TEXT, qr_code TEXT, created_at INTEGER NOT NULL, UNIQUE(outlet_code, name)
);
CREATE TABLE IF NOT EXISTS zones (
    id INTEGER PRIMARY KEY AUTOINCREMENT, outlet_code TEXT NOT NULL, name TEXT NOT NULL, locations_data_id INTEGER REFERENCES value_sets(id),
    description TEXT, created_at INTEGER NOT NULL, UNIQUE(outlet_code, name)
);
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, username TEXT, role TEXT NOT NULL, email TEXT NOT NULL UNIQUE, department TEXT,
    password_hash TEXT, active INTEGER NOT NULL DEFAULT 1, reset_required INTEGER NOT NULL DEFAULT 0, last_login_at TEXT,
    login_count INTEGER NOT NULL DEFAULT 0, title TEXT, responsibilities TEXT, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS roles (
    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, description TEXT, permissions_data_id INTEGER REFERENCES value_sets(id),
    protected INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS priority_levels (
    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, classification TEXT NOT NULL, due_days INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS audit_types (
    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, description TEXT, active INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS app_settings (key TEXT PRIMARY KEY, value_data_id INTEGER REFERENCES value_sets(id));
CREATE TABLE IF NOT EXISTS comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT, record_type TEXT NOT NULL, record_id INTEGER NOT NULL, comment TEXT NOT NULL, author TEXT, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS notifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, message TEXT NOT NULL, channel TEXT NOT NULL, status TEXT NOT NULL,
    related_type TEXT, related_id INTEGER, created_at INTEGER NOT NULL, read_at INTEGER
);
CREATE TABLE IF NOT EXISTS user_login_activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL, email TEXT NOT NULL, logged_at TEXT NOT NULL,
    remember_me INTEGER NOT NULL DEFAULT 0, user_agent TEXT, FOREIGN KEY(user_id) REFERENCES users(id)
);
";

/// The attribute columns of a table that exists: add each value-set column it lacks, and refuse a
/// database that still keeps attributes as JSON text (converted only by the Python release).
fn attribute_columns(db: &Db) -> R<()> {
    db::initialize_values(db)?;
    for (table, fields) in FIELDS {
        let columns = db.columns(table)?;
        if columns.is_empty() {
            continue;
        }
        for (old, new) in fields.iter() {
            if columns.iter().any(|column| column == old) {
                return Err(internal(format!(
                    "{} keeps record attributes as JSON text (column {table}.{old}). Start it once with the Python release (audit-app-old), which converts it, then start this server again.",
                    db.path.display()
                )));
            }
            if !columns.iter().any(|column| column == new) {
                db.exec(&format!("ALTER TABLE \"{table}\" ADD COLUMN \"{new}\" INTEGER REFERENCES value_sets(id)"), params![])?;
            }
        }
    }
    Ok(())
}

fn flag_set(db: &Db, key: &str) -> R<bool> {
    db.exists("SELECT 1 FROM app_settings WHERE key = ?", params![key])
}

fn set_flag(db: &Db, key: &str) -> R<()> {
    db.exec("INSERT INTO app_settings (key, value_data_id) VALUES (?, ?)", params![key, save_value(db, &Value::Bool(true))?])?;
    Ok(())
}

/// Create the database at this thread's path, or bring it up to date.
pub fn init_db() -> R<()> {
    let path = config::active_db_path();
    media_migration(&path)?;
    let database = db::open(&path)?;
    database.rows("PRAGMA journal_mode=WAL", params![])?;
    let result = (|| -> R<()> {
        database.begin_immediate()?;
        upgrade(&database)
    })();
    db::finish(&database, result)?;
    // Only when no Super account exists anywhere: after moving any out of this organization.
    control::ensure_super_account()
}

fn media_migration(path: &Path) -> R<()> {
    let database = db::open(path)?;
    crate::media::ensure_schema(&database)?;
    db::finish(&database, Ok(()))?;
    drop(database);
    crate::media::migrate_directory(path, &config::data_dir().join("media"))?;
    Ok(())
}

fn upgrade(db: &Db) -> R<()> {
    attribute_columns(db)?;
    db.batch(SCHEMA)?;
    attribute_columns(db)?;
    ensure_column(db, "audits", "audit_ref", "TEXT")?;
    ensure_column(db, "audits", "scoring_data_id", "INTEGER REFERENCES value_sets(id)")?;
    for table in ["audits", "inspection_sessions"] {
        for column in ["audit_time", "remarks"] {
            ensure_column(db, table, column, "TEXT")?;
        }
    }
    ensure_column(db, "inspection_sessions", "audit_type", "TEXT")?;
    ensure_column(db, "findings", "images_data_id", "INTEGER REFERENCES value_sets(id)")?;
    // Findings name the item they are about and the work request raised for it.
    for (column, kind) in [("equipment_id", "INTEGER"), ("item_name", "TEXT"), ("criterion", "TEXT"), ("work_request_id", "INTEGER")] {
        ensure_column(db, "findings", column, kind)?;
    }
    ensure_column(db, "work_orders", "work_request_id", "INTEGER")?;
    // A role limits its users to all outlets, one outlet, or the outlets chosen for each user.
    ensure_column(db, "roles", "outlet_scope", "TEXT NOT NULL DEFAULT 'all'")?;
    ensure_column(db, "roles", "department", "TEXT")?;
    // The chain of command: the role each role reports to.
    ensure_column(db, "roles", "reports_to_id", "INTEGER")?;
    ensure_column(db, "findings", "due_date", "TEXT")?;
    ensure_column(db, "findings", "priority_classification", "TEXT")?;
    for column in ["cause", "recommendation", "required_action", "images_data_id"] {
        ensure_column(db, "work_orders", column, "TEXT")?;
    }
    ensure_column(db, "inspection_items", "finding_id", "INTEGER")?;
    for (column, kind) in [("work_order_ref", "TEXT"), ("source_finding_id", "INTEGER"), ("category", "TEXT"), ("pic", "TEXT"), ("action_taken", "TEXT"),
                           ("completion_date", "TEXT"), ("completion_remark", "TEXT"), ("completion_photo_data_id", "INTEGER REFERENCES value_sets(id)"),
                           ("verified_by", "TEXT"), ("verified_at", "TEXT"), ("verification_remark", "TEXT"), ("closed_at", "TEXT")] {
        ensure_column(db, "work_orders", column, kind)?;
    }
    for (column, kind) in [("completion_photo_data_id", "INTEGER REFERENCES value_sets(id)"), ("verified_by", "TEXT"), ("verified_at", "TEXT"),
                           ("verification_remark", "TEXT"), ("closed_at", "TEXT")] {
        ensure_column(db, "findings", column, kind)?;
    }
    ensure_column(db, "inspection_sessions", "inspection_name", "TEXT")?;
    ensure_column(db, "inspection_sessions", "signatures_data_id", "INTEGER REFERENCES value_sets(id)")?;
    ensure_column(db, "users", "department", "TEXT")?;
    ensure_column(db, "users", "username", "TEXT")?;
    for user in db.rows("SELECT id, email FROM users WHERE username IS NULL OR username = ''", params![])? {
        let candidate = user.str("email").split('@').next().unwrap_or("").to_lowercase();
        if !candidate.is_empty() && !db.exists("SELECT 1 FROM users WHERE lower(username) = ?", params![candidate])? {
            db.exec("UPDATE users SET username = ? WHERE id = ?", params![candidate, user.int("id")])?;
        }
    }
    for (column, kind) in [("password_hash", "TEXT"), ("active", "INTEGER NOT NULL DEFAULT 1"), ("reset_required", "INTEGER NOT NULL DEFAULT 0"),
                           ("last_login_at", "TEXT"), ("login_count", "INTEGER NOT NULL DEFAULT 0")] {
        ensure_column(db, "users", column, kind)?;
    }
    ensure_column(db, "roles", "description", "TEXT")?;
    ensure_column(db, "roles", "permissions_data_id", "INTEGER REFERENCES value_sets(id)")?;
    ensure_column(db, "roles", "protected", "INTEGER NOT NULL DEFAULT 0")?;
    for (column, kind) in [("floor", "TEXT"), ("area", "TEXT"), ("display_order", "INTEGER NOT NULL DEFAULT 0"), ("qr_code", "TEXT")] {
        ensure_column(db, "locations", column, kind)?;
    }
    for (column, kind) in [("due_date", "TEXT"), ("vendor", "TEXT"), ("sla_status", "TEXT"), ("cost", "REAL NOT NULL DEFAULT 0")] {
        ensure_column(db, "work_orders", column, kind)?;
    }
    for column in ["cause", "recommendation", "required_action"] {
        ensure_column(db, "findings", column, "TEXT")?;
    }
    for column in ["zone", "priority", "audit_type", "due_date"] {
        ensure_column(db, "schedules", column, "TEXT")?;
    }
    ensure_column(db, "equipment", "kind", "TEXT NOT NULL DEFAULT 'asset'")?;
    ensure_column(db, "equipment", "category", "TEXT")?;
    ensure_column(db, "categories", "department", "TEXT")?;
    for column in ["name", "description", "type", "operational_status", "code", "model", "serial_number", "brand", "location", "installation_date",
                   "temporary_relocation", "warranty_date", "calibration_date", "expiry_date"] {
        ensure_column(db, "equipment", column, "TEXT")?;
    }
    ensure_column(db, "equipment", "photos_data_id", "INTEGER REFERENCES value_sets(id)")?;
    for column in ["inverter_model", "motor_capacity", "source_file", "source_sheet"] {
        ensure_column(db, "equipment", column, "TEXT")?;
    }
    ensure_column(db, "equipment", "inspection_criteria_data_id", "INTEGER REFERENCES value_sets(id)")?;
    db.exec("UPDATE inspection_sessions SET audit_date = ? WHERE audit_date IS NULL OR audit_date = '' OR audit_date = 'Today'", params![today_date()])?;
    for row in db.rows("SELECT id, audit_date FROM audits WHERE audit_ref IS NULL OR audit_ref = ''", params![])? {
        db.exec("UPDATE audits SET audit_ref = ? WHERE id = ?", params![audit_ref(row.int("id"), &row.value("audit_date")), row.int("id")])?;
    }
    for (table, column, prefix) in [("work_orders", "work_order_ref", "WO"), ("findings", "finding_ref", "F")] {
        for row in db.rows(&format!("SELECT id, created_at FROM {table} WHERE {column} IS NULL OR {column} = ''"), params![])? {
            let created = row.opt_int("created_at").filter(|value| *value != 0).unwrap_or_else(clock::now_ms);
            let year = clock::local_from_ms(created).date.year;
            db.exec(&format!("UPDATE {table} SET {column} = ? WHERE id = ?"), params![format!("{prefix}-{year}-{:05}", row.int("id")), row.int("id")])?;
        }
    }
    db.exec("UPDATE inspection_sessions SET inspection_name = outlet || '_' || audit_date || '_' || id WHERE inspection_name IS NULL OR inspection_name = ''", params![])?;
    db.exec("UPDATE inspection_sessions SET inspection_name = outlet || '_' || audit_date || '_' || id WHERE inspection_name LIKE '%_Today_%'", params![])?;
    for row in db.rows("SELECT id, items_data_id FROM inspection_sessions", params![])? {
        let progress = inspection_progress(&load_or(db, row.get("items_data_id"), Value::array())?);
        db.exec("UPDATE inspection_sessions SET progress = ? WHERE id = ?", params![progress, row.int("id")])?;
    }
    db.exec(
        "UPDATE equipment SET inspection_criteria_data_id = ? WHERE inspection_criteria_data_id IS NULL OR inspection_criteria_data_id = ''",
        params![save_value(db, &strings(&DEFAULT_INSPECTION_CRITERIA))?],
    )?;
    for row in db.rows("SELECT id, outlet_code, name FROM locations WHERE qr_code IS NULL OR qr_code = ''", params![])? {
        db.exec("UPDATE locations SET qr_code = ? WHERE id = ?", params![location_qr_code(&row.str("outlet_code"), &row.str("name")), row.int("id")])?;
    }
    db.exec("UPDATE schedules SET zone = 'All Locations' WHERE zone IS NULL OR zone = '' OR zone = 'Unassigned'", params![])?;
    // A visit scheduled for one location before several could be chosen covers just that location.
    for row in db.rows("SELECT id, outlet, zone FROM schedules WHERE locations_data_id IS NULL", params![])? {
        let found = db.exists("SELECT 1 FROM locations WHERE outlet_code = ? AND name = ?", params![row.value("outlet"), row.value("zone")])?;
        let locations = if found { Value::Array(vec![row.value("zone")]) } else { Value::array() };
        db.exec("UPDATE schedules SET locations_data_id = ? WHERE id = ?", params![save_value(db, &locations)?, row.int("id")])?;
    }
    db.batch(
        "UPDATE equipment SET name = asset_id WHERE name IS NULL OR name = '';
         UPDATE equipment SET description = notes WHERE description IS NULL;
         UPDATE equipment SET type = equipment_type WHERE type IS NULL OR type = '';
         UPDATE equipment SET operational_status = health_status WHERE operational_status IS NULL OR operational_status = '';
         UPDATE equipment SET code = asset_id WHERE code IS NULL OR code = '';
         UPDATE equipment SET asset_id = code WHERE code IS NOT NULL AND code != '';
         UPDATE equipment SET qr_code = code WHERE code IS NOT NULL AND code != '';
         CREATE UNIQUE INDEX IF NOT EXISTS idx_equipment_code ON equipment(code);
         CREATE INDEX IF NOT EXISTS idx_equipment_outlet_name ON equipment(outlet, COALESCE(name, asset_id), id);
         CREATE INDEX IF NOT EXISTS idx_equipment_name ON equipment(COALESCE(name, asset_id), id);
         CREATE INDEX IF NOT EXISTS idx_equipment_outlet_location ON equipment(outlet, location);
         CREATE INDEX IF NOT EXISTS idx_audits_outlet_recent ON audits(business_unit, outlet, created_at DESC, id DESC);
         CREATE INDEX IF NOT EXISTS idx_sessions_completed ON inspection_sessions(status, audit_id);
         CREATE INDEX IF NOT EXISTS idx_sessions_updated ON inspection_sessions(updated_at DESC, id DESC);
         CREATE INDEX IF NOT EXISTS idx_findings_audit ON findings(audit_id);
         CREATE INDEX IF NOT EXISTS idx_locations_outlet ON locations(outlet_code, name);
         CREATE INDEX IF NOT EXISTS idx_zones_outlet ON zones(outlet_code, name);
         CREATE INDEX IF NOT EXISTS idx_users_email_lower ON users(lower(email));",
    )?;
    ensure_column(db, "comments", "system_generated", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(db, "users", "profile_photo_data_id", "INTEGER REFERENCES value_sets(id)")?;
    ensure_column(db, "users", "permission_overrides_data_id", "INTEGER REFERENCES value_sets(id)")?;
    ensure_column(db, "roles", "inspection_permissions_data_id", "INTEGER REFERENCES value_sets(id)")?;
    ensure_column(db, "users", "signature_image_data_id", "INTEGER REFERENCES value_sets(id)")?;
    db.exec("CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_lower ON users(lower(username)) WHERE username IS NOT NULL AND username != ''", params![])?;
    for (column, kind) in [("owner_user_id", "INTEGER"), ("schedule_id", "INTEGER"), ("audit_ref", "TEXT"), ("closed_at", "INTEGER"), ("closed_by", "TEXT")] {
        ensure_column(db, "inspection_sessions", column, kind)?;
    }
    db.exec("CREATE TABLE IF NOT EXISTS audit_reference_counters(year TEXT PRIMARY KEY, next_number INTEGER NOT NULL)", params![])?;
    for session in db.rows("SELECT id, audit_id, audit_date FROM inspection_sessions WHERE audit_ref IS NULL OR audit_ref = ''", params![])? {
        let mut reference = db.scalar("SELECT audit_ref FROM audits WHERE id = ?", params![session.value("audit_id")])?.text();
        // An audit number already on another inspection would stop the start; give a new one instead.
        if reference.is_empty() || db.exists("SELECT 1 FROM inspection_sessions WHERE audit_ref = ? AND id != ?", params![reference, session.int("id")])? {
            reference = allocate_reference(db, &session.value("audit_date"), "")?;
        }
        db.exec("UPDATE inspection_sessions SET audit_ref = ? WHERE id = ?", params![reference, session.int("id")])?;
    }
    db.batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_session_audit_reference ON inspection_sessions(audit_ref) WHERE audit_ref IS NOT NULL AND audit_ref != '';
         CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_schedule ON inspection_sessions(schedule_id) WHERE schedule_id IS NOT NULL;",
    )?;
    ensure_column(db, "notifications", "recipient_user_id", "INTEGER")?;
    db.batch(
        "CREATE INDEX IF NOT EXISTS idx_notifications_recipient ON notifications(recipient_user_id, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_notifications_related ON notifications(related_type, related_id);
         UPDATE equipment SET location = zone WHERE location IS NULL OR location = '';
         UPDATE equipment SET installation_date = last_checked WHERE installation_date IS NULL OR installation_date = '';
         DELETE FROM audits WHERE auditor = 'Sample Auditor';",
    )?;
    if db.count("SELECT COUNT(*) FROM schedules", params![])? == 0 {
        seed_schedules(db)?;
    }
    if db.count("SELECT COUNT(*) FROM equipment", params![])? == 0 {
        seed_equipment(db)?;
    }
    normalize_loudspeaker_outlets(db)?;
    seed_setup_records(db)?;
    seed_locations(db)?;
    seed_zones(db)?;
    seed_roles(db)?;
    retire_corrective_actions_page(db)?;
    seed_operation_roles(db)?;
    outlets_on_people(db)?;
    split_history_from_findings(db)?;
    crate::inspections::repair_item_details(db)?;
    convert_audit_codes(db)?;
    adopt_variable_assets(db)?;
    grant_existing_access(db)?;
    // Earlier audits, requests, and work orders, as far as their records tell who and when.
    activity::backfill(db)?;
    // Work orders no longer have Completed and Verified steps: work that had reached them is done.
    for table in ["work_orders", "findings"] {
        db.exec(
            &format!("UPDATE {table} SET status = 'Closed', closed_at = COALESCE(NULLIF(closed_at, ''), NULLIF(verified_at, ''), NULLIF(completion_date, ''), date('now')) WHERE status IN ('Completed', 'Verified')"),
            params![],
        )?;
    }
    seed_priority_levels(db)?;
    seed_audit_types(db)?;
    ensure_column(db, "audit_types", "style", "TEXT NOT NULL DEFAULT 'Detailed'")?;
    seed_audit_styles(db)?;
    seed_settings(db)?;
    seed_users(db)?;
    // Super accounts belong to the control database, never to an organization.
    control::adopt_organization_supers(db)?;
    let department = first_department(db)?;
    if !department.is_empty() {
        db.exec("UPDATE users SET department = ? WHERE department IS NULL OR department = ''", params![department])?;
    }
    if db.exists("SELECT 1 FROM users WHERE password_hash IS NULL OR password_hash = ''", params![])? {
        db.exec("UPDATE users SET password_hash = ? WHERE password_hash IS NULL OR password_hash = ''", params![hash_password(DEFAULT_PASSWORD)])?;
    }
    db.batch(
        "CREATE INDEX IF NOT EXISTS idx_login_activity_user_time ON user_login_activity(user_id, logged_at DESC, id DESC);
         CREATE TABLE IF NOT EXISTS user_navigation(user_id INTEGER NOT NULL REFERENCES users(id), page_id TEXT NOT NULL, position INTEGER NOT NULL, PRIMARY KEY(user_id,page_id), UNIQUE(user_id,position));
         CREATE TABLE IF NOT EXISTS auth_sessions(token_hash TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id), expires_at REAL NOT NULL);",
    )?;
    db.exec("DELETE FROM auth_sessions WHERE expires_at < ?", params![clock::now_secs()])?;
    db.batch(
        "CREATE TABLE IF NOT EXISTS password_reset_requests(user_id INTEGER PRIMARY KEY REFERENCES users(id), requested_at INTEGER NOT NULL, resolved_at INTEGER);
         CREATE TABLE IF NOT EXISTS due_notification_events(user_id INTEGER NOT NULL, work_order_id INTEGER NOT NULL, kind TEXT NOT NULL, day TEXT NOT NULL, due_date TEXT NOT NULL, PRIMARY KEY(user_id,work_order_id,kind,day,due_date));",
    )?;
    db::install_reference_cleanup(db)
}

// ---- Records a new database starts with ----

fn seed_schedules(db: &Db) -> R<()> {
    let now = clock::now_ms();
    let rows = [
        ("Mini Studio", "MST", "Server Room", "2026-09-01", "Ah Fai", "Room checklist before peak hours"),
        ("Mini Studio", "MQS", "Entrance", "2026-09-02", "Ah Fan", "Photo evidence follow-up"),
        ("Loudspeaker", "STP", "Display Zone", "2026-09-01", "Gavin", "Speaker display readiness"),
    ];
    for (index, (unit, outlet, zone, date, auditor, remarks)) in rows.iter().enumerate() {
        db.exec(
            "INSERT INTO schedules (business_unit, outlet, zone, scheduled_date, auditor, remarks, status, created_at) VALUES (?, ?, ?, ?, ?, ?, 'Pending', ?)",
            params![*unit, *outlet, *zone, *date, *auditor, *remarks, now - index as i64 * 100_000],
        )?;
    }
    Ok(())
}

fn seed_equipment(db: &Db) -> R<()> {
    let now = clock::now_ms();
    let rows = [
        ("EQ-MST-UPS-001", "QR-MST-UPS-001", "Mini Studio", "MST", "Server Room", "UPS", "Monitor", "2026-08-20", 1, "Battery age needs budget review"),
        ("EQ-MQS-CCTV-002", "QR-MQS-CCTV-002", "Mini Studio", "MQS", "Entrance", "CCTV", "Normal", "2026-08-18", 0, "Camera view clear"),
        ("EQ-STP-SPK-001", "QR-STP-SPK-001", "Loudspeaker", "STP", "Display Zone", "Speaker Display", "Normal", "2026-08-22", 0, "Demo unit working"),
        ("EQ-MDP-SRV-001", "QR-MDP-SRV-001", "Mini Studio", "MDP", "Server Room", "Server", "Replace", "2026-08-15", 1, "Old hard disk health warning"),
    ];
    // Without a code these rows are skipped by OR IGNORE wherever code is required (every database
    // created since codes were introduced); only very old databases took them.
    for (asset, qr, unit, outlet, zone, kind, health, checked, flag, notes) in rows {
        db.exec(
            "INSERT OR IGNORE INTO equipment (asset_id, qr_code, business_unit, outlet, zone, equipment_type, health_status, last_checked, replacement_flag, notes, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![asset, qr, unit, outlet, zone, kind, health, checked, flag as i64, notes, now],
        )?;
    }
    Ok(())
}

fn normalize_loudspeaker_outlets(db: &Db) -> R<()> {
    for (old, new) in [("MAM", "STP"), ("MQS", "SBA"), ("MDP", "TPG"), ("MST", "AQP")] {
        for table in ["audits", "schedules", "work_orders", "equipment"] {
            db.exec(&format!("UPDATE {table} SET outlet = ? WHERE business_unit = 'Loudspeaker' AND outlet = ?"), params![new, old])?;
        }
    }
    Ok(())
}

fn seed_setup_records(db: &Db) -> R<()> {
    let now = clock::now_ms();
    if db.count("SELECT COUNT(*) FROM departments", params![])? == 0 {
        for (code, description, responsibilities) in [
            ("SSD", "SSD department", "Store support and display issue ownership"),
            ("FMS", "FMS department", "Facilities, fixtures, maintenance, and site readiness"),
            ("AVC", "AVC department", "Audio visual, cabling, playback, and demo equipment"),
            ("CLD", "CLD department", "Cloud, system access, and connected workflow support"),
            ("MD", "MD department", "Management decisions, approvals, and escalation ownership"),
        ] {
            db.exec("INSERT INTO departments (code, description, responsibilities, created_at) VALUES (?, ?, ?, ?)", params![code, description, responsibilities, now])?;
        }
    }
    if db.count("SELECT COUNT(*) FROM outlets", params![])? == 0 {
        for code in ["MAM", "MQS", "MDP", "MST"].iter().chain(LOUDSPEAKER_OUTLETS.iter()) {
            db.exec("INSERT INTO outlets (code, location, description, created_at) VALUES (?, '', 'Ottotree outlet', ?)", params![*code, now])?;
        }
    }
    Ok(())
}

fn seed_locations(db: &Db) -> R<()> {
    let now = clock::now_ms();
    for outlet in db.strings("SELECT code FROM outlets ORDER BY code", params![])? {
        for (name, size) in [("R-01", "Room 01"), ("R-02", "Room 02"), ("R-03", "Room 03")] {
            db.exec("INSERT OR IGNORE INTO locations (outlet_code, name, size, created_at) VALUES (?, ?, ?, ?)", params![outlet, name, size, now])?;
        }
    }
    Ok(())
}

fn seed_zones(db: &Db) -> R<()> {
    let now = clock::now_ms();
    for outlet in db.strings("SELECT code FROM outlets ORDER BY code", params![])? {
        add_locations_to_default_zone(db, &outlet, now)?;
    }
    Ok(())
}

/// Starter accounts for a new database. Their passwords are published, so each must be changed at first sign-in.
fn seed_users(db: &Db) -> R<()> {
    // Only a database without people gets them. Once it has people, their accounts are theirs to
    // change or remove.
    if db.exists("SELECT 1 FROM users LIMIT 1", params![])? {
        return Ok(());
    }
    let now = clock::now_ms();
    let rows = [
        ("Ottotree System Administrator", "admin", ADMIN_ROLE, "admin@ottotree.local", "SSD", "System Administrator", "Ottotree system administrator staff", DEFAULT_PASSWORD),
        ("Gavin", "gavin", "System Support Executive", "gavin@audit.local", "SSD", "System Support Executive", "System support executive", "123456"),
        ("Jacky", "jacky", "System Support Officer", "jacky@audit.local", "SSD", "System Support Officer", "System support officer", "123456"),
        ("Fan", "fan", "Facilities Officer", "fan@audit.local", "AVC", "Facilities Officer", "Facilities officer", "123456"),
        ("Hui", "hui", "Facilities Executive", "hui@audit.local", "FMS", "Facilities Executive", "Facilities executive", "123456"),
    ];
    for (name, username, role, email, department, title, responsibilities, password) in rows {
        db.exec(
            "INSERT INTO users (name, username, role, email, department, password_hash, active, reset_required, title, responsibilities, created_at) VALUES (?, ?, ?, ?, ?, ?, 1, 1, ?, ?, ?)",
            params![name, username, role, email, department, hash_password(password), title, responsibilities, now],
        )?;
    }
    Ok(())
}

fn seed_roles(db: &Db) -> R<()> {
    let now = clock::now_ms();
    let admin_permissions: Vec<&str> = APP_TABS.iter().map(|(id, _)| *id).filter(|id| *id != "settings").collect();
    let roles: [(&str, &str, Vec<&str>); 8] = [
        (ADMIN_ROLE, "Company administrator access", admin_permissions),
        ("Auditor", "Field inspection and verification access", vec!["today", "inspections", "equipment", "reports", "history", "findings", "work-orders"]),
        ("Department/PIC", "Corrective action ownership", vec!["today", "history", "findings", "work-orders", "notifications", "reports"]),
        ("Management", "Management reporting access", vec!["reports", "history", "findings", "notifications"]),
        ("System Support Executive", "System support executive access", vec!["today", "equipment", "history", "findings", "work-orders", "notifications"]),
        ("System Support Officer", "System support officer access", vec!["today", "equipment", "history", "findings", "work-orders", "notifications"]),
        ("Facilities Officer", "Facilities officer access", vec!["today", "inspections", "equipment", "history", "findings", "work-orders", "notifications"]),
        ("Facilities Executive", "Facilities executive access", vec!["today", "inspections", "equipment", "reports", "history", "findings", "work-orders", "notifications"]),
    ];
    // Default roles are for a new database only; once roles exist, removing one keeps it removed.
    if !db.exists("SELECT 1 FROM roles LIMIT 1", params![])? {
        for (name, description, permissions) in &roles {
            db.exec(
                "INSERT OR IGNORE INTO roles (name, description, permissions_data_id, protected, created_at) VALUES (?, ?, ?, 0, ?)",
                params![*name, *description, save_value(db, &strings(permissions))?, now],
            )?;
        }
    }
    if let Some(admin) = db.row("SELECT id, permissions_data_id FROM roles WHERE name = ?", params![ADMIN_ROLE])? {
        let stored = load_or(db, admin.get("permissions_data_id"), Value::array())?;
        let mut permissions: Vec<String> = stored.items().iter().map(Value::py_str).collect();
        permissions.push("roles".into());
        permissions.sort();
        permissions.dedup();
        db.exec("UPDATE roles SET permissions_data_id = ? WHERE id = ?", params![save_value(db, &strings(&permissions))?, admin.int("id")])?;
    }
    let auditor = db.row("SELECT permissions_data_id FROM roles WHERE name = 'Auditor'", params![])?;
    for (role, capabilities) in [
        ("Super", &["auditor", "verifier", "acknowledger"][..]),
        ("Admin", &["auditor", "verifier", "acknowledger"][..]),
        ("Auditor", &["auditor", "verifier"][..]),
        ("Department/PIC", &["acknowledger"][..]),
        ("Management", &["verifier", "acknowledger"][..]),
    ] {
        if db.exists("SELECT 1 FROM roles WHERE name = ? AND inspection_permissions_data_id IS NULL", params![role])? {
            db.exec("UPDATE roles SET inspection_permissions_data_id = ? WHERE name = ? AND inspection_permissions_data_id IS NULL", params![save_value(db, &strings(capabilities))?, role])?;
        }
    }
    // Upgrade only the original defaults, preserving administrator-customized roles.
    if let Some(auditor) = auditor {
        let mut pages: Vec<String> = load_value(db, auditor.get("permissions_data_id"))?.items().iter().map(Value::py_str).collect();
        pages.sort();
        pages.dedup();
        if pages == ["equipment", "inspections", "reports", "today"] {
            db.exec(
                "UPDATE roles SET permissions_data_id = ? WHERE name = 'Auditor'",
                params![save_value(db, &strings(&["today", "inspections", "equipment", "reports", "history", "findings", "work-orders"]))?],
            )?;
        }
    }
    Ok(())
}

/// Corrective Actions was a second view of Work Orders with the same access; fold its permission into Work Orders.
fn retire_corrective_actions_page(db: &Db) -> R<()> {
    let merged = |permissions: &Value| -> Value {
        Value::Array(crate::common::unique(permissions.items().iter().map(|page| if page.as_str() == Some("corrective-actions") { Value::str("work-orders") } else { page.clone() })))
    };
    let has_page = |permissions: &Value| permissions.items().iter().any(|page| page.as_str() == Some("corrective-actions"));
    for role in db.rows("SELECT id, permissions_data_id FROM roles WHERE permissions_data_id IS NOT NULL", params![])? {
        let permissions = load_value(db, role.get("permissions_data_id"))?;
        if has_page(&permissions) {
            db.exec("UPDATE roles SET permissions_data_id = ? WHERE id = ?", params![save_value(db, &merged(&permissions))?, role.int("id")])?;
        }
    }
    for user in db.rows("SELECT id, permission_overrides_data_id FROM users WHERE permission_overrides_data_id IS NOT NULL", params![])? {
        let mut overrides = load_value(db, user.get("permission_overrides_data_id"))?;
        if !overrides.truthy() {
            overrides = Value::object();
        }
        if has_page(overrides.at("permissions")) {
            let pages = merged(overrides.at("permissions"));
            overrides.set("permissions", pages);
            db.exec("UPDATE users SET permission_overrides_data_id = ? WHERE id = ?", params![save_value(db, &overrides)?, user.int("id")])?;
        }
    }
    db.exec("DELETE FROM app_settings WHERE key = 'system.correctiveActionsEnabled'", params![])?;
    Ok(())
}

fn seed_priority_levels(db: &Db) -> R<()> {
    let now = clock::now_ms();
    for (name, classification, days) in config::DEFAULT_PRIORITY_LEVELS {
        db.exec("INSERT OR IGNORE INTO priority_levels (name, classification, due_days, active, created_at) VALUES (?, ?, ?, 1, ?)", params![name, classification, days, now])?;
    }
    Ok(())
}

fn seed_audit_types(db: &Db) -> R<()> {
    let now = clock::now_ms();
    for (name, description, active) in config::DEFAULT_AUDIT_TYPES {
        db.exec("INSERT OR IGNORE INTO audit_types (name, description, active, created_at) VALUES (?, ?, ?, ?)", params![name, description, active as i64, now])?;
    }
    Ok(())
}

/// Once: the two grading styles become audit types of their own, Detailed and Casual.
fn seed_audit_styles(db: &Db) -> R<()> {
    if flag_set(db, "system.auditStylesSeeded")? {
        return Ok(());
    }
    let now = clock::now_ms();
    for (name, description) in [("Detailed", "Every asset is checked and graded on its own"), ("Casual", "Assets with the same name in a location are checked and graded together")] {
        db.exec("INSERT OR IGNORE INTO audit_types (name, description, active, created_at, style) VALUES (?, ?, 1, ?, ?)", params![name, description, now, name])?;
    }
    set_flag(db, "system.auditStylesSeeded")
}

fn seed_settings(db: &Db) -> R<()> {
    let company = read_setting(db, "report.companyName", Value::str(config::report_default("companyName")))?;
    let mut theme = config::default_theme_settings();
    if company.py_str().trim().to_lowercase() == "ottotree" {
        theme.set("preset", "ottotree");
        theme.set("font", "noto-sans-sc");
    }
    let mut settings: Vec<(String, Value)> = Vec::new();
    settings.extend(theme.entries().iter().map(|(key, value)| (format!("theme.{key}"), value.clone())));
    settings.extend(config::default_scoring_settings().entries().iter().map(|(key, value)| (format!("scoring.{key}"), value.clone())));
    settings.extend(config::DEFAULT_REPORT_SETTINGS.iter().map(|(key, value)| (format!("report.{key}"), Value::str(*value))));
    settings.extend(config::default_system_settings().entries().iter().map(|(key, value)| (format!("system.{key}"), value.clone())));
    for (key, value) in settings {
        if !flag_set(db, &key)? {
            db.exec("INSERT INTO app_settings(key, value_data_id) VALUES (?, ?)", params![key, save_value(db, &value)?])?;
        }
    }
    for (key, previous) in [("reportHeading", "monthly audit report"), ("companyName", "Audit App"), ("logoText", "AUDIT"), ("appTitle", "Audit App"),
                            ("appSubtitle", "Facilities audit workspace"), ("businessUnitLabel", "Facilities"), ("loginTitle", "Audit App")] {
        let name = format!("report.{key}");
        if let Some(row) = db.row("SELECT value_data_id FROM app_settings WHERE key = ?", params![name])? {
            if load_value(db, row.get("value_data_id"))?.as_str() == Some(previous) {
                db.exec("UPDATE app_settings SET value_data_id = ? WHERE key = ?", params![save_value(db, &Value::str(config::report_default(key)))?, name])?;
            }
        }
    }
    Ok(())
}

const OPERATION_ROLES: [(&str, &str, &[&str], &[&str]); 4] = [
    ("Regional Manager", "Oversees the outlets of a region", &["today", "inspections", "history", "findings", "work-orders", "equipment", "reports", "notifications"], &["verifier", "acknowledger"]),
    ("Operation Manager", "Runs one outlet", &["today", "inspections", "history", "findings", "work-orders", "equipment", "reports", "notifications"], &["auditor", "acknowledger"]),
    ("PIC", "Person in charge at one outlet", &["today", "history", "findings", "work-orders", "notifications"], &["acknowledger"]),
    ("Captain", "Team lead at one outlet", &["today", "inspections", "history", "findings", "equipment", "work-orders", "notifications"], &["auditor"]),
];

/// The Operation department and its roles. Applied once per database: an administrator may later change these roles freely.
fn seed_operation_roles(db: &Db) -> R<()> {
    if flag_set(db, "system.operationRolesSeeded")? {
        return Ok(());
    }
    let now = clock::now_ms();
    db.exec(
        "INSERT OR IGNORE INTO departments (code, description, responsibilities, created_at) VALUES (?, ?, ?, ?)",
        params!["Operation", "Operation department", "Outlet operations: regional and outlet managers, PICs, and captains", now],
    )?;
    for (name, description, permissions, capabilities) in OPERATION_ROLES {
        if db.exists("SELECT 1 FROM roles WHERE name = ?", params![name])? {
            // PIC and Captain may already exist with their own access; only place them in Operation.
            db.exec("UPDATE roles SET department = 'Operation' WHERE name = ?", params![name])?;
        } else {
            db.exec(
                "INSERT INTO roles (name, description, permissions_data_id, inspection_permissions_data_id, protected, created_at, department) VALUES (?, ?, ?, ?, 0, ?, 'Operation')",
                params![name, description, save_value(db, &strings(permissions))?, save_value(db, &strings(capabilities))?, now],
            )?;
        }
    }
    set_flag(db, "system.operationRolesSeeded")
}

/// Once: outlet access belongs to each person, with every outlet the default.
fn outlets_on_people(db: &Db) -> R<()> {
    if flag_set(db, "system.outletsOnPeople")? {
        return Ok(());
    }
    if db.columns("roles")?.iter().any(|column| column == "outlet_scope") {
        db.exec("UPDATE users SET outlets_data_id = NULL WHERE role NOT IN (SELECT name FROM roles WHERE outlet_scope IN ('one', 'several'))", params![])?;
    }
    set_flag(db, "system.outletsOnPeople")
}

/// Once: History became its own page beside Findings; whoever could open both keeps both.
fn split_history_from_findings(db: &Db) -> R<()> {
    if flag_set(db, "system.historySplit")? {
        return Ok(());
    }
    let needs = |pages: &Value| crate::permissions::list_has(pages, "findings") && !crate::permissions::list_has(pages, "history");
    for row in db.rows("SELECT id, permissions_data_id FROM roles WHERE permissions_data_id IS NOT NULL", params![])? {
        let mut pages = load_value(db, row.get("permissions_data_id"))?;
        if needs(&pages) {
            pages.push("history");
            db.exec("UPDATE roles SET permissions_data_id = ? WHERE id = ?", params![save_value(db, &pages)?, row.int("id")])?;
        }
    }
    for row in db.rows("SELECT id, permission_overrides_data_id FROM users WHERE permission_overrides_data_id IS NOT NULL", params![])? {
        let overrides = load_value(db, row.get("permission_overrides_data_id"))?;
        let overrides = if overrides.truthy() { overrides } else { Value::object() };
        let mut pages = overrides.at("permissions").clone();
        if !pages.truthy() {
            pages = Value::array();
        }
        if needs(&pages) {
            pages.push("history");
            let mut updated = overrides.clone();
            updated.set("permissions", pages);
            db.exec("UPDATE users SET permission_overrides_data_id = ? WHERE id = ?", params![save_value(db, &updated)?, row.int("id")])?;
        }
    }
    set_flag(db, "system.historySplit")
}

const VARIABLE_SAMPLES: [(&str, [&str; 2]); 15] = [
    ("AV Equipment", ["Projector screen", "Wireless microphone"]), ("COM Equipment", ["Desk telephone", "PABX unit"]),
    ("Facility", ["Door closer", "Signage board"]), ("F&B Equipment", ["Ice machine", "Bottle cooler"]),
    ("Electrical", ["Power socket", "Distribution board"]), ("Plumbing", ["Wash basin tap", "Floor trap"]),
    ("Air Conditioning", ["Wall-mounted air conditioner", "Exhaust fan"]), ("Lighting", ["LED panel light", "Emergency light"]),
    ("Furniture", ["Sofa", "Coffee table"]), ("Building", ["Glass partition", "Ceiling board"]),
    ("Safety", ["Fire extinguisher", "Smoke detector"]), ("Cleanliness", ["Waste bin", "Hand soap dispenser"]),
    ("IT / Network", ["Wi-Fi access point", "Network switch"]), ("KTV Equipment", ["Song selection tablet", "Karaoke mixer"]),
    ("Others", ["Umbrella stand", "Notice board"]),
];

/// Once: Fixtures & Finishes become Variable Assets and categories are retired, with a few
/// sample variable assets of each type at the test outlet (or the first outlet with locations).
fn adopt_variable_assets(db: &Db) -> R<()> {
    if flag_set(db, "system.variableAssets")? {
        return Ok(());
    }
    let names = if db.table_exists("categories")? { db.strings("SELECT name FROM categories WHERE active = 1 ORDER BY sequence, name", params![])? } else { Vec::new() };
    let types: Vec<String> = crate::common::unique(if names.is_empty() { VARIABLE_ASSET_TYPES.iter().map(|name| name.to_string()).collect() } else { names });
    db.exec("INSERT OR REPLACE INTO app_settings (key, value_data_id) VALUES ('assets.variableTypes', ?)", params![save_value(db, &strings(&types))?])?;
    db.batch(
        "UPDATE equipment SET type = 'Building', equipment_type = 'Building' WHERE kind = 'fixture' AND COALESCE(type, '') IN ('', 'Fixture & Finish');
         UPDATE equipment SET category = '' WHERE COALESCE(category, '') != '';",
    )?;
    db.exec("INSERT OR REPLACE INTO app_settings (key, value_data_id) VALUES ('scoring.weighting', ?)", params![save_value(db, &Value::str("Equal"))?])?;
    let mut outlet = db.scalar("SELECT outlet_code FROM locations WHERE outlet_code = 'TEST' LIMIT 1", params![])?;
    if outlet.is_null() {
        outlet = db.scalar("SELECT outlet_code FROM locations ORDER BY outlet_code LIMIT 1", params![])?;
    }
    if !outlet.is_null() {
        let outlet = outlet.text();
        let locations = db.strings("SELECT name FROM locations WHERE outlet_code = ? ORDER BY name", params![outlet])?;
        let now = clock::now_ms();
        let mut index = 0usize;
        for item_type in &types {
            let fallback = [format!("{item_type} item")];
            let samples: Vec<String> = match VARIABLE_SAMPLES.iter().find(|(name, _)| name == item_type) {
                Some((_, names)) => names.iter().map(|name| name.to_string()).collect(),
                None => fallback.to_vec(),
            };
            for name in samples {
                let location = &locations[index % locations.len()];
                index += 1;
                let record_id = db.insert(
                    "INSERT INTO equipment (asset_id, qr_code, code, business_unit, outlet, zone, location, equipment_type, type, health_status, operational_status, last_checked, kind, name, category, description, created_at, inspection_criteria_data_id) VALUES ('pending', 'pending', ?, 'Ottotree', ?, ?, ?, ?, ?, 'Operational', 'Operational', 'Today', 'fixture', ?, '', ?, ?, ?)",
                    params![
                        format!("pending-{now}-{index}"),
                        outlet,
                        location,
                        location,
                        item_type,
                        item_type,
                        name,
                        format!("Sample {} item", item_type.to_lowercase()),
                        now,
                        save_value(db, &strings(&DEFAULT_FIXTURE_CRITERIA))?
                    ],
                )?;
                let code = format!("VAR-{record_id:05}");
                db.exec("UPDATE equipment SET asset_id = ?, qr_code = ?, code = ? WHERE id = ?", params![code, code, code, record_id])?;
            }
        }
    }
    set_flag(db, "system.variableAssets")
}

/// Once: everyone keeps what they can do today. A role (or a person's own permissions) that opens
/// a page may add, edit, delete, and approve the records on it.
fn grant_existing_access(db: &Db) -> R<()> {
    if flag_set(db, "system.changePermissions")? {
        return Ok(());
    }
    let actions_for = |pages: &[String]| -> Value {
        let mut actions = Vec::new();
        for (record, _, page, _) in CHANGE_RECORDS {
            if pages.iter().any(|name| name == page) {
                for level in ["manage", "approve"] {
                    actions.push(format!("{record}.{level}"));
                }
            }
        }
        strings(&actions)
    };
    for row in db.rows("SELECT id, name, permissions_data_id FROM roles", params![])? {
        let stored = if row.get("permissions_data_id").truthy() { load_value(db, row.get("permissions_data_id"))? } else { Value::array() };
        let mut pages: Vec<String> = stored.items().iter().map(Value::py_str).collect();
        // Users and roles were changed only by administrators before.
        if row.str("name") != ADMIN_ROLE {
            pages.retain(|page| page != "users" && page != "roles");
        } else {
            pages.extend(["users", "roles", "departments", "outlets", "equipment"].iter().map(|page| page.to_string()));
        }
        db.exec("UPDATE roles SET action_permissions_data_id = ? WHERE id = ?", params![save_value(db, &actions_for(&pages))?, row.int("id")])?;
    }
    for row in db.rows("SELECT id, role, permission_overrides_data_id FROM users WHERE permission_overrides_data_id IS NOT NULL", params![])? {
        let overrides = load_value(db, row.get("permission_overrides_data_id"))?;
        let mut overrides = if overrides.truthy() { overrides } else { Value::object() };
        let admin = row.str("role") == ADMIN_ROLE;
        let pages: Vec<String> = overrides.at("permissions").items().iter().map(Value::py_str).filter(|page| admin || (page != "users" && page != "roles")).collect();
        overrides.set("actions", actions_for(&pages));
        db.exec("UPDATE users SET permission_overrides_data_id = ? WHERE id = ?", params![save_value(db, &overrides)?, row.int("id")])?;
    }
    set_flag(db, "system.changePermissions")
}
