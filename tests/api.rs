//! The application over HTTP: each test starts the real server on a fresh data directory and
//! talks to it as the browser does.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use audit_app::json::{obj, parse_bytes, Value};

const JPEG: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAYEBQYFBAYGBQYHBwYIChAKCgkJChQODwwQFxQYGBcUFhYaHSUfGhsjHBYWICwgIyYnKSopGR8tMC0oMCUoKSj/2wBDAQcHBwoIChMKChMoGhYaKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCj/wAARCAAIAAgDASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwDiqKKK8A/Wz//Z";
const NEW_PASSWORD: &str = "Change-me-9";

struct Server {
    child: Child,
    port: u16,
    dir: PathBuf,
    owned: bool,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("audit-test-{}-{}", std::process::id(), audit_app::crypto::token_hex(6)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

impl Server {
    fn start() -> Server {
        let dir = temp_dir();
        Server::start_in(&dir, true, &[])
    }

    fn start_in(dir: &Path, owned: bool, extra: &[(&str, &str)]) -> Server {
        let port = free_port();
        let mut command = Command::new(env!("CARGO_BIN_EXE_audit-app"));
        command
            .env("AUDIT_DATA_DIR", dir.join("data"))
            .env("AUDIT_CONTROL_DB", dir.join("control.db"))
            .env("AUDIT_WEB_ROOT", Path::new(env!("CARGO_MANIFEST_DIR")).join("web"))
            .env("PORT", port.to_string())
            .env_remove("AUDIT_SESSION_COOKIE")
            .env_remove("AUDIT_TRUST_PROXY")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (key, value) in extra {
            command.env(key, value);
        }
        let child = command.spawn().expect("the server starts");
        let started = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(started.elapsed() < Duration::from_secs(60), "the server did not start");
            std::thread::sleep(Duration::from_millis(50));
        }
        Server { child, port, dir: dir.to_path_buf(), owned }
    }

    fn client(&self) -> Client {
        Client { port: self.port, cookie: String::new() }
    }

    /// A client signed in as the starter administrator, past the required password change.
    fn admin(&self) -> Client {
        let mut client = self.client();
        assert_eq!(client.post("/api/auth/login", obj([("identifier", "admin".into()), ("password", "password123".into())])).0, 200);
        let (status, _) = client.post("/api/auth/change-password", obj([("oldPassword", "password123".into()), ("newPassword", NEW_PASSWORD.into())]));
        assert_eq!(status, 200);
        client
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
        if self.owned {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    fn json(&self) -> Value {
        parse_bytes(&self.body).unwrap_or(Value::Null)
    }
}

struct Client {
    port: u16,
    cookie: String,
}

fn cookie_name(port: u16) -> String {
    if port == 41883 {
        "ottotree_session".into()
    } else {
        format!("ottotree_session_{port}")
    }
}

impl Client {
    fn raw(&mut self, method: &str, path: &str, body: &[u8], headers: &[(&str, &str)]) -> Reply {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
        let mut head = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
        if !self.cookie.is_empty() {
            head.push_str(&format!("Cookie: {}={}\r\n", cookie_name(self.port), self.cookie));
        }
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).unwrap();
        let _ = stream.write_all(body);
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
        let split = response.windows(4).position(|window| window == b"\r\n\r\n").expect("a complete response") + 4;
        let text = String::from_utf8_lossy(&response[..split]).into_owned();
        let mut lines = text.lines();
        let status = lines.next().unwrap().split(' ').nth(1).unwrap().parse().unwrap();
        let headers: Vec<(String, String)> = lines.filter_map(|line| line.split_once(':').map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))).collect();
        for (key, value) in &headers {
            if key.eq_ignore_ascii_case("Set-Cookie") {
                if let Some(rest) = value.strip_prefix(&format!("{}=", cookie_name(self.port))) {
                    self.cookie = rest.split(';').next().unwrap_or("").to_string();
                }
            }
        }
        Reply { status, headers, body: response[split..].to_vec() }
    }

    fn call(&mut self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let bytes = body.map(|value| value.to_json_bytes()).unwrap_or_default();
        let reply = self.raw(method, path, &bytes, &[("Content-Type", "application/json")]);
        let json = reply.json();
        (reply.status, json)
    }

    fn get(&mut self, path: &str) -> (u16, Value) {
        self.call("GET", path, None)
    }

