"""Routes accounts for the audit application."""
from backend.relational_values import load_value, save_value
import json
import sqlite3
import time
from backend.outlet_access import validate_outlets
import os
import secrets
import re
from datetime import datetime
from backend import config
from backend.accounts import is_company_admin_user, is_super_user, public_user
from backend.common import hash_password, verify_password
from backend.config import ADMIN_ROLE, APP_TABS, DEFAULT_PASSWORD, SESSION_TOKENS, SUPER_ROLE, SUPER_TABS
from backend.database import connect, first_department, insert_record
from backend.workflow import WorkflowError
from backend.permissions import INSPECTION_PERMISSIONS, validate_list, validate_overrides
from backend import control
from backend.database_manager import create_database, remove_database, switch_database
from backend.login_throttle import LOGIN_THROTTLE, REGISTRATION_THROTTLE, client_address

EMAIL_PATTERN = re.compile(r"[^@\s]+@[^@\s]+\.[^@\s]+")
USERNAME_PATTERN = re.compile(r"[a-z0-9][a-z0-9._-]{2,63}")
MIN_PASSWORD_LENGTH = 8
_unknown_account_hash = None


def unknown_account_hash():
    """A hash nobody can match, so a sign-in for a missing account costs the same as a real one."""
    global _unknown_account_hash
    if _unknown_account_hash is None:
        _unknown_account_hash = hash_password(secrets.token_urlsafe(32))
    return _unknown_account_hash


