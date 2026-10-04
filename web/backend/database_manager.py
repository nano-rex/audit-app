"""Super-user management for company SQLite databases."""
import re
import sqlite3
import threading
from pathlib import Path

from backend import config

DATABASE_LOCK = threading.RLock()
NAME_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$")
REQUIRED_TABLES = {"users", "roles", "auth_sessions"}
ACTIVE_MARKER = "active-database"


def _path(name):
    if not isinstance(name, str) or not NAME_PATTERN.fullmatch(name):
        raise ValueError("Database name must use 1–64 letters, numbers, hyphens, or underscores")
    return config.DATA_DIR / f"{name}.db"


def _remove_files(path):
    # A leftover WAL or shared-memory file would be replayed into a later database of the same name.
    for target in (path, path.with_name(path.name + "-wal"), path.with_name(path.name + "-shm")):
        target.unlink(missing_ok=True)


def _require_audit_database(path):
    try:
        db = sqlite3.connect(path, timeout=15)
        try:
            tables = {row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'")}
        finally:
            db.close()
    except sqlite3.DatabaseError:
        tables = set()
    if not REQUIRED_TABLES.issubset(tables):
        raise ValueError("That file is not an initialized audit database")


def list_databases():
    with DATABASE_LOCK:
        config.DATA_DIR.mkdir(parents=True, exist_ok=True)
        return [{"name": path.stem, "active": path.resolve() == Path(config.DB_PATH).resolve(), "size": path.stat().st_size}
                for path in sorted(config.DATA_DIR.glob("*.db"))]


def create_database(name):
    path = _path(name)
    with DATABASE_LOCK:
        if path.exists():
            raise ValueError("That database already exists")
        from backend.migrations import init_db
        # Only this thread sees the new path; other requests keep using the active database.
        token = config.DB_PATH_OVERRIDE.set(path)
        try:
            init_db()
        except Exception:
            _remove_files(path)
            raise
        finally:
            config.DB_PATH_OVERRIDE.reset(token)
    return {"name": path.stem, "active": False, "size": path.stat().st_size}


def switch_database(name):
    path = _path(name)
    with DATABASE_LOCK:
        if not path.is_file():
            raise ValueError("Database not found")
        _require_audit_database(path)
        # A database last used by an older release is upgraded before anyone is sent to it.
        from backend.migrations import init_db
        token = config.DB_PATH_OVERRIDE.set(path)
        try:
            init_db()
        finally:
            config.DB_PATH_OVERRIDE.reset(token)
        config.DB_PATH = path.resolve()
        config.EQUIPMENT_CACHE.clear()
        (config.DATA_DIR / ACTIVE_MARKER).write_text(path.stem, encoding="utf-8")
    return {"name": path.stem, "active": True, "requiresLogin": True}


def restore_active_database():
    """At startup, return to the database that was selected before the restart."""
    marker = config.DATA_DIR / ACTIVE_MARKER
    try:
        path = _path(marker.read_text(encoding="utf-8").strip())
    except (OSError, ValueError):
        return False
    if not path.is_file():
        return False
    config.DB_PATH = path.resolve()
    return True


def remove_database(name):
    path = _path(name)
    with DATABASE_LOCK:
        if path.resolve() == Path(config.DB_PATH).resolve():
            raise ValueError("Switch to another database before removing this one")
        if not path.is_file():
            raise ValueError("Database not found")
        _remove_files(path)
    return {"name": path.stem, "removed": True}
