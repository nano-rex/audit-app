//! The HTTP boundary: sessions, access checks, reads, the mutation registry, static files, and
//! how failures are answered.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use crate::accounts::{self, is_company_admin_user, is_super_user, public_user};
use crate::catalog;
use crate::config;
use crate::control;
use crate::crypto::sha256_hex;
use crate::db::{read_setting, with_db, Prepared};
use crate::error::{bad, forbidden, log_error, log_warning, AppError, R};
use crate::flate::gzip;
use crate::http::{Body, Request, Response};
use crate::json::{obj, Value};
use crate::params;
use crate::permissions::{change_record, list_has};

pub const BODY_LIMIT: usize = 20 * 1024 * 1024;
/// Sign-in forms are small; unauthenticated callers cannot make the server buffer uploads.
pub const UNAUTHENTICATED_BODY_LIMIT: usize = 64 * 1024;

/// One request as the routes see it. A held change replays through the same routes with the
/// approver as its user and no connection behind it.
pub struct Ctx {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub client: String,
    token: String,
    user: RefCell<Option<Value>>,
}

impl Ctx {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    pub fn param(&self, name: &str) -> Option<String> {
        self.query.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone())
    }

    pub fn param_all(&self, name: &str) -> Vec<String> {
        self.query.iter().filter(|(key, _)| key == name).map(|(_, value)| value.clone()).collect()
    }

    pub fn session_token(&self) -> &str {
        &self.token
    }

    pub fn session_key(&self) -> String {
        sha256_hex(self.token.as_bytes())
    }

    /// The signed-in account, or null.
    pub fn user(&self) -> R<Value> {
        if let Some(user) = self.user.borrow().as_ref() {
            return Ok(user.clone());
        }
        let loaded = load_user(&self.token)?;
        *self.user.borrow_mut() = Some(loaded.clone());
        Ok(loaded)
    }

    /// The last path segment (the record id of /api/<route>/<id>).
    pub fn tail(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or("")
    }

    /// A context for replaying a held change as `user`.
    pub fn replay(user: Value, method: &str, path: &str) -> Ctx {
        Ctx { method: method.into(), path: path.into(), query: Vec::new(), headers: Vec::new(), client: String::new(), token: String::new(), user: RefCell::new(Some(user)) }
    }
}

fn load_user(token: &str) -> R<Value> {
    if token.is_empty() {
        return Ok(Value::Null);
    }
    let key = sha256_hex(token.as_bytes());
    with_db(|db| {
        let session = db.row("SELECT user_id, expires_at FROM auth_sessions WHERE token_hash = ?", params![key])?;
        match session {
            Some(session) if session.get("expires_at").as_f64().unwrap_or(0.0) < crate::clock::now_secs() => {
                db.exec("DELETE FROM auth_sessions WHERE token_hash = ?", params![key])?;
                Ok(Value::Null)
            }
            // Not an organization session: a Super account signs in through the control database.
            None => Ok(control::user_for_session(&key)?.unwrap_or(Value::Null)),
            Some(session) => {
                let row = db.row(
                    "SELECT id, name, username, role, email, department, active, reset_required, last_login_at, login_count, title, responsibilities, profile_photo_data_id, signature_image_data_id FROM users WHERE id = ? AND active = 1",
                    params![session.int("user_id")],
                )?;
                match row {
                    Some(row) => public_user(&row, db),
                    None => Ok(Value::Null),
                }
            }
        }
    })
}

/// What a route answers.
pub enum Reply {
    Json(u16, Value),
    /// A cached response shared by a burst of readers.
    Shared(Arc<Prepared>),
    /// BaseHTTPRequestHandler.send_error: a short HTML page.
    Error(u16),
    /// A response with headers of its own (cookies, downloads).
    Raw(Response),
}

pub fn ok(value: Value) -> R<Reply> {
    Ok(Reply::Json(200, value))
}

pub fn ok_true() -> R<Reply> {
    ok(obj([("ok", true.into())]))
}

pub fn json_status(status: u16, value: Value) -> R<Reply> {
    Ok(Reply::Json(status, value))
}

pub fn error_json(status: u16, message: &str) -> R<Reply> {
    Ok(Reply::Json(status, obj([("error", message.into())])))
}

pub fn send_error(status: u16) -> R<Reply> {
    Ok(Reply::Error(status))
}

/// The record id at the end of the path, or a 400 page.
pub fn record_id(ctx: &Ctx) -> Result<i64, Reply> {
    let tail = ctx.tail();
    if crate::common::is_digits(tail) {
        tail.parse().map_err(|_| Reply::Error(400))
    } else {
        Err(Reply::Error(400))
    }
}

/// `let id = id_or_reply!(ctx);` returns the 400 page from the route when the id is not a number.
#[macro_export]
macro_rules! id_or_reply {
    ($ctx:expr) => {
        match $crate::api::record_id($ctx) {
            Ok(id) => id,
            Err(reply) => return Ok(reply),
        }
    };
}

// ---- Failed sign-in limits, held in memory; the application runs as one process ----

pub struct Throttle {
    limit: usize,
    window: f64,
    capacity: usize,
    failures: Mutex<(HashMap<String, VecDeque<Instant>>, VecDeque<String>)>,
}

impl Throttle {
    fn new(limit: usize, window: f64) -> Throttle {
        Throttle { limit, window, capacity: 10_000, failures: Mutex::new((HashMap::new(), VecDeque::new())) }
    }