def post_auth_login(self, parsed, payload=None):
    identifier = str(payload.get("identifier") or payload.get("email") or payload.get("username") or "").strip().lower()
    password = payload.get("password") if isinstance(payload.get("password"), str) else ""
    remember = bool(payload.get("remember"))
    throttle_key = (client_address(self), identifier)
    wait = LOGIN_THROTTLE.retry_after(throttle_key)
    if wait:
        self.json({"ok": False, "error": f"Too many failed sign-in attempts. Try again in {(wait + 59) // 60} minute(s)."},
                  status=429, headers=(("Retry-After", str(wait)),))
        return
    # Super accounts are checked first, in their own database; organizations cannot reuse their names.
    super_row = control.find_super(identifier)
    if super_row is not None:
        matches = verify_password(password, super_row["password_hash"] or unknown_account_hash())
        if not super_row["active"] or not super_row["password_hash"] or not matches:
            LOGIN_THROTTLE.failure(throttle_key)
            self.json({"ok": False, "error": "Invalid email or password"}, status=401)
            return
        LOGIN_THROTTLE.success(throttle_key)
        token = secrets.token_urlsafe(32)
        max_age = 60 * 60 * 24 * 30 if remember else 60 * 60 * 8
        user = control.start_session(super_row, SESSION_TOKENS.key(token), time.time() + max_age, remember, self.headers.get("User-Agent", ""))
        send_session_cookie(self, token, max_age, {"ok": True, "user": user})
        return
    with connect() as db:
        row = db.execute(
            """
            SELECT id, name, username, role, email, department, password_hash, active,
                   reset_required, last_login_at, login_count, title, responsibilities
            FROM users
            WHERE lower(email) = ? OR lower(username) = ?
            """,
            (identifier, identifier),
        ).fetchone()
        stored_hash = row["password_hash"] if row and row["active"] else None
        # Always hash: response time must not reveal which accounts exist.
        matches = verify_password(password, stored_hash or unknown_account_hash())
        if not stored_hash or not matches:
            LOGIN_THROTTLE.failure(throttle_key)
            self.json({"ok": False, "error": "Invalid email or password"}, status=401)
            return
        LOGIN_THROTTLE.success(throttle_key)
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
            SELECT id, name, username, role, email, department, active, reset_required,
                   last_login_at, login_count, title, responsibilities, profile_photo_data_id, signature_image_data_id
            FROM users
            WHERE id = ?
            """,
            (row["id"],),
        ).fetchone()
    send_session_cookie(self, token, max_age, {"ok": True, "user": public_user(refreshed)})


def send_session_cookie(self, token, max_age, body):
    self.send_response(200)
    self.send_header("Content-Type", "application/json")
    self.send_header("Cache-Control", "no-store")
    secure = "; Secure" if os.environ.get("AUDIT_SECURE_COOKIES") == "1" else ""
    self.send_header("Set-Cookie", f"{config.SESSION_COOKIE}={token}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}")
    self.end_headers()
    self.wfile.write(json.dumps(body).encode("utf-8"))


def post_auth_logout(self, parsed, payload=None):
    SESSION_TOKENS.pop(self.session_token(), None)
    if self.session_token():
        control.end_session(SESSION_TOKENS.key(self.session_token()))
    self.send_response(200)
    self.send_header("Content-Type", "application/json")
    self.send_header("Cache-Control", "no-store")
    self.send_header("Set-Cookie", f"{config.SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
    self.end_headers()
    self.wfile.write(json.dumps({"ok": True}).encode("utf-8"))
    return


def patch_account(self, parsed, payload=None):
    user = self.current_user()
    super_id = control.control_id(user)
    if super_id is not None:
        patch_super_account(self, super_id, user, payload)
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        existing = db.execute("SELECT * FROM users WHERE id = ?", (user["id"],)).fetchone()
        name = str(payload.get("name", existing["name"]) or "").strip()
        email = str(payload.get("email", existing["email"]) or "").strip().lower()
        username = str(payload.get("username") or existing["username"] or email.split("@", 1)[0] or "").strip().lower()
        role = payload.get("role", existing["role"])
        department = payload.get("department", existing["department"])
        # Only a changed value is checked, so accounts with older addresses or login names can still save their profile.
        unchanged_email = email == str(existing["email"] or "").strip().lower()
        unchanged_username = username == str(existing["username"] or "").strip().lower()
        if (not name or len(name) > 100 or not (unchanged_username or USERNAME_PATTERN.fullmatch(username))
                or not (unchanged_email or EMAIL_PATTERN.fullmatch(email))):
            self.json({"error": "Enter a valid display name, username, and email address"}, 400)
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
        if db.execute("SELECT 1 FROM users WHERE lower(email) = ? AND id != ?", (email, user["id"])).fetchone() or control.is_reserved_identifier(email):
            self.json({"error": "That email address belongs to another account"}, 409)
            return
        if db.execute("SELECT 1 FROM users WHERE lower(username) = ? AND id != ?", (username, user["id"])).fetchone() or control.is_reserved_identifier(username):
            self.json({"error": "That username belongs to another account"}, 409)
            return
        photo = payload.get("profilePhoto", load_value(existing["profile_photo_data_id"] or "{}")) or {}
        if not isinstance(photo, dict) or photo and not photo.get("url", "").startswith("/api/media/"):
            self.json({"error": "Upload a profile picture first"}, 400)
            return
        signature = payload.get("signatureImage", load_value(existing["signature_image_data_id"] or "{}")) or {}
        if not isinstance(signature, dict) or signature and not signature.get("url", "").startswith("/api/media/"):
            self.json({"error": "Upload a signature image first"}, 400)
            return
        db.execute("UPDATE users SET name = ?, username = ?, email = ?, department = ?, role = ?, profile_photo_data_id = ?, signature_image_data_id = ? WHERE id = ?",
                   (name, username, email, department, role, save_value(db, photo), save_value(db, signature), user["id"]))
        refreshed = db.execute("SELECT * FROM users WHERE id = ?", (user["id"],)).fetchone()
    self.json({"ok": True, "user": public_user(refreshed)})


def patch_super_account(self, super_id, user, payload):
    """A Super account's own details live in the control database; it keeps no picture or saved signature."""
    name = str(payload.get("name", user["name"]) or "").strip()
    email = str(payload.get("email", user["email"]) or "").strip().lower()
    username = str(payload.get("username") or user["username"] or "").strip().lower()
    if (not name or len(name) > 100 or not (username == user["username"] or USERNAME_PATTERN.fullmatch(username))
            or not (email == user["email"] or EMAIL_PATTERN.fullmatch(email))):
        self.json({"error": "Enter a valid display name, username, and email address"}, 400)
        return
    if payload.get("role", SUPER_ROLE) != SUPER_ROLE:
        self.json({"error": "The Super account's role cannot be changed"}, 403)
        return
    if payload.get("profilePhoto") or payload.get("signatureImage"):
        self.json({"error": "The Super account keeps no profile picture or saved signature"}, 400)
        return
    with connect() as db:
        if db.execute("SELECT 1 FROM users WHERE lower(email) = ? OR lower(username) IN (?, ?)", (email, username, email)).fetchone():
            self.json({"error": "That username or email belongs to an account in this organization"}, 409)
            return
    try:
        refreshed = control.update_profile(super_id, name, username, email)
    except ValueError as error:
        self.json({"error": str(error)}, 409)
        return
    self.json({"ok": True, "user": refreshed})


