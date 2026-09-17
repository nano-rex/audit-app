"""Routes setup for the audit application."""
import json
import time
from accounts import is_super_user
from database import connect


def post_setup_departments(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute(
            """
            INSERT OR REPLACE INTO departments (code, description, responsibilities, created_at)
            VALUES (?, ?, ?, ?)
            """,
            (
                (payload.get("code") or "Department").upper(),
                payload.get("description", ""),
                payload.get("responsibilities", ""),
                now,
            ),
        )
    self.json({"ok": True})


def post_setup_categories(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute(
            """
            INSERT OR REPLACE INTO categories (name, description, sequence, active, created_at)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                payload.get("name", "New Category"),
                payload.get("description", ""),
                max(0, int(payload.get("sequence") or 0)),
                1 if payload.get("active", True) else 0,
                now,
            ),
        )
    self.json({"ok": True})


def post_setup_priorities(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute(
            """
            INSERT OR REPLACE INTO priority_levels (name, classification, due_days, active, created_at)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                payload.get("name", "Priority"),
                payload.get("classification", "Priority"),
                int(payload.get("dueDays") or 0),
                1 if payload.get("active", True) else 0,
                now,
            ),
        )
    self.json({"ok": True})


def post_setup_audit_types(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute(
            """
            INSERT OR REPLACE INTO audit_types (name, description, active, created_at)
            VALUES (?, ?, ?, ?)
            """,
            (
                payload.get("name", "Standard"),
                payload.get("description", ""),
                1 if payload.get("active", True) else 0,
                now,
            ),
        )
    self.json({"ok": True})


def post_settings(self, parsed, payload=None):
    with connect() as db:
        if not is_super_user(self.current_user()):
            self.json({"ok": False, "error": "Super access required"}, status=403)
            return
        for key, value in (payload.get("settings") or {}).items():
            db.execute("INSERT OR REPLACE INTO app_settings (key, value) VALUES (?, ?)", (key, json.dumps(value)))
    self.json({"ok": True})


def patch_setup_priorities(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute(
            """
            UPDATE priority_levels
            SET name = ?, classification = ?, due_days = ?, active = ?
            WHERE id = ?
            """,
            (
                payload.get("name", "Priority"),
                payload.get("classification", "Priority"),
                int(payload.get("dueDays") or 0),
                1 if payload.get("active", True) else 0,
                int(record_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def patch_setup_audit_types(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute(
            """
            UPDATE audit_types
            SET name = ?, description = ?, active = ?
            WHERE id = ?
            """,
            (
                payload.get("name", "Standard"),
                payload.get("description", ""),
                1 if payload.get("active", True) else 0,
                int(record_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def patch_setup_departments(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute(
            """
            UPDATE departments
            SET code = ?, description = ?, responsibilities = ?
            WHERE id = ?
            """,
            (
                (payload.get("code") or "Department").upper(),
                payload.get("description", ""),
                payload.get("responsibilities", ""),
                int(record_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def patch_setup_categories(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute(
            """
            UPDATE categories
            SET name = ?, description = ?, sequence = ?, active = ?
            WHERE id = ?
            """,
            (
                payload.get("name", "New Category"),
                payload.get("description", ""),
                max(0, int(payload.get("sequence") or 0)),
                1 if payload.get("active", True) else 0,
                int(record_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_setup_departments(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute("DELETE FROM departments WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_setup_categories(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute("DELETE FROM categories WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_setup_priorities(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute("DELETE FROM priority_levels WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_setup_audit_types(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    with connect() as db:
        cursor = db.execute("DELETE FROM audit_types WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return