    fn recent<'a>(&self, map: &'a mut HashMap<String, VecDeque<Instant>>, key: &str) -> Option<&'a mut VecDeque<Instant>> {
        let attempts = map.get_mut(key)?;
        while attempts.front().is_some_and(|first| first.elapsed().as_secs_f64() >= self.window) {
            attempts.pop_front();
        }
        if attempts.is_empty() {
            map.remove(key);
            return None;
        }
        map.get_mut(key)
    }

    /// Seconds until another attempt is accepted; zero when the caller may try now.
    pub fn retry_after(&self, key: &str) -> u64 {
        let mut state = self.failures.lock().unwrap_or_else(|poison| poison.into_inner());
        let (map, _) = &mut *state;
        match self.recent(map, key) {
            Some(attempts) if attempts.len() >= self.limit => {
                let left = self.window - attempts[0].elapsed().as_secs_f64();
                (left + 0.999).max(1.0) as u64
            }
            _ => 0,
        }
    }

    pub fn failure(&self, key: &str) {
        let mut state = self.failures.lock().unwrap_or_else(|poison| poison.into_inner());
        let (map, order) = &mut *state;
        if self.recent(map, key).is_none() {
            map.insert(key.to_string(), VecDeque::new());
        }
        let attempts = map.get_mut(key).expect("just inserted");
        attempts.push_back(Instant::now());
        while attempts.len() > self.limit {
            attempts.pop_front();
        }
        order.retain(|name| name != key);
        order.push_back(key.to_string());
        while order.len() > self.capacity {
            if let Some(oldest) = order.pop_front() {
                map.remove(&oldest);
            }
        }
    }

    pub fn success(&self, key: &str) {
        let mut state = self.failures.lock().unwrap_or_else(|poison| poison.into_inner());
        state.0.remove(key);
        state.1.retain(|name| name != key);
    }
}

pub static LOGIN_THROTTLE: std::sync::LazyLock<Throttle> = std::sync::LazyLock::new(|| Throttle::new(5, 15.0 * 60.0));
/// Accounts created per client address: each costs a password hash and a row awaiting review.
pub static REGISTRATION_THROTTLE: std::sync::LazyLock<Throttle> = std::sync::LazyLock::new(|| Throttle::new(10, 60.0 * 60.0));

/// The caller's address; the reverse proxy's forwarded address only when explicitly trusted.
fn client_address(request: &Request) -> String {
    if config::env_flag("AUDIT_TRUST_PROXY") {
        if let Some(forwarded) = request.header("X-Forwarded-For") {
            if let Some(last) = forwarded.split(',').map(str::trim).filter(|part| !part.is_empty()).next_back() {
                return last.to_string();
            }
        }
    }
    request.peer.to_string()
}

fn cookie_value(header: &str, name: &str) -> String {
    for part in header.split(';') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else { continue };
        if key.trim() == name {
            let value = value.trim();
            let value = value.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(value);
            return value.to_string();
        }
    }
    String::new()
}

// ---- Program files newer than the running server ----

fn executable_stamp() -> Option<SystemTime> {
    std::env::current_exe().ok().and_then(|path| std::fs::metadata(path).ok()).and_then(|meta| meta.modified().ok())
}

struct Staleness {
    started_with: Option<SystemTime>,
    checked: Option<Instant>,
    stale: bool,
}

fn staleness() -> &'static Mutex<Staleness> {
    static STATE: OnceLock<Mutex<Staleness>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(Staleness { started_with: executable_stamp(), checked: None, stale: false }))
}

/// True once the program on disk has changed since this server started; checked at most every 30 s.
/// Until it is restarted, the pages (read fresh from disk) can be newer than the server.
pub fn server_outdated() -> bool {
    let mut state = staleness().lock().unwrap_or_else(|poison| poison.into_inner());
    if !state.stale && state.checked.is_none_or(|at| at.elapsed().as_secs() > 30) {
        state.checked = Some(Instant::now());
        state.stale = executable_stamp() != state.started_with;
    }
    state.stale
}

pub fn note_startup() {
    let _ = staleness();
}

// ---- Access ----

/// Pages that open each API route.
fn route_pages(route: &str) -> &'static [&'static str] {
    match route {
        "dashboard" => &["today", "reports"],
        "reports" | "activity" | "location-report.pdf" | "location-report.xlsx" => &["reports"],
        "inspection-sessions" | "audits" => &["inspections"],
        "equipment" => &["equipment"],
        "users" => &["users"],
        "roles" => &["roles"],
        "settings" => &["settings"],
        "schedules" => &["today", "inspections"],
        "findings" => &["findings", "inspections"],
        "work-orders" => &["work-orders"],
        "work-requests" => &["work-orders", "findings"],
        "notifications" => &["notifications"],
        "locations" | "zones" => &["outlets"],
        "comments" => &["findings", "work-orders", "inspections"],
        _ => &[],
    }
}

