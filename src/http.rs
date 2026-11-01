//! A small HTTP/1.1 server: one thread per connection, keep-alive, and a bound on how many
//! requests are processed at once (AUDIT_WORKERS), so a burst queues instead of exhausting the
//! database. Request bodies are read only when the handler asks, with the limit it chooses.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const MAX_LINE: usize = 65_536;
const MAX_HEADERS: usize = 100;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// An unread body up to this size is drained so the response can still be delivered.
const DRAIN_LIMIT: usize = 64 * 1024;
const MAX_CONNECTIONS: usize = 512;

pub struct Request<'a> {
    pub method: String,
    /// The request target as sent: path and query, still percent-encoded.
    pub target: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub peer: IpAddr,
    input: &'a mut dyn Read,
    content_length: Option<usize>,
    consumed: bool,
}

pub enum Body {
    Bytes(Vec<u8>),
    TooLarge,
    Invalid(String),
}

impl Request<'_> {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    /// Read the body when it is at most `limit` bytes.
    pub fn read_body(&mut self, limit: usize) -> Body {
        if self.consumed {
            return Body::Bytes(Vec::new());
        }
        if self.header("Transfer-Encoding").is_some() {
            return Body::Invalid("Transfer-Encoding is not supported".into());
        }
        let length = match self.header("Content-Length") {
            None => 0,
            Some(value) => match value.trim().parse::<i64>() {
                Ok(length) if length < 0 => return Body::Invalid("Invalid Content-Length".into()),
                Ok(length) => length as usize,
                Err(_) => return Body::Invalid(format!("invalid literal for int() with base 10: '{}'", value.trim())),
            },
        };
        if length > limit {
            return Body::TooLarge;
        }
        let mut bytes = vec![0u8; length];
        if self.input.read_exact(&mut bytes).is_err() {
            return Body::Invalid("The request body ended early".into());
        }
        self.consumed = true;
        Body::Bytes(bytes)
    }

    /// Whether the body was left unread and is too large to drain.
    fn finish(&mut self) -> bool {
        if self.consumed {
            return true;
        }
        match self.content_length {
            None | Some(0) => true,
            Some(length) if length <= DRAIN_LIMIT => {
                let mut sink = vec![0u8; length];
                let ok = self.input.read_exact(&mut sink).is_ok();
                self.consumed = true;
                ok
            }
            Some(_) => false,
        }
    }

    /// The query's parameters, decoded as urllib.parse.parse_qs does: blank values are left out.
    pub fn params(&self) -> Vec<(String, String)> {
        parse_query(&self.query)
    }

    /// The first value of a query parameter.
    pub fn param(&self, name: &str) -> Option<String> {
        self.params().into_iter().find(|(key, _)| key == name).map(|(_, value)| value)
    }

    /// Every value of a query parameter.
    pub fn param_all(&self, name: &str) -> Vec<String> {
        self.params().into_iter().filter(|(key, _)| key == name).map(|(_, value)| value).collect()
    }
}

pub fn parse_query(query: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in query.split(['&', ';']) {
        if part.is_empty() {
            continue;
        }
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        if value.is_empty() {
            continue;
        }
        out.push((percent_decode(&key.replace('+', " ")), percent_decode(&value.replace('+', " "))));
    }
    out
}

/// urllib.parse.unquote: %XX sequences decoded as UTF-8 (invalid ones replaced).
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let digit = |byte: u8| (byte as char).to_digit(16);
            if let (Some(high), Some(low)) = (digit(bytes[index + 1]), digit(bytes[index + 2])) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Close the connection after this response.
    pub close: bool,
}

impl Response {
    pub fn new(status: u16, content_type: &str, body: Vec<u8>) -> Response {
        Response { status, headers: vec![("Content-Type".into(), content_type.into())], body, close: false }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Response {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    /// BaseHTTPRequestHandler.send_error: a short HTML page, and the connection is closed.
    pub fn error(status: u16) -> Response {
        let (reason, explanation) = (reason(status), explanation(status));
        let body = format!(
            "<!DOCTYPE HTML>\n<html lang=\"en\">\n    <head>\n        <meta charset=\"utf-8\">\n        <title>Error response</title>\n    </head>\n    <body>\n        <h1>Error response</h1>\n        <p>Error code: {status}</p>\n        <p>Message: {reason}.</p>\n        <p>Error code explanation: {status} - {explanation}.</p>\n    </body>\n</html>\n"
        );
        Response { status, headers: vec![("Content-Type".into(), "text/html;charset=utf-8".into())], body: body.into_bytes(), close: true }
    }
}

pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        413 => "Request Entity Too Large",
        414 => "Request-URI Too Long",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "Unknown",
    }
}

