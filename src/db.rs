//! Organization database connections, and the typed relational storage of record attributes.
//!
//! A connection follows the Python application's `with connect() as db:` blocks: data changes
//! open a transaction, which `with_db` commits when the block succeeds and rolls back when it
//! fails. Committing a change also clears the short-lived response cache.
//!
//! Record attributes (checklists, scores, settings, permissions, signatures, photos) are stored
//! as typed rows in value_sets / value_nodes: each scalar in a typed column, containers as child
//! rows, never a serialized JSON document. Domain rows hold the id of an immutable value set.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::config;
use crate::error::{bad, AppError, R};
use crate::json::Value;
use crate::params;
use crate::sqlite::{Connection, Row, Sql, ToSql};

pub struct Db {
    pub conn: Connection,
    pub path: PathBuf,
}

/// Open the database this thread works on.
pub fn connect() -> R<Db> {
    open(&config::active_db_path())
}

pub fn open(path: &Path) -> R<Db> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path, Duration::from_secs(15))?;
    conn.execute("PRAGMA foreign_keys = ON", params![])?;
    Ok(Db { conn, path: path.to_path_buf() })
}

impl Db {
    /// Run a statement; returns the number of rows it changed.
    pub fn exec(&self, sql: &str, params: &[&dyn ToSql]) -> R<i64> {
        Ok(self.conn.execute(sql, params)?)
    }