    fn post(&mut self, path: &str, body: Value) -> (u16, Value) {
        self.call("POST", path, Some(body))
    }

    fn patch(&mut self, path: &str, body: Value) -> (u16, Value) {
        self.call("PATCH", path, Some(body))
    }

    fn delete(&mut self, path: &str) -> (u16, Value) {
        self.call("DELETE", path, None)
    }

    fn photo(&mut self) -> Value {
        let (status, body) = self.post("/api/media", obj([("image", obj([("name", "p.jpg".into()), ("dataUrl", Value::str(format!("data:image/jpeg;base64,{JPEG}")))]))]));
        assert_eq!(status, 200, "{}", body.to_json());
        body.at("image").clone()
    }
}

fn items(value: &Value) -> Vec<Value> {
    value.at("items").items().to_vec()
}

// ---- Accounts and sessions ----

#[test]
fn new_databases_hold_accounts_until_their_password_changes() {
    let server = Server::start();
    let mut client = server.client();
    assert_eq!(client.get("/api/todo").0, 401);
    let (status, body) = client.post("/api/auth/login", obj([("identifier", "admin@ottotree.local".into()), ("password", "password123".into())]));
    assert_eq!(status, 200);
    assert_eq!(body.at("user").at("resetRequired"), &Value::Bool(true));
    let (status, body) = client.get("/api/todo");
    assert_eq!((status, body.at("resetRequired").clone()), (403, Value::Bool(true)));
    assert_eq!(client.get("/api/account").0, 200);
    // The page shell holds the password dialog, so it opens before the password is changed.
    assert_eq!(client.raw("GET", "/", b"", &[]).status, 200);
    assert_eq!(client.post("/api/auth/change-password", obj([("oldPassword", "wrong".into()), ("newPassword", NEW_PASSWORD.into())])).0, 400);
    assert_eq!(client.post("/api/auth/change-password", obj([("oldPassword", "password123".into()), ("newPassword", "short".into())])).0, 400);
    assert_eq!(client.post("/api/auth/change-password", obj([("oldPassword", "password123".into()), ("newPassword", NEW_PASSWORD.into())])).0, 200);
    assert_eq!(client.get("/api/todo").0, 200);
    // The new password signs in; the old one no longer does.
    let mut other = server.client();
    assert_eq!(other.post("/api/auth/login", obj([("identifier", "admin".into()), ("password", "password123".into())])).0, 401);
    assert_eq!(other.post("/api/auth/login", obj([("identifier", "ADMIN".into()), ("password", NEW_PASSWORD.into())])).0, 200);
    assert_eq!(other.post("/api/auth/logout", Value::object()).0, 200);
    assert_eq!(other.get("/api/todo").0, 401);
}