def post_auth_forgot_password(self, parsed, payload=None):
    email = str(payload.get("email") or "").strip().lower()
    now = int(time.time() * 1000)
    with connect() as db:
        user = db.execute("SELECT id, name FROM users WHERE lower(email) = ? AND active = 1", (email,)).fetchone()
        if user:
            previous = db.execute("SELECT requested_at FROM password_reset_requests WHERE user_id = ?", (user["id"],)).fetchone()
            if not previous or now - previous["requested_at"] >= 15 * 60 * 1000:
                db.execute("INSERT INTO password_reset_requests(user_id, requested_at) VALUES (?, ?) ON CONFLICT(user_id) DO UPDATE SET requested_at = excluded.requested_at, resolved_at = NULL", (user["id"], now))
                for admin in db.execute("SELECT id FROM users WHERE role = ? AND active = 1", (ADMIN_ROLE,)).fetchall():
                    db.execute("INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread','user',?,?,?)",
                               ("Password reset requested", f"{user['name']} requested a password reset. Review this account in Users.", user["id"], now, admin["id"]))
    self.json({"ok": True, "message": "If an active account matches, a reset request has been sent to your administrator. Contact them to verify your identity and receive a temporary password."})


def post_auth_register(self, parsed, payload=None):
    now = int(time.time() * 1000)
    name = (payload.get("name") or "").strip()
    email = (payload.get("email") or "").strip().lower()
    password = payload.get("password") or ""
    address = client_address(self)
    wait = REGISTRATION_THROTTLE.retry_after(address)
    if wait:
        self.json({"ok": False, "error": "Too many registrations from this address. Try again later."}, status=429, headers=(("Retry-After", str(wait)),))
        return
    if not name or len(name) > 100 or not EMAIL_PATTERN.fullmatch(email) or len(password) < MIN_PASSWORD_LENGTH:
        self.json({"ok": False, "error": "Name, email, and an 8-character password are required"}, status=400)
        return
    if control.is_reserved_identifier(email):
        self.json({"ok": False, "error": "An account with this email already exists"}, status=409)
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
    REGISTRATION_THROTTLE.failure(address)  # Each created account counts toward the limit.
    self.json({"ok": True, "message": "Account registered. A Super user must activate it and assign a role before login."})
    return


def post_auth_change_password(self, parsed, payload=None):
    user = self.current_user()
    if not user:
        self.json({"ok": False, "error": "Login required"}, status=401)
        return
    old_password = payload.get("oldPassword") or ""
    new_password = payload.get("newPassword") or ""
    if not isinstance(new_password, str) or len(new_password) < MIN_PASSWORD_LENGTH:
        self.json({"ok": False, "error": "New password must be at least 8 characters"}, status=400)
        return
    super_id = control.control_id(user)
    if super_id is not None:
        if not verify_password(old_password, control.password_hash(super_id)):
            self.json({"ok": False, "error": "Current password is incorrect"}, status=400)
            return
        control.change_password(super_id, hash_password(new_password), SESSION_TOKENS.key(self.session_token()))
        self.json({"ok": True})
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


def role_department(db, value):
    value = str(value or "").strip()
    if value and not db.execute("SELECT 1 FROM departments WHERE code = ?", (value,)).fetchone():
        raise ValueError("Select an existing department")
    return value or None


def role_reports_to(db, value, role_id=None):
    """The role this one reports to; refused if it is itself or reports to it, which would make a loop."""
    if value in (None, "", 0):
        return None
    try:
        parent = int(value)
    except (TypeError, ValueError):
        raise ValueError("Choose the role this one reports to") from None
    if not db.execute("SELECT 1 FROM roles WHERE id = ?", (parent,)).fetchone():
        raise ValueError("Choose an existing role to report to")
    seen, current = set(), parent
    while current is not None and current not in seen:
        if role_id is not None and current == role_id:
            raise ValueError("A role cannot report to itself or to a role below it")
        seen.add(current)
        row = db.execute("SELECT reports_to_id FROM roles WHERE id = ?", (current,)).fetchone()
        current = row["reports_to_id"] if row else None
    return parent