    /// Run an INSERT; returns the new row's id.
    pub fn insert(&self, sql: &str, params: &[&dyn ToSql]) -> R<i64> {
        self.conn.execute(sql, params)?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rows(&self, sql: &str, params: &[&dyn ToSql]) -> R<Vec<Row>> {
        Ok(self.conn.query(sql, params)?)
    }

    pub fn row(&self, sql: &str, params: &[&dyn ToSql]) -> R<Option<Row>> {
        Ok(self.conn.query_one(sql, params)?)
    }

    pub fn exists(&self, sql: &str, params: &[&dyn ToSql]) -> R<bool> {
        Ok(self.conn.query_one(sql, params)?.is_some())
    }

    /// The first column of the first row.
    pub fn scalar(&self, sql: &str, params: &[&dyn ToSql]) -> R<Sql> {
        Ok(self.conn.query_one(sql, params)?.map(|row| row.at(0).clone()).unwrap_or(Sql::Null))
    }

    pub fn count(&self, sql: &str, params: &[&dyn ToSql]) -> R<i64> {
        Ok(self.scalar(sql, params)?.as_i64().unwrap_or(0))
    }

    /// The first column of every row.
    pub fn column(&self, sql: &str, params: &[&dyn ToSql]) -> R<Vec<Sql>> {
        Ok(self.rows(sql, params)?.into_iter().map(|row| row.at(0).clone()).collect())
    }

    pub fn strings(&self, sql: &str, params: &[&dyn ToSql]) -> R<Vec<String>> {
        Ok(self.column(sql, params)?.iter().map(|value| value.to_value().text()).collect())
    }

    pub fn begin_immediate(&self) -> R<()> {
        if !self.conn.in_transaction() {
            self.conn.execute("BEGIN IMMEDIATE", params![])?;
        }
        Ok(())
    }

    pub fn batch(&self, script: &str) -> R<()> {
        Ok(self.conn.execute_batch(script)?)
    }

    pub fn table_exists(&self, name: &str) -> R<bool> {
        self.exists("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?", params![name])
    }

    pub fn columns(&self, table: &str) -> R<Vec<String>> {
        Ok(self.rows(&format!("PRAGMA table_info(\"{table}\")"), params![])?.iter().map(|row| row.str("name")).collect())
    }

    /// Commit now (and remove attribute sets that lost their last reference first).
    pub fn commit(&self) -> R<()> {
        if self.conn.in_transaction() {
            if self.conn.total_changes() > 0 && self.table_exists("pending_value_cleanup")? {
                collect_pending_values(self)?;
            }
            self.conn.execute("COMMIT", params![])?;
        }
        if self.conn.total_changes() > 0 {
            clear_cache();
        }
        Ok(())
    }

    fn rollback(&self) {
        if self.conn.in_transaction() {
            let _ = self.conn.execute("ROLLBACK", params![]);
        }
    }
}

/// `with connect() as db:` — commit when `work` succeeds, roll back when it fails.
pub fn with_db<T>(work: impl FnOnce(&Db) -> R<T>) -> R<T> {
    let db = connect()?;
    finish(&db, work(&db))
}

pub fn finish<T>(db: &Db, result: R<T>) -> R<T> {
    match result {
        Ok(value) => {
            db.commit()?;
            Ok(value)
        }
        Err(error) => {
            db.rollback();
            Err(error)
        }
    }
}

pub fn first_department(db: &Db) -> R<String> {
    Ok(db.scalar("SELECT code FROM departments ORDER BY code LIMIT 1", params![])?.text())
}

pub fn first_outlet(db: &Db) -> R<String> {
    Ok(db.scalar("SELECT code FROM outlets ORDER BY code LIMIT 1", params![])?.text())
}

/// INSERT INTO table (columns) VALUES (...) for the given (column, value) pairs.
pub fn insert_record(db: &Db, table: &str, values: &[(&str, Value)]) -> R<i64> {
    let columns: Vec<&str> = values.iter().map(|(column, _)| *column).collect();
    let marks = vec!["?"; values.len()].join(", ");
    let params: Vec<&dyn ToSql> = values.iter().map(|(_, value)| value as &dyn ToSql).collect();
    db.insert(&format!("INSERT INTO {table} ({}) VALUES ({marks})", columns.join(", ")), &params)
}

/// UPDATE table SET column = ?, ... WHERE id = ?
pub fn update_record(db: &Db, table: &str, values: &[(String, Value)], id: i64) -> R<i64> {
    if values.is_empty() {
        return Ok(0);
    }
    let assignments: Vec<String> = values.iter().map(|(column, _)| format!("{column} = ?")).collect();
    let mut params: Vec<&dyn ToSql> = values.iter().map(|(_, value)| value as &dyn ToSql).collect();
    params.push(&id);
    db.exec(&format!("UPDATE {table} SET {} WHERE id = ?", assignments.join(", ")), &params)
}

/// `(?, ?, ?)` placeholders for an IN list.
pub fn marks(count: usize) -> String {
    vec!["?"; count].join(",")
}

// ---- Typed relational attributes ----

/// Attribute columns: each table's old JSON column name and its value-set column.
pub const FIELDS: [(&str, &[(&str, &str)]); 12] = [
    ("audits", &[("scoring_json", "scoring_data_id")]),
    ("inspection_sessions", &[("items_json", "items_data_id"), ("signatures_json", "signatures_data_id"), ("locations_json", "locations_data_id"), ("scope_json", "scope_data_id")]),
    ("schedules", &[("locations_json", "locations_data_id"), ("assignees_json", "assignees_data_id"), ("scope_json", "scope_data_id")]),
    ("findings", &[("images_json", "images_data_id"), ("completion_photo", "completion_photo_data_id")]),
    ("work_orders", &[("images_json", "images_data_id"), ("completion_photo", "completion_photo_data_id")]),
    ("work_requests", &[("images_json", "images_data_id")]),
    ("equipment", &[("photos", "photos_data_id"), ("inspection_criteria", "inspection_criteria_data_id")]),
    ("zones", &[("locations_json", "locations_data_id")]),
    ("roles", &[("permissions_json", "permissions_data_id"), ("inspection_permissions", "inspection_permissions_data_id"), ("action_permissions", "action_permissions_data_id")]),
    ("change_requests", &[("payload_json", "payload_data_id"), ("before_json", "before_data_id")]),
    ("users", &[("permission_overrides", "permission_overrides_data_id"), ("profile_photo", "profile_photo_data_id"), ("signature_image", "signature_image_data_id"), ("outlets_json", "outlets_data_id")]),
    ("app_settings", &[("value", "value_data_id")]),
];

const MEDIA_PREFIX: &str = "/api/media/";

pub fn initialize_values(db: &Db) -> R<()> {
    db.batch(
        "CREATE TABLE IF NOT EXISTS value_sets(id INTEGER PRIMARY KEY AUTOINCREMENT);
         CREATE TABLE IF NOT EXISTS value_nodes(
            set_id INTEGER NOT NULL REFERENCES value_sets(id) ON DELETE CASCADE,
            node_id INTEGER NOT NULL, parent_id INTEGER, position INTEGER NOT NULL,
            field_name TEXT, kind TEXT NOT NULL CHECK(kind IN ('null','object','array','text','integer','real','boolean','image')),
            text_value TEXT, integer_value INTEGER, real_value REAL,
            PRIMARY KEY(set_id, node_id),
            FOREIGN KEY(set_id, parent_id) REFERENCES value_nodes(set_id, node_id)
         );
         CREATE INDEX IF NOT EXISTS idx_value_nodes_parent ON value_nodes(set_id, parent_id);",
    )
}

/// Store a value as a new value set; returns its id.
pub fn save_value(db: &Db, value: &Value) -> R<i64> {
    let set_id = db.insert("INSERT INTO value_sets DEFAULT VALUES", params![])?;
    struct Node<'a> {
        parent: Option<i64>,
        position: i64,
        key: Option<&'a str>,
        value: &'a Value,
    }
    let mut stack = vec![Node { parent: None, position: 0, key: None, value }];
    let mut next_id = 0i64;
    // Depth first in order: a node's id is assigned before its children's, as Python's visitor did.
    let mut ordered: Vec<(i64, Node)> = Vec::new();
    while let Some(node) = stack.pop() {
        let id = next_id;
        next_id += 1;
        let children: Vec<Node> = match node.value {
            Value::Object(items) => items.iter().enumerate().map(|(index, (key, child))| Node { parent: Some(id), position: index as i64, key: Some(key), value: child }).collect(),
            Value::Array(items) => items.iter().enumerate().map(|(index, child)| Node { parent: Some(id), position: index as i64, key: None, value: child }).collect(),
            _ => Vec::new(),
        };
        ordered.push((id, node));
        stack.extend(children.into_iter().rev());
    }
    for (id, node) in ordered {
        let (kind, text, integer, real): (&str, Option<String>, Option<i64>, Option<f64>) = match node.value {
            Value::Null => ("null", None, None, None),
            Value::Bool(flag) => ("boolean", None, Some(*flag as i64), None),
            Value::Int(number) => ("integer", None, Some(*number), None),
            Value::Float(number) => ("real", None, None, Some(*number)),
            Value::Str(text) => match text.strip_prefix(MEDIA_PREFIX) {
                Some(identifier) => ("image", Some(identifier.to_string()), None, None),
                None => ("text", Some(text.clone()), None, None),
            },
            Value::Object(_) => ("object", None, None, None),
            Value::Array(_) => ("array", None, None, None),
        };
        db.exec(
            "INSERT INTO value_nodes VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![set_id, id, node.parent, node.position, node.key.map(str::to_string), kind, text, integer, real],
        )?;
    }
    Ok(set_id)
}