#[test]
fn failed_sign_ins_are_limited_per_account_and_reset_by_success() {
    let server = Server::start();
    let mut client = server.client();
    for _ in 0..5 {
        assert_eq!(client.post("/api/auth/login", obj([("identifier", "gavin".into()), ("password", "nope".into())])).0, 401);
    }
    let reply = client.raw("POST", "/api/auth/login", br#"{"identifier":"gavin","password":"123456"}"#, &[]);
    assert_eq!(reply.status, 429);
    assert!(reply.header("Retry-After").unwrap().parse::<u64>().unwrap() > 0);
    // Another account is not affected.
    assert_eq!(client.post("/api/auth/login", obj([("identifier", "jacky".into()), ("password", "123456".into())])).0, 200);
}

#[test]
fn unknown_accounts_and_registrations() {
    let server = Server::start();
    let mut client = server.client();
    assert_eq!(client.post("/api/auth/login", obj([("identifier", "nobody@example.com".into()), ("password", "x".into())])).0, 401);
    assert_eq!(client.post("/api/auth/register", obj([("name", "New".into()), ("email", "new@example.com".into()), ("password", "short".into())])).0, 400);
    assert_eq!(client.post("/api/auth/register", obj([("name", "New".into()), ("email", "new@example.com".into()), ("password", "long enough".into())])).0, 200);
    assert_eq!(client.post("/api/auth/register", obj([("name", "New".into()), ("email", "NEW@example.com".into()), ("password", "long enough".into())])).0, 409);
    // A registered account waits for activation.
    assert_eq!(client.post("/api/auth/login", obj([("identifier", "new@example.com".into()), ("password", "long enough".into())])).0, 401);
    // The Super account's name cannot be registered by an organization.
    assert_eq!(client.post("/api/auth/register", obj([("name", "S".into()), ("email", "super@sudo".into()), ("password", "long enough".into())])).0, 400);
}

#[test]
fn requests_are_refused_before_their_body_is_read_and_bodies_are_checked() {
    let server = Server::start();
    let mut anonymous = server.client();
    let large = vec![b'x'; 3 * 1024 * 1024];
    let reply = anonymous.raw("POST", "/api/equipment", &large, &[("Content-Type", "application/json")]);
    assert_eq!(reply.status, 401);
    let reply = anonymous.raw("POST", "/api/auth/login", &large, &[("Content-Type", "application/json")]);
    assert_eq!(reply.status, 413);
    let mut admin = server.admin();
    assert_eq!(admin.raw("POST", "/api/equipment", b"[1,2]", &[]).status, 400);
    assert_eq!(admin.raw("POST", "/api/equipment", b"{bad", &[]).status, 400);
    assert_eq!(admin.raw("POST", "/api/inspection-sessions", br#"{"items":{"a":1}}"#, &[]).status, 400);
    assert_eq!(admin.raw("POST", "/api/nowhere", b"{}", &[]).status, 404);
    assert_eq!(admin.raw("PUT", "/api/equipment", b"{}", &[]).status, 501);
}

#[test]
fn static_files_are_allowlisted_compressed_and_revalidated() {
    let server = Server::start();
    let mut anonymous = server.client();
    for path in ["/", "/index.html"] {
        let away = anonymous.raw("GET", path, b"", &[]);
        assert_eq!((away.status, away.header("Location")), (302, Some("/login.html")), "{path}");
        assert_eq!(away.header("Cache-Control"), Some("no-store"));
    }
    assert_eq!(anonymous.raw("GET", "/login.html", b"", &[]).status, 200);
    let mut client = server.admin();
    let page = client.raw("GET", "/", b"", &[("Accept-Encoding", "gzip")]);
    assert_eq!(page.status, 200);
    assert_eq!(page.header("Content-Encoding"), Some("gzip"));
    assert_eq!(page.header("X-Content-Type-Options"), Some("nosniff"));
    assert_eq!(page.header("X-Frame-Options"), Some("SAMEORIGIN"));
    assert_eq!(page.header("Referrer-Policy"), Some("same-origin"));
    let plain = client.raw("GET", "/index.html", b"", &[]);
    let html = String::from_utf8_lossy(&plain.body);
    assert!(!html.contains("<!-- include:"), "includes are expanded");
    let etag = plain.header("ETag").unwrap().to_string();
    assert_eq!(client.raw("GET", "/index.html", b"", &[("If-None-Match", &etag)]).status, 304);
    assert_eq!(client.raw("GET", "/js/utils.js", b"", &[]).status, 200);
    for path in ["/html/tabs/today.html", "/README.md", "/js/../index.html%00", "/../Cargo.toml", "/fonts/noto-sans-latin-400.ttf", "/data/x.db"] {
        assert_eq!(client.raw("GET", path, b"", &[]).status, 404, "{path}");
    }
}

#[test]
fn each_instance_port_names_its_own_session_cookie() {
    let server = Server::start();
    let mut client = server.client();
    let reply = client.raw("POST", "/api/auth/login", br#"{"identifier":"admin","password":"password123"}"#, &[]);
    let cookie = reply.header("Set-Cookie").unwrap();
    assert!(cookie.starts_with(&format!("ottotree_session_{}=", server.port)), "{cookie}");
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax") && !cookie.contains("Secure"));
}

// ---- The audit workflow ----

/// Schedule a visit to TST's Lobby with one asset, start it, and return (schedule, session, asset).
fn audit_setup(client: &mut Client) -> (i64, i64, i64) {
    assert_eq!(client.post("/api/setup/outlets", obj([("code", "tst".into())])).0, 200);
    assert_eq!(client.post("/api/locations", obj([("outlet", "TST".into()), ("name", "Lobby".into())])).0, 200);
    let (status, asset) = client.post("/api/equipment", obj([("name", "Speaker".into()), ("outlet", "TST".into()), ("location", "Lobby".into()), ("serialNumber", "SN-1".into())]));
    assert_eq!(status, 200, "{}", asset.to_json());
    let (status, schedule) = client.post("/api/schedules", obj([("outlet", "TST".into()), ("scheduledDate", "2026-11-02".into()), ("locations", Value::Array(vec!["Lobby".into()]))]));
    assert_eq!(status, 200, "{}", schedule.to_json());
    let schedule_id = schedule.at("id").as_i64().unwrap();
    let (status, started) = client.post("/api/schedules/start", obj([("scheduleId", Value::Int(schedule_id))]));
    assert_eq!(status, 200);
    (schedule_id, started.at("id").as_i64().unwrap(), asset.at("id").as_i64().unwrap())
}

fn checks(asset: i64, photo: Option<&Value>, pass_second: bool) -> Value {
    let images = photo.map(|photo| Value::Array(vec![photo.clone()])).unwrap_or_else(Value::array);
    Value::Array(vec![
        obj([("equipmentId", Value::str(asset.to_string())), ("section", "Speaker".into()), ("item", "Clean".into()), ("location", "Lobby".into()), ("passed", true.into()), ("images", images.clone())]),
        obj([("equipmentId", Value::str(asset.to_string())), ("section", "Speaker".into()), ("item", "Works".into()), ("location", "Lobby".into()), ("passed", pass_second.into()), ("notes", "No sound".into()), ("images", images)]),
    ])
}

#[test]
fn an_audit_goes_from_schedule_to_closure() {
    let server = Server::start();
    let mut client = server.admin();
    let (schedule_id, session, asset) = audit_setup(&mut client);
    // Starting again resumes the same inspection.
    assert_eq!(client.post("/api/schedules/start", obj([("scheduleId", Value::Int(schedule_id))])).1.at("id").as_i64(), Some(session));
    let path = format!("/api/inspection-sessions/{session}");
    // An incomplete inspection cannot be completed; photos are required.
    let draft = obj([("items", checks(asset, None, false)), ("complete", true.into())]);
    let (status, _) = client.patch(&path, draft);
    assert_eq!(status, 409);
    let photo = client.photo();
    assert!(photo.at("url").as_str().unwrap().starts_with("/api/media/"));
    let (status, saved) = client.patch(&path, obj([("items", checks(asset, Some(&photo), false)), ("auditTime", "10:30".into())]));
    assert_eq!((status, saved.at("progress").as_i64()), (200, Some(100)));
    let (status, done) = client.patch(&path, obj([("items", checks(asset, Some(&photo), false)), ("complete", true.into())]));
    assert_eq!(status, 200, "{}", done.to_json());
    assert_eq!(done.at("status").as_str(), Some("Completed"));
    // Completed items are read-only.
    assert_eq!(client.patch(&path, obj([("items", checks(asset, Some(&photo), true))])).0, 409);
    let (_, record) = client.get(&path);
    assert_eq!(record.at("scoring").at("score").as_i64(), Some(50));
    assert_eq!(record.at("findings").items().len(), 1);
    // The evidence joined the asset's photo library.
    let (_, register) = client.get("/api/equipment?outlet=TST");
    assert_eq!(items(&register)[0].at("photos").items().len(), 1);
    // The finding becomes a work request, then a work order, which closes it.
    let (_, findings) = client.get("/api/findings");
    let finding = items(&findings)[0].at("id").as_i64().unwrap();
    assert_eq!(client.patch(&format!("/api/findings/{finding}"), obj([("cause", "Loose cable".into()), ("dueDate", "soon".into())])).0, 400);
    assert_eq!(client.patch(&format!("/api/findings/{finding}"), obj([("cause", "Loose cable".into())])).0, 200);
    let (status, request) = client.post("/api/work-requests", obj([("findingIds", Value::Array(vec![Value::Int(finding)])), ("description", "Fix it".into())]));
    assert_eq!(status, 200);
    assert_eq!(client.post("/api/work-requests", obj([("findingIds", Value::Array(vec![Value::Int(finding)])), ("description", "Again".into())])).0, 409);
    let (status, order) = client.post("/api/work-orders", obj([("workRequestId", request.at("id").clone()), ("title", "Repair".into()), ("priority", "High".into())]));
    assert_eq!(status, 200, "{}", order.to_json());
    let order_path = format!("/api/work-orders/{}", order.at("id").py_str());
    // The audit cannot close while signatures and the order are open.
    assert_eq!(client.post("/api/inspection-sessions/close", obj([("id", Value::Int(session))])).0, 409);
    assert_eq!(client.patch(&order_path, obj([("status", "Done".into())])).0, 400);
    assert_eq!(client.patch(&order_path, obj([("status", "Closed".into())])).0, 200);
    assert_eq!(client.patch(&order_path, obj([("status", "Open".into())])).0, 409);
    let (_, findings) = client.get("/api/findings");
    assert_eq!(items(&findings)[0].at("status").as_str(), Some("Closed"));
    // Signatures need images; each role signs once.
    assert_eq!(client.patch(&path, obj([("signatures", obj([("auditedBy", obj([("url", "x".into())]))]))])).0, 400);
    let signature = client.photo();
    let signed = obj([("signatures", obj([("auditedBy", signature.clone()), ("verifiedBy", signature.clone()), ("acknowledgedBy", signature)]))]);
    assert_eq!(client.patch(&path, signed).0, 200);
    let (status, closed) = client.post("/api/inspection-sessions/close", obj([("id", Value::Int(session))]));
    assert_eq!(status, 200);
    assert!(closed.at("closedAt").as_i64().is_some());
    assert_eq!(client.patch(&path, obj([("remarks", "late".into())])).0, 409);
    assert_eq!(client.delete(&path).0, 409);
    // Reports and the activity log see it.
    let (_, report) = client.get("/api/reports");
    assert_eq!(report.at("monthlySummary").at("auditsCompleted").as_i64(), Some(1));
    let (_, activity) = client.get("/api/activity");
    let actions: Vec<String> = items(&activity).iter().map(|row| row.at("action").py_str()).collect();
    for action in ["audit_started", "audit_completed", "request_raised", "order_created", "order_closed", "audit_signed", "audit_closed"] {
        assert!(actions.iter().any(|known| known == action), "{action} in {actions:?}");
    }
}

#[test]
fn concurrent_completion_creates_one_audit() {
    let server = Server::start();
    let mut client = server.admin();
    let (_, session, asset) = audit_setup(&mut client);
    let photo = client.photo();
    let cookie = client.cookie.clone();
    let port = server.port;
    let workers: Vec<_> = (0..6)
        .map(|_| {
            let (cookie, photo) = (cookie.clone(), photo.clone());
            std::thread::spawn(move || {
                let mut client = Client { port, cookie };
                client.patch(&format!("/api/inspection-sessions/{session}"), obj([("items", checks(asset, Some(&photo), true)), ("complete", true.into())])).0
            })
        })
        .collect();
    let statuses: Vec<u16> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
    assert_eq!(statuses.iter().filter(|status| **status == 200).count(), 1, "{statuses:?}");
    let (_, report) = client.get("/api/reports");
    assert_eq!(report.at("monthlySummary").at("auditsCompleted").as_i64(), Some(1));
}

#[test]
fn outlet_limited_accounts_and_held_changes() {
    let server = Server::start();
    let mut admin = server.admin();
    audit_setup(&mut admin);
    assert_eq!(admin.post("/api/roles", obj([("name", "Clerk".into()), ("permissions", Value::Array(vec!["today".into(), "equipment".into()])), ("actions", Value::Array(vec!["assets.manage".into()]))])).0, 200);
    assert_eq!(admin.post("/api/users", obj([("name", "Clara".into()), ("email", "clara@example.com".into()), ("role", "Clerk".into()), ("password", "Clara-pass-1".into()), ("outlets", Value::Array(vec!["TST".into()]))])).0, 200);
    let mut clara = server.client();
    assert_eq!(clara.post("/api/auth/login", obj([("identifier", "clara@example.com".into()), ("password", "Clara-pass-1".into())])).0, 200);
    let (_, register) = clara.get("/api/equipment");
    assert!(items(&register).iter().all(|row| row.at("outlet").as_str() == Some("TST")));
    assert_eq!(clara.get("/api/equipment?outlet=STP").0, 403);
    assert_eq!(clara.get("/api/users").0, 403);
    assert_eq!(clara.post("/api/equipment", obj([("name", "Fan".into()), ("outlet", "STP".into())])).0, 403);
    let (status, held) = clara.post("/api/equipment", obj([("name", "Fan".into()), ("outlet", "TST".into()), ("location", "Lobby".into())]));
    assert_eq!((status, held.at("pending").clone()), (200, Value::Bool(true)));
    let change = held.at("changeId").as_i64().unwrap();
    assert_eq!(clara.patch(&format!("/api/changes/{change}"), obj([("decision", "approve".into())])).0, 403);
    let (_, waiting) = admin.get("/api/changes");
    assert!(items(&waiting).iter().any(|row| row.at("id").as_i64() == Some(change) && row.at("canDecide").as_bool() == Some(true)));
    assert_eq!(admin.patch(&format!("/api/changes/{change}"), obj([("decision", "approve".into())])).0, 200);
    assert_eq!(admin.patch(&format!("/api/changes/{change}"), obj([("decision", "reject".into())])).0, 409);
    let (_, register) = admin.get("/api/equipment?outlet=TST");
    assert!(items(&register).iter().any(|row| row.at("name").as_str() == Some("Fan")));
    let (_, notices) = clara.get("/api/notifications");
    assert!(items(&notices).iter().any(|row| row.at("title").as_str() == Some("Your change was approved")));
}

#[test]
fn super_accounts_live_apart_and_manage_company_databases() {
    let server = Server::start();
    let mut root = server.client();
    let (status, body) = root.post("/api/auth/login", obj([("identifier", "super".into()), ("password", "doas".into())]));
    assert_eq!(status, 200);
    assert_eq!(body.at("user").at("accountScope").as_str(), Some("control"));
    assert!(body.at("user").at("id").as_i64().unwrap() < 0);
    assert_eq!(root.post("/api/auth/change-password", obj([("oldPassword", "doas".into()), ("newPassword", "Super-pass-1".into())])).0, 200);
    let (_, users) = root.get("/api/users");
    assert!(items(&users).iter().all(|row| row.at("role").as_str() != Some("Super")));
    let mut admin = server.admin();
    assert_eq!(admin.post("/api/users", obj([("name", "Fake".into()), ("email", "x@example.com".into()), ("role", "Super".into())])).0, 403);
    assert_eq!(admin.get("/api/account/databases").0, 403);
    assert_eq!(root.post("/api/account/databases", obj([("name", "../escape".into())])).0, 400);
    assert_eq!(root.post("/api/account/databases", obj([("name", "second".into())])).0, 200);
    assert_eq!(root.post("/api/account/databases", obj([("name", "second".into())])).0, 400);
    let (_, list) = root.get("/api/account/databases");
    assert_eq!(list.at("databases").items().len(), 2);
    assert_eq!(root.patch("/api/account/databases", obj([("name", "second".into())])).0, 200);
    // Switching signs every organization session out; the Super session continues.
    assert_eq!(admin.get("/api/todo").0, 401);
    assert_eq!(root.get("/api/todo").0, 200);
    assert_eq!(root.delete("/api/account/databases/second").0, 400);
    assert_eq!(root.patch("/api/account/databases", obj([("name", "ottotree_audit_web".into())])).0, 200);
    assert_eq!(root.delete("/api/account/databases/second").0, 200);
}

#[test]
fn exports_are_real_pdf_and_excel_files() {
    let server = Server::start();
    let mut client = server.admin();
    let (_, session, asset) = audit_setup(&mut client);
    let photo = client.photo();
    assert_eq!(client.patch(&format!("/api/inspection-sessions/{session}"), obj([("items", checks(asset, Some(&photo), false)), ("complete", true.into())])).0, 200);
    for (path, kind) in [
        (format!("/api/inspection-sessions/{session}/export.pdf"), "pdf"),
        (format!("/api/inspection-sessions/{session}/export.pdf?location=Lobby"), "pdf"),
        (format!("/api/inspection-sessions/{session}/export.xlsx"), "xlsx"),
        ("/api/reports/export.pdf".into(), "pdf"),
        ("/api/reports/export.xlsx".into(), "xlsx"),
        ("/api/location-report.pdf?outlet=TST&location=Lobby".into(), "pdf"),
        ("/api/location-report.xlsx?outlet=TST&location=Lobby".into(), "xlsx"),
    ] {
        let reply = client.raw("GET", &path, b"", &[]);
        assert_eq!(reply.status, 200, "{path}: {}", String::from_utf8_lossy(&reply.body));
        assert!(reply.header("Content-Disposition").unwrap().starts_with("attachment; filename="));
        if kind == "pdf" {
            assert!(reply.body.starts_with(b"%PDF-1.4") && reply.body.ends_with(b"%%EOF\n"), "{path}");
        } else {
            let sheets = audit_app::xlsx_read::read_workbook(&reply.body).unwrap();
            assert!(!sheets.is_empty(), "{path}");
        }
    }
    assert_eq!(client.raw("GET", &format!("/api/inspection-sessions/{session}/export.pdf?location=Nowhere"), b"", &[]).status, 400);
    assert_eq!(client.raw("GET", "/api/location-report.pdf?outlet=TST", b"", &[]).status, 400);
}

#[test]
fn reminders_are_addressed_once_a_day_and_stop_when_closed() {
    let server = Server::start();
    let mut client = server.admin();
    let (status, request) = client.post("/api/work-requests", obj([("outlet", "STP".into()), ("location", "R-01".into()), ("itemName", "Door".into()), ("description", "Hinge".into())]));
    assert_eq!(status, 200);
    let (status, order) = client.post("/api/work-orders", obj([("workRequestId", request.at("id").clone()), ("title", "Hinge".into()), ("assignee", "Gavin".into()), ("dueDate", "2020-01-01".into())]));
    assert_eq!(status, 200);
    let mut gavin = server.client();
    assert_eq!(gavin.post("/api/auth/login", obj([("identifier", "gavin".into()), ("password", "123456".into())])).0, 200);
    assert_eq!(gavin.post("/api/auth/change-password", obj([("oldPassword", "123456".into()), ("newPassword", "Gavin-pass-1".into())])).0, 200);
    let overdue = |client: &mut Client| items(&client.get("/api/notifications").1).iter().filter(|row| row.at("title").as_str() == Some("Overdue")).count();
    assert_eq!(overdue(&mut gavin), 1);
    assert_eq!(overdue(&mut gavin), 1);
    assert_eq!(overdue(&mut client), 0);
    let (_, orders) = client.get("/api/work-orders");
    assert_eq!(items(&orders)[0].at("sla_status").as_str(), Some("Overdue"));
    assert_eq!(client.patch(&format!("/api/work-orders/{}", order.at("id").py_str()), obj([("status", "Closed".into())])).0, 200);
    assert_eq!(gavin.patch("/api/notifications/all", Value::object()).0, 200);
    let (_, notices) = gavin.get("/api/notifications");
    assert!(items(&notices).iter().all(|row| row.at("status").as_str() == Some("Read")));
}

#[test]
fn master_data_keeps_audit_history_and_codes_unique() {
    let server = Server::start();
    let mut client = server.admin();
    audit_setup(&mut client);
    let (_, locations) = client.get("/api/locations?outlet=TST");
    let lobby = items(&locations)[0].at("id").as_i64().unwrap();
    // The location is used by scheduled work, so its name is kept.
    assert_eq!(client.patch(&format!("/api/locations/{lobby}"), obj([("name", "Hall".into())])).0, 409);
    assert_eq!(client.delete(&format!("/api/locations/{lobby}")).0, 409);
    assert_eq!(client.post("/api/locations", obj([("outlet", "TST".into()), ("name", "lobby".into())])).0, 409);
    // Bulk add gives every item its own code; a bulk edit refuses a repeated code.
    let (status, added) = client.post("/api/equipment/bulk", obj([("name", "Chair".into()), ("outlets", Value::Array(vec!["STP".into()])), ("locations", "all".into()), ("kind", "fixture".into())]));
    assert_eq!((status, added.at("count").as_i64()), (200, Some(3)));
    let ids: Vec<Value> = added.at("items").items().iter().map(|item| item.at("id").clone()).collect();
    let codes: Vec<String> = added.at("items").items().iter().map(|item| item.at("code").py_str()).collect();
    assert!(codes.iter().all(|code| code.starts_with("VAR-")));
    let repeated = Value::Array(ids.iter().map(|id| obj([("id", id.clone()), ("code", "SAME".into())])).collect());
    assert_eq!(client.patch("/api/equipment/bulk", obj([("ids", Value::Array(ids.clone())), ("items", repeated)])).0, 400);
    assert_eq!(client.patch("/api/equipment/bulk", obj([("ids", Value::Array(ids)), ("fields", obj([("brand", "Ikea".into())]))])).0, 200);
    assert_eq!(client.post("/api/equipment", obj([("name", "Other".into()), ("outlet", "TST".into()), ("serialNumber", "SN-1".into())])).0, 400);
    // Roles cannot report in a loop.
    let (_, roles) = client.get("/api/roles");
    let id_of = |name: &str| items(&roles).iter().find(|role| role.at("name").as_str() == Some(name)).unwrap().at("id").clone();
    let (auditor, management) = (id_of("Auditor"), id_of("Management"));
    let body = |reports: &Value, name: &str| obj([("name", name.into()), ("permissions", Value::array()), ("reportsTo", reports.clone())]);
    assert_eq!(client.patch(&format!("/api/roles/{}", auditor.py_str()), body(&management, "Auditor")).0, 200);
    assert_eq!(client.patch(&format!("/api/roles/{}", management.py_str()), body(&auditor, "Management")).0, 400);
}

#[test]
fn settings_scoring_and_theme_are_validated() {
    let server = Server::start();
    let mut client = server.admin();
    // An administrator may change only the workflow options.
    assert_eq!(client.post("/api/settings", obj([("settings", obj([("system.requirePhotoEveryAsset", false.into()), ("theme.preset", "ocean".into())]))])).0, 200);
    let (_, setup) = client.get("/api/setup");
    assert_eq!(setup.at("settings").at("system.requirePhotoEveryAsset"), &Value::Bool(false));
    assert_eq!(setup.at("settings").at("theme.preset").as_str(), Some("ottotree"));
    let mut root = server.client();
    root.post("/api/auth/login", obj([("identifier", "super".into()), ("password", "doas".into())]));
    root.post("/api/auth/change-password", obj([("oldPassword", "doas".into()), ("newPassword", "Super-pass-1".into())]));
    assert_eq!(root.post("/api/settings", obj([("settings", obj([("theme.preset", "neon".into())]))])).0, 400);
    assert_eq!(root.post("/api/settings", obj([("settings", obj([("theme.accent", "red".into())]))])).0, 400);
    assert_eq!(root.post("/api/settings", obj([("settings", obj([("scoring.goodBand", Value::Int(95))]))])).0, 400);
    assert_eq!(root.post("/api/settings", obj([("settings", obj([("theme.preset", "ocean".into()), ("theme.accent", "#112233".into())]))])).0, 200);
    let (_, branding) = server.client().get("/api/branding");
    assert_eq!(branding.at("theme").at("accent").as_str(), Some("#112233"));
}

#[test]
fn an_empty_dashboard_invents_nothing_and_the_todo_list_guides() {
    let server = Server::start();
    let mut client = server.admin();
    let (status, dashboard) = client.get("/api/dashboard");
    assert_eq!(status, 200);
    assert_eq!(dashboard.at("stats").at("overallAuditScore"), &Value::Null);
    assert_eq!(dashboard.at("stats").at("auditsCompleted").as_i64(), Some(0));
    let (_, session, _) = audit_setup(&mut client);
    let (_, todo) = client.get("/api/todo");
    assert!(items(&todo).iter().any(|item| item.at("id").as_i64() == Some(session) && item.at("action").as_str() == Some("Continue inspection")));
    // The starter Department/PIC role sees no audit drafts.
    assert_eq!(todo.at("counts").at("guided").as_i64(), Some(1));
}

#[test]
fn images_are_validated_and_served_only_with_a_session() {
    let server = Server::start();
    let mut client = server.admin();
    let photo = client.photo();
    let url = photo.at("url").py_str();
    assert_eq!(client.raw("GET", &url, b"", &[]).status, 200);
    assert_eq!(server.client().raw("GET", &url, b"", &[]).status, 401);
    assert_eq!(client.raw("GET", "/api/media/nothing.png", b"", &[]).status, 404);
    let bad = obj([("image", obj([("dataUrl", "data:image/png;base64,AAAA".into())]))]);
    assert_eq!(client.post("/api/media", bad).0, 400);
    let missing = obj([("profilePhoto", obj([("url", Value::str(format!("/api/media/{}.png", "a".repeat(64))))]))]);
    assert_eq!(client.patch("/api/account", missing).0, 400);
}

#[test]
fn the_server_upgrades_and_reopens_its_database() {
    let dir = temp_dir();
    {
        let mut server = Server::start_in(&dir, false, &[]);
        let mut client = server.admin();
        assert_eq!(client.post("/api/setup/departments", obj([("code", "qa".into())])).0, 200);
        server.stop();
    }
    let server = Server::start_in(&dir, true, &[]);
    let mut client = server.client();
    assert_eq!(client.post("/api/auth/login", obj([("identifier", "admin".into()), ("password", NEW_PASSWORD.into())])).0, 200);
    let (_, setup) = client.get("/api/setup");
    assert!(setup.at("departments").items().iter().any(|row| row.at("code").as_str() == Some("QA")));
}
