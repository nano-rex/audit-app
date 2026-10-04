"""The Super account's own database, kept apart from every organization's database.

Super accounts, their sessions, sign-in history, and page order live here. Organization
databases hold only their own people. A Super account is presented to the rest of the
application with a negative id, so it can never be mistaken for an organization user.
"""
import os
import sqlite3
import threading
import time
from datetime import datetime
from pathlib import Path

from backend import config

_SCHEMA_READY = set()
_SCHEMA_LOCK = threading.Lock()

SCHEMA = """
CREATE TABLE IF NOT EXISTS super_users(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    username TEXT NOT NULL,
    email TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    reset_required INTEGER NOT NULL DEFAULT 0,
    last_login_at TEXT,
    login_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_super_users_username ON super_users(lower(username));
CREATE UNIQUE INDEX IF NOT EXISTS idx_super_users_email ON super_users(lower(email));
CREATE TABLE IF NOT EXISTS super_sessions(
    token_hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES super_users(id) ON DELETE CASCADE,
    expires_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS super_login_activity(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES super_users(id) ON DELETE CASCADE,
    logged_at TEXT NOT NULL,
    remember_me INTEGER NOT NULL DEFAULT 0,
    user_agent TEXT,
    organization TEXT
);
CREATE TABLE IF NOT EXISTS super_navigation(
    user_id INTEGER NOT NULL REFERENCES super_users(id) ON DELETE CASCADE,
    page_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY(user_id, page_id)
);
"""


def control_path():
    """AUDIT_CONTROL_DB lets several instances share one set of Super accounts."""
    configured = os.environ.get("AUDIT_CONTROL_DB", "").strip()
    return Path(configured).resolve() if configured else config.DATA_DIR / "control" / "control.db"


class ControlConnection(sqlite3.Connection):
    """Commit or roll back, then close, like organization connections."""

    def __exit__(self, *args):
        try:
            return super().__exit__(*args)
        finally:
            self.close()


def connect_control():
    path = control_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    db = sqlite3.connect(path, timeout=15, factory=ControlConnection)
    db.row_factory = sqlite3.Row
    db.execute("PRAGMA foreign_keys = ON")
    key = str(path)
    if key not in _SCHEMA_READY:
        with _SCHEMA_LOCK:
            if key not in _SCHEMA_READY:
                db.execute("PRAGMA journal_mode=WAL")
                db.executescript(SCHEMA)
                _SCHEMA_READY.add(key)
    return db


def public_id(super_id):
    return -int(super_id)


def control_id(user):
    """The control-database id of a Super account presented to the application, else None."""
    if user and user.get("accountScope") == "control":
        return -int(user["id"])
    return None


def public_super(row, db):
    navigation = [item[0] for item in db.execute("SELECT page_id FROM super_navigation WHERE user_id = ? ORDER BY position", (row["id"],))]
    return {
        "navigationOrder": navigation,
        "id": public_id(row["id"]),
        "name": row["name"],
        "username": row["username"],
        "role": config.SUPER_ROLE,
        "email": row["email"],
        "department": "",
        "title": "Super account",
        "responsibilities": "",
        "active": bool(row["active"]),
        "lastLoginAt": row["last_login_at"] or "",
        "resetRequired": bool(row["reset_required"]),
        "permissions": [tab[0] for tab in config.APP_TABS],
        "inspectionPermissions": ["auditor", "verifier", "acknowledger"],
        "permissionOverrides": None,
        "permissionSource": "role",
        "profilePhoto": {},
        "signatureImage": {},
        # Marks an account that lives in the control database rather than the organization's.
        "accountScope": "control",
    }


def find_super(identifier):
    identifier = str(identifier or "").strip().lower()
    if not identifier:
        return None
    with connect_control() as db:
        return db.execute("SELECT * FROM super_users WHERE lower(email) = ? OR lower(username) = ?", (identifier, identifier)).fetchone()


def is_reserved_identifier(*identifiers):
    """True when an organization account would take a Super account's username or email."""
    values = [str(value or "").strip().lower() for value in identifiers if value]
    if not values:
        return False
    marks = ",".join("?" for _ in values)
    with connect_control() as db:
        return db.execute(f"SELECT 1 FROM super_users WHERE lower(email) IN ({marks}) OR lower(username) IN ({marks})", (*values, *values)).fetchone() is not None


def user_for_session(token_hash):
    with connect_control() as db:
        session = db.execute("SELECT user_id, expires_at FROM super_sessions WHERE token_hash = ?", (token_hash,)).fetchone()
        if not session:
            return None
        if session["expires_at"] < time.time():
            db.execute("DELETE FROM super_sessions WHERE token_hash = ?", (token_hash,))
            return None
        row = db.execute("SELECT * FROM super_users WHERE id = ? AND active = 1", (session["user_id"],)).fetchone()
        return public_super(row, db) if row else None


def start_session(row, token_hash, expires_at, remember, user_agent):
    now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    with connect_control() as db:
        db.execute("INSERT OR REPLACE INTO super_sessions(token_hash, user_id, expires_at) VALUES (?, ?, ?)", (token_hash, row["id"], expires_at))
        db.execute("UPDATE super_users SET last_login_at = ?, login_count = login_count + 1 WHERE id = ?", (now, row["id"]))
        db.execute("INSERT INTO super_login_activity(user_id, logged_at, remember_me, user_agent, organization) VALUES (?, ?, ?, ?, ?)",
                   (row["id"], now, 1 if remember else 0, user_agent, Path(config.DB_PATH).stem))
        return public_super(db.execute("SELECT * FROM super_users WHERE id = ?", (row["id"],)).fetchone(), db)