/// None when the request may go on; otherwise the refusal to send.
fn require_auth(ctx: &Ctx) -> R<Option<Reply>> {
    let path = ctx.path.as_str();
    if !path.starts_with("/api/") || path == "/api/branding" || path.starts_with("/api/auth/") {
        return Ok(None);
    }
    if path.starts_with("/api/media/") && ctx.method == "GET" {
        // The organization's logo is shown on the sign-in page, before anyone has a session.
        let logo = with_db(|db| read_setting(db, "report.logoUrl", Value::str("")))?;
        if logo.as_str() == Some(path) {
            return Ok(None);
        }
    }
    let user = ctx.user()?;
    if user.is_null() {
        return Ok(Some(Reply::Json(401, obj([("ok", false.into()), ("error", "Login required".into())]))));
    }
    if user.at("resetRequired").truthy() && !(ctx.method == "GET" && path == "/api/account") {
        // The account holds a temporary or default password; nothing else is available until it is replaced.
        return Ok(Some(Reply::Json(403, obj([("ok", false.into()), ("error", "Change your password to continue".into()), ("resetRequired", true.into())]))));
    }
    let rest = path.strip_prefix("/api/").unwrap_or(path);
    let route = rest.split('/').next().unwrap_or("");
    if ((route == "audits" && ctx.method == "POST") || (route == "inspection-sessions" && ctx.method == "DELETE")) && !list_has(user.at("inspectionPermissions"), "auditor") {
        return Ok(Some(Reply::Json(403, obj([("error", "Auditor permission is required".into())]))));
    }
    if is_super_user(&user) {
        return Ok(None);
    }
    if route == "settings" && is_company_admin_user(&user) {
        return Ok(None);
    }
    if route == "findings" {
        let enabled = with_db(|db| read_setting(db, "system.findingsEnabled", Value::Bool(true)))?;
        if enabled.as_bool() == Some(false) {
            return Ok(Some(Reply::Json(403, obj([("ok", false.into()), ("error", "Findings is turned off".into())]))));
        }
    }
    let mut allowed: Vec<&str> = if route == "setup" {
        match path.split('/').nth(3) {
            Some("priorities" | "audit-types") => vec!["settings"],
            Some(section) => vec![section],
            None => Vec::new(),
        }
    } else {
        route_pages(route).to_vec()
    };
    if route == "notifications" && matches!(ctx.method.as_str(), "GET" | "PATCH" | "DELETE") {
        allowed.clear(); // Each user can access their own addressed notifications.
    }
    if ctx.method == "POST" && route == "work-requests" {
        allowed.push("inspections"); // Inspectors can request work for what failed.
    }
    if ctx.method == "GET" {
        if route == "inspection-sessions" {
            allowed.extend(["findings", "history"]);
        }
        if matches!(route, "setup" | "locations" | "zones") {
            return Ok(None); // Shared selection lists used by the permitted workflows.
        }
        if route == "equipment" {
            allowed.extend(["inspections", "outlets"]);
        }
    }
    if !allowed.is_empty() && !allowed.iter().any(|page| list_has(user.at("permissions"), page)) {
        return Ok(Some(Reply::Json(403, obj([("ok", false.into()), ("error", "You do not have access to this section".into())]))));
    }
    Ok(None)
}

fn guard_outlets(ctx: &Ctx, payload: &Value) -> R<()> {
    let user = ctx.user()?;
    if accounts::allowed(&user).is_some() {
        with_db(|db| accounts::guard_mutation(db, &user, &ctx.method, &ctx.path, payload))?;
    }
    Ok(())
}

/// Changes to managed records need the person's permission; without approval rights they wait for an approver.
fn hold_for_approval(ctx: &Ctx, payload: &Value) -> R<Option<Reply>> {
    let Some((record, record_id)) = crate::changes::record_for(&ctx.method, &ctx.path) else { return Ok(None) };
    let user = ctx.user()?;
    let label = change_record(record).map(|entry| entry.1).unwrap_or(record);
    if !crate::changes::allowed(&user, record, "manage") {
        return Err(forbidden(format!("You cannot add, edit, or delete {}", label.to_lowercase())));
    }
    if crate::changes::allowed(&user, record, "approve") {
        return Ok(None);
    }
    Ok(Some(Reply::Json(200, crate::changes::hold(&user, &ctx.method, &ctx.path, payload, record, record_id)?)))
}

// ---- Reads ----

fn listed(viewer: &Value, data: &Value, key: &str) -> Value {
    let mut result = data.clone();
    let items = accounts::keep(viewer, data.at("items").items().to_vec(), key);
    result.set("items", Value::Array(items));
    result
}

fn unit_param(ctx: &Ctx) -> String {
    ctx.param("unit").unwrap_or_else(|| "Ottotree".into())
}

fn download(body: Vec<u8>, mime: &str, filename: &str) -> Reply {
    let filename: String = filename.chars().map(|c| if c.is_ascii_alphanumeric() || "._ -".contains(c) { c } else { '_' }).collect();
    Reply::Raw(
        Response::new(200, mime, body)
            .header("Content-Disposition", format!("attachment; filename=\"{filename}\""))
            .header("Cache-Control", "no-store"),
    )
}

const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

