#!/usr/bin/env python3
"""Application entry point; domain logic and HTTP routes live in focused modules."""
import os
from pathlib import Path
import sys
import threading

sys.path.insert(0, str(Path(__file__).resolve().parent))
import config
from api import Handler
from reminders import reminder_loop
from http_support import AuditHTTPServer
from database import connect
from migrations import init_db
from common import hash_password, verify_password, inspection_progress
from response_cache import cached_response
from catalog import setup_records
from reports import report, dashboard

# Compatibility surface for existing maintenance scripts and test harnesses.
__all__ = ["Handler", "AuditHTTPServer", "connect", "init_db", "hash_password",
           "verify_password", "inspection_progress", "cached_response",
           "setup_records", "report", "dashboard", "configure_data_directory", "SESSION_TOKENS"]

SESSION_TOKENS = config.SESSION_TOKENS


def configure_data_directory(directory):
    config.DATA_DIR = Path(directory).resolve()
    config.DB_PATH = config.DATA_DIR / "ottotree_audit_web.db"
    config.EQUIPMENT_CACHE.clear()


if __name__ == "__main__":
    init_db()
    port = int(os.environ.get("PORT", "41883"))
    server = AuditHTTPServer(("127.0.0.1", port), Handler)
    print(f"Serving Ottotree Audit at http://127.0.0.1:{port}")
    print(f"SQLite database: {config.DB_PATH}")
    stop = threading.Event()
    threading.Thread(target=reminder_loop, args=(stop,), daemon=True).start()
    try:
        server.serve_forever()
    finally:
        stop.set()
        server.server_close()
