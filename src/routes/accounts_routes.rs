//! Sign-in, accounts, roles, page order, and company databases.

use std::sync::OnceLock;

use crate::accounts::{is_company_admin_user, is_super_user, public_user, validate_outlets};
use crate::api::{error_json, json_status, ok, ok_true, send_error, Ctx, Reply, LOGIN_THROTTLE, REGISTRATION_THROTTLE};
use crate::clock;
use crate::common::{hash_password, verify_password};
use crate::config::{self, ADMIN_ROLE, DEFAULT_PASSWORD, SUPER_ROLE, SUPER_TABS};
use crate::control;
use crate::crypto::{sha256_hex, token_urlsafe};
use crate::databases;
use crate::db::{first_department, insert_record, load_or, load_value, save_value, update_record, with_db, Db};
use crate::error::{bad, status, R};
use crate::http::Response;
use crate::json::{obj, Value};
use crate::params;
use crate::permissions::{validate_actions, validate_list, validate_overrides, INSPECTION_PERMISSIONS};
use crate::sqlite::Row;

const MIN_PASSWORD_LENGTH: usize = 8;

/// EMAIL_PATTERN: something@something.something, without spaces.
fn valid_email(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else { return false };
    let clean = |part: &str| !part.is_empty() && !part.chars().any(|c| c == '@' || c.is_whitespace());
    let Some((host, top)) = domain.rsplit_once('.') else { return false };
    clean(local) && clean(host) && clean(top) && !domain.contains(char::is_whitespace)
}

/// USERNAME_PATTERN: 3-64 of a-z, 0-9, ".", "_", "-", starting with a letter or digit.
fn valid_username(text: &str) -> bool {
    let bytes = text.as_bytes();
    (3..=64).contains(&bytes.len()) && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
}

/// A hash nobody can match, so a sign-in for a missing account costs the same as a real one.
fn unknown_account_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password(&token_urlsafe(32)))
}

fn session_cookie(token: &str, max_age: i64, body: &Value) -> Reply {
    let secure = if config::env_flag("AUDIT_SECURE_COOKIES") { "; Secure" } else { "" };
    Reply::Raw(
        Response::new(200, "application/json", body.to_json_bytes())
            .header("Cache-Control", "no-store")
            .header("Set-Cookie", format!("{}={token}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}", config::session_cookie())),
    )
}

const USER_COLUMNS: &str = "id, name, username, role, email, department, active, reset_required, last_login_at, login_count, title, responsibilities, profile_photo_data_id, signature_image_data_id";

