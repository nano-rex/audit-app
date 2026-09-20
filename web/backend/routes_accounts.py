"""Routes accounts for the audit application."""
from backend.relational_values import load_value, save_value
import json
import sqlite3
import time
import os
import secrets
import re
from datetime import datetime
from backend.accounts import is_company_admin_user, is_super_user, public_user
from backend.common import hash_password, verify_password
from backend.config import APP_TABS, DEFAULT_PASSWORD, SESSION_TOKENS, SUPER_ROLE
from backend.database import connect, first_department
from backend.workflow import WorkflowError
from backend.permissions import INSPECTION_PERMISSIONS, validate_list, validate_overrides


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
                   last_login_at, login_count, title, responsibilities, profile_photo_data_id, signature_image_data_id
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


def patch_account(self, parsed, payload=None):
    user = self.current_user()
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        existing = db.execute("SELECT * FROM users WHERE id = ?", (user["id"],)).fetchone()
        name = str(payload.get("name", existing["name"]) or "").strip()
        email = str(payload.get("email", existing["email"]) or "").strip().lower()
        role = payload.get("role", existing["role"])
        department = payload.get("department", existing["department"])
        if not name or len(name) > 100 or not re.fullmatch(r"[^@\s]+@[^@\s]+\.[^@\s]+", email):
            self.json({"error": "Enter a name (up to 100 characters) and a valid email address"}, 400)
            return
        if not is_company_admin_user(user) and (role != existing["role"] or department != existing["department"]):
            self.json({"error": "An administrator must change your role or department"}, 403)
            return
        if role == SUPER_ROLE and not is_super_user(user):
            self.json({"error": "Only a Super user can assign the Super role"}, 403)
            return
        if existing["role"] == SUPER_ROLE and role != SUPER_ROLE and not db.execute("SELECT 1 FROM users WHERE role = ? AND active = 1 AND id != ?", (SUPER_ROLE, user["id"])).fetchone():
            self.json({"error": "Keep at least one active Super user"}, 409)
            return
        if not db.execute("SELECT 1 FROM roles WHERE name = ?", (role,)).fetchone():
            self.json({"error": "Select an existing role"}, 400)
            return
        if department and not db.execute("SELECT 1 FROM departments WHERE code = ?", (department,)).fetchone():
            self.json({"error": "Select an existing department"}, 400)
            return
        if db.execute("SELECT 1 FROM users WHERE lower(email) = ? AND id != ?", (email, user["id"])).fetchone():
            self.json({"error": "That email address belongs to another account"}, 409)
            return
        photo = payload.get("profilePhoto", load_value(existing["profile_photo_data_id"] or "{}")) or {}
        if not isinstance(photo, dict) or photo and not photo.get("url", "").startswith("/api/media/"):
            self.json({"error": "Upload a profile picture first"}, 400)
            return
        signature = payload.get("signatureImage", load_value(existing["signature_image_data_id"] or "{}")) or {}
        if not isinstance(signature, dict) or signature and not signature.get("url", "").startswith("/api/media/"):
            self.json({"error": "Upload a signature image first"}, 400)
            return
        db.execute("UPDATE users SET name = ?, email = ?, department = ?, role = ?, profile_photo_data_id = ?, signature_image_data_id = ? WHERE id = ?",
                   (name, email, department, role, save_value(db, photo), save_value(db, signature), user["id"]))
        refreshed = db.execute("SELECT * FROM users WHERE id = ?", (user["id"],)).fetchone()
    self.json({"ok": True, "user": public_user(refreshed)})


def post_auth_forgot_password(self, parsed, payload=None):
    email = str(payload.get("email") or "").strip().lower()
    now = int(time.time() * 1000)
    with connect() as db:
        user = db.execute("SELECT id, name FROM users WHERE lower(email) = ? AND active = 1", (email,)).fetchone()
        if user:
            previous = db.execute("SELECT requested_at FROM password_reset_requests WHERE user_id = ?", (user["id"],)).fetchone()
            if not previous or now - previous["requested_at"] >= 15 * 60 * 1000:
                db.execute("INSERT INTO password_reset_requests(user_id, requested_at) VALUES (?, ?) ON CONFLICT(user_id) DO UPDATE SET requested_at = excluded.requested_at, resolved_at = NULL", (user["id"], now))
                for admin in db.execute("SELECT id FROM users WHERE role = ? AND active = 1", (SUPER_ROLE,)).fetchall():
                    db.execute("INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread','user',?,?,?)",
                               ("Password reset requested", f"{user['name']} requested a password reset. Review this account in Users.", user["id"], now, admin["id"]))
    self.json({"ok": True, "message": "If an active account matches, a reset request has been sent to your administrator. Contact them to verify your identity and receive a temporary password."})


