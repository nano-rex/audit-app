//! Audit App: a facilities audit application served to the browser by its own HTTP server.
//!
//! Everything is the Rust standard library and the code in this crate; SQLite is the system's
//! libsqlite3 (src/sqlite.rs). The binary (src/main.rs) serves the application or runs a tool.

pub mod accounts;
pub mod activity;
pub mod api;
pub mod audits;
pub mod catalog;
pub mod changes;
pub mod clock;
pub mod common;
pub mod config;
pub mod control;
pub mod crypto;
pub mod databases;
pub mod db;
pub mod error;
pub mod exports;
pub mod figures;
pub mod flate;
pub mod http;
pub mod inspections;
pub mod json;
pub mod locations;
pub mod media;
pub mod migrate;
pub mod notify;
pub mod pdf;
pub mod pdf_report;
pub mod permissions;
pub mod png;
pub mod reports;
pub mod routes;
pub mod sqlite;
pub mod todo;
pub mod tools;
pub mod webp;
pub mod webp_tables;
pub mod work;
pub mod xlsx;
pub mod xlsx_read;
pub mod xlsx_report;
pub mod zip;


use std::net::TcpListener;
use std::sync::Arc;

const USAGE: &str = "Audit App: a facilities audit application.

Usage:
  audit-app                                  Serve the application on 127.0.0.1:$PORT (41883)
  audit-app import-assets [SOURCE] [--db PATH] [--limit N]
                                             Import Fixed Asset Listing workbooks (.xlsx file or folder)
  audit-app load-test [--users N]            Run a local load test on a temporary database
  audit-app --version | --help

Settings are environment variables: AUDIT_DATA_DIR, AUDIT_WORKERS, AUDIT_SECURE_COOKIES,
AUDIT_TRUST_PROXY, AUDIT_CONTROL_DB, AUDIT_SESSION_COOKIE, AUDIT_WEB_ROOT, and PORT.";

pub fn main_with(arguments: Vec<String>) {
    match arguments.first().map(String::as_str) {
        Some("--help" | "-h" | "help") => {
            println!("{USAGE}");
            return;
        }
        Some("--version" | "-V") => {
            println!("audit-app {} (SQLite {})", env!("CARGO_PKG_VERSION"), sqlite::version());
            return;
        }
        _ => {}
    }
    if let Err(message) = config::init() {
        eprintln!("{message}");
        std::process::exit(2);
    }
    let outcome = match arguments.first().map(String::as_str) {
        None => serve(),
        Some("import-assets") => {
            databases::restore_active_database();
            tools::import_assets(&arguments[1..])
        }
        Some("load-test") => tools::load_test(&arguments[1..]),
        Some(other) => Err(format!("Unknown command {other}\n\n{USAGE}")),
    };
    if let Err(message) = outcome {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn serve() -> Result<(), String> {
    databases::restore_active_database();
    migrate::init_db().map_err(|error| format!("Cannot open the database {}: {error}", config::db_path().display()))?;
    api::note_startup();
    let port = std::env::var("PORT").unwrap_or_else(|_| config::DEFAULT_PORT.into());
    let port: u16 = port.trim().parse().map_err(|_| "PORT must be a number from 1 to 65535".to_string())?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|error| format!("Cannot listen on 127.0.0.1:{port}: {error}"))?;
    let workers = std::env::var("AUDIT_WORKERS").ok().and_then(|value| value.trim().parse::<usize>().ok()).unwrap_or(8).max(1);
    println!("Serving Audit App at http://127.0.0.1:{port}");
    println!("SQLite database: {} (SQLite {})", config::db_path().display(), sqlite::version());
    std::thread::Builder::new().name("reminders".into()).spawn(notify::reminder_loop).map_err(|error| error.to_string())?;
    http::serve(listener, Arc::new(api::handle), workers);
    Ok(())
}