pub fn login(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let identifier = payload.at("identifier").or(payload.at("email")).or(payload.at("username")).text().trim().to_lowercase();
    let password = payload.at("password").as_str().unwrap_or("").to_string();
    let remember = payload.at("remember").truthy();
    let key = format!("{}\u{0}{identifier}", ctx.client);
    let wait = LOGIN_THROTTLE.retry_after(&key);
    if wait > 0 {
        let reply = Response::new(429, "application/json", obj([("ok", false.into()), ("error", Value::str(format!("Too many failed sign-in attempts. Try again in {} minute(s).", wait.div_ceil(60))))]).to_json_bytes())
            .header("Cache-Control", "no-store")
            .header("Retry-After", wait.to_string());
        return Ok(Reply::Raw(reply));
    }
    let max_age: i64 = if remember { 60 * 60 * 24 * 30 } else { 60 * 60 * 8 };
    let user_agent = ctx.header("User-Agent").unwrap_or("").to_string();
    let refused = || json_status(401, obj([("ok", false.into()), ("error", "Invalid email or password".into())]));
    // Super accounts are checked first, in their own database; organizations cannot reuse their names.
    if let Some(row) = control::find_super(&identifier)? {
        let stored = row.str("password_hash");
        let matches = verify_password(&password, if stored.is_empty() { unknown_account_hash() } else { &stored });
        if row.int("active") == 0 || stored.is_empty() || !matches {
            LOGIN_THROTTLE.failure(&key);
            return refused();
        }
        LOGIN_THROTTLE.success(&key);
        let token = token_urlsafe(32);
        let user = control::start_session(&row, &sha256_hex(token.as_bytes()), clock::now_secs() + max_age as f64, remember, &user_agent)?;
        return Ok(session_cookie(&token, max_age, &obj([("ok", true.into()), ("user", user)])));
    }
    with_db(|db| {
        let row = db.row(
            "SELECT id, name, username, role, email, department, password_hash, active, reset_required, last_login_at, login_count, title, responsibilities FROM users WHERE lower(email) = ? OR lower(username) = ?",
            params![identifier, identifier],
        )?;
        let stored = row.as_ref().filter(|row| row.int("active") != 0).map(|row| row.str("password_hash")).unwrap_or_default();
        // Always hash: response time must not reveal which accounts exist.
        let matches = verify_password(&password, if stored.is_empty() { unknown_account_hash() } else { &stored });
        let Some(row) = row.filter(|_| !stored.is_empty() && matches) else {
            LOGIN_THROTTLE.failure(&key);
            return refused();
        };
        LOGIN_THROTTLE.success(&key);
        if !stored.starts_with("pbkdf2_sha256$") {
            db.exec("UPDATE users SET password_hash = ? WHERE id = ?", params![hash_password(&password), row.int("id")])?;
        }
        let token = token_urlsafe(32);
        db.exec(
            "INSERT OR REPLACE INTO auth_sessions(token_hash, user_id, expires_at) VALUES (?, ?, ?)",
            params![sha256_hex(token.as_bytes()), row.int("id"), clock::now_secs() + max_age as f64],
        )?;
        let logged_at = clock::local_now().date_time_text();
        db.exec("UPDATE users SET last_login_at = ?, login_count = COALESCE(login_count, 0) + 1 WHERE id = ?", params![logged_at, row.int("id")])?;
        db.exec(
            "INSERT INTO user_login_activity (user_id, email, logged_at, remember_me, user_agent) VALUES (?, ?, ?, ?, ?)",
            params![row.int("id"), row.value("email"), logged_at, remember as i64, user_agent],
        )?;
        let refreshed = db.row(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?"), params![row.int("id")])?.ok_or_else(|| bad("Account not found"))?;
        let user = public_user(&refreshed, db)?;
        Ok(session_cookie(&token, max_age, &obj([("ok", true.into()), ("user", user)])))
    })
}

pub fn logout(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let key = ctx.session_key();
    with_db(|db| db.exec("DELETE FROM auth_sessions WHERE token_hash = ?", params![key]))?;
    if !ctx.session_token().is_empty() {
        control::end_session(&key)?;
    }
    Ok(Reply::Raw(
        Response::new(200, "application/json", br#"{"ok": true}"#.to_vec())
            .header("Cache-Control", "no-store")
            .header("Set-Cookie", format!("{}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax", config::session_cookie())),
    ))
}

fn photo_value(value: &Value) -> Option<Value> {
    let value = if value.truthy() { value.clone() } else { Value::object() };
    let valid = value.is_object() && (!value.truthy() || value.at("url").as_str().unwrap_or("").starts_with("/api/media/"));
    valid.then_some(value)
}

pub fn patch_account(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    if let Some(super_id) = control::control_id(&user) {
        return patch_super_account(super_id, &user, payload);
    }
    let user_id = user.at("id").as_i64().unwrap_or(0);
    with_db(|db| {
        db.begin_immediate()?;
        let existing = db.row("SELECT * FROM users WHERE id = ?", params![user_id])?.ok_or_else(|| status(404, "Account not found"))?;
        let name = payload.get("name").cloned().unwrap_or_else(|| existing.value("name")).text().trim().to_string();
        let email = payload.get("email").cloned().unwrap_or_else(|| existing.value("email")).text().trim().to_lowercase();
        let username = {
            let sent = payload.at("username");
            let chosen = if sent.truthy() { sent.text() } else if existing.get("username").truthy() { existing.str("username") } else { email.split('@').next().unwrap_or("").to_string() };
            chosen.trim().to_lowercase()
        };
        let role = payload.get("role").cloned().unwrap_or_else(|| existing.value("role"));
        let department = payload.get("department").cloned().unwrap_or_else(|| existing.value("department"));
        // Only a changed value is checked, so accounts with older addresses or login names can still save their profile.
        let unchanged_email = email == existing.str("email").trim().to_lowercase();
        let unchanged_username = username == existing.str("username").trim().to_lowercase();
        if name.is_empty() || name.chars().count() > 100 || !(unchanged_username || valid_username(&username)) || !(unchanged_email || valid_email(&email)) {
            return error_json(400, "Enter a valid display name, username, and email address");
        }
        if !is_company_admin_user(&user) && (!role.py_eq(&existing.value("role")) || !department.py_eq(&existing.value("department"))) {
            return error_json(403, "An administrator must change your role or department");
        }
        if role.as_str() == Some(SUPER_ROLE) && !is_super_user(&user) {
            return error_json(403, "Only a Super user can assign the Super role");
        }
        if existing.str("role") == SUPER_ROLE && role.as_str() != Some(SUPER_ROLE) && !db.exists("SELECT 1 FROM users WHERE role = ? AND active = 1 AND id != ?", params![SUPER_ROLE, user_id])? {
            return error_json(409, "Keep at least one active Super user");
        }
        if !db.exists("SELECT 1 FROM roles WHERE name = ?", params![role])? {
            return error_json(400, "Select an existing role");
        }
        if department.truthy() && !db.exists("SELECT 1 FROM departments WHERE code = ?", params![department])? {
            return error_json(400, "Select an existing department");
        }
        if db.exists("SELECT 1 FROM users WHERE lower(email) = ? AND id != ?", params![email, user_id])? || control::is_reserved_identifier(&[&email])? {
            return error_json(409, "That email address belongs to another account");
        }
        if db.exists("SELECT 1 FROM users WHERE lower(username) = ? AND id != ?", params![username, user_id])? || control::is_reserved_identifier(&[&username])? {
            return error_json(409, "That username belongs to another account");
        }
        let photo = match payload.get("profilePhoto") {
            Some(value) => value.clone(),
            None => load_or(db, existing.get("profile_photo_data_id"), Value::object())?,
        };
        let Some(photo) = photo_value(&photo) else { return error_json(400, "Upload a profile picture first") };
        let signature = match payload.get("signatureImage") {
            Some(value) => value.clone(),
            None => load_or(db, existing.get("signature_image_data_id"), Value::object())?,
        };
        let Some(signature) = photo_value(&signature) else { return error_json(400, "Upload a signature image first") };
        db.exec(
            "UPDATE users SET name = ?, username = ?, email = ?, department = ?, role = ?, profile_photo_data_id = ?, signature_image_data_id = ? WHERE id = ?",
            params![name, username, email, department, role, save_value(db, &photo)?, save_value(db, &signature)?, user_id],
        )?;
        let refreshed = db.row("SELECT * FROM users WHERE id = ?", params![user_id])?.ok_or_else(|| status(404, "Account not found"))?;
        ok(obj([("ok", true.into()), ("user", public_user(&refreshed, db)?)]))
    })
}

/// A Super account's own details live in the control database; it keeps no picture or saved signature.
fn patch_super_account(super_id: i64, user: &Value, payload: &Value) -> R<Reply> {
    let name = payload.get("name").cloned().unwrap_or_else(|| user.at("name").clone()).text().trim().to_string();
    let email = payload.get("email").cloned().unwrap_or_else(|| user.at("email").clone()).text().trim().to_lowercase();
    let username = payload.at("username").or(user.at("username")).text().trim().to_lowercase();
    if name.is_empty() || name.chars().count() > 100 || !(username == user.at("username").text() || valid_username(&username)) || !(email == user.at("email").text() || valid_email(&email)) {
        return error_json(400, "Enter a valid display name, username, and email address");
    }
    if payload.get("role").is_some_and(|role| role.as_str() != Some(SUPER_ROLE)) {
        return error_json(403, "The Super account's role cannot be changed");
    }
    if payload.at("profilePhoto").truthy() || payload.at("signatureImage").truthy() {
        return error_json(400, "The Super account keeps no profile picture or saved signature");
    }
    let taken = with_db(|db| db.exists("SELECT 1 FROM users WHERE lower(email) = ? OR lower(username) IN (?, ?)", params![email, username, email]))?;
    if taken {
        return error_json(409, "That username or email belongs to an account in this organization");
    }
    let refreshed = control::update_profile(super_id, &name, &username, &email)?;
    ok(obj([("ok", true.into()), ("user", refreshed)]))
}

pub fn forgot_password(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let email = payload.at("email").text().trim().to_lowercase();
    let now = clock::now_ms();
    with_db(|db| {
        if let Some(user) = db.row("SELECT id, name FROM users WHERE lower(email) = ? AND active = 1", params![email])? {
            let previous = db.row("SELECT requested_at FROM password_reset_requests WHERE user_id = ?", params![user.int("id")])?;
            if previous.as_ref().is_none_or(|row| now - row.int("requested_at") >= 15 * 60 * 1000) {
                db.exec(
                    "INSERT INTO password_reset_requests(user_id, requested_at) VALUES (?, ?) ON CONFLICT(user_id) DO UPDATE SET requested_at = excluded.requested_at, resolved_at = NULL",
                    params![user.int("id"), now],
                )?;
                for admin in db.rows("SELECT id FROM users WHERE role = ? AND active = 1", params![ADMIN_ROLE])? {
                    db.exec(
                        "INSERT INTO notifications(title,message,channel,status,related_type,related_id,created_at,recipient_user_id) VALUES (?,?,'In-App','Unread','user',?,?,?)",
                        params!["Password reset requested", format!("{} requested a password reset. Review this account in Users.", user.str("name")), user.int("id"), now, admin.int("id")],
                    )?;
                }
            }
        }
        Ok(())
    })?;
    ok(obj([
        ("ok", true.into()),
        ("message", "If an active account matches, a reset request has been sent to your administrator. Contact them to verify your identity and receive a temporary password.".into()),
    ]))
}

pub fn register(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    let name = payload.at("name").text().trim().to_string();
    let email = payload.at("email").text().trim().to_lowercase();
    let password = payload.at("password").text();
    let wait = REGISTRATION_THROTTLE.retry_after(&ctx.client);
    if wait > 0 {
        let reply = Response::new(429, "application/json", obj([("ok", false.into()), ("error", "Too many registrations from this address. Try again later.".into())]).to_json_bytes())
            .header("Cache-Control", "no-store")
            .header("Retry-After", wait.to_string());
        return Ok(Reply::Raw(reply));
    }
    if name.is_empty() || name.chars().count() > 100 || !valid_email(&email) || password.chars().count() < MIN_PASSWORD_LENGTH {
        return json_status(400, obj([("ok", false.into()), ("error", "Name, email, and an 8-character password are required".into())]));
    }
    if control::is_reserved_identifier(&[&email])? {
        return json_status(409, obj([("ok", false.into()), ("error", "An account with this email already exists".into())]));
    }
    let hashed = hash_password(&password);
    let created = with_db(|db| {
        match db.exec(
            "INSERT INTO users (name, role, email, department, password_hash, active, reset_required, title, responsibilities, created_at) VALUES (?, '', ?, '', ?, 0, 0, '', '', ?)",
            params![name, email, hashed, now],
        ) {
            Ok(_) => Ok(true),
            Err(crate::error::AppError::Db(error)) if error.is_constraint() => Ok(false),
            Err(error) => Err(error),
        }
    })?;
    if !created {
        return json_status(409, obj([("ok", false.into()), ("error", "An account with this email already exists".into())]));
    }
    REGISTRATION_THROTTLE.failure(&ctx.client); // Each created account counts toward the limit.
    ok(obj([("ok", true.into()), ("message", "Account registered. A Super user must activate it and assign a role before login.".into())]))
}

pub fn change_password(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    if user.is_null() {
        return json_status(401, obj([("ok", false.into()), ("error", "Login required".into())]));
    }
    let old_password = payload.at("oldPassword").text();
    let new_password = payload.at("newPassword");
    let Some(new_password) = new_password.as_str().filter(|text| text.chars().count() >= MIN_PASSWORD_LENGTH).map(str::to_string) else {
        return json_status(400, obj([("ok", false.into()), ("error", "New password must be at least 8 characters".into())]));
    };
    let incorrect = || json_status(400, obj([("ok", false.into()), ("error", "Current password is incorrect".into())]));
    if let Some(super_id) = control::control_id(&user) {
        if !verify_password(&old_password, &control::password_hash(super_id)?) {
            return incorrect();
        }
        control::change_password(super_id, &hash_password(&new_password), &ctx.session_key())?;
        return ok(obj([("ok", true.into())]));
    }
    let user_id = user.at("id").as_i64().unwrap_or(0);
    let stored = with_db(|db| Ok(db.scalar("SELECT password_hash FROM users WHERE id = ?", params![user_id])?.text()))?;
    if !verify_password(&old_password, &stored) {
        return incorrect();
    }
    // Hash before the write so the slow derivation never holds the database write lock.
    let hashed = hash_password(&new_password);
    with_db(|db| {
        db.exec("UPDATE users SET password_hash = ?, reset_required = 0 WHERE id = ?", params![hashed, user_id])?;
        db.exec("DELETE FROM auth_sessions WHERE user_id = ? AND token_hash != ?", params![user_id, ctx.session_key()])?;
        db.exec("UPDATE password_reset_requests SET resolved_at = ? WHERE user_id = ?", params![clock::now_ms(), user_id])?;
        Ok(())
    })?;
    ok(obj([("ok", true.into())]))
}

fn role_department(db: &Db, value: &Value) -> R<Value> {
    let value = value.text().trim().to_string();
    if !value.is_empty() && !db.exists("SELECT 1 FROM departments WHERE code = ?", params![value])? {
        return Err(bad("Select an existing department"));
    }
    Ok(if value.is_empty() { Value::Null } else { Value::str(value) })
}

/// The role this one reports to; refused if it is itself or reports to it, which would make a loop.
fn role_reports_to(db: &Db, value: &Value, role_id: Option<i64>) -> R<Value> {
    if value.is_null() || value.as_str() == Some("") || value.py_eq(&Value::Int(0)) {
        return Ok(Value::Null);
    }
    let parent = value.py_int().map_err(|_| bad("Choose the role this one reports to"))?;
    if !db.exists("SELECT 1 FROM roles WHERE id = ?", params![parent])? {
        return Err(bad("Choose an existing role to report to"));
    }
    let mut seen = Vec::new();
    let mut current = Some(parent);
    while let Some(id) = current {
        if seen.contains(&id) {
            break;
        }
        if role_id == Some(id) {
            return Err(bad("A role cannot report to itself or to a role below it"));
        }
        seen.push(id);
        current = db.row("SELECT reports_to_id FROM roles WHERE id = ?", params![id])?.and_then(|row| row.opt_int("reports_to_id"));
    }
    Ok(Value::Int(parent))
}

/// Validated account columns shared by administrator create and edit. Role, department, and email
/// format are checked only when they change, so a partial edit of an older account is not blocked
/// by values that were valid when saved.
fn account_fields(db: &Db, payload: &Value, existing: Option<&Row>) -> R<(Vec<(String, Value)>, String)> {
    let text = |key: &str, label: &str, limit: usize| -> R<String> {
        let value = payload.at(key);
        let value = if value.is_null() { Value::str("") } else { value.clone() };
        match value.as_str() {
            Some(text) if text.trim().chars().count() <= limit => Ok(text.trim().to_string()),
            _ => Err(bad(format!("{label} must be text of at most {limit} characters"))),
        }
    };
    let name = text("name", "Name", 100)?;
    if name.is_empty() {
        return Err(bad("Enter a name"));
    }
    let email = text("email", "Email", 254)?.to_lowercase();
    let stored_email = existing.map(|row| row.str("email")).unwrap_or_default().trim().to_lowercase();
    if email != stored_email && !valid_email(&email) {
        return Err(bad("Enter a valid email address"));
    }
    let mut username = text("username", "Username", 64)?.to_lowercase();
    if username.is_empty() {
        username = email.split('@').next().unwrap_or("").to_string();
    }
    let role = text("role", "Role", 100)?;
    if role == SUPER_ROLE {
        return Err(bad("The Super account is kept in its own database and cannot be given to an organization user"));
    }
    if !role.is_empty() && Some(role.clone()) != existing.map(|row| row.str("role")) && !db.exists("SELECT 1 FROM roles WHERE name = ?", params![role])? {
        return Err(bad("Select an existing role"));
    }
    let mut department = text("department", "Department", 100)?;
    if department.is_empty() {
        department = first_department(db)?;
    }
    let stored_department = existing.map(|row| row.value("department"));
    if !department.is_empty() && stored_department.as_ref().is_none_or(|stored| !stored.py_eq(&Value::str(&department))) && !db.exists("SELECT 1 FROM departments WHERE code = ?", params![department])? {
        return Err(bad("Select an existing department"));
    }
    let password = payload.at("password");
    let password = if password.truthy() { password.clone() } else { Value::str("") };
    let Some(password) = password.as_str().map(str::to_string) else { return Err(bad(format!("Passwords must be at least {MIN_PASSWORD_LENGTH} characters"))) };
    if !password.is_empty() && password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(bad(format!("Passwords must be at least {MIN_PASSWORD_LENGTH} characters")));
    }
    let user_id = existing.map(|row| row.int("id")).unwrap_or(0);
    if control::is_reserved_identifier(&[&email, &username])? {
        return Err(status(409, "That username or email is reserved for a Super account"));
    }
    if db.exists("SELECT 1 FROM users WHERE lower(email) = ? AND id != ?", params![email, user_id])? {
        return Err(status(409, "That email address belongs to another account"));
    }
    if db.exists("SELECT 1 FROM users WHERE lower(username) = ? AND id != ?", params![username, user_id])? {
        return Err(status(409, "That username belongs to another account"));
    }
    let active = payload.get("active").map(Value::truthy).unwrap_or(true);
    let reset = payload.get("resetRequired").map(Value::truthy).unwrap_or(false);
    let fields = vec![
        ("name".into(), Value::str(name)),
        ("username".into(), Value::str(username)),
        ("role".into(), Value::str(role)),
        ("email".into(), Value::str(email)),
        ("department".into(), Value::str(department)),
        ("active".into(), Value::Int(active as i64)),
        ("reset_required".into(), Value::Int(reset as i64)),
        ("title".into(), Value::str(text("title", "Title", 200)?)),
        ("responsibilities".into(), Value::str(text("responsibilities", "Responsibilities", 4000)?)),
    ];
    Ok((fields, password))
}

fn set_field(fields: &mut Vec<(String, Value)>, column: &str, value: Value) {
    match fields.iter_mut().find(|(name, _)| name == column) {
        Some(slot) => slot.1 = value,
        None => fields.push((column.to_string(), value)),
    }
}

pub fn post_user(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    if !is_super_user(&ctx.user()?) && payload.at("role").as_str() == Some(SUPER_ROLE) {
        return json_status(403, obj([("ok", false.into()), ("error", "Super role assignment requires Super access".into())]));
    }
    // Checked and hashed before the write, so the slow derivation never holds the write lock.
    let (mut fields, password) = with_db(|db| account_fields(db, payload, None))?;
    if password.is_empty() {
        // The shared default is known to every administrator; the owner must replace it.
        set_field(&mut fields, "reset_required", Value::Int(1));
    }
    let overrides = validate_overrides(payload.at("permissionOverrides"))?;
    let hashed = hash_password(if password.is_empty() { DEFAULT_PASSWORD } else { &password });
    with_db(|db| {
        set_field(&mut fields, "password_hash", Value::str(&hashed));
        set_field(&mut fields, "created_at", Value::Int(now));
        set_field(&mut fields, "permission_overrides_data_id", if overrides.is_null() { Value::Null } else { Value::Int(save_value(db, &overrides)?) });
        let outlets = validate_outlets(db, payload.at("outlets"))?;
        set_field(&mut fields, "outlets_data_id", match outlets {
            None => Value::Null,
            Some(list) => Value::Int(save_value(db, &list)?),
        });
        let values: Vec<(&str, Value)> = fields.iter().map(|(column, value)| (column.as_str(), value.clone())).collect();
        insert_record(db, "users", &values)?;
        ok_true()
    })
}

fn tab_names() -> Vec<&'static str> {
    config::tab_ids()
}

