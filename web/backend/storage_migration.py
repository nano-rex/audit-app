"""Create a SQLite backup before the one-way relational storage migration."""
from datetime import datetime, timezone
from pathlib import Path
import sqlite3

from backend.relational_values import FIELDS


def backup_legacy_database(path):
    path = Path(path)
    if not path.is_file():
        return None
    with sqlite3.connect(path) as source:
        legacy = any(set(fields).intersection(row[1] for row in source.execute(f'PRAGMA table_info("{table}")')) for table, fields in FIELDS.items())
        if not legacy:
            return None
        directory = path.parent / "backups"
        directory.mkdir(exist_ok=True)
        target = directory / f"before-relational-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')}.db"
        with sqlite3.connect(target) as destination:
            source.backup(destination)
            if destination.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                raise ValueError("Database backup integrity check failed")
    return target