/// Rebuild the values of rows (set_id, node_id, parent_id, field_name, kind, text, integer, real)
/// ordered by set and node.
fn assemble(rows: &[Row]) -> HashMap<i64, Value> {
    let mut by_set: HashMap<i64, Vec<&Row>> = HashMap::new();
    for row in rows {
        by_set.entry(row.int("set_id")).or_default().push(row);
    }
    let mut values = HashMap::new();
    for (set_id, nodes) in by_set {
        // Children always follow their parent, so the nodes are joined from the last to the first.
        let index_of: HashMap<i64, usize> = nodes.iter().enumerate().map(|(index, row)| (row.int("node_id"), index)).collect();
        let mut built: Vec<Option<Value>> = nodes.iter().map(|row| Some(scalar_of(row))).collect();
        let mut children: Vec<Vec<(Option<String>, Value)>> = vec![Vec::new(); nodes.len()];
        for index in (0..nodes.len()).rev() {
            let mut value = built[index].take().unwrap_or(Value::Null);
            let kids = std::mem::take(&mut children[index]);
            match &mut value {
                Value::Object(items) => items.extend(kids.into_iter().rev().map(|(key, child)| (key.unwrap_or_default(), child))),
                Value::Array(items) => items.extend(kids.into_iter().rev().map(|(_, child)| child)),
                _ => {}
            }
            match nodes[index].opt_int("parent_id").and_then(|parent| index_of.get(&parent)) {
                Some(&parent) => children[parent].push((nodes[index].get("field_name").as_str().map(str::to_string), value)),
                None => {
                    if nodes[index].opt_int("parent_id").is_none() {
                        values.insert(set_id, value);
                    }
                }
            }
        }
    }
    values
}

fn scalar_of(row: &Row) -> Value {
    match row.str("kind").as_str() {
        "object" => Value::object(),
        "array" => Value::array(),
        "text" => row.value("text_value"),
        "integer" => row.value("integer_value"),
        "boolean" => Value::Bool(row.int("integer_value") != 0),
        "real" => row.value("real_value"),
        "image" => Value::Str(format!("{MEDIA_PREFIX}{}", row.str("text_value"))),
        _ => Value::Null,
    }
}