pub fn post_role(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let now = clock::now_ms();
    with_db(|db| {
        let name = payload.at("name").or(&Value::str("New Role")).text().trim().to_string();
        if matches!(name.to_lowercase().as_str(), "admin" | "super") {
            return json_status(400, obj([("ok", false.into()), ("error", "The Super role is built in and cannot be recreated".into())]));
        }
        let empty = Value::array();
        db.exec(
            "INSERT INTO roles (name, description, permissions_data_id, inspection_permissions_data_id, action_permissions_data_id, protected, created_at, department, reports_to_id) VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)",
            params![
                name,
                payload.get("description").cloned().unwrap_or_else(|| "".into()),
                save_value(db, &validate_list(payload.get("permissions").unwrap_or(&empty), &tab_names())?)?,
                save_value(db, &validate_list(payload.get("inspectionPermissions").unwrap_or(&empty), &INSPECTION_PERMISSIONS)?)?,
                save_value(db, &validate_actions(payload.get("actions").unwrap_or(&empty))?)?,
                now,
                role_department(db, payload.at("department"))?,
                role_reports_to(db, payload.at("reportsTo"), None)?
            ],
        )?;
        ok_true()
    })
}

fn protect_last_super(db: &Db, user_id: i64) -> R<()> {
    if !db.exists("SELECT 1 FROM users WHERE role = ? AND active = 1 AND id != ?", params![SUPER_ROLE, user_id])? {
        return Err(status(409, "Keep at least one active Super user"));
    }
    Ok(())
}

