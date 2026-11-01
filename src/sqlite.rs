//! A small, safe layer over the system's SQLite C library.
//!
//! Only the functions the application uses are declared. Every pointer handed out by SQLite is
//! owned by exactly one Connection or Statement, which finalizes or closes it when dropped.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fmt;
use std::path::Path;
use std::ptr;
use std::time::Duration;

use crate::json::Value;

#[repr(C)]
struct RawDb {
    _private: [u8; 0],
}

#[repr(C)]
struct RawStmt {
    _private: [u8; 0],
}

#[repr(C)]
struct RawBackup {
    _private: [u8; 0],
}

// The destructor argument of the bind functions: -1 (SQLITE_TRANSIENT) makes SQLite copy the
// value before the call returns, so Rust keeps ownership of its buffers.
const SQLITE_TRANSIENT: isize = -1;

extern "C" {
    fn sqlite3_open_v2(filename: *const c_char, db: *mut *mut RawDb, flags: c_int, vfs: *const c_char) -> c_int;
    fn sqlite3_close_v2(db: *mut RawDb) -> c_int;
    fn sqlite3_errmsg(db: *mut RawDb) -> *const c_char;
    fn sqlite3_extended_errcode(db: *mut RawDb) -> c_int;
    fn sqlite3_busy_timeout(db: *mut RawDb, ms: c_int) -> c_int;
    fn sqlite3_prepare_v2(db: *mut RawDb, sql: *const c_char, bytes: c_int, stmt: *mut *mut RawStmt, tail: *mut *const c_char) -> c_int;
    fn sqlite3_finalize(stmt: *mut RawStmt) -> c_int;
    fn sqlite3_step(stmt: *mut RawStmt) -> c_int;
    fn sqlite3_bind_parameter_count(stmt: *mut RawStmt) -> c_int;
    fn sqlite3_bind_null(stmt: *mut RawStmt, index: c_int) -> c_int;
    fn sqlite3_bind_int64(stmt: *mut RawStmt, index: c_int, value: i64) -> c_int;
    fn sqlite3_bind_double(stmt: *mut RawStmt, index: c_int, value: f64) -> c_int;
    fn sqlite3_bind_text(stmt: *mut RawStmt, index: c_int, text: *const c_char, bytes: c_int, destructor: isize) -> c_int;
    fn sqlite3_bind_blob(stmt: *mut RawStmt, index: c_int, data: *const c_void, bytes: c_int, destructor: isize) -> c_int;
    fn sqlite3_column_count(stmt: *mut RawStmt) -> c_int;
    fn sqlite3_column_name(stmt: *mut RawStmt, index: c_int) -> *const c_char;
    fn sqlite3_column_type(stmt: *mut RawStmt, index: c_int) -> c_int;
    fn sqlite3_column_int64(stmt: *mut RawStmt, index: c_int) -> i64;
    fn sqlite3_column_double(stmt: *mut RawStmt, index: c_int) -> f64;
    fn sqlite3_column_text(stmt: *mut RawStmt, index: c_int) -> *const u8;
    fn sqlite3_column_blob(stmt: *mut RawStmt, index: c_int) -> *const c_void;
    fn sqlite3_column_bytes(stmt: *mut RawStmt, index: c_int) -> c_int;
    fn sqlite3_changes(db: *mut RawDb) -> c_int;
    fn sqlite3_total_changes(db: *mut RawDb) -> c_int;
    fn sqlite3_last_insert_rowid(db: *mut RawDb) -> i64;
    fn sqlite3_get_autocommit(db: *mut RawDb) -> c_int;
    fn sqlite3_libversion() -> *const c_char;
    fn sqlite3_backup_init(dest: *mut RawDb, dest_name: *const c_char, source: *mut RawDb, source_name: *const c_char) -> *mut RawBackup;
    fn sqlite3_backup_step(backup: *mut RawBackup, pages: c_int) -> c_int;
    fn sqlite3_backup_finish(backup: *mut RawBackup) -> c_int;
}

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
const SQLITE_OPEN_READWRITE: c_int = 0x0000_0002;
const SQLITE_OPEN_CREATE: c_int = 0x0000_0004;
const SQLITE_OPEN_READONLY: c_int = 0x0000_0001;
const SQLITE_OPEN_NOMUTEX: c_int = 0x0000_8000;