def account_fields(db, payload, existing=None):
    """Validated account columns shared by administrator create and edit.

    Role, department, and email format are checked only when they change, so a partial edit of an
    older account (for example deactivating it) is not blocked by values that were valid when saved.
    """
    def text(key, label, limit, fallback=""):
        value = payload.get(key)
        value = fallback if value is None else value
        if not isinstance(value, str) or len(value.strip()) > limit:
            raise ValueError(f"{label} must be text of at most {limit} characters")
        return value.strip()

    name = text("name", "Name", 100)
    if not name:
        raise ValueError("Enter a name")
    email = text("email", "Email", 254).lower()
    if email != str(existing["email"] if existing else "").strip().lower() and not EMAIL_PATTERN.fullmatch(email):
        raise ValueError("Enter a valid email address")
    username = text("username", "Username", 64).lower() or email.split("@", 1)[0]
    role = text("role", "Role", 100)
    if role == SUPER_ROLE:
        raise ValueError("The Super account is kept in its own database and cannot be given to an organization user")
    if role and role != (existing["role"] if existing else None) and not db.execute("SELECT 1 FROM roles WHERE name = ?", (role,)).fetchone():
        raise ValueError("Select an existing role")
    department = text("department", "Department", 100) or first_department(db)
    if department and department != (existing["department"] if existing else None) and not db.execute("SELECT 1 FROM departments WHERE code = ?", (department,)).fetchone():
        raise ValueError("Select an existing department")
    password = payload.get("password") or ""
    if not isinstance(password, str) or password and len(password) < MIN_PASSWORD_LENGTH:
        raise ValueError(f"Passwords must be at least {MIN_PASSWORD_LENGTH} characters")
    user_id = existing["id"] if existing else 0
    if control.is_reserved_identifier(email, username):
        raise WorkflowError("That username or email is reserved for a Super account")
    if db.execute("SELECT 1 FROM users WHERE lower(email) = ? AND id != ?", (email, user_id)).fetchone():
        raise WorkflowError("That email address belongs to another account")
    if db.execute("SELECT 1 FROM users WHERE lower(username) = ? AND id != ?", (username, user_id)).fetchone():
        raise WorkflowError("That username belongs to another account")
    return {
        "name": name, "username": username, "role": role, "email": email, "department": department,
        "active": 1 if payload.get("active", True) else 0,
        "reset_required": 1 if payload.get("resetRequired", False) else 0,
        "title": text("title", "Title", 200), "responsibilities": text("responsibilities", "Responsibilities", 4000),
    }, password