pub fn patch_user(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user_id = crate::id_or_reply!(ctx);
    let actor = ctx.user()?;
    // The new password is hashed first, outside the write transaction.
    let password_sent = payload.at("password").as_str().filter(|text| !text.is_empty()).map(str::to_string);
    let reset_password = payload.at("resetPassword").truthy();
    let hashed = if reset_password || password_sent.is_some() {
        Some(hash_password(password_sent.as_deref().filter(|text| text.chars().count() >= MIN_PASSWORD_LENGTH).unwrap_or(DEFAULT_PASSWORD)))
    } else {
        None
    };
    with_db(|db| {
        db.begin_immediate()?;
        let existing = db.row("SELECT * FROM users WHERE id = ?", params![user_id])?.ok_or_else(|| status(404, "User not found"))?;
        let fields = [("name", "name"), ("username", "username"), ("role", "role"), ("email", "email"), ("department", "department"),
                      ("active", "active"), ("resetRequired", "reset_required"), ("title", "title"), ("responsibilities", "responsibilities")];
        let mut merged = Value::object();
        for (key, column) in fields {
            merged.set(key, existing.value(column));
        }
        for (key, value) in payload.entries() {
            if key != "username" || value.truthy() {
                merged.set(key, value.clone());
            }
        }
        if existing.str("role") == SUPER_ROLE && existing.int("active") != 0 && (merged.at("role").as_str() != Some(SUPER_ROLE) || !merged.at("active").truthy()) {
            protect_last_super(db, user_id)?;
        }
        if !is_super_user(&actor) && (merged.at("role").as_str() == Some(SUPER_ROLE) || existing.str("role") == SUPER_ROLE) {
            return json_status(403, obj([("ok", false.into()), ("error", "Super users require Super access".into())]));
        }
        let (mut values, password) = account_fields(db, &merged, Some(&existing))?;
        if reset_password || !password.is_empty() {
            let hashed = hashed.clone().unwrap_or_else(|| hash_password(if password.is_empty() { DEFAULT_PASSWORD } else { &password }));
            set_field(&mut values, "password_hash", Value::str(hashed));
            if reset_password {
                // A password chosen by an administrator is temporary.
                set_field(&mut values, "reset_required", Value::Int(1));
            }
        }
        if merged.has("outlets") {
            let outlets = validate_outlets(db, merged.at("outlets"))?;
            set_field(&mut values, "outlets_data_id", match outlets {
                None => Value::Null,
                Some(list) => Value::Int(save_value(db, &list)?),
            });
        }
        update_record(db, "users", &values, user_id)?;
        if !merged.at("active").truthy() || reset_password || !password.is_empty() {
            db.exec("DELETE FROM auth_sessions WHERE user_id = ?", params![user_id])?;
        }
        if reset_password || !password.is_empty() {
            db.exec("UPDATE password_reset_requests SET resolved_at = ? WHERE user_id = ?", params![clock::now_ms(), user_id])?;
        }
        if merged.has("permissionOverrides") {
            let overrides = validate_overrides(merged.at("permissionOverrides"))?;
            let reference = if overrides.is_null() { None } else { Some(save_value(db, &overrides)?) };
            db.exec("UPDATE users SET permission_overrides_data_id = ? WHERE id = ?", params![reference, user_id])?;
        }
        ok_true()
    })
}