pub const CONSTRAINT: i32 = 19;
pub const BUSY: i32 = 5;
pub const LOCKED: i32 = 6;

/// An SQLite failure: its (primary) result code and message.
#[derive(Debug, Clone)]
pub struct Error {
    pub code: i32,
    pub message: String,
}

impl Error {
    pub fn misuse(message: impl Into<String>) -> Error {
        Error { code: 21, message: message.into() }
    }

    pub fn is_constraint(&self) -> bool {
        self.code & 0xff == CONSTRAINT
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.code & 0xff, BUSY | LOCKED)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (SQLite code {})", self.message, self.code)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A value as SQLite stores it.
#[derive(Debug, Clone, PartialEq)]
pub enum Sql {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

static NULL_SQL: Sql = Sql::Null;

impl Sql {
    pub fn is_null(&self) -> bool {
        matches!(self, Sql::Null)
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Sql::Int(value) => Some(*value),
            Sql::Real(value) => Some(*value as i64),
            Sql::Text(text) => text.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Sql::Int(value) => Some(*value as f64),
            Sql::Real(value) => Some(*value),
            Sql::Text(text) => text.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Sql::Text(text) => Some(text),
            _ => None,
        }
    }

    /// A column value carried in a JSON row object back to its stored form.
    pub fn from_value(value: &Value) -> Sql {
        match value {
            Value::Null => Sql::Null,
            Value::Bool(flag) => Sql::Int(*flag as i64),
            Value::Int(number) => Sql::Int(*number),
            Value::Float(number) => Sql::Real(*number),
            Value::Str(text) => Sql::Text(text.clone()),
            other => Sql::Text(other.to_json()),
        }
    }

    /// The value as JSON: what Python's sqlite3 rows turn into once encoded.
    pub fn to_value(&self) -> Value {
        match self {
            Sql::Null => Value::Null,
            Sql::Int(value) => Value::Int(*value),
            Sql::Real(value) => Value::Float(*value),
            Sql::Text(text) => Value::Str(text.clone()),
            Sql::Blob(bytes) => Value::Str(String::from_utf8_lossy(bytes).into_owned()),
        }
    }

    /// Python's truthiness of the column value.
    pub fn truthy(&self) -> bool {
        match self {
            Sql::Null => false,
            Sql::Int(value) => *value != 0,
            Sql::Real(value) => *value != 0.0,
            Sql::Text(text) => !text.is_empty(),
            Sql::Blob(bytes) => !bytes.is_empty(),
        }
    }

    /// `str(value or "")`.
    pub fn text(&self) -> String {
        if self.truthy() {
            self.to_value().py_str()
        } else {
            String::new()
        }
    }
}

/// Anything that can be bound to a statement parameter.
pub trait ToSql {
    fn to_sql(&self) -> std::result::Result<Sql, String>;
}

impl ToSql for Sql {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(self.clone())
    }
}

impl ToSql for i64 {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(Sql::Int(*self))
    }
}

impl ToSql for i32 {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(Sql::Int(*self as i64))
    }
}

impl ToSql for usize {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(Sql::Int(*self as i64))
    }
}

impl ToSql for f64 {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(Sql::Real(*self))
    }
}

impl ToSql for bool {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(Sql::Int(*self as i64))
    }
}

impl ToSql for str {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(Sql::Text(self.to_string()))
    }
}

impl ToSql for String {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(Sql::Text(self.clone()))
    }
}

impl ToSql for Vec<u8> {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        Ok(Sql::Blob(self.clone()))
    }
}

impl<T: ToSql> ToSql for Option<T> {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        match self {
            Some(value) => value.to_sql(),
            None => Ok(Sql::Null),
        }
    }
}

impl ToSql for Value {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        match self {
            Value::Null => Ok(Sql::Null),
            Value::Bool(value) => Ok(Sql::Int(*value as i64)),
            Value::Int(value) => Ok(Sql::Int(*value)),
            Value::Float(value) => Ok(Sql::Real(*value)),
            Value::Str(text) => Ok(Sql::Text(text.clone())),
            Value::Array(_) => Err("type 'list' is not supported".into()),
            Value::Object(_) => Err("type 'dict' is not supported".into()),
        }
    }
}