def end_session(token_hash):
    with connect_control() as db:
        db.execute("DELETE FROM super_sessions WHERE token_hash = ?", (token_hash,))


def password_hash(super_id):
    with connect_control() as db:
        row = db.execute("SELECT password_hash FROM super_users WHERE id = ?", (super_id,)).fetchone()
        return row["password_hash"] if row else None


def change_password(super_id, new_hash, keep_token_hash):
    with connect_control() as db:
        db.execute("UPDATE super_users SET password_hash = ?, reset_required = 0 WHERE id = ?", (new_hash, super_id))
        db.execute("DELETE FROM super_sessions WHERE user_id = ? AND token_hash != ?", (super_id, keep_token_hash))


def update_profile(super_id, name, username, email):
    with connect_control() as db:
        for column, value in (("email", email), ("username", username)):
            if db.execute(f"SELECT 1 FROM super_users WHERE lower({column}) = ? AND id != ?", (value, super_id)).fetchone():
                raise ValueError(f"That {column} belongs to another Super account")
        db.execute("UPDATE super_users SET name = ?, username = ?, email = ? WHERE id = ?", (name, username, email, super_id))
        return public_super(db.execute("SELECT * FROM super_users WHERE id = ?", (super_id,)).fetchone(), db)


def save_navigation(super_id, order):
    with connect_control() as db:
        db.execute("DELETE FROM super_navigation WHERE user_id = ?", (super_id,))
        db.executemany("INSERT INTO super_navigation(user_id, page_id, position) VALUES (?, ?, ?)",
                       [(super_id, page, position) for position, page in enumerate(order)])


def ensure_super_account(hash_password):
    """A new installation starts with one Super account; its password is published, so it must be changed."""
    with connect_control() as db:
        if db.execute("SELECT 1 FROM super_users LIMIT 1").fetchone():
            return
        db.execute("INSERT INTO super_users(name, username, email, password_hash, active, reset_required, created_at) VALUES (?, ?, ?, ?, 1, 1, ?)",
                   ("Super User", "super", "super@sudo", hash_password("doas"), int(time.time() * 1000)))


def adopt_organization_supers(org_db):
    """Move Super accounts out of an organization database into the control database.

    Their password and name are kept. What they did in the organization (inspections they own)
    stays attached to them through their new id; their sign-in history moves with them.
    """
    rows = org_db.execute("SELECT * FROM users WHERE role = ?", (config.SUPER_ROLE,)).fetchall()
    organization = Path(config.active_db_path()).stem
    for row in rows:
        email = str(row["email"] or "").strip().lower()
        username = str(row["username"] or email.split("@", 1)[0]).strip().lower()
        with connect_control() as db:
            existing = db.execute("SELECT id FROM super_users WHERE lower(email) = ? OR lower(username) = ?", (email, username)).fetchone()
            if existing:
                super_id = existing["id"]
            else:
                super_id = db.execute(
                    "INSERT INTO super_users(name, username, email, password_hash, active, reset_required, last_login_at, login_count, created_at) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (row["name"], username, email, row["password_hash"] or "", row["active"], row["reset_required"],
                     row["last_login_at"], row["login_count"] or 0, row["created_at"] or int(time.time() * 1000))).lastrowid
            history = org_db.execute("SELECT logged_at, remember_me, user_agent FROM user_login_activity WHERE user_id = ?", (row["id"],)).fetchall()
            db.executemany("INSERT INTO super_login_activity(user_id, logged_at, remember_me, user_agent, organization) VALUES (?, ?, ?, ?, ?)",
                           [(super_id, item["logged_at"], item["remember_me"], item["user_agent"], organization) for item in history])
        org_db.execute("UPDATE inspection_sessions SET owner_user_id = ? WHERE owner_user_id = ?", (public_id(super_id), row["id"]))
        for table, column in (("user_login_activity", "user_id"), ("auth_sessions", "user_id"), ("user_navigation", "user_id"),
                              ("password_reset_requests", "user_id"), ("due_notification_events", "user_id"), ("notifications", "recipient_user_id")):
            if org_db.execute("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?", (table,)).fetchone():
                org_db.execute(f"DELETE FROM {table} WHERE {column} = ?", (row["id"],))
        org_db.execute("DELETE FROM users WHERE id = ?", (row["id"],))
    # The role belongs to the control database now; organizations cannot grant it.
    org_db.execute("DELETE FROM roles WHERE name = ?", (config.SUPER_ROLE,))
    return len(rows)


class SuperSessions:
    """Sessions of Super accounts, for maintenance tools and tests: SUPER_SESSIONS[token] = {...}."""

    @staticmethod
    def key(token):
        from backend.session_store import SessionStore
        return SessionStore.key(token)

    def __setitem__(self, token, value):
        with connect_control() as db:
            db.execute("INSERT OR REPLACE INTO super_sessions(token_hash, user_id, expires_at) VALUES (?, ?, ?)",
                       (self.key(token), value["user_id"], value["expires_at"]))

    def get(self, token, default=None):
        with connect_control() as db:
            row = db.execute("SELECT user_id, expires_at FROM super_sessions WHERE token_hash = ?", (self.key(token),)).fetchone()
        return dict(row) if row else default


def first_super_id():
    with connect_control() as db:
        row = db.execute("SELECT id FROM super_users WHERE active = 1 ORDER BY id LIMIT 1").fetchone()
    return row["id"] if row else None
