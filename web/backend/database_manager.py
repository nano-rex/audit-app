"""Super-user management for company SQLite databases."""
import re
import threading
from pathlib import Path

from backend import config

DATABASE_LOCK = threading.RLock()
NAME_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$")


def _path(name):
    if not isinstance(name, str) or not NAME_PATTERN.fullmatch(name):
        raise ValueError("Database name must use 1–64 letters, numbers, hyphens, or underscores")
    return config.DATA_DIR / f"{name}.db"


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
        previous = config.DB_PATH
        try:
            config.DB_PATH = path
            init_db()
        except Exception:
            path.unlink(missing_ok=True)
            raise
        finally:
            config.DB_PATH = previous
    return {"name": path.stem, "active": False, "size": path.stat().st_size}


def switch_database(name):
    path = _path(name)
    with DATABASE_LOCK:
        if not path.is_file():
            raise ValueError("Database not found")
        config.DB_PATH = path.resolve()
        config.EQUIPMENT_CACHE.clear()
    return {"name": path.stem, "active": True, "requiresLogin": True}


def remove_database(name):
    path = _path(name)
    with DATABASE_LOCK:
        if path.resolve() == Path(config.DB_PATH).resolve():
            raise ValueError("Switch to another database before removing this one")
        if not path.is_file():
            raise ValueError("Database not found")
        path.unlink()
    return {"name": path.stem, "removed": True}