impl<T: ToSql + ?Sized> ToSql for &T {
    fn to_sql(&self) -> std::result::Result<Sql, String> {
        (**self).to_sql()
    }
}

/// Statement parameters: `params![a, b, c]`.
#[macro_export]
macro_rules! params {
    () => { &[] as &[&dyn $crate::sqlite::ToSql] };
    ($($value:expr),+ $(,)?) => { &[$(&$value as &dyn $crate::sqlite::ToSql),+] as &[&dyn $crate::sqlite::ToSql] };
}

/// One result row, addressed by column name as Python's sqlite3.Row is.
#[derive(Debug, Clone)]
pub struct Row {
    pub columns: std::rc::Rc<Vec<String>>,
    pub values: Vec<Sql>,
}

impl Row {
    pub fn index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|column| column == name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.index(name).is_some()
    }

    /// The column's value; null when there is no such column.
    pub fn get(&self, name: &str) -> &Sql {
        self.index(name).map(|index| &self.values[index]).unwrap_or(&NULL_SQL)
    }

    pub fn at(&self, index: usize) -> &Sql {
        self.values.get(index).unwrap_or(&NULL_SQL)
    }

    pub fn int(&self, name: &str) -> i64 {
        self.get(name).as_i64().unwrap_or(0)
    }

    pub fn opt_int(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Sql::Null => None,
            other => other.as_i64(),
        }
    }

    /// The text of the column, empty for null.
    pub fn str(&self, name: &str) -> String {
        match self.get(name) {
            Sql::Null => String::new(),
            Sql::Text(text) => text.clone(),
            other => other.to_value().py_str(),
        }
    }

    pub fn value(&self, name: &str) -> Value {
        self.get(name).to_value()
    }

    /// The row as an object of its columns, as dict(row) is.
    pub fn to_object(&self) -> Value {
        Value::Object(self.columns.iter().cloned().zip(self.values.iter().map(Sql::to_value)).collect())
    }
}

pub struct Connection {
    raw: *mut RawDb,
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            sqlite3_close_v2(self.raw);
        }
    }
}

struct Statement<'a> {
    raw: *mut RawStmt,
    connection: &'a Connection,
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        unsafe {
            sqlite3_finalize(self.raw);
        }
    }
}

pub fn version() -> String {
    unsafe { CStr::from_ptr(sqlite3_libversion()).to_string_lossy().into_owned() }
}

/// Whether a statement is one Python's sqlite3 module opens a transaction for.
fn begins_transaction(sql: &str) -> bool {
    let word: String = sql.trim_start().chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_uppercase();
    matches!(word.as_str(), "INSERT" | "UPDATE" | "DELETE" | "REPLACE")
}

impl Connection {
    pub fn open(path: &Path, timeout: Duration) -> Result<Connection> {
        Self::open_with(path, timeout, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_NOMUTEX)
    }

    pub fn open_read_only(path: &Path, timeout: Duration) -> Result<Connection> {
        Self::open_with(path, timeout, SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX)
    }

