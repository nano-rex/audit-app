"""Routes setup for the audit application."""
from backend.relational_values import save_value, load_value
import re
import time
from backend import config
from backend.config import THEME_CHOICES
from backend.media_store import MediaStore
from backend.scoring import validate_settings
from backend.accounts import is_company_admin_user, is_super_user
from backend.database import connect


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


def validate_theme(key, value):
    name = key.removeprefix("theme.")
    if name in THEME_CHOICES:
        if value not in THEME_CHOICES[name]:
            raise ValueError(f"Choose a valid theme {name}")
    elif name == "accent":
        if value != "" and not (isinstance(value, str) and re.fullmatch(r"#[0-9a-fA-F]{6}", value)):
            raise ValueError("The accent colour must look like #1e99b4")
    elif name == "userChoice":
        if not isinstance(value, bool):
            raise ValueError("Choose whether users may pick light or dark")
    else:
        raise ValueError("Unknown theme setting")


def category_department(db, payload):
    """The department that normally handles findings in this category; empty means no default."""
    department = str(payload.get("department") or "").strip()
    if department and not db.execute("SELECT 1 FROM departments WHERE code = ?", (department,)).fetchone():
        raise ValueError("Select an existing department")
    return department


def post_setup_categories(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute(
            """
            INSERT OR REPLACE INTO categories (name, description, sequence, active, department, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            (
                payload.get("name", "New Category"),
                payload.get("description", ""),
                max(0, int(payload.get("sequence") or 0)),
                1 if payload.get("active", True) else 0,
                category_department(db, payload),
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
        user = self.current_user()
        if not is_company_admin_user(user):
            self.json({"ok": False, "error": "Admin access required"}, status=403)
            return
        incoming = payload.get("settings") or {}
        if not is_super_user(user):
            incoming = {key: value for key, value in incoming.items()
                        if key in {"system.findingsEnabled", "system.requirePhotoEveryAsset"}}
        for key, value in incoming.items():
            if key.startswith("theme."):
                validate_theme(key, value)
        if any(key.startswith("scoring.") for key in incoming):
            saved = {row["key"]: load_value(row["value_data_id"]) for row in db.execute("SELECT * FROM app_settings WHERE key LIKE 'scoring.%'")}
            validate_settings(saved | incoming)
        for key, value in incoming.items():
            if key == "report.logoUrl" and value:
                MediaStore(config.DB_PATH).normalize({"url": value})
            db.execute("INSERT OR REPLACE INTO app_settings (key, value_data_id) VALUES (?, ?)", (key, save_value(db, value)))
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
        db.execute("BEGIN IMMEDIATE")
        existing = db.execute("SELECT name FROM categories WHERE id = ?", (int(record_id),)).fetchone()
        if not existing:
            self.send_error(404)
            return
        name = payload.get("name", "New Category")
        db.execute(
            """
            UPDATE categories
            SET name = ?, description = ?, sequence = ?, active = ?, department = ?
            WHERE id = ?
            """,
            (
                name,
                payload.get("description", ""),
                max(0, int(payload.get("sequence") or 0)),
                1 if payload.get("active", True) else 0,
                category_department(db, payload),
                int(record_id),
            ),
        )
        if name != existing["name"]:
            # Items carry the category by name; keep them attached through a rename.
            db.execute("UPDATE equipment SET category = ? WHERE category = ?", (name, existing["name"]))
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
        db.execute("UPDATE equipment SET category = '' WHERE category = (SELECT name FROM categories WHERE id = ?)", (int(record_id),))
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