fn explanation(status: u16) -> &'static str {
    match status {
        400 => "Bad request syntax or unsupported method",
        401 => "No permission -- see authorization schemes",
        403 => "Request forbidden -- authorization will not help",
        404 => "Nothing matches the given URI",
        405 => "Specified method is invalid for this resource",
        413 => "Entity is too large",
        414 => "URI is too long",
        431 => "The server refused this request because the request header fields are too large",
        500 => "Server got itself in trouble",
        501 => "Server does not support this operation",
        503 => "The server cannot process the request due to a high load",
        _ => "",
    }
}

/// HTTP-date of the current time (RFC 9110), for the Date header.
fn http_date() -> String {
    let now = crate::clock::utc_now();
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][now.date.days().rem_euclid(7) as usize];
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][now.date.month as usize - 1];
    format!("{weekday}, {:02} {month} {} {:02}:{:02}:{:02} GMT", now.date.day, now.date.year, now.hour, now.minute, now.second)
}

/// Counts requests in progress; `acquire` waits while the limit is reached.
pub struct Slots {
    used: Mutex<usize>,
    freed: Condvar,
    limit: usize,
}

impl Slots {
    pub fn new(limit: usize) -> Slots {
        Slots { used: Mutex::new(0), freed: Condvar::new(), limit: limit.max(1) }
    }

    fn acquire(&self) {
        let mut used = self.used.lock().unwrap_or_else(|poison| poison.into_inner());
        while *used >= self.limit {
            used = self.freed.wait(used).unwrap_or_else(|poison| poison.into_inner());
        }
        *used += 1;
    }

    fn release(&self) {
        let mut used = self.used.lock().unwrap_or_else(|poison| poison.into_inner());
        *used -= 1;
        self.freed.notify_one();
    }
}

pub type Handler = dyn Fn(&mut Request) -> Response + Send + Sync;

fn read_line(reader: &mut BufReader<TcpStream>) -> std::io::Result<Option<String>> {
    let mut line = Vec::new();
    let read = reader.by_ref().take(MAX_LINE as u64 + 1).read_until(b'\n', &mut line)?;
    if read == 0 {
        return Ok(None);
    }
    if line.len() > MAX_LINE {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "line too long"));
    }
    while line.last().is_some_and(|byte| *byte == b'\n' || *byte == b'\r') {
        line.pop();
    }
    Ok(Some(String::from_utf8_lossy(&line).into_owned()))
}

fn write_response(stream: &mut TcpStream, response: &Response, head_only: bool, keep_alive: bool) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {} {}\r\nServer: audit-app\r\nDate: {}\r\n", response.status, reason(response.status), http_date());
    for (name, value) in &response.headers {
        // Header values never carry line breaks, whatever a handler was given.
        head.push_str(&format!("{name}: {}\r\n", value.replace(['\r', '\n'], " ")));
    }
    head.push_str("X-Content-Type-Options: nosniff\r\nX-Frame-Options: SAMEORIGIN\r\nReferrer-Policy: same-origin\r\n");
    if !response.headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("Content-Length")) {
        head.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
    }
    head.push_str(if keep_alive { "Connection: keep-alive\r\n\r\n" } else { "Connection: close\r\n\r\n" });
    let mut out = head.into_bytes();
    if !head_only && response.status != 304 {
        out.extend_from_slice(&response.body);
    }
    stream.write_all(&out)?;
    stream.flush()
}