    fn open_with(path: &Path, timeout: Duration, flags: c_int) -> Result<Connection> {
        let name = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| Error::misuse("database path contains a NUL byte"))?;
        let mut raw = ptr::null_mut();
        let code = unsafe { sqlite3_open_v2(name.as_ptr(), &mut raw, flags, ptr::null()) };
        if raw.is_null() {
            return Err(Error { code, message: "out of memory opening the database".into() });
        }
        let connection = Connection { raw };
        if code != SQLITE_OK {
            return Err(connection.error(code));
        }
        unsafe {
            sqlite3_busy_timeout(raw, timeout.as_millis().min(i32::MAX as u128) as c_int);
        }
        Ok(connection)
    }

    fn error(&self, code: c_int) -> Error {
        let message = unsafe { CStr::from_ptr(sqlite3_errmsg(self.raw)).to_string_lossy().into_owned() };
        let extended = unsafe { sqlite3_extended_errcode(self.raw) };
        Error { code: if extended != 0 { extended } else { code }, message }
    }

    fn prepare<'a>(&'a self, sql: &str) -> Result<Statement<'a>> {
        let text = CString::new(sql).map_err(|_| Error::misuse("SQL contains a NUL byte"))?;
        let mut raw = ptr::null_mut();
        let code = unsafe { sqlite3_prepare_v2(self.raw, text.as_ptr(), -1, &mut raw, ptr::null_mut()) };
        if code != SQLITE_OK {
            return Err(self.error(code));
        }
        if raw.is_null() {
            return Err(Error::misuse("empty SQL statement"));
        }
        Ok(Statement { raw, connection: self })
    }

    pub fn in_transaction(&self) -> bool {
        unsafe { sqlite3_get_autocommit(self.raw) == 0 }
    }

    pub fn changes(&self) -> i64 {
        unsafe { sqlite3_changes(self.raw) as i64 }
    }

    pub fn total_changes(&self) -> i64 {
        unsafe { sqlite3_total_changes(self.raw) as i64 }
    }

    pub fn last_insert_rowid(&self) -> i64 {
        unsafe { sqlite3_last_insert_rowid(self.raw) }
    }

    /// Run a statement that returns no rows of interest; begins a transaction first for data
    /// changes, as Python's sqlite3 does, so they are kept only on commit.
    pub fn execute(&self, sql: &str, params: &[&dyn ToSql]) -> Result<i64> {
        if begins_transaction(sql) && !self.in_transaction() {
            self.run_plain("BEGIN")?;
        }
        let statement = self.prepare(sql)?;
        statement.bind(params)?;
        loop {
            match unsafe { sqlite3_step(statement.raw) } {
                SQLITE_ROW => continue,
                SQLITE_DONE => break,
                code => return Err(self.error(code)),
            }
        }
        Ok(self.changes())
    }

    fn run_plain(&self, sql: &str) -> Result<()> {
        let statement = self.prepare(sql)?;
        match unsafe { sqlite3_step(statement.raw) } {
            SQLITE_ROW | SQLITE_DONE => Ok(()),
            code => Err(self.error(code)),
        }
    }

    /// Run several statements separated by semicolons (no parameters), like executescript.
    pub fn execute_batch(&self, script: &str) -> Result<()> {
        for statement in split_statements(script) {
            if !statement.trim().is_empty() {
                self.execute(&statement, &[])?;
            }
        }
        Ok(())
    }

    pub fn query(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>> {
        if begins_transaction(sql) && !self.in_transaction() {
            self.run_plain("BEGIN")?;
        }
        let statement = self.prepare(sql)?;
        statement.bind(params)?;
        let count = unsafe { sqlite3_column_count(statement.raw) };
        let columns: std::rc::Rc<Vec<String>> = std::rc::Rc::new(
            (0..count).map(|index| unsafe { CStr::from_ptr(sqlite3_column_name(statement.raw, index)).to_string_lossy().into_owned() }).collect(),
        );
        let mut rows = Vec::new();
        loop {
            match unsafe { sqlite3_step(statement.raw) } {
                SQLITE_ROW => rows.push(Row { columns: columns.clone(), values: (0..count).map(|index| statement.column(index)).collect() }),
                SQLITE_DONE => break,
                code => return Err(self.error(code)),
            }
        }
        Ok(rows)
    }

    pub fn query_one(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Option<Row>> {
        Ok(self.query(sql, params)?.into_iter().next())
    }

    /// Copy the whole database into another connection (SQLite's online backup).
    pub fn backup_to(&self, destination: &Connection) -> Result<()> {
        let main = CString::new("main").unwrap();
        let backup = unsafe { sqlite3_backup_init(destination.raw, main.as_ptr(), self.raw, main.as_ptr()) };
        if backup.is_null() {
            return Err(destination.error(1));
        }
        let code = loop {
            match unsafe { sqlite3_backup_step(backup, -1) } {
                SQLITE_DONE => break SQLITE_OK,
                SQLITE_OK => continue,
                5 | 6 => std::thread::sleep(Duration::from_millis(50)),
                code => break code,
            }
        };
        let finish = unsafe { sqlite3_backup_finish(backup) };
        if code != SQLITE_OK || finish != SQLITE_OK {
            return Err(destination.error(if code != SQLITE_OK { code } else { finish }));
        }
        Ok(())
    }
}

impl Statement<'_> {
    fn bind(&self, params: &[&dyn ToSql]) -> Result<()> {
        let expected = unsafe { sqlite3_bind_parameter_count(self.raw) } as usize;
        if expected != params.len() {
            return Err(Error::misuse(format!("Incorrect number of bindings supplied. The current statement uses {expected}, and there are {} supplied.", params.len())));
        }
        for (offset, param) in params.iter().enumerate() {
            let index = offset as c_int + 1;
            let value = param.to_sql().map_err(|message| Error::misuse(format!("Error binding parameter {index}: {message}")))?;
            let code = unsafe {
                match &value {
                    Sql::Null => sqlite3_bind_null(self.raw, index),
                    Sql::Int(number) => sqlite3_bind_int64(self.raw, index, *number),
                    Sql::Real(number) => sqlite3_bind_double(self.raw, index, *number),
                    Sql::Text(text) => sqlite3_bind_text(self.raw, index, text.as_ptr() as *const c_char, text.len() as c_int, SQLITE_TRANSIENT),
                    Sql::Blob(bytes) => sqlite3_bind_blob(self.raw, index, bytes.as_ptr() as *const c_void, bytes.len() as c_int, SQLITE_TRANSIENT),
                }
            };
            if code != SQLITE_OK {
                return Err(self.connection.error(code));
            }
        }
        Ok(())
    }

    fn column(&self, index: c_int) -> Sql {
        unsafe {
            match sqlite3_column_type(self.raw, index) {
                1 => Sql::Int(sqlite3_column_int64(self.raw, index)),
                2 => Sql::Real(sqlite3_column_double(self.raw, index)),
                3 => {
                    let text = sqlite3_column_text(self.raw, index);
                    let length = sqlite3_column_bytes(self.raw, index) as usize;
                    if text.is_null() {
                        Sql::Text(String::new())
                    } else {
                        Sql::Text(String::from_utf8_lossy(std::slice::from_raw_parts(text, length)).into_owned())
                    }
                }
                4 => {
                    let data = sqlite3_column_blob(self.raw, index);
                    let length = sqlite3_column_bytes(self.raw, index) as usize;
                    if data.is_null() {
                        Sql::Blob(Vec::new())
                    } else {
                        Sql::Blob(std::slice::from_raw_parts(data as *const u8, length).to_vec())
                    }
                }
                _ => Sql::Null,
            }
        }
    }
}