def post_auth_register(self, parsed, payload=None):
    now = int(time.time() * 1000)
    name = (payload.get("name") or "").strip()
    email = (payload.get("email") or "").strip().lower()
    password = payload.get("password") or ""
    if not name or not re.fullmatch(r"[^@\s]+@[^@\s]+\.[^@\s]+", email) or len(password) < 8:
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
        db.execute("DELETE FROM auth_sessions WHERE user_id = ? AND token_hash != ?", (user["id"], SESSION_TOKENS.key(self.session_token())))
        db.execute("UPDATE password_reset_requests SET resolved_at = ? WHERE user_id = ?", (int(time.time() * 1000), user["id"]))
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
        cursor = db.execute(
            """
            INSERT INTO users
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
        overrides = validate_overrides(payload.get("permissionOverrides"))
        db.execute("UPDATE users SET permission_overrides_data_id = ? WHERE id = ?", (save_value(db, overrides) if overrides is not None else None, cursor.lastrowid))
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
            INSERT INTO roles (name, description, permissions_data_id, inspection_permissions_data_id, protected, created_at)
            VALUES (?, ?, ?, ?, 0, ?)
            """,
            (
                name,
                payload.get("description", ""),
                save_value(db, validate_list(payload.get("permissions", []), {tab[0] for tab in APP_TABS})),
                save_value(db, validate_list(payload.get("inspectionPermissions", []), INSPECTION_PERMISSIONS)),
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
        db.execute("BEGIN IMMEDIATE")
        existing_user = db.execute("SELECT * FROM users WHERE id = ?", (int(user_id),)).fetchone()
        if not existing_user:
            raise WorkflowError("User not found", 404)
        fields = {"name": "name", "role": "role", "email": "email", "department": "department", "active": "active", "resetRequired": "reset_required", "title": "title", "responsibilities": "responsibilities"}
        payload = {key: existing_user[column] for key, column in fields.items()} | payload
        if existing_user["role"] == SUPER_ROLE and existing_user["active"] and (payload["role"] != SUPER_ROLE or not payload["active"]):
            protect_last_super(db, int(user_id))
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
        if not payload["active"]:
            db.execute("DELETE FROM auth_sessions WHERE user_id = ?", (int(user_id),))
        if reset_password or password:
            db.execute("DELETE FROM auth_sessions WHERE user_id = ?", (int(user_id),))
            db.execute("UPDATE password_reset_requests SET resolved_at = ? WHERE user_id = ?", (int(time.time() * 1000), int(user_id)))
        if "permissionOverrides" in payload:
            overrides = validate_overrides(payload["permissionOverrides"])
            db.execute("UPDATE users SET permission_overrides_data_id = ? WHERE id = ?", (save_value(db, overrides) if overrides is not None else None, int(user_id)))
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
        role = db.execute("SELECT name, protected, inspection_permissions_data_id FROM roles WHERE id = ?", (int(role_id),)).fetchone()
        if not role:
            self.send_error(404)
            return
        if role["protected"]:
            self.json({"ok": False, "error": "The Super role cannot be changed"}, status=400)
            return
        name = (payload.get("name") or "New Role").strip()
        if name.lower() == "super" or name.lower() == "admin" and role["name"] != "Admin":
            self.json({"ok": False, "error": "Super is reserved"}, status=400)
            return
        cursor = db.execute(
            """
            UPDATE roles
            SET name = ?, description = ?, permissions_data_id = ?, inspection_permissions_data_id = ?
            WHERE id = ?
            """,
            (
                name,
                payload.get("description", ""),
                save_value(db, validate_list(payload.get("permissions", []), {tab[0] for tab in APP_TABS})),
                save_value(db, validate_list(payload.get("inspectionPermissions", load_value(role["inspection_permissions_data_id"] or "[]")), INSPECTION_PERMISSIONS)),
                int(role_id),
            ),
        )
        if cursor.rowcount == 0:
            self.send_error(404)
            return
        if name != role["name"]:
            db.execute("UPDATE users SET role = ? WHERE role = ?", (name, role["name"]))
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
        db.execute("BEGIN IMMEDIATE")
        existing_user = db.execute("SELECT role, active FROM users WHERE id = ?", (int(record_id),)).fetchone()
        if not is_super_user(self.current_user()) and existing_user and existing_user["role"] == SUPER_ROLE:
            self.json({"ok": False, "error": "Super users require Super access"}, status=403)
            return
        if existing_user and existing_user["role"] == SUPER_ROLE and existing_user["active"]:
            protect_last_super(db, int(record_id))
        if db.execute("SELECT 1 FROM user_login_activity WHERE user_id = ? LIMIT 1", (int(record_id),)).fetchone() or db.execute("SELECT 1 FROM inspection_sessions WHERE owner_user_id = ? LIMIT 1", (int(record_id),)).fetchone():
            raise WorkflowError("This account has login or audit history. Deactivate it to retain that history.")
        db.execute("DELETE FROM user_navigation WHERE user_id = ?", (int(record_id),))
        db.execute("DELETE FROM auth_sessions WHERE user_id = ?", (int(record_id),))
        db.execute("DELETE FROM password_reset_requests WHERE user_id = ?", (int(record_id),))
        db.execute("DELETE FROM notifications WHERE recipient_user_id = ?", (int(record_id),))
        db.execute("DELETE FROM due_notification_events WHERE user_id = ?", (int(record_id),))
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


def protect_last_super(db, user_id):
    if not db.execute("SELECT 1 FROM users WHERE role = ? AND active = 1 AND id != ?", (SUPER_ROLE, user_id)).fetchone():
        raise WorkflowError("Keep at least one active Super user")


def patch_navigation(self, parsed, payload=None):
    order = payload.get("order")
    pages = ({tab[0] for tab in APP_TABS} | {"account"}) - {"departments", "roles"}
    if not isinstance(order, list) or len(order) > len(pages) or any(not isinstance(page, str) or page not in pages for page in order) or len(set(order)) != len(order):
        raise ValueError("Choose each available page at most once")
    user_id = self.current_user()["id"]
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        db.execute("DELETE FROM user_navigation WHERE user_id = ?", (user_id,))
        db.executemany("INSERT INTO user_navigation(user_id,page_id,position) VALUES (?,?,?)", [(user_id, page, position) for position, page in enumerate(order)])
    self.json({"ok": True, "navigationOrder": order})