fn get(ctx: &Ctx) -> R<Reply> {
    if let Some(refusal) = require_auth(ctx)? {
        return Ok(refusal);
    }
    let path = ctx.path.as_str();
    if path == "/api/auth/me" || path == "/api/account" {
        let user = ctx.user()?;
        if user.is_null() {
            return json_status(401, obj([("ok", false.into()), ("error", "Login required".into())]));
        }
        return ok(obj([("ok", true.into()), ("user", user)]));
    }
    if path == "/api/account/databases" {
        if !is_super_user(&ctx.user()?) {
            return error_json(403, "Super access required");
        }
        return ok(obj([("databases", crate::databases::list_databases()?)]));
    }
    if path == "/api/branding" {
        return ok(accounts::branding_settings()?);
    }
    if let Some(identifier) = path.strip_prefix("/api/media/") {
        let stored = (|| -> R<Option<(Vec<u8>, String)>> {
            let database = crate::db::open(&config::db_path())?;
            crate::media::read(&database, identifier)
        })();
        return match stored {
            Ok(Some((body, mime))) => Ok(Reply::Raw(Response::new(200, &mime, body).header("Cache-Control", "private, max-age=3600"))),
            Ok(None) | Err(AppError::Status(400, _)) => send_error(404),
            Err(error) => Err(error),
        };
    }
    let viewer = ctx.user()?;
    let scope = accounts::allowed(&viewer);
    // Outlet-limited accounts get their own cached copy; everyone else shares one.
    let scope_key = match &scope {
        Some(codes) => codes.join(","),
        None => "*".into(),
    };
    let scope_filter = || -> Value { scope.as_ref().map(|codes| crate::json::strings(codes)).unwrap_or(Value::Null) };
    match path {
        "/api/dashboard" => {
            let unit = unit_param(ctx);
            let filters = obj([("outlets", scope_filter())]);
            let prepared = crate::db::cached(&format!("dashboard|{unit}|{scope_key}"), || crate::reports::dashboard(&unit, &filters, false))?;
            return Ok(Reply::Shared(prepared));
        }
        "/api/todo" => return ok(listed(&viewer, &crate::todo::todo_items(&viewer)?, "outlet")),
        "/api/schedules" => return ok(listed(&viewer, &crate::inspections::schedule_items()?, "outlet")),
        "/api/schedules/assignees" => {
            let outlet = ctx.param("outlet").unwrap_or_default();
            if !outlet.is_empty() {
                accounts::require(&viewer, &outlet)?;
            }
            return ok(obj([("items", Value::Array(with_db(|db| crate::notify::assignable_people(db, &outlet))?))]));
        }
        "/api/work-orders" => return ok(listed(&viewer, &crate::work::work_order_items(&viewer)?, "outlet")),
        "/api/work-requests" => return ok(listed(&viewer, &crate::work::work_request_items(&viewer)?, "outlet")),
        "/api/changes" => return ok(crate::changes::change_items(&viewer)?),
        "/api/findings" => return ok(listed(&viewer, &crate::work::finding_items("Ottotree", &Value::object(), &viewer)?, "outlet")),
        "/api/equipment" => {
            let outlet = ctx.param("outlet");
            if let Some(outlet) = &outlet {
                accounts::require(&viewer, outlet)?;
            }
            let compact = ctx.param("view").as_deref() == Some("inspection");
            let data = catalog::equipment_items(outlet.as_deref(), compact)?;
            return Ok(if scope.is_none() { Reply::Shared(data) } else { Reply::Json(200, listed(&viewer, &data.data, "outlet")) });
        }
        "/api/inspection-sessions" => return ok(listed(&viewer, &crate::inspections::inspection_sessions()?, "outlet")),
        "/api/locations" => {
            let outlet = ctx.param("outlet").unwrap_or_default();
            if !outlet.is_empty() {
                accounts::require(&viewer, &outlet)?;
            }
            let brief = ctx.param_all("brief") == ["1"];
            return ok(listed(&viewer, &catalog::locations(&outlet, brief)?, "outlet_code"));
        }
        "/api/zones" => {
            let outlet = ctx.param("outlet").unwrap_or_default();
            if !outlet.is_empty() {
                accounts::require(&viewer, &outlet)?;
            }
            return ok(listed(&viewer, &catalog::zones(&outlet)?, "outlet_code"));
        }
        "/api/location-report.pdf" | "/api/location-report.xlsx" => {
            let value = |key: &str| ctx.param(key).unwrap_or_default();
            let (outlet, location, from, to) = (value("outlet"), value("location"), value("from"), value("to"));
            if outlet.is_empty() || location.is_empty() {
                return Err(bad("Choose an outlet and a location for the location report"));
            }
            accounts::require(&viewer, &outlet)?;
            let name = format!("{outlet}_{location}").replace('/', "-");
            return Ok(if path.ends_with(".pdf") {
                download(crate::exports::location_pdf(&outlet, &location, &from, &to)?, "application/pdf", &format!("{name}.pdf"))
            } else {
                download(crate::exports::location_xlsx(&outlet, &location, &from, &to)?, XLSX, &format!("{name}.xlsx"))
            });
        }
        "/api/setup" => {
            let include_super = is_super_user(&viewer);
            let prepared = crate::db::cached(&format!("setup|{include_super}"), || catalog::setup_records(include_super))?;
            let Some(codes) = &scope else { return Ok(Reply::Shared(prepared)) };
            let mut data = prepared.data.clone();
            let keep = |rows: &Value, key: &str| Value::Array(rows.items().iter().filter(|row| row.at(key).as_str().is_some_and(|code| codes.iter().any(|allowed| allowed == code))).cloned().collect());
            data.set("outlets", keep(prepared.data.at("outlets"), "code"));
            data.set("zones", keep(prepared.data.at("zones"), "outlet_code"));
            return ok(data);
        }
        "/api/roles" => return ok(catalog::role_items(is_super_user(&viewer))?),
        "/api/users" => return ok(catalog::users(is_super_user(&viewer))?),
        "/api/notifications" => return ok(crate::work::notifications(viewer.at("id").as_i64().unwrap_or(0))?),
        "/api/comments" => {
            let kind = ctx.param("type").unwrap_or_default();
            let id = ctx.param("id").unwrap_or_default();
            if scope.is_some() && kind == "work_order" && crate::common::is_digits(&id) {
                let outlet = with_db(|db| accounts::record_outlet(db, "work-orders", id.parse().unwrap_or(0)))?;
                accounts::require(&viewer, &outlet.text())?;
            }
            let id = if id.is_empty() { "0".to_string() } else { id };
            return ok(crate::work::comments(&kind, &id)?);
        }
        _ => {}
    }
    if let Some(rest) = path.strip_prefix("/api/inspection-sessions/") {
        return inspection_read(ctx, &viewer, rest);
    }
    if path.starts_with("/api/reports") || path == "/api/activity" {
        let mut filters = Value::object();
        for key in ["outlet", "from", "to"] {
            filters.set(key, ctx.param(key).unwrap_or_default());
        }
        let outlet = filters.at("outlet").text();
        if !outlet.is_empty() {
            accounts::require(&viewer, &outlet)?;
        }
        if let Some(codes) = &scope {
            filters.set("outlets", crate::json::strings(codes));
        }
        let unit = unit_param(ctx);
        match path {
            "/api/activity" => return ok(crate::reports::activity_items(&unit, &filters)?),
            "/api/reports" => {
                let key = format!("report|{unit}|{outlet}|{}|{}|{scope_key}", filters.at("from").text(), filters.at("to").text());
                return Ok(Reply::Shared(crate::db::cached(&key, || crate::reports::report(&unit, &filters))?));
            }
            "/api/reports/export.pdf" => return Ok(download(crate::exports::report_pdf(&unit, &filters)?, "application/pdf", "audit-report.pdf")),
            "/api/reports/export.xls" | "/api/reports/export.xlsx" => return Ok(download(crate::exports::report_xlsx(&unit, &filters)?, XLSX, "audit-report.xlsx")),
            _ => {}
        }
    }
    if let Some(rest) = path.strip_prefix("/api/users/").and_then(|rest| rest.strip_suffix("/activity")) {
        if crate::common::is_digits(rest) {
            if !is_company_admin_user(&viewer) {
                return Err(forbidden("Admin access required"));
            }
            let offset = ctx.param("offset").unwrap_or_else(|| "0".into());
            return ok(catalog::user_login_activity(rest.parse().unwrap_or(0), &offset)?);
        }
    }
    static_file(ctx)
}