pub fn patch_role(_ctx: &Ctx, payload: &Value) -> R<Reply> {
    let role_id = crate::id_or_reply!(_ctx);
    with_db(|db| {
        let Some(role) = db.row("SELECT name, protected, inspection_permissions_data_id, action_permissions_data_id, department, reports_to_id FROM roles WHERE id = ?", params![role_id])? else {
            return send_error(404);
        };
        if role.int("protected") != 0 {
            return json_status(400, obj([("ok", false.into()), ("error", "The Super role cannot be changed".into())]));
        }
        let name = payload.at("name").or(&Value::str("New Role")).text().trim().to_string();
        if name.to_lowercase() == "super" || (name.to_lowercase() == "admin" && role.str("name") != "Admin") {
            return json_status(400, obj([("ok", false.into()), ("error", "Super is reserved".into())]));
        }
        let empty = Value::array();
        let inspection = match payload.get("inspectionPermissions") {
            Some(value) => value.clone(),
            None => load_or(db, role.get("inspection_permissions_data_id"), Value::array())?,
        };
        let actions = match payload.get("actions") {
            Some(value) => value.clone(),
            None if role.get("action_permissions_data_id").truthy() => {
                let stored = load_value(db, role.get("action_permissions_data_id"))?;
                if stored.truthy() { stored } else { Value::array() }
            }
            None => Value::array(),
        };
        let department = payload.get("department").cloned().unwrap_or_else(|| role.value("department"));
        let reports_to = payload.get("reportsTo").cloned().unwrap_or_else(|| role.value("reports_to_id"));
        let changed = db.exec(
            "UPDATE roles SET name = ?, description = ?, permissions_data_id = ?, inspection_permissions_data_id = ?, action_permissions_data_id = ?, department = ?, reports_to_id = ? WHERE id = ?",
            params![
                name,
                payload.get("description").cloned().unwrap_or_else(|| "".into()),
                save_value(db, &validate_list(payload.get("permissions").unwrap_or(&empty), &tab_names())?)?,
                save_value(db, &validate_list(&inspection, &INSPECTION_PERMISSIONS)?)?,
                save_value(db, &validate_actions(&actions)?)?,
                role_department(db, &department)?,
                role_reports_to(db, &reports_to, Some(role_id))?,
                role_id
            ],
        )?;
        if changed == 0 {
            return send_error(404);
        }
        if name != role.str("name") {
            db.exec("UPDATE users SET role = ? WHERE role = ?", params![name, role.str("name")])?;
        }
        ok_true()
    })
}