/// load_value: a value set by id; text references are legacy JSON.
pub fn load_value(db: &Db, reference: &Sql) -> R<Value> {
    match reference {
        Sql::Int(id) => {
            let rows = db.rows(
                "SELECT set_id, node_id, parent_id, field_name, kind, text_value, integer_value, real_value FROM value_nodes WHERE set_id = ? ORDER BY node_id",
                params![id],
            )?;
            assemble(&rows).remove(id).ok_or_else(|| bad("Saved record attributes are missing"))
        }
        Sql::Text(text) => crate::json::parse(text).map_err(bad),
        Sql::Null => Ok(Value::Null),
        other => Ok(other.to_value()),
    }
}

/// `load_value(reference or fallback)`: the fallback for a missing (or zero) reference.
pub fn load_or(db: &Db, reference: &Sql, fallback: Value) -> R<Value> {
    if reference.truthy() {
        load_value(db, reference)
    } else {
        Ok(fallback)
    }
}

/// Load many value sets with bounded queries.
pub fn load_values(db: &Db, references: &[Sql]) -> R<Vec<Value>> {
    let mut ids: Vec<i64> = references.iter().filter_map(|reference| if let Sql::Int(id) = reference { Some(*id) } else { None }).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut values = HashMap::new();
    for chunk in ids.chunks(900) {
        let params: Vec<&dyn ToSql> = chunk.iter().map(|id| id as &dyn ToSql).collect();
        let rows = db.rows(
            &format!("SELECT set_id, node_id, parent_id, field_name, kind, text_value, integer_value, real_value FROM value_nodes WHERE set_id IN ({}) ORDER BY set_id, node_id", marks(chunk.len())),
            &params,
        )?;
        values.extend(assemble(&rows));
    }
    references
        .iter()
        .map(|reference| match reference {
            Sql::Int(id) => values.get(id).cloned().ok_or_else(|| bad("Saved record attributes are missing")),
            Sql::Text(text) => crate::json::parse(text).map_err(bad),
            Sql::Null => Ok(Value::Null),
            other => Ok(other.to_value()),
        })
        .collect()
}

/// `data_value`: a null value becomes the fallback; a text value is read as JSON (or a list of it).
pub fn data_value(db: &Db, value: &Value, fallback: Value) -> R<i64> {
    let value = match value {
        Value::Null => fallback,
        Value::Str(text) => crate::json::parse(text).unwrap_or_else(|_| Value::Array(vec![Value::Str(text.clone())])),
        other => other.clone(),
    };
    save_value(db, &value)
}

/// Rows as objects, each attribute column replaced by its decoded value under its old name.
pub fn hydrate_many(db: &Db, rows: &[Row]) -> R<Vec<Value>> {
    let mappings: Vec<(&str, &str)> = FIELDS.iter().flat_map(|(_, fields)| fields.iter().copied()).collect();
    let mut references = Vec::new();
    let mut positions = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        for (old, new) in &mappings {
            if row.has(new) && !row.get(new).is_null() {
                positions.push((row_index, *old, references.len()));
                references.push(row.get(new).clone());
            }
        }
    }
    let decoded = load_values(db, &references)?;
    let mut results: Vec<Value> = rows.iter().map(Row::to_object).collect();
    for (row_index, old, value_index) in positions {
        results[row_index].set(old, decoded[value_index].clone());
    }
    for result in &mut results {
        for (old, new) in &mappings {
            if result.has(new) {
                result.set_default(old, Value::Null);
                result.remove(new);
            }
        }
    }
    Ok(results)
}

pub fn collect_pending_values(db: &Db) -> R<()> {
    if !db.exists("SELECT 1 FROM pending_value_cleanup LIMIT 1", params![])? {
        return Ok(());
    }
    let unused: Vec<String> = FIELDS
        .iter()
        .flat_map(|(table, fields)| fields.iter().map(move |(_, column)| format!("NOT EXISTS(SELECT 1 FROM \"{table}\" WHERE \"{column}\" = value_sets.id)")))
        .collect();
    db.exec(&format!("DELETE FROM value_sets WHERE id IN (SELECT value_id FROM pending_value_cleanup) AND {}", unused.join(" AND ")), params![])?;
    db.exec("DELETE FROM pending_value_cleanup", params![])?;
    Ok(())
}

