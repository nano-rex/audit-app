//! The Super account's own database, kept apart from every organization's database.
//!
//! Super accounts, their sessions, sign-in history, and page order live here. Organization
//! databases hold only their own people. A Super account is presented to the rest of the
//! application with a negative id, so it can never be mistaken for an organization user.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::clock;
use crate::config::{self, SUPER_ROLE};
use crate::db::{self, finish, Db};
use crate::error::{bad, R};
use crate::json::{obj, strings, Value};
use crate::params;
use crate::permissions::{action_permissions, INSPECTION_PERMISSIONS};
use crate::sqlite::Row;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS super_users(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    username TEXT NOT NULL,
    email TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    reset_required INTEGER NOT NULL DEFAULT 0,
    last_login_at TEXT,
    login_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_super_users_username ON super_users(lower(username));
CREATE UNIQUE INDEX IF NOT EXISTS idx_super_users_email ON super_users(lower(email));
CREATE TABLE IF NOT EXISTS super_sessions(
    token_hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES super_users(id) ON DELETE CASCADE,
    expires_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS super_login_activity(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES super_users(id) ON DELETE CASCADE,
    logged_at TEXT NOT NULL,
    remember_me INTEGER NOT NULL DEFAULT 0,
    user_agent TEXT,
    organization TEXT
);
CREATE TABLE IF NOT EXISTS super_navigation(
    user_id INTEGER NOT NULL REFERENCES super_users(id) ON DELETE CASCADE,
    page_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY(user_id, page_id)
);
";

/// AUDIT_CONTROL_DB lets several instances share one set of Super accounts.
pub fn control_path() -> PathBuf {
    match std::env::var("AUDIT_CONTROL_DB") {
        Ok(path) if !path.trim().is_empty() => {
            let path = PathBuf::from(path.trim());
            if path.is_absolute() {
                path
            } else {
                std::env::current_dir().map(|dir| dir.join(&path)).unwrap_or(path)
            }
        }
        _ => config::data_dir().join("control").join("control.db"),
    }
}

fn ready() -> &'static Mutex<HashSet<PathBuf>> {
    static READY: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    READY.get_or_init(|| Mutex::new(HashSet::new()))
}

fn connect_control() -> R<Db> {
    let path = control_path();
    let database = db::open(&path)?;
    let mut prepared = ready().lock().unwrap_or_else(|poison| poison.into_inner());
    if !prepared.contains(&path) {
        database.rows("PRAGMA journal_mode=WAL", params![])?;
        database.batch(SCHEMA)?;
        prepared.insert(path);
    }
    Ok(database)
}

/// Work on the control database: committed on success, rolled back on failure.
fn with_control<T>(work: impl FnOnce(&Db) -> R<T>) -> R<T> {
    let database = connect_control()?;
    let result = work(&database);
    finish(&database, result)
}

pub fn public_id(super_id: i64) -> i64 {
    -super_id
}

/// The control-database id of a Super account presented to the application, else None.
pub fn control_id(user: &Value) -> Option<i64> {
    if user.at("accountScope").as_str() == Some("control") {
        user.at("id").as_i64().map(|id| -id)
    } else {
        None
    }
}

fn public_super(row: &Row, database: &Db) -> R<Value> {
    let navigation = database.strings("SELECT page_id FROM super_navigation WHERE user_id = ? ORDER BY position", params![row.int("id")])?;
    Ok(obj([
        ("navigationOrder", strings(&navigation)),
        ("id", Value::Int(public_id(row.int("id")))),
        ("name", row.value("name")),
        ("username", row.value("username")),
        ("role", SUPER_ROLE.into()),
        ("email", row.value("email")),
        ("department", "".into()),
        ("title", "Super account".into()),
        ("responsibilities", "".into()),
        ("active", Value::Bool(row.int("active") != 0)),
        ("lastLoginAt", Value::str(row.str("last_login_at"))),
        ("resetRequired", Value::Bool(row.int("reset_required") != 0)),
        ("permissions", config::all_tabs()),
        ("inspectionPermissions", strings(&INSPECTION_PERMISSIONS)),
        ("permissionOverrides", Value::Null),
        ("permissionSource", "role".into()),
        ("actions", strings(&action_permissions())),
        ("outletScope", "all".into()),
        ("outlets", Value::array()),
        ("profilePhoto", Value::object()),
        ("signatureImage", Value::object()),
        // Marks an account that lives in the control database rather than the organization's.
        ("accountScope", "control".into()),
    ]))
}

pub fn find_super(identifier: &str) -> R<Option<Row>> {
    let identifier = identifier.trim().to_lowercase();
    if identifier.is_empty() {
        return Ok(None);
    }
    with_control(|database| database.row("SELECT * FROM super_users WHERE lower(email) = ? OR lower(username) = ?", params![identifier, identifier]))
}

/// True when an organization account would take a Super account's username or email.
pub fn is_reserved_identifier(identifiers: &[&str]) -> R<bool> {
    let values: Vec<String> = identifiers.iter().filter(|value| !value.is_empty()).map(|value| value.trim().to_lowercase()).collect();
    if values.is_empty() {
        return Ok(false);
    }
    let marks = db::marks(values.len());
    let mut params: Vec<&dyn crate::sqlite::ToSql> = values.iter().map(|value| value as &dyn crate::sqlite::ToSql).collect();
    params.extend(values.iter().map(|value| value as &dyn crate::sqlite::ToSql));
    with_control(|database| database.exists(&format!("SELECT 1 FROM super_users WHERE lower(email) IN ({marks}) OR lower(username) IN ({marks})"), &params))
}

pub fn user_for_session(token_hash: &str) -> R<Option<Value>> {
    with_control(|database| {
        let Some(session) = database.row("SELECT user_id, expires_at FROM super_sessions WHERE token_hash = ?", params![token_hash])? else {
            return Ok(None);
        };
        if session.get("expires_at").as_f64().unwrap_or(0.0) < clock::now_secs() {
            database.exec("DELETE FROM super_sessions WHERE token_hash = ?", params![token_hash])?;
            return Ok(None);
        }
        match database.row("SELECT * FROM super_users WHERE id = ? AND active = 1", params![session.int("user_id")])? {
            Some(row) => Ok(Some(public_super(&row, database)?)),
            None => Ok(None),
        }
    })
}

pub fn start_session(row: &Row, token_hash: &str, expires_at: f64, remember: bool, user_agent: &str) -> R<Value> {
    let now = clock::local_now().date_time_text();
    let organization = config::db_path().file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    with_control(|database| {
        let id = row.int("id");
        database.exec("INSERT OR REPLACE INTO super_sessions(token_hash, user_id, expires_at) VALUES (?, ?, ?)", params![token_hash, id, expires_at])?;
        database.exec("UPDATE super_users SET last_login_at = ?, login_count = login_count + 1 WHERE id = ?", params![now, id])?;
        database.exec(
            "INSERT INTO super_login_activity(user_id, logged_at, remember_me, user_agent, organization) VALUES (?, ?, ?, ?, ?)",
            params![id, now, remember as i64, user_agent, organization],
        )?;
        let refreshed = database.row("SELECT * FROM super_users WHERE id = ?", params![id])?.ok_or_else(|| bad("Super account not found"))?;
        public_super(&refreshed, database)
    })
}

pub fn end_session(token_hash: &str) -> R<()> {
    with_control(|database| database.exec("DELETE FROM super_sessions WHERE token_hash = ?", params![token_hash]).map(|_| ()))
}

pub fn password_hash(super_id: i64) -> R<String> {
    with_control(|database| Ok(database.scalar("SELECT password_hash FROM super_users WHERE id = ?", params![super_id])?.text()))
}

pub fn change_password(super_id: i64, new_hash: &str, keep_token_hash: &str) -> R<()> {
    with_control(|database| {
        database.exec("UPDATE super_users SET password_hash = ?, reset_required = 0 WHERE id = ?", params![new_hash, super_id])?;
        database.exec("DELETE FROM super_sessions WHERE user_id = ? AND token_hash != ?", params![super_id, keep_token_hash])?;
        Ok(())
    })
}

pub fn update_profile(super_id: i64, name: &str, username: &str, email: &str) -> R<Value> {
    with_control(|database| {
        for (column, value) in [("email", email), ("username", username)] {
            if database.exists(&format!("SELECT 1 FROM super_users WHERE lower({column}) = ? AND id != ?"), params![value, super_id])? {
                return Err(crate::error::status(409, format!("That {column} belongs to another Super account")));
            }
        }
        database.exec("UPDATE super_users SET name = ?, username = ?, email = ? WHERE id = ?", params![name, username, email, super_id])?;
        let row = database.row("SELECT * FROM super_users WHERE id = ?", params![super_id])?.ok_or_else(|| bad("Super account not found"))?;
        public_super(&row, database)
    })
}

pub fn save_navigation(super_id: i64, order: &[String]) -> R<()> {
    with_control(|database| {
        database.exec("DELETE FROM super_navigation WHERE user_id = ?", params![super_id])?;
        for (position, page) in order.iter().enumerate() {
            database.exec("INSERT INTO super_navigation(user_id, page_id, position) VALUES (?, ?, ?)", params![super_id, page, position as i64])?;
        }
        Ok(())
    })
}

/// A new installation starts with one Super account; its password is published, so it must be changed.
pub fn ensure_super_account() -> R<()> {
    with_control(|database| {
        if database.exists("SELECT 1 FROM super_users LIMIT 1", params![])? {
            return Ok(());
        }
        database.exec(
            "INSERT INTO super_users(name, username, email, password_hash, active, reset_required, created_at) VALUES (?, ?, ?, ?, 1, 1, ?)",
            params!["Super User", "super", "super@sudo", crate::common::hash_password("doas"), clock::now_ms()],
        )?;
        Ok(())
    })
}

/// Move Super accounts out of an organization database into the control database.
///
/// Their password and name are kept. What they did in the organization (inspections they own)
/// stays attached to them through their new id; their sign-in history moves with them.
pub fn adopt_organization_supers(org: &Db) -> R<usize> {
    let rows = org.rows("SELECT * FROM users WHERE role = ?", params![SUPER_ROLE])?;
    let organization = config::active_db_path().file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    for row in &rows {
        let email = row.str("email").trim().to_lowercase();
        let username = {
            let stored = row.str("username");
            if stored.is_empty() { email.split('@').next().unwrap_or("").to_string() } else { stored }
        }
        .trim()
        .to_lowercase();
        let history = org.rows("SELECT logged_at, remember_me, user_agent FROM user_login_activity WHERE user_id = ?", params![row.int("id")])?;
        let super_id = with_control(|database| {
            let existing = database.row("SELECT id FROM super_users WHERE lower(email) = ? OR lower(username) = ?", params![email, username])?;
            let super_id = match existing {
                Some(found) => found.int("id"),
                None => database.insert(
                    "INSERT INTO super_users(name, username, email, password_hash, active, reset_required, last_login_at, login_count, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        row.value("name"),
                        username,
                        email,
                        row.str("password_hash"),
                        row.value("active"),
                        row.value("reset_required"),
                        row.value("last_login_at"),
                        row.int("login_count"),
                        row.opt_int("created_at").filter(|value| *value != 0).unwrap_or_else(clock::now_ms)
                    ],
                )?,
            };
            for item in &history {
                database.exec(
                    "INSERT INTO super_login_activity(user_id, logged_at, remember_me, user_agent, organization) VALUES (?, ?, ?, ?, ?)",
                    params![super_id, item.value("logged_at"), item.value("remember_me"), item.value("user_agent"), organization],
                )?;
            }
            Ok(super_id)
        })?;
        org.exec("UPDATE inspection_sessions SET owner_user_id = ? WHERE owner_user_id = ?", params![public_id(super_id), row.int("id")])?;
        for (table, column) in [("user_login_activity", "user_id"), ("auth_sessions", "user_id"), ("user_navigation", "user_id"),
                                ("password_reset_requests", "user_id"), ("due_notification_events", "user_id"), ("notifications", "recipient_user_id")] {
            if org.table_exists(table)? {
                org.exec(&format!("DELETE FROM {table} WHERE {column} = ?"), params![row.int("id")])?;
            }
        }
        org.exec("DELETE FROM users WHERE id = ?", params![row.int("id")])?;
    }
    // The role belongs to the control database now; organizations cannot grant it.
    org.exec("DELETE FROM roles WHERE name = ?", params![SUPER_ROLE])?;
    Ok(rows.len())
}

pub fn first_super_id() -> R<Option<i64>> {
    with_control(|database| Ok(database.scalar("SELECT id FROM super_users WHERE active = 1 ORDER BY id LIMIT 1", params![])?.as_i64()))
}