pub fn delete_user(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    let actor = ctx.user()?;
    with_db(|db| {
        db.begin_immediate()?;
        let existing = db.row("SELECT role, active FROM users WHERE id = ?", params![record_id])?;
        if let Some(existing) = &existing {
            if !is_super_user(&actor) && existing.str("role") == SUPER_ROLE {
                return json_status(403, obj([("ok", false.into()), ("error", "Super users require Super access".into())]));
            }
            if existing.str("role") == SUPER_ROLE && existing.int("active") != 0 {
                protect_last_super(db, record_id)?;
            }
        }
        if db.exists("SELECT 1 FROM user_login_activity WHERE user_id = ? LIMIT 1", params![record_id])? || db.exists("SELECT 1 FROM inspection_sessions WHERE owner_user_id = ? LIMIT 1", params![record_id])? {
            return Err(status(409, "This account has login or audit history. Deactivate it to retain that history."));
        }
        for (table, column) in [("user_navigation", "user_id"), ("auth_sessions", "user_id"), ("password_reset_requests", "user_id"), ("notifications", "recipient_user_id"), ("due_notification_events", "user_id")] {
            db.exec(&format!("DELETE FROM {table} WHERE {column} = ?"), params![record_id])?;
        }
        if db.exec("DELETE FROM users WHERE id = ?", params![record_id])? == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn delete_role(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    let record_id = crate::id_or_reply!(ctx);
    with_db(|db| {
        let Some(role) = db.row("SELECT name, protected, reports_to_id FROM roles WHERE id = ?", params![record_id])? else { return send_error(404) };
        if role.int("protected") != 0 {
            return json_status(400, obj([("ok", false.into()), ("error", "The Super role cannot be deleted".into())]));
        }
        // The roles that reported to it now report to its own manager, so the chain stays whole.
        db.exec("UPDATE roles SET reports_to_id = ? WHERE reports_to_id = ?", params![role.value("reports_to_id"), record_id])?;
        let changed = db.exec("DELETE FROM roles WHERE id = ?", params![record_id])?;
        db.exec("UPDATE users SET role = '' WHERE role = ?", params![role.str("name")])?;
        if changed == 0 {
            return send_error(404);
        }
        ok_true()
    })
}

pub fn patch_navigation(ctx: &Ctx, payload: &Value) -> R<Reply> {
    let user = ctx.user()?;
    let order = payload.at("order");
    let mut pages: Vec<&str> = config::tab_ids();
    if is_super_user(&user) {
        pages.extend(SUPER_TABS.iter().map(|(id, _)| *id));
    }
    pages.push("account");
    pages.retain(|page| *page != "departments" && *page != "roles");
    let valid = match order {
        Value::Array(items) => {
            let names: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            let mut unique = names.clone();
            unique.sort_unstable();
            unique.dedup();
            names.len() == items.len() && items.len() <= pages.len() && names.iter().all(|page| pages.contains(page)) && unique.len() == names.len()
        }
        _ => false,
    };
    if !valid {
        return Err(bad("Choose each available page at most once"));
    }
    let order: Vec<String> = order.items().iter().map(Value::py_str).collect();
    if let Some(super_id) = control::control_id(&user) {
        control::save_navigation(super_id, &order)?;
        return ok(obj([("ok", true.into()), ("navigationOrder", crate::json::strings(&order))]));
    }
    let user_id = user.at("id").as_i64().unwrap_or(0);
    with_db(|db| {
        db.begin_immediate()?;
        db.exec("DELETE FROM user_navigation WHERE user_id = ?", params![user_id])?;
        for (position, page) in order.iter().enumerate() {
            db.exec("INSERT INTO user_navigation(user_id,page_id,position) VALUES (?,?,?)", params![user_id, page, position as i64])?;
        }
        Ok(())
    })?;
    ok(obj([("ok", true.into()), ("navigationOrder", crate::json::strings(&order))]))
}

pub fn post_database(ctx: &Ctx, payload: &Value) -> R<Reply> {
    if !is_company_admin_user(&ctx.user()?) {
        return error_json(403, "Admin access required");
    }
    ok(obj([("ok", true.into()), ("database", databases::create_database(payload.get("name").unwrap_or(&Value::str("")))?)]))
}

pub fn patch_database(ctx: &Ctx, payload: &Value) -> R<Reply> {
    if !is_super_user(&ctx.user()?) {
        return error_json(403, "Super access required");
    }
    let result = databases::switch_database(payload.get("name").unwrap_or(&Value::str("")))?;
    // Every organization session ends; people sign in to the database now in use.
    with_db(|db| db.exec("DELETE FROM auth_sessions", params![]))?;
    ok(obj([("ok", true.into()), ("database", result)]))
}

pub fn delete_database(ctx: &Ctx, _payload: &Value) -> R<Reply> {
    if !is_super_user(&ctx.user()?) {
        return error_json(403, "Super access required");
    }
    ok(obj([("ok", true.into()), ("database", databases::remove_database(&Value::str(ctx.tail()))?)]))
}
