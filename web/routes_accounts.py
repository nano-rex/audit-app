"""Routes accounts for the audit application."""
import json
import sqlite3
import time
import os
import secrets
from datetime import datetime
from accounts import is_company_admin_user, is_super_user, public_user
from common import hash_password, verify_password
from config import DEFAULT_PASSWORD, SESSION_TOKENS, SUPER_ROLE
from database import connect, first_department


def post_auth_login(self, parsed, payload=None):
    email = (payload.get("email") or "").strip().lower()
    password = payload.get("password") or ""
    remember = bool(payload.get("remember"))
    with connect() as db:
        row = db.execute(
            """
            SELECT id, name, role, email, department, password_hash, active,
                   reset_required, last_login_at, login_count, title, responsibilities
            FROM users
            WHERE lower(email) = ?
            """,
            (email,),
        ).fetchone()
        if not row or not row["active"] or not verify_password(password, row["password_hash"]):
            self.json({"ok": False, "error": "Invalid email or password"}, status=401)
            return
        if not row["password_hash"].startswith("pbkdf2_sha256$"):
            db.execute("UPDATE users SET password_hash = ? WHERE id = ?", (hash_password(password), row["id"]))
        token = secrets.token_urlsafe(32)
        max_age = 60 * 60 * 24 * 30 if remember else 60 * 60 * 8
        SESSION_TOKENS[token] = {"user_id": row["id"], "expires_at": time.time() + max_age}
        logged_at = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        db.execute(
            "UPDATE users SET last_login_at = ?, login_count = COALESCE(login_count, 0) + 1 WHERE id = ?",
            (logged_at, row["id"]),
        )
        db.execute(
            """
            INSERT INTO user_login_activity (user_id, email, logged_at, remember_me, user_agent)
            VALUES (?, ?, ?, ?, ?)
            """,
            (row["id"], row["email"], logged_at, 1 if remember else 0, self.headers.get("User-Agent", "")),
        )
        refreshed = db.execute(
            """
            SELECT id, name, role, email, department, active, reset_required,
                   last_login_at, login_count, title, responsibilities
            FROM users
            WHERE id = ?
            """,
            (row["id"],),
        ).fetchone()
    self.send_response(200)
    self.send_header("Content-Type", "application/json")
    self.send_header("Cache-Control", "no-store")
    secure = "; Secure" if os.environ.get("AUDIT_SECURE_COOKIES") == "1" else ""
    self.send_header("Set-Cookie", f"ottotree_session={token}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}")
    self.end_headers()
    self.wfile.write(json.dumps({"ok": True, "user": public_user(refreshed)}).encode("utf-8"))
    return