/// Split a script at the semicolons that end statements, keeping trigger bodies together.
fn split_statements(script: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0i32; // BEGIN ... END blocks of triggers
    let mut word = String::new();
    for ch in script.chars() {
        current.push(ch);
        if let Some(open) = quote {
            if ch == open {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        if ch.is_ascii_alphabetic() {
            word.push(ch.to_ascii_uppercase());
            continue;
        }
        if word == "BEGIN" && current.to_ascii_uppercase().contains("TRIGGER") {
            depth += 1;
        } else if word == "END" && depth > 0 {
            depth -= 1;
        }
        word.clear();
        if ch == ';' && depth == 0 {
            statements.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        statements.push(current);
    }
    statements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_bind_and_transactions() {
        let connection = Connection::open(Path::new(":memory:"), Duration::from_secs(1)).unwrap();
        connection.execute_batch("CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT, score REAL); CREATE INDEX i ON t(name);").unwrap();
        assert!(!connection.in_transaction());
        connection.execute("INSERT INTO t(name, score) VALUES (?, ?)", params!["a", 1.5]).unwrap();
        assert!(connection.in_transaction());
        connection.execute("COMMIT", params![]).unwrap();
        let rows = connection.query("SELECT * FROM t WHERE name = ?", params![Value::str("a")]).unwrap();
        assert_eq!(rows[0].int("id"), 1);
        assert_eq!(rows[0].get("score"), &Sql::Real(1.5));
        assert!(connection.query("SELECT ?", params![Value::array()]).is_err());
        let error = connection.execute("INSERT INTO t(id) VALUES (1)", params![]).unwrap_err();
        assert!(error.is_constraint());
        assert_eq!(split_statements("CREATE TRIGGER x AFTER DELETE ON t BEGIN DELETE FROM t; END; SELECT 1;").len(), 2);
    }
}