fn serve_connection(stream: TcpStream, peer: SocketAddr, handler: &Handler, slots: &Slots) {
    let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IDLE_TIMEOUT));
    let _ = stream.set_nodelay(true);
    let Ok(mut writer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    loop {
        let line = match read_line(&mut reader) {
            Ok(Some(line)) if line.is_empty() => continue,
            Ok(Some(line)) => line,
            Ok(None) => return,
            Err(error) => {
                if error.kind() == std::io::ErrorKind::InvalidData {
                    let _ = write_response(&mut writer, &Response::error(414), false, false);
                }
                return;
            }
        };
        let parts: Vec<&str> = line.split(' ').collect();
        if parts.len() != 3 || !parts[2].starts_with("HTTP/") {
            let _ = write_response(&mut writer, &Response::error(400), false, false);
            return;
        }
        let (method, target, version) = (parts[0].to_string(), parts[1].to_string(), parts[2]);
        if version != "HTTP/1.1" && version != "HTTP/1.0" {
            let _ = write_response(&mut writer, &Response::error(505), false, false);
            return;
        }
        let mut headers = Vec::new();
        loop {
            match read_line(&mut reader) {
                Ok(Some(header)) if header.is_empty() => break,
                Ok(Some(header)) => {
                    if headers.len() >= MAX_HEADERS {
                        let _ = write_response(&mut writer, &Response::error(431), false, false);
                        return;
                    }
                    if let Some((name, value)) = header.split_once(':') {
                        headers.push((name.trim().to_string(), value.trim().to_string()));
                    }
                }
                _ => return,
            }
        }
        let connection = headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("Connection")).map(|(_, value)| value.to_ascii_lowercase());
        let mut keep_alive = if version == "HTTP/1.1" { connection.as_deref() != Some("close") } else { connection.as_deref() == Some("keep-alive") };
        let (path, query) = {
            // Absolute-form targets ("http://host/path") are reduced to their path.
            let local = match target.find("://") {
                Some(index) => target[index + 3..].find('/').map(|slash| &target[index + 3 + slash..]).unwrap_or("/"),
                None => target.as_str(),
            };
            let local = local.split('#').next().unwrap_or("");
            match local.split_once('?') {
                Some((path, query)) => (path.to_string(), query.to_string()),
                None => (local.to_string(), String::new()),
            }
        };
        let content_length = headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("Content-Length")).and_then(|(_, value)| value.trim().parse::<usize>().ok());
        let head_only = method == "HEAD";
        let mut request = Request {
            method: if head_only { "GET".into() } else { method },
            target: target.clone(),
            path,
            query,
            headers,
            peer: peer.ip(),
            input: &mut reader,
            content_length,
            consumed: false,
        };
        slots.acquire();
        let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&mut request)));
        slots.release();
        let response = response.unwrap_or_else(|_| {
            eprintln!("ERROR: the handler for {} {} panicked", request.method, request.target);
            let mut response = Response::new(500, "application/json", br#"{"error":"The server could not complete this request"}"#.to_vec());
            response.close = true;
            response
        });
        if !request.finish() || response.close || request.header("Transfer-Encoding").is_some() {
            keep_alive = false;
        }
        if write_response(&mut writer, &response, head_only, keep_alive).is_err() || !keep_alive {
            return;
        }
    }
}

/// Accept connections until the process ends.
pub fn serve(listener: TcpListener, handler: Arc<Handler>, workers: usize) {
    let slots = Arc::new(Slots::new(workers));
    let open = Arc::new((Mutex::new(0usize), Condvar::new()));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let Ok(peer) = stream.peer_addr() else { continue };
        {
            let (count, freed) = &*open;
            let mut count = count.lock().unwrap_or_else(|poison| poison.into_inner());
            while *count >= MAX_CONNECTIONS {
                count = freed.wait(count).unwrap_or_else(|poison| poison.into_inner());
            }
            *count += 1;
        }
        let (task_handler, task_slots, task_open) = (handler.clone(), slots.clone(), open.clone());
        let spawned = std::thread::Builder::new().name("http".into()).spawn(move || {
            serve_connection(stream, peer, &*task_handler, &task_slots);
            let (count, freed) = &*task_open;
            *count.lock().unwrap_or_else(|poison| poison.into_inner()) -= 1;
            freed.notify_one();
        });
        if spawned.is_err() {
            let (count, _) = &*open;
            *count.lock().unwrap_or_else(|poison| poison.into_inner()) -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_decoding() {
        assert_eq!(parse_query("a=1&b=&c=x+y%2Fz&a=2"), vec![("a".into(), "1".into()), ("c".into(), "x y/z".into()), ("a".into(), "2".into())]);
        assert_eq!(percent_decode("caf%C3%A9%2"), "café%2");
    }
}
