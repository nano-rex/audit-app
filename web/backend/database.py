"""Database for the audit application."""
import sqlite3
from backend import config
from backend.relational_values import ACTIVE_CONNECTION
from backend.config import EQUIPMENT_CACHE, EQUIPMENT_CACHE_LOCK


class DatabaseConnection(sqlite3.Connection):
    """Commit/rollback and release the connection at the end of each operation."""

    def __enter__(self):
        self._active_token = ACTIVE_CONNECTION.set(self)
        return super().__enter__()

    def __exit__(self, *args):
        try:
            changed = self.total_changes > 0
            if changed and args[0] is None and self.execute("SELECT 1 FROM sqlite_master WHERE name = 'pending_value_cleanup'").fetchone():
                from backend.relational_values import collect_pending_values
                collect_pending_values(self)
            result = super().__exit__(*args)
            if changed:
                with EQUIPMENT_CACHE_LOCK:
                    EQUIPMENT_CACHE.clear()
            return result
        finally:
            ACTIVE_CONNECTION.reset(self._active_token)
            self.close()


def connect():
    config.DATA_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(config.DB_PATH, timeout=15, factory=DatabaseConnection)
    conn.database_path = config.DB_PATH
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON")
    return conn


def first_department(db):
    row = db.execute("SELECT code FROM departments ORDER BY code LIMIT 1").fetchone()
    return row["code"] if row else ""


def first_category(db):
    row = db.execute("SELECT name FROM categories WHERE active = 1 ORDER BY sequence, name LIMIT 1").fetchone()
    return row["name"] if row else "Others"


def first_outlet(db):
    row = db.execute("SELECT code FROM outlets ORDER BY code LIMIT 1").fetchone()
    return row["code"] if row else ""


def insert_record(db, table, values):
    columns = ", ".join(values)
    placeholders = ", ".join("?" for _ in values)
    return db.execute(f"INSERT INTO {table} ({columns}) VALUES ({placeholders})", tuple(values.values())).lastrowid