def post_auth_logout(self, parsed, payload=None):
    SESSION_TOKENS.pop(self.session_token(), None)
    self.send_response(200)
    self.send_header("Content-Type", "application/json")
    self.send_header("Cache-Control", "no-store")
    self.send_header("Set-Cookie", "ottotree_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
    self.end_headers()
    self.wfile.write(json.dumps({"ok": True}).encode("utf-8"))
    return


def post_auth_forgot_password(self, parsed, payload=None):
    self.json({"ok": True, "message": "Ask a Super user to reset this user's password from Users setup."})
    return


def post_auth_register(self, parsed, payload=None):
    now = int(time.time() * 1000)
    name = (payload.get("name") or "").strip()
    email = (payload.get("email") or "").strip().lower()
    password = payload.get("password") or ""
    if not name or not email or len(password) < 8:
        self.json({"ok": False, "error": "Name, email, and an 8-character password are required"}, status=400)
        return
    with connect() as db:
        try:
            db.execute(
                """
                INSERT INTO users
                (name, role, email, department, password_hash, active, reset_required, title, responsibilities, created_at)
                VALUES (?, '', ?, '', ?, 0, 0, '', '', ?)
                """,
                (name, email, hash_password(password), now),
            )
        except sqlite3.IntegrityError:
            self.json({"ok": False, "error": "An account with this email already exists"}, status=409)
            return
    self.json({"ok": True, "message": "Account registered. A Super user must activate it and assign a role before login."})
    return


def post_auth_change_password(self, parsed, payload=None):
    user = self.current_user()
    if not user:
        self.json({"ok": False, "error": "Login required"}, status=401)
        return
    old_password = payload.get("oldPassword") or ""
    new_password = payload.get("newPassword") or ""
    if len(new_password) < 8:
        self.json({"ok": False, "error": "New password must be at least 8 characters"}, status=400)
        return
    with connect() as db:
        row = db.execute("SELECT password_hash FROM users WHERE id = ?", (user["id"],)).fetchone()
        if not row or not verify_password(old_password, row["password_hash"]):
            self.json({"ok": False, "error": "Current password is incorrect"}, status=400)
            return
        db.execute(
            "UPDATE users SET password_hash = ?, reset_required = 0 WHERE id = ?",
            (hash_password(new_password), user["id"]),
        )
    self.json({"ok": True})
    return


def post_users(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        default_department = first_department(db)
        if not is_company_admin_user(self.current_user()):
            self.json({"ok": False, "error": "Admin access required"}, status=403)
            return
        if not is_super_user(self.current_user()) and payload.get("role") == SUPER_ROLE:
            self.json({"ok": False, "error": "Super role assignment requires Super access"}, status=403)
            return
        password = payload.get("password") or DEFAULT_PASSWORD
        db.execute(
            """
            INSERT OR REPLACE INTO users
            (name, role, email, department, password_hash, active, reset_required, title, responsibilities, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                payload.get("name", "New User"),
                payload.get("role", ""),
                payload.get("email", "user@example.com"),
                payload.get("department") or default_department,
                hash_password(password),
                1 if payload.get("active", True) else 0,
                1 if payload.get("resetRequired", False) else 0,
                payload.get("title", ""),
                payload.get("responsibilities", ""),
                now,
            ),
        )
    self.json({"ok": True})


def post_roles(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        if not is_super_user(self.current_user()):
            self.json({"ok": False, "error": "Super access required"}, status=403)
            return
        name = (payload.get("name") or "New Role").strip()
        if name.lower() in ("admin", "super"):
            self.json({"ok": False, "error": "The Super role is built in and cannot be recreated"}, status=400)
            return
        db.execute(
            """
            INSERT OR REPLACE INTO roles (name, description, permissions_json, protected, created_at)
            VALUES (?, ?, ?, 0, ?)
            """,
            (
                name,
                payload.get("description", ""),
                json.dumps(payload.get("permissions") or []),
                now,
            ),
        )
    self.json({"ok": True})


def patch_users(self, parsed, payload=None):
    user_id = parsed.path.rsplit("/", 1)[-1]
    if not user_id.isdigit():
        self.send_error(400)
        return
    if not is_company_admin_user(self.current_user()):
        self.json({"ok": False, "error": "Admin access required"}, status=403)
        return
    with connect() as db:
        existing_user = db.execute("SELECT role FROM users WHERE id = ?", (int(user_id),)).fetchone()
        if not is_super_user(self.current_user()) and (payload.get("role") == SUPER_ROLE or (existing_user and existing_user["role"] == SUPER_ROLE)):
            self.json({"ok": False, "error": "Super users require Super access"}, status=403)
            return
        password = payload.get("password") or ""
        reset_password = bool(payload.get("resetPassword"))
        updates = [
            payload.get("name", "New User"),
            payload.get("role", ""),
            payload.get("email", "user@example.com"),
            payload.get("department") or first_department(db),
            1 if payload.get("active", True) else 0,
            1 if payload.get("resetRequired", False) else 0,
            payload.get("title", ""),
            payload.get("responsibilities", ""),
        ]
        password_sql = ""
        if reset_password or password:
            password_sql = ", password_hash = ?"
            updates.append(hash_password(password or DEFAULT_PASSWORD))
            if reset_password:
                updates[5] = 1
        updates.append(int(user_id))
        cursor = db.execute(
            f"""
            UPDATE users
            SET name = ?, role = ?, email = ?, department = ?, active = ?, reset_required = ?,
                title = ?, responsibilities = ?{password_sql}
            WHERE id = ?
            """,
            tuple(updates),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def patch_roles(self, parsed, payload=None):
    role_id = parsed.path.rsplit("/", 1)[-1]
    if not role_id.isdigit():
        self.send_error(400)
        return
    if not is_super_user(self.current_user()):
        self.json({"ok": False, "error": "Super access required"}, status=403)
        return
    with connect() as db:
        role = db.execute("SELECT protected FROM roles WHERE id = ?", (int(role_id),)).fetchone()
        if not role:
            self.send_error(404)
            return
        if role["protected"]:
            self.json({"ok": False, "error": "The Super role cannot be changed"}, status=400)
            return
        name = (payload.get("name") or "New Role").strip()
        if name.lower() in ("admin", "super"):
            self.json({"ok": False, "error": "Super is reserved"}, status=400)
            return
        cursor = db.execute(
            """
            UPDATE roles
            SET name = ?, description = ?, permissions_json = ?
            WHERE id = ?
            """,
            (
                name,
                payload.get("description", ""),
                json.dumps(payload.get("permissions") or []),
                int(role_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_users(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    if not is_company_admin_user(self.current_user()):
        self.json({"ok": False, "error": "Admin access required"}, status=403)
        return
    with connect() as db:
        existing_user = db.execute("SELECT role FROM users WHERE id = ?", (int(record_id),)).fetchone()
        if not is_super_user(self.current_user()) and existing_user and existing_user["role"] == SUPER_ROLE:
            self.json({"ok": False, "error": "Super users require Super access"}, status=403)
            return
        cursor = db.execute("DELETE FROM users WHERE id = ?", (int(record_id),))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return


def delete_roles(self, parsed, payload=None):
    record_id = parsed.path.rsplit("/", 1)[-1]
    if not record_id.isdigit():
        self.send_error(400)
        return
    if not is_super_user(self.current_user()):
        self.json({"ok": False, "error": "Super access required"}, status=403)
        return
    with connect() as db:
        role = db.execute("SELECT name, protected FROM roles WHERE id = ?", (int(record_id),)).fetchone()
        if not role:
            self.send_error(404)
            return
        if role["protected"]:
            self.json({"ok": False, "error": "The Super role cannot be deleted"}, status=400)
            return
        cursor = db.execute("DELETE FROM roles WHERE id = ?", (int(record_id),))
        db.execute("UPDATE users SET role = '' WHERE role = ?", (role["name"],))
        if cursor.rowcount == 0:
            self.send_error(404)
            return
    self.json({"ok": True})
    return