fn inspection_read(ctx: &Ctx, viewer: &Value, rest: &str) -> R<Reply> {
    let suffix = ctx.path.rsplit('/').next().unwrap_or("");
    if suffix == "export.xlsx" || suffix == "export.pdf" {
        let session_id = ctx.path.split('/').rev().nth(1).unwrap_or("");
        if !crate::common::is_digits(session_id) {
            return send_error(400);
        }
        let Some(session) = crate::inspections::inspection_session(session_id.parse().unwrap_or(0))? else { return send_error(404) };
        accounts::require(viewer, &session.at("outlet").text())?;
        let name = if session.at("audit_ref").truthy() {
            session.at("audit_ref").py_str()
        } else {
            crate::common::inspection_name("", &session.at("outlet").text(), session.at("audit_date"))
        };
        if suffix == "export.xlsx" {
            return Ok(download(crate::exports::inspection_xlsx(&session)?, XLSX, &format!("{name}.xlsx")));
        }
        // Overall, or the chosen locations (?location=A&location=B) in full.
        let chosen = ctx.param_all("location");
        if chosen.is_empty() {
            return Ok(download(crate::exports::inspection_pdf(&session)?, "application/pdf", &format!("{name}.pdf")));
        }
        let label = if chosen.len() == 1 { chosen[0].clone() } else { format!("{} locations", chosen.len()) };
        return Ok(download(crate::exports::inspection_locations_pdf(&session, &chosen)?, "application/pdf", &format!("{name} - {label}.pdf")));
    }
    let _ = rest;
    if !crate::common::is_digits(suffix) {
        return send_error(400);
    }
    let Some(session) = crate::inspections::inspection_session(suffix.parse().unwrap_or(0))? else { return send_error(404) };
    accounts::require(viewer, &session.at("outlet").text())?;
    ok(session)
}

// ---- Changes ----

fn read_payload(request: &mut Request, limit: usize) -> Result<Value, Reply> {
    let body = match request.read_body(limit) {
        Body::Bytes(bytes) => bytes,
        Body::TooLarge => {
            let message = if limit >= 1024 * 1024 { format!("Request body exceeds {} MiB", limit / 1024 / 1024) } else { "Request body is too large".into() };
            let mut response = json_response(413, &obj([("error", message.into())]), false);
            response.close = true;
            return Err(Reply::Raw(response));
        }
        Body::Invalid(message) => return Err(Reply::Json(400, obj([("error", message.into())]))),
    };
    let payload = if body.is_empty() { Ok(Value::object()) } else { crate::json::parse_bytes(&body) };
    let payload = match payload {
        Ok(value) => value,
        Err(message) => return Err(Reply::Json(400, obj([("error", message.into())]))),
    };
    if !payload.is_object() {
        return Err(Reply::Json(400, obj([("error", "JSON body must be an object".into())])));
    }
    if let Some(items) = payload.get("items") {
        if !items.is_array() || items.items().iter().any(|item| !item.is_object()) {
            return Err(Reply::Json(400, obj([("error", "items must be an array of objects".into())])));
        }
    }
    Ok(payload)
}

pub type Route = fn(&Ctx, &Value) -> R<Reply>;

