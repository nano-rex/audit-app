#!/usr/bin/env python3
"""Application entry point; domain logic and HTTP routes live in focused modules."""
import importlib.util
import os
from pathlib import Path
import sys
import threading

sys.path.insert(0, str(Path(__file__).resolve().parent))
from backend import config
from backend.api import Handler
from backend.reminders import reminder_loop
from backend.http_support import AuditHTTPServer
from backend.database import connect
from backend.database_manager import restore_active_database
from backend.control import SuperSessions, first_super_id
from backend.migrations import init_db
from backend.common import hash_password, verify_password, inspection_progress
from backend.response_cache import cached_response
from backend.catalog import setup_records
from backend.reports import report, dashboard

# Compatibility surface for existing maintenance scripts and test harnesses.
__all__ = ["Handler", "AuditHTTPServer", "connect", "init_db", "hash_password",
           "verify_password", "inspection_progress", "cached_response",
           "setup_records", "report", "dashboard", "configure_data_directory", "SESSION_TOKENS",
           "SUPER_SESSIONS", "first_super_id"]

SESSION_TOKENS = config.SESSION_TOKENS
SUPER_SESSIONS = SuperSessions()


def configure_data_directory(directory):
    config.DATA_DIR = Path(directory).resolve()
    config.DB_PATH = config.DATA_DIR / "ottotree_audit_web.db"
    config.EQUIPMENT_CACHE.clear()


if __name__ == "__main__":
    restore_active_database()
    init_db()
    port = int(os.environ.get("PORT", "41883"))
    server = AuditHTTPServer(("127.0.0.1", port), Handler)
    print(f"Serving Audit App at http://127.0.0.1:{port}", flush=True)
    print(f"SQLite database: {config.DB_PATH}", flush=True)
    # Exports and photo thumbnails need these; without them the rest of the app still runs.
    missing = [name for module, name in (("reportlab", "reportlab (PDF export)"), ("openpyxl", "openpyxl (Excel export)"), ("PIL", "Pillow (photo thumbnails)"))
               if importlib.util.find_spec(module) is None]
    if missing:
        print(f"WARNING: {sys.executable} is missing {', '.join(missing)}. Start the server with the project's "
              "virtual environment (.venv/bin/python web/server.py) or install requirements.txt.", flush=True)
    stop = threading.Event()
    threading.Thread(target=reminder_loop, args=(stop,), daemon=True).start()
    try:
        server.serve_forever()
    finally:
        stop.set()
        server.server_close()
