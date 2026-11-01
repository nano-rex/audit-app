//! Super-user management of company databases: creating, switching, and removing them without
//! redirecting other requests.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::config;
use crate::error::{bad, R};
use crate::json::{obj, Value};

static DATABASE_LOCK: Mutex<()> = Mutex::new(());
const REQUIRED_TABLES: [&str; 3] = ["users", "roles", "auth_sessions"];
pub const ACTIVE_MARKER: &str = "active-database";

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty() && bytes.len() <= 64 && bytes[0].is_ascii_alphanumeric() && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}

fn path_of(name: &Value) -> R<PathBuf> {
    match name.as_str() {
        Some(name) if valid_name(name) => Ok(config::data_dir().join(format!("{name}.db"))),
        _ => Err(bad("Database name must use 1–64 letters, numbers, hyphens, or underscores")),
    }
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A leftover WAL or shared-memory file would be replayed into a later database of the same name.
fn remove_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

fn require_audit_database(path: &Path) -> R<()> {
    let tables: Vec<String> = crate::sqlite::Connection::open_read_only(path, std::time::Duration::from_secs(15))
        .and_then(|connection| connection.query("SELECT name FROM sqlite_master WHERE type = 'table'", &[]))
        .map(|rows| rows.iter().map(|row| row.str("name")).collect())
        .unwrap_or_default();
    if !REQUIRED_TABLES.iter().all(|table| tables.iter().any(|name| name == table)) {
        return Err(bad("That file is not an initialized audit database"));
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub fn list_databases() -> R<Value> {
    let _guard = DATABASE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    std::fs::create_dir_all(config::data_dir())?;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(config::data_dir())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "db") && path.is_file())
        .collect();
    paths.sort();
    let active = config::db_path();
    Ok(Value::Array(
        paths
            .iter()
            .map(|path| obj([
                ("name", Value::str(stem(path))),
                ("active", Value::Bool(same_file(path, &active))),
                ("size", Value::Int(std::fs::metadata(path).map(|meta| meta.len() as i64).unwrap_or(0))),
            ]))
            .collect(),
    ))
}

pub fn create_database(name: &Value) -> R<Value> {
    let path = path_of(name)?;
    let _guard = DATABASE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    if path.exists() {
        return Err(bad("That database already exists"));
    }
    // Only this thread sees the new path; other requests keep using the active database.
    if let Err(error) = config::with_db_override(path.clone(), crate::migrate::init_db) {
        remove_files(&path);
        return Err(error);
    }
    Ok(obj([
        ("name", Value::str(stem(&path))),
        ("active", false.into()),
        ("size", Value::Int(std::fs::metadata(&path).map(|meta| meta.len() as i64).unwrap_or(0))),
    ]))
}

pub fn switch_database(name: &Value) -> R<Value> {
    let path = path_of(name)?;
    let _guard = DATABASE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    if !path.is_file() {
        return Err(bad("Database not found"));
    }
    require_audit_database(&path)?;
    // A database last used by an older release is upgraded before anyone is sent to it.
    config::with_db_override(path.clone(), crate::migrate::init_db)?;
    config::set_db_path(std::fs::canonicalize(&path).unwrap_or(path.clone()));
    crate::db::clear_cache();
    std::fs::write(config::data_dir().join(ACTIVE_MARKER), stem(&path))?;
    Ok(obj([("name", Value::str(stem(&path))), ("active", true.into()), ("requiresLogin", true.into())]))
}

/// At startup, return to the database that was selected before the restart.
pub fn restore_active_database() -> bool {
    let Ok(text) = std::fs::read_to_string(config::data_dir().join(ACTIVE_MARKER)) else { return false };
    let Ok(path) = path_of(&Value::str(text.trim())) else { return false };
    if !path.is_file() {
        return false;
    }
    config::set_db_path(std::fs::canonicalize(&path).unwrap_or(path));
    true
}

pub fn remove_database(name: &Value) -> R<Value> {
    let path = path_of(name)?;
    let _guard = DATABASE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    if same_file(&path, &config::db_path()) {
        return Err(bad("Switch to another database before removing this one"));
    }
    if !path.is_file() {
        return Err(bad("Database not found"));
    }
    remove_files(&path);
    Ok(obj([("name", Value::str(stem(&path))), ("removed", true.into())]))
}