/// The mutation registry: (method, path) → route. PATCH and DELETE also match the path without
/// its last segment (the record id).
pub fn find_route(method: &str, path: &str) -> Option<Route> {
    use crate::routes::*;
    let table: &[(&str, &str, Route)] = &[
        ("POST", "/api/auth/login", accounts_routes::login),
        ("POST", "/api/auth/logout", accounts_routes::logout),
        ("POST", "/api/auth/forgot-password", accounts_routes::forgot_password),
        ("POST", "/api/auth/register", accounts_routes::register),
        ("POST", "/api/auth/change-password", accounts_routes::change_password),
        ("POST", "/api/audits/start", inspection_routes::start_audit),
        ("POST", "/api/inspection-sessions", inspection_routes::post_inspection),
        ("POST", "/api/inspection-sessions/close", inspection_routes::close_audit),
        ("POST", "/api/schedules", inspection_routes::post_schedule),
        ("POST", "/api/schedules/start", inspection_routes::start_schedule),
        ("POST", "/api/work-orders", work_routes::post_work_order),
        ("POST", "/api/work-requests", work_routes::post_work_request),
        ("POST", "/api/equipment", asset_routes::post_equipment),
        ("POST", "/api/equipment/bulk", asset_routes::post_equipment_bulk),
        ("POST", "/api/locations", location_routes::post_location),
        ("POST", "/api/zones", location_routes::post_zone),
        ("POST", "/api/setup/departments", setup_routes::post_department),
        ("POST", "/api/setup/outlets", location_routes::post_outlet),
        ("POST", "/api/users", accounts_routes::post_user),
        ("POST", "/api/roles", accounts_routes::post_role),
        ("POST", "/api/setup/priorities", setup_routes::post_priority),
        ("POST", "/api/setup/audit-types", setup_routes::post_audit_type),
        ("POST", "/api/settings", setup_routes::post_settings),
        ("POST", "/api/comments", work_routes::post_comment),
        ("POST", "/api/account/databases", accounts_routes::post_database),
        ("PATCH", "/api/account", accounts_routes::patch_account),
        ("PATCH", "/api/account/navigation", accounts_routes::patch_navigation),
        ("PATCH", "/api/account/databases", accounts_routes::patch_database),
        ("PATCH", "/api/inspection-sessions", inspection_routes::patch_inspection),
        ("PATCH", "/api/users", accounts_routes::patch_user),
        ("PATCH", "/api/roles", accounts_routes::patch_role),
        ("PATCH", "/api/setup/priorities", setup_routes::patch_priority),
        ("PATCH", "/api/setup/audit-types", setup_routes::patch_audit_type),
        ("PATCH", "/api/notifications", work_routes::patch_notification),
        ("PATCH", "/api/setup/departments", setup_routes::patch_department),
        ("PATCH", "/api/setup/outlets", location_routes::patch_outlet),
        ("PATCH", "/api/equipment", asset_routes::patch_equipment),
        ("PATCH", "/api/equipment/bulk", asset_routes::patch_equipment_bulk),
        ("PATCH", "/api/work-orders", work_routes::patch_work_order),
        ("PATCH", "/api/work-requests", work_routes::patch_work_request),
        ("PATCH", "/api/findings", finding_routes::patch_finding),
        ("PATCH", "/api/changes", crate::changes::patch_change),
        ("PATCH", "/api/locations", location_routes::patch_location),
        ("PATCH", "/api/zones", location_routes::patch_zone),
        ("PATCH", "/api/schedules", inspection_routes::patch_schedule),
        ("DELETE", "/api/inspection-sessions", inspection_routes::delete_inspection),
        ("DELETE", "/api/setup/departments", setup_routes::delete_department),
        ("DELETE", "/api/setup/priorities", setup_routes::delete_priority),
        ("DELETE", "/api/setup/audit-types", setup_routes::delete_audit_type),
        ("DELETE", "/api/notifications", work_routes::delete_notification),
        ("DELETE", "/api/comments", work_routes::delete_comment),
        ("DELETE", "/api/setup/outlets", location_routes::delete_outlet),
        ("DELETE", "/api/users", accounts_routes::delete_user),
        ("DELETE", "/api/roles", accounts_routes::delete_role),
        ("DELETE", "/api/locations", location_routes::delete_location),
        ("DELETE", "/api/zones", location_routes::delete_zone),
        ("DELETE", "/api/equipment", asset_routes::delete_equipment),
        ("DELETE", "/api/work-orders", work_routes::delete_work_order),
        ("DELETE", "/api/findings", finding_routes::delete_finding),
        ("DELETE", "/api/schedules", inspection_routes::delete_schedule),
        ("DELETE", "/api/account/databases", accounts_routes::delete_database),
    ];
    let parent = if method == "POST" { path } else { path.rsplit_once('/').map(|(head, _)| head).unwrap_or(path) };
    table.iter().find(|(m, p, _)| *m == method && *p == path).or_else(|| table.iter().find(|(m, p, _)| *m == method && *p == parent)).map(|(_, _, route)| *route)
}

/// Run the route for this method and path, as the change's original request or its replay.
pub fn dispatch(ctx: &Ctx, payload: &Value) -> R<Option<Reply>> {
    match find_route(&ctx.method, &ctx.path) {
        Some(route) => route(ctx, payload).map(Some),
        None => Ok(None),
    }
}

fn mutate(ctx: &Ctx, request: &mut Request) -> R<Reply> {
    if ctx.method == "POST" && ctx.path.starts_with("/api/auth/") {
        let payload = match read_payload(request, UNAUTHENTICATED_BODY_LIMIT) {
            Ok(payload) => payload,
            Err(reply) => return Ok(reply),
        };
        return Ok(dispatch(ctx, &payload)?.unwrap_or(Reply::Error(404)));
    }
    if let Some(refusal) = require_auth(ctx)? {
        return Ok(Reply::Raw(close(render(refusal, ctx))));
    }
    let payload = if ctx.method == "DELETE" {
        Value::Null
    } else {
        let payload = match read_payload(request, BODY_LIMIT) {
            Ok(payload) => payload,
            Err(reply) => return Ok(reply),
        };
        let normalized = (|| -> R<Value> {
            let database = crate::db::open(&config::db_path())?;
            let result = crate::media::normalize(&database, &payload);
            crate::db::finish(&database, result)
        })()?;
        if ctx.method == "POST" && ctx.path == "/api/media" {
            if !normalized.at("image").is_object() || !normalized.at("image").at("url").truthy() {
                return error_json(400, "An image is required");
            }
            return ok(obj([("image", normalized.at("image").clone())]));
        }
        normalized
    };
    guard_outlets(ctx, &payload)?;
    if let Some(held) = hold_for_approval(ctx, &payload)? {
        return Ok(held);
    }
    Ok(dispatch(ctx, &payload)?.unwrap_or(Reply::Error(404)))
}