def post_users(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        if not is_company_admin_user(self.current_user()):
            self.json({"ok": False, "error": "Admin access required"}, status=403)
            return
        if not is_super_user(self.current_user()) and payload.get("role") == SUPER_ROLE:
            self.json({"ok": False, "error": "Super role assignment requires Super access"}, status=403)
            return
        fields, password = account_fields(db, payload)
        if not password:
            # The shared default is known to every administrator; the owner must replace it.
            fields["reset_required"] = 1
        overrides = validate_overrides(payload.get("permissionOverrides"))
        # Hash before the first write so the slow derivation never holds the database write lock.
        fields.update(password_hash=hash_password(password or DEFAULT_PASSWORD), created_at=now)
        fields["permission_overrides_data_id"] = save_value(db, overrides) if overrides is not None else None
        outlets = validate_outlets(db, payload.get("outlets"))
        fields["outlets_data_id"] = None if outlets is None else save_value(db, outlets)
        insert_record(db, "users", fields)
    self.json({"ok": True})


def post_roles(self, parsed, payload=None):
    now = int(time.time() * 1000)
    with connect() as db:
        if not is_company_admin_user(self.current_user()):
            self.json({"ok": False, "error": "Admin access required"}, status=403)
            return
        name = (payload.get("name") or "New Role").strip()
        if name.lower() in ("admin", "super"):
            self.json({"ok": False, "error": "The Super role is built in and cannot be recreated"}, status=400)
            return
        db.execute(
            """
            INSERT INTO roles (name, description, permissions_data_id, inspection_permissions_data_id, protected, created_at, department, reports_to_id)
            VALUES (?, ?, ?, ?, 0, ?, ?, ?)
            """,
            (
                name,
                payload.get("description", ""),
                save_value(db, validate_list(payload.get("permissions", []), {tab[0] for tab in APP_TABS})),
                save_value(db, validate_list(payload.get("inspectionPermissions", []), INSPECTION_PERMISSIONS)),
                now,
                role_department(db, payload.get("department")),
                role_reports_to(db, payload.get("reportsTo")),
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
        fields = {"name": "name", "username": "username", "role": "role", "email": "email", "department": "department", "active": "active", "resetRequired": "reset_required", "title": "title", "responsibilities": "responsibilities"}
        payload = {key: existing_user[column] for key, column in fields.items()} | {key: value for key, value in payload.items() if key != "username" or value}
        if existing_user["role"] == SUPER_ROLE and existing_user["active"] and (payload["role"] != SUPER_ROLE or not payload["active"]):
            protect_last_super(db, int(user_id))
        if not is_super_user(self.current_user()) and (payload.get("role") == SUPER_ROLE or (existing_user and existing_user["role"] == SUPER_ROLE)):
            self.json({"ok": False, "error": "Super users require Super access"}, status=403)
            return
        values, password = account_fields(db, payload, existing_user)
        reset_password = bool(payload.get("resetPassword"))
        if reset_password or password:
            values["password_hash"] = hash_password(password or DEFAULT_PASSWORD)
            if reset_password:
                # A password chosen by an administrator is temporary.
                values["reset_required"] = 1
        if "outlets" in payload:
            outlets = validate_outlets(db, payload["outlets"])
            values["outlets_data_id"] = None if outlets is None else save_value(db, outlets)
        db.execute(f"UPDATE users SET {', '.join(f'{column} = ?' for column in values)} WHERE id = ?", (*values.values(), int(user_id)))
        if not payload["active"] or reset_password or password:
            db.execute("DELETE FROM auth_sessions WHERE user_id = ?", (int(user_id),))
        if reset_password or password:
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
    if not is_company_admin_user(self.current_user()):
        self.json({"ok": False, "error": "Admin access required"}, status=403)
        return
    with connect() as db:
        role = db.execute("SELECT name, protected, inspection_permissions_data_id, department, reports_to_id FROM roles WHERE id = ?", (int(role_id),)).fetchone()
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
            SET name = ?, description = ?, permissions_data_id = ?, inspection_permissions_data_id = ?, department = ?, reports_to_id = ?
            WHERE id = ?
            """,
            (
                name,
                payload.get("description", ""),
                save_value(db, validate_list(payload.get("permissions", []), {tab[0] for tab in APP_TABS})),
                save_value(db, validate_list(payload.get("inspectionPermissions", load_value(role["inspection_permissions_data_id"] or "[]")), INSPECTION_PERMISSIONS)),
                role_department(db, payload.get("department", role["department"])),
                role_reports_to(db, payload.get("reportsTo", role["reports_to_id"]), int(role_id)),
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
    if not is_company_admin_user(self.current_user()):
        self.json({"ok": False, "error": "Admin access required"}, status=403)
        return
    with connect() as db:
        role = db.execute("SELECT name, protected, reports_to_id FROM roles WHERE id = ?", (int(record_id),)).fetchone()
        if not role:
            self.send_error(404)
            return
        if role["protected"]:
            self.json({"ok": False, "error": "The Super role cannot be deleted"}, status=400)
            return
        # The roles that reported to it now report to its own manager, so the chain stays whole.
        db.execute("UPDATE roles SET reports_to_id = ? WHERE reports_to_id = ?", (role["reports_to_id"], int(record_id)))
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
    super_user = is_super_user(self.current_user())
    pages = ({tab[0] for tab in APP_TABS} | ({tab[0] for tab in SUPER_TABS} if super_user else set()) | {"account"}) - {"departments", "roles"}
    if not isinstance(order, list) or len(order) > len(pages) or any(not isinstance(page, str) or page not in pages for page in order) or len(set(order)) != len(order):
        raise ValueError("Choose each available page at most once")
    user_id = self.current_user()["id"]
    super_id = control.control_id(self.current_user())
    if super_id is not None:
        control.save_navigation(super_id, order)
        self.json({"ok": True, "navigationOrder": order})
        return
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        db.execute("DELETE FROM user_navigation WHERE user_id = ?", (user_id,))
        db.executemany("INSERT INTO user_navigation(user_id,page_id,position) VALUES (?,?,?)", [(user_id, page, position) for position, page in enumerate(order)])
    self.json({"ok": True, "navigationOrder": order})


def post_database(self, parsed, payload=None):
    if not is_company_admin_user(self.current_user()):
        self.json({"error": "Admin access required"}, 403)
        return
    self.json({"ok": True, "database": create_database((payload or {}).get("name", ""))})


def patch_database(self, parsed, payload=None):
    if not is_super_user(self.current_user()):
        self.json({"error": "Super access required"}, 403)
        return
    result = switch_database((payload or {}).get("name", ""))
    SESSION_TOKENS.clear()
    self.json({"ok": True, "database": result})


def delete_database(self, parsed, payload=None):
    if not is_super_user(self.current_user()):
        self.json({"error": "Super access required"}, 403)
        return
    self.json({"ok": True, "database": remove_database(parsed.path.rsplit("/", 1)[-1])})