/// Queue replaced sets; remove unreferenced attributes at transaction commit.
pub fn install_reference_cleanup(db: &Db) -> R<()> {
    db.batch(
        "CREATE TABLE IF NOT EXISTS pending_value_cleanup(value_id INTEGER PRIMARY KEY);
         CREATE TRIGGER IF NOT EXISTS delete_value_nodes AFTER DELETE ON value_sets BEGIN DELETE FROM value_nodes WHERE set_id = OLD.id; END;",
    )?;
    for (table, fields) in FIELDS {
        for (_, column) in fields.iter() {
            db.exec(&format!("CREATE INDEX IF NOT EXISTS \"idx_{table}_{column}\" ON \"{table}\"(\"{column}\")"), params![])?;
            for action in ["DELETE".to_string(), format!("UPDATE OF \"{column}\"")] {
                let name = format!("cleanup_{table}_{column}_{}", action.split(' ').next().unwrap_or("").to_lowercase());
                db.exec(&format!("DROP TRIGGER IF EXISTS \"{name}\""), params![])?;
                db.exec(
                    &format!("CREATE TRIGGER \"{name}\" AFTER {action} ON \"{table}\" WHEN OLD.\"{column}\" IS NOT NULL BEGIN INSERT OR IGNORE INTO pending_value_cleanup(value_id) VALUES (OLD.\"{column}\"); END"),
                    params![],
                )?;
            }
        }
    }
    db.exec("INSERT OR IGNORE INTO pending_value_cleanup SELECT id FROM value_sets", params![])?;
    collect_pending_values(db)
}

/// Every app setting, decoded: {key: value}.
pub fn all_settings(db: &Db, pattern: Option<&str>) -> R<Value> {
    let rows = match pattern {
        Some(like) => db.rows("SELECT key, value_data_id FROM app_settings WHERE key LIKE ? ORDER BY key", params![like])?,
        None => db.rows("SELECT key, value_data_id FROM app_settings ORDER BY key", params![])?,
    };
    let values = load_values(db, &rows.iter().map(|row| row.get("value_data_id").clone()).collect::<Vec<_>>())?;
    Ok(Value::Object(rows.iter().map(|row| row.str("key")).zip(values).collect()))
}

/// read_setting: a stored setting, or the fallback when it is missing.
pub fn read_setting(db: &Db, key: &str, fallback: Value) -> R<Value> {
    match db.row("SELECT value_data_id FROM app_settings WHERE key = ?", params![key])? {
        None => Ok(fallback),
        Some(row) => load_value(db, row.get("value_data_id")),
    }
}

pub fn write_setting(db: &Db, key: &str, value: &Value) -> R<()> {
    let reference = save_value(db, value)?;
    db.exec("INSERT OR REPLACE INTO app_settings (key, value_data_id) VALUES (?, ?)", params![key, reference])?;
    Ok(())
}

// ---- Responses shared by a burst of readers ----

/// A response computed once and kept for a few seconds; any committed change clears it.
pub struct Prepared {
    pub data: Value,
    encoded: OnceLock<(Vec<u8>, Vec<u8>)>,
}

impl Prepared {
    pub fn new(data: Value) -> Prepared {
        Prepared { data, encoded: OnceLock::new() }
    }

    /// The JSON body and its gzip form.
    pub fn bodies(&self) -> &(Vec<u8>, Vec<u8>) {
        self.encoded.get_or_init(|| {
            let body = self.data.to_json_bytes();
            let packed = crate::flate::gzip(&body);
            (body, packed)
        })
    }
}

type CacheEntry = (Instant, Arc<Prepared>);

fn cache() -> &'static Mutex<Vec<(String, CacheEntry)>> {
    static CACHE: OnceLock<Mutex<Vec<(String, CacheEntry)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn clear_cache() {
    cache().lock().unwrap_or_else(|poison| poison.into_inner()).clear();
}

/// The cached response for this key (on the active database), computing it when stale.
pub fn cached(key: &str, loader: impl FnOnce() -> R<Value>) -> R<Arc<Prepared>> {
    let key = format!("{}|{key}", config::db_path().display());
    {
        let mut entries = cache().lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(index) = entries.iter().position(|(name, _)| *name == key) {
            let (stamp, prepared) = &entries[index].1;
            if stamp.elapsed() < Duration::from_secs(5) {
                let prepared = prepared.clone();
                let entry = entries.remove(index);
                entries.push(entry);
                return Ok(prepared);
            }
            entries.remove(index);
        }
    }
    // Computed outside the lock; two readers may both compute a stale entry, which is harmless.
    let prepared = Arc::new(Prepared::new(loader()?));
    let mut entries = cache().lock().unwrap_or_else(|poison| poison.into_inner());
    entries.retain(|(name, _)| *name != key);
    entries.push((key, (Instant::now(), prepared.clone())));
    while entries.len() > 32 {
        entries.remove(0);
    }
    Ok(prepared)
}

/// Text helper for `db.execute(...)` results that Python compared with rowcount.
pub fn not_found_if_zero(changed: i64) -> Result<(), AppError> {
    if changed == 0 {
        Err(AppError::Status(404, String::new()))
    } else {
        Ok(())
    }
}