fn close(mut response: Response) -> Response {
    response.close = true;
    response
}

// ---- Static files ----

struct StaticEntry {
    fingerprint: Vec<(String, u128, u64)>,
    body: Arc<Vec<u8>>,
    packed: Arc<Vec<u8>>,
    etag: String,
}

fn static_cache() -> &'static Mutex<HashMap<String, StaticEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, StaticEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stamp(path: &std::path::Path) -> (String, u128, u64) {
    let meta = std::fs::metadata(path).ok();
    let modified = meta.as_ref().and_then(|meta| meta.modified().ok()).and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|elapsed| elapsed.as_nanos()).unwrap_or(0);
    (path.display().to_string(), modified, meta.map(|meta| meta.len()).unwrap_or(0))
}

fn html_partials(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                html_partials(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "html") {
                out.push(path);
            }
        }
    }
}

/// Expand `<!-- include: html/... -->` comments, refusing loops and deep nesting.
fn expand_includes(text: &str, parents: &[std::path::PathBuf]) -> R<String> {
    let root = config::root();
    let html = root.join("html");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        let Some(end) = rest[start..].find("-->") else { break };
        let comment = &rest[start + 4..start + end];
        let inner = comment.trim();
        let target = inner.strip_prefix("include:").map(str::trim).filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c)));
        out.push_str(&rest[..start]);
        match target {
            Some(name) => {
                let partial = std::fs::canonicalize(root.join(name)).ok().filter(|path| path.starts_with(&html) && path.is_file());
                if let Some(partial) = partial {
                    if parents.contains(&partial) || parents.len() >= 8 {
                        return Err(bad("Circular or excessively nested HTML include"));
                    }
                    let mut chain = parents.to_vec();
                    chain.push(partial.clone());
                    out.push_str(&expand_includes(&std::fs::read_to_string(&partial)?, &chain)?);
                }
            }
            None => out.push_str(&rest[start..start + end + 3]),
        }
        rest = &rest[start + end + 3..];
    }
    out.push_str(rest);
    Ok(out)
}

fn accepts_gzip(headers: &[(String, String)]) -> bool {
    let header = headers.iter().find(|(key, _)| key.eq_ignore_ascii_case("Accept-Encoding")).map(|(_, value)| value.as_str()).unwrap_or("");
    for entry in header.split(',') {
        let mut parts = entry.trim().split(';');
        if parts.next().map(str::trim) == Some("gzip") {
            return !parts.any(|part| {
                let part = part.trim();
                part.strip_prefix('q').map(|rest| rest.trim_start()).and_then(|rest| rest.strip_prefix('=')).is_some_and(|value| {
                    let value = value.trim();
                    value == "0" || (value.starts_with("0.") && value[2..].chars().all(|c| c == '0'))
                })
            });
        }
    }
    false
}

fn static_file(ctx: &Ctx) -> R<Reply> {
    let request_path = ctx.path.as_str();
    let relative = if request_path.is_empty() || request_path == "/" { "index.html".to_string() } else { crate::http::percent_decode(request_path).trim_start_matches('/').to_string() };
    let root = config::root();
    let target = std::fs::canonicalize(root.join(&relative)).ok();
    let Some(target) = target.filter(|path| path.is_file() && path.starts_with(root)) else { return send_error(404) };
    let under = |dir: &str, extension: &str| relative.starts_with(&format!("{dir}/")) && target.starts_with(root.join(dir)) && target.extension().is_some_and(|ext| ext == extension);
    let allowed = matches!(relative.as_str(), "index.html" | "login.html" | "register.html") || under("fonts", "woff2") || under("js", "js") || under("css", "css");
    if !allowed {
        return send_error(404);
    }
    let is_index = target.file_name().is_some_and(|name| name == "index.html");
    if is_index && ctx.user()?.is_null() {
        // Without a session the app shell would draw, then send the visitor away once its scripts
        // asked who is signed in; send them to the sign-in page before anything is drawn.
        return Ok(Reply::Raw(Response::new(302, "text/plain; charset=utf-8", Vec::new()).header("Location", "/login.html").header("Cache-Control", "no-store")));
    }
    let mut dependencies = vec![target.clone()];
    if is_index {
        let mut partials = Vec::new();
        html_partials(&root.join("html"), &mut partials);
        partials.sort();
        dependencies.extend(partials);
    }
    let fingerprint: Vec<(String, u128, u64)> = dependencies.iter().map(|path| stamp(path)).collect();
    let key = target.display().to_string();
    let (body, packed, etag) = {
        let cached = static_cache().lock().unwrap_or_else(|poison| poison.into_inner()).get(&key).filter(|entry| entry.fingerprint == fingerprint).map(|entry| (entry.body.clone(), entry.packed.clone(), entry.etag.clone()));
        match cached {
            Some(found) => found,
            None => {
                let body = if is_index {
                    expand_includes(&std::fs::read_to_string(&target)?, &[])?.into_bytes()
                } else {
                    std::fs::read(&target)?
                };
                let etag = format!("\"{}\"", sha256_hex(&body));
                let entry = StaticEntry { fingerprint, body: Arc::new(body), packed: Arc::new(Vec::new()), etag: etag.clone() };
                let packed = Arc::new(gzip(&entry.body));
                let entry = StaticEntry { packed: packed.clone(), ..entry };
                let body = entry.body.clone();
                static_cache().lock().unwrap_or_else(|poison| poison.into_inner()).insert(key, entry);
                (body, packed, etag)
            }
        }
    };
    let use_gzip = accepts_gzip(&ctx.headers);
    let (body, etag) = if use_gzip { (packed, format!("{}-gzip\"", &etag[..etag.len() - 1])) } else { (body, etag) };
    let extension = target.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    let mime = match extension {
        "woff2" => "font/woff2",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    };
    let matches = ctx.header("If-None-Match").map(|value| value.split(',').any(|tag| tag.trim() == etag)).unwrap_or(false);
    if matches {
        return Ok(Reply::Raw(
            Response { status: 304, headers: Vec::new(), body: Vec::new(), close: false }
                .header("ETag", etag)
                .header("Cache-Control", "public, max-age=0, must-revalidate")
                .header("Vary", "Accept-Encoding"),
        ));
    }
    // Fonts never change in place; everything else is revalidated so edits show at once.
    let cache = if mime.starts_with("font/") { "public, max-age=604800, immutable" } else { "public, max-age=0, must-revalidate" };
    let mut response = Response::new(200, mime, body.to_vec()).header("Cache-Control", cache).header("ETag", etag).header("Vary", "Accept-Encoding");
    if use_gzip {
        response = response.header("Content-Encoding", "gzip");
    }
    Ok(Reply::Raw(response))
}

// ---- Answers ----

fn json_response(status: u16, value: &Value, use_gzip: bool) -> Response {
    let body = value.to_json_bytes();
    json_body(status, body, None, use_gzip)
}

fn json_body(status: u16, body: Vec<u8>, packed: Option<&Vec<u8>>, use_gzip: bool) -> Response {
    let use_gzip = use_gzip && body.len() >= 1024;
    let mut response = Response::new(status, "application/json", Vec::new()).header("Cache-Control", "no-store").header("Vary", "Accept-Encoding");
    if use_gzip {
        response.body = packed.cloned().unwrap_or_else(|| gzip(&body));
        response = response.header("Content-Encoding", "gzip");
    } else {
        response.body = body;
    }
    response
}

fn render(reply: Reply, ctx: &Ctx) -> Response {
    let use_gzip = accepts_gzip(&ctx.headers);
    match reply {
        Reply::Json(status, value) => json_response(status, &value, use_gzip),
        Reply::Shared(prepared) => {
            let (body, packed) = prepared.bodies();
            json_body(200, body.clone(), Some(packed), use_gzip)
        }
        Reply::Error(status) => Response::error(status),
        Reply::Raw(response) => response,
    }
}

fn failure(error: AppError, ctx: &Ctx) -> Response {
    let use_gzip = accepts_gzip(&ctx.headers);
    match error {
        AppError::Status(code, message) => {
            let message = if message.is_empty() { "Invalid request fields".to_string() } else { message };
            json_response(code, &obj([("error", message.into())]), use_gzip)
        }
        AppError::Db(error) if error.is_constraint() => json_response(409, &obj([("error", "This record conflicts with an existing record".into())]), use_gzip),
        AppError::Db(error) if error.code == 21 => json_response(400, &obj([("error", error.message.into())]), use_gzip),
        AppError::Db(error) => {
            log_warning(&format!("Database unavailable during {} {}: {error}", ctx.method, ctx.path));
            json_response(503, &obj([("error", "Database is busy; retry the request".into())]), use_gzip)
        }
        AppError::Internal(message) => {
            // Without this, an unexpected fault drops the connection and the browser reports a network error.
            log_error(&format!("Unhandled error during {} {}: {message}", ctx.method, ctx.path));
            close(json_response(500, &obj([("error", "The server could not complete this request".into())]), use_gzip))
        }
    }
}

/// Answer one HTTP request.
pub fn handle(request: &mut Request) -> Response {
    let token = cookie_value(request.header("Cookie").unwrap_or(""), config::session_cookie());
    let ctx = Ctx {
        method: request.method.clone(),
        path: request.path.clone(),
        query: request.params(),
        headers: request.headers.clone(),
        client: client_address(request),
        token,
        user: RefCell::new(None),
    };
    let result = match ctx.method.as_str() {
        "GET" => get(&ctx),
        "POST" | "PATCH" | "DELETE" => mutate(&ctx, request),
        _ => Ok(Reply::Error(501)),
    };
    match result {
        Ok(reply) => render(reply, &ctx),
        Err(error) => failure(error, &ctx),
    }
}

/// A route's answer as (status, body) for a replayed change.
pub fn reply_parts(reply: Reply) -> (u16, Value) {
    match reply {
        Reply::Json(status, value) => (status, value),
        Reply::Shared(prepared) => (200, prepared.data.clone()),
        Reply::Error(status) => (status, obj([("error", "The change could not be applied".into())])),
        Reply::Raw(response) => (response.status, crate::json::parse_bytes(&response.body).unwrap_or(Value::Null)),
    }
}

/// Status of an AppError as a replayed route would have answered it.
pub fn error_parts(error: &AppError) -> (u16, String) {
    match error {
        AppError::Status(code, message) => (*code, if message.is_empty() { "Invalid request fields".into() } else { message.clone() }),
        AppError::Db(error) if error.is_constraint() => (409, "This record conflicts with an existing record".into()),
        AppError::Db(_) => (503, "Database is busy; retry the request".into()),
        AppError::Internal(_) => (500, "The server could not complete this request".into()),
    }
}
