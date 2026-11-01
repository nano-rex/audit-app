//! Maintenance commands: importing fixed-asset listings, and a local load test.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::clock;
use crate::common::location_qr_code;
use crate::config::{self, DEFAULT_INSPECTION_CRITERIA};
use crate::db::{self, save_value, Db};
use crate::error::{bad, R};
use crate::json::{obj, strings, Value};
use crate::params;
use crate::xlsx_read::{read_workbook, Cell};

const HEADER_ALIASES: [(&str, &str); 20] = [
    ("name", "name"), ("description", "description"), ("asset type", "type"), ("operational status", "operational_status"),
    ("temporary relocation", "temporary_relocation"), ("code", "code"), ("location", "location"), ("model", "model"),
    ("serial no.", "serial_number"), ("serial no", "serial_number"), ("serial number", "serial_number"),
    ("inverter model (if any)", "inverter_model"), ("inverter model", "inverter_model"), ("motor capacity", "motor_capacity"),
    ("brand", "brand"), ("installation date", "installation_date"), ("warranty date", "warranty_date"),
    ("calibration date", "calibration_date"), ("expiry date", "expiry_date"), ("photos", "photos"),
];

fn clean(cell: &Cell) -> String {
    let text = cell.text().trim().to_string();
    if matches!(text.as_str(), "-" | "N/A" | "n/a" | "None") {
        String::new()
    } else {
        text
    }
}

/// The outlet a listing belongs to: the code before the first hyphen of its file name.
fn outlet_from_file(path: &Path) -> String {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let code: String = name.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    let rest = name[code.len()..].trim_start();
    if !code.is_empty() && rest.starts_with('-') {
        code.to_uppercase()
    } else {
        String::new()
    }
}

/// The header row (within the first 20) naming name, code, and asset type, and the column of each field.
fn find_header(rows: &[Vec<Cell>]) -> Option<(usize, Vec<(&'static str, usize)>)> {
    for (index, row) in rows.iter().take(20).enumerate() {
        let labels: Vec<String> = row.iter().map(|cell| clean(cell).to_lowercase()).collect();
        if ["name", "code", "asset type"].iter().all(|label| labels.iter().any(|known| known == label)) {
            let mut mapping: Vec<(&'static str, usize)> = Vec::new();
            for (column, label) in labels.iter().enumerate() {
                if let Some((_, field)) = HEADER_ALIASES.iter().find(|(alias, _)| alias == label) {
                    mapping.retain(|(known, _)| known != field);
                    mapping.push((field, column));
                }
            }
            return Some((index, mapping));
        }
    }
    None
}

fn asset_files(source: &Path) -> Result<Vec<PathBuf>, String> {
    if source.is_file() {
        if !source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx")) {
            return Err(format!("Expected an .xlsx file, got {}", source.display()));
        }
        return Ok(vec![source.to_path_buf()]);
    }
    if source.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(source).map_err(|error| error.to_string())?.filter_map(|entry| entry.ok()).map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "xlsx")).collect();
        files.sort();
        return Ok(files);
    }
    Err(format!("Fixed asset source does not exist: {}", source.display()))
}

fn ensure_location(db: &Db, outlet: &str, location: &str, now: i64) -> R<bool> {
    if outlet.is_empty() || db.exists("SELECT 1 FROM locations WHERE outlet_code = ? AND lower(name) = lower(?)", params![outlet, location])? {
        return Ok(false);
    }
    let order = db.count("SELECT COALESCE(MAX(display_order), 0) + 1 FROM locations WHERE outlet_code = ?", params![outlet])?;
    db.exec(
        "INSERT INTO locations (outlet_code, name, floor, area, display_order, size, qr_code, created_at) VALUES (?, ?, '', '', ?, '', ?, ?)",
        params![outlet, location, order, location_qr_code(outlet, location), now],
    )?;
    let locations = db.strings("SELECT name FROM locations WHERE outlet_code = ? ORDER BY name", params![outlet])?;
    let reference = save_value(db, &strings(&locations))?;
    match db.row("SELECT id FROM zones WHERE outlet_code = ? AND lower(name) = 'zone-1'", params![outlet])? {
        Some(zone) => db.exec("UPDATE zones SET locations_data_id = ? WHERE id = ?", params![reference, zone.int("id")])?,
        None => db.exec(
            "INSERT INTO zones (outlet_code, name, locations_data_id, description, created_at) VALUES (?, 'Zone-1', ?, 'Default zone containing all locations', ?)",
            params![outlet, reference, now],
        )?,
    };
    Ok(true)
}

fn import_photo(db: &Db, workbook: &Path, name: &str) -> R<Value> {
    if name.is_empty() {
        return Ok(Value::array());
    }
    let photo = workbook.parent().unwrap_or(Path::new(".")).join(name);
    let bytes = std::fs::read(&photo).map_err(|_| bad(format!("Photo file must exist beside the workbook: {name}")))?;
    let (identifier, mime) = crate::media::put(db, &bytes)?;
    let file_name = photo.file_name().map(|value| value.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Value::Array(vec![obj([("id", Value::str(&identifier)), ("url", Value::str(format!("/api/media/{identifier}"))), ("name", Value::str(file_name)), ("type", mime.into())])]))
}

/// Import Fixed Asset Listing workbooks: each row with a code and a name becomes (or updates)
/// the asset with that code, and its location is created when missing.
pub fn import_assets(arguments: &[String]) -> Result<(), String> {
    let mut source = std::env::current_dir().map_err(|error| error.to_string())?.join("Fixed_Assets");
    let mut database = None;
    let mut limit = 0usize;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--db" => {
                index += 1;
                database = Some(PathBuf::from(arguments.get(index).ok_or("--db needs a path")?));
            }
            "--limit" => {
                index += 1;
                limit = arguments.get(index).and_then(|value| value.parse().ok()).ok_or("--limit needs a number")?;
            }
            other => source = PathBuf::from(other),
        }
        index += 1;
    }
    let files = asset_files(&source)?;
    if files.is_empty() {
        return Err(format!("No .xlsx files found in {}", source.display()));
    }
    let path = database.unwrap_or_else(config::db_path);
    config::with_db_override(path.clone(), crate::migrate::init_db).map_err(|error| error.to_string())?;
    let database = db::open(&path).map_err(|error| error.to_string())?;
    let (mut total, mut created, mut updated, mut locations) = (0usize, 0usize, 0usize, 0usize);
    let result = (|| -> R<()> {
        for file in &files {
            let outlet = outlet_from_file(file);
            let bytes = std::fs::read(file)?;
            let sheets = read_workbook(&bytes).map_err(bad)?;
            let file_name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            for sheet in sheets {
                if matches!(sheet.name.to_lowercase().as_str(), "example" | "format") {
                    continue;
                }
                let Some((header, mapping)) = find_header(&sheet.rows) else { continue };
                let field = |row: &Vec<Cell>, name: &str| -> String {
                    mapping.iter().find(|(known, _)| *known == name).and_then(|(_, column)| row.get(*column)).map(clean).unwrap_or_default()
                };
                for row in sheet.rows.iter().skip(header + 1) {
                    let code = field(row, "code");
                    let name = field(row, "name");
                    if code.is_empty() || name.is_empty() {
                        continue;
                    }
                    let exists = database.exists("SELECT 1 FROM equipment WHERE code = ?", params![code])?;
                    let asset_type = Some(field(row, "type")).filter(|value| !value.is_empty()).unwrap_or_else(|| "Fixed Asset".into());
                    let status = Some(field(row, "operational_status")).filter(|value| !value.is_empty()).unwrap_or_else(|| "Active".into());
                    let location = Some(field(row, "location")).filter(|value| !value.is_empty()).unwrap_or_else(|| "Unassigned".into());
                    let description = field(row, "description");
                    if ensure_location(&database, &outlet, &location, clock::now_ms())? {
                        locations += 1;
                    }
                    let photos = import_photo(&database, file, &field(row, "photos"))?;
                    database.exec(
                        "INSERT INTO equipment (asset_id, qr_code, business_unit, outlet, zone, equipment_type, health_status, last_checked, replacement_flag, notes, name, description, type, operational_status, code, model, serial_number, brand, location, installation_date, temporary_relocation, warranty_date, calibration_date, expiry_date, photos_data_id, inverter_model, motor_capacity, source_file, source_sheet, inspection_criteria_data_id, created_at)
                         VALUES (?, ?, 'Ottotree', ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%s','now') * 1000)
                         ON CONFLICT(code) DO UPDATE SET asset_id = excluded.code, qr_code = excluded.code, outlet = excluded.outlet, zone = excluded.zone,
                            equipment_type = excluded.equipment_type, health_status = excluded.health_status, last_checked = excluded.last_checked, notes = excluded.notes,
                            name = excluded.name, description = excluded.description, type = excluded.type, operational_status = excluded.operational_status, code = excluded.code,
                            model = excluded.model, serial_number = excluded.serial_number, brand = excluded.brand, location = excluded.location,
                            installation_date = excluded.installation_date, temporary_relocation = excluded.temporary_relocation, warranty_date = excluded.warranty_date,
                            calibration_date = excluded.calibration_date, expiry_date = excluded.expiry_date, photos_data_id = excluded.photos_data_id,
                            inverter_model = excluded.inverter_model, motor_capacity = excluded.motor_capacity, source_file = excluded.source_file, source_sheet = excluded.source_sheet",
                        params![
                            code, code, outlet, location, asset_type, status, field(row, "installation_date"), description,
                            name, description, asset_type, status, code, field(row, "model"), field(row, "serial_number"), field(row, "brand"), location,
                            field(row, "installation_date"), field(row, "temporary_relocation"), field(row, "warranty_date"), field(row, "calibration_date"),
                            field(row, "expiry_date"), save_value(&database, &photos)?, field(row, "inverter_model"), field(row, "motor_capacity"), file_name,
                            sheet.name, save_value(&database, &strings(&DEFAULT_INSPECTION_CRITERIA))?
                        ],
                    )?;
                    total += 1;
                    if exists {
                        updated += 1;
                    } else {
                        created += 1;
                    }
                    if limit > 0 && total >= limit {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    })();
    db::finish(&database, result).map_err(|error| error.to_string())?;
    println!("Synced {total} fixed asset rows: {created} created, {updated} updated, {locations} locations created in {}", path.display());
    Ok(())
}

// ---- Load test ----

fn request(port: u16, method: &str, path: &str, cookie: &str, body: Option<&str>) -> Result<(u16, usize), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|error| error.to_string())?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
    let body = body.unwrap_or("");
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nCookie: {}={cookie}\r\nAccept-Encoding: gzip\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        config::session_cookie(),
        body.len()
    );
    stream.write_all(head.as_bytes()).map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&response[..response.len().min(64)]).into_owned();
    let status = text.split(' ').nth(1).and_then(|code| code.parse().ok()).ok_or("no status line")?;
    let split = response.windows(4).position(|window| window == b"\r\n\r\n").map(|at| at + 4).unwrap_or(response.len());
    Ok((status, response.len() - split))
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() as f64 * fraction).ceil() as usize).saturating_sub(1).min(values.len() - 1);
    (values[index] * 10_000.0).round() / 10.0
}

/// Seed a temporary database, serve it on a free port, and send each of `users` simultaneous
/// people through the start page, dashboard, asset list, and a draft save.
pub fn load_test(arguments: &[String]) -> Result<(), String> {
    let users: usize = match arguments.iter().position(|argument| argument == "--users") {
        Some(index) => arguments.get(index + 1).and_then(|value| value.parse().ok()).ok_or("--users needs a number")?,
        None => 100,
    };
    if !(1..=200).contains(&users) {
        return Err("users must be between 1 and 200".into());
    }
    let storage = std::env::temp_dir().join(format!("audit-load-{}", crate::crypto::token_hex(6)));
    std::fs::create_dir_all(&storage).map_err(|error| error.to_string())?;
    let path = storage.join(config::DATABASE_FILE);
    config::set_db_path(path.clone());
    let outcome = (|| -> Result<String, String> {
        crate::migrate::init_db().map_err(|error| error.to_string())?;
        let database = db::open(&path).map_err(|error| error.to_string())?;
        let user_id = (|| -> R<i64> {
            database.exec("UPDATE users SET reset_required = 0", params![])?;
            let user_id = database.count("SELECT id FROM users WHERE role = 'Admin'", params![])?;
            for number in 0..2500 {
                let code = format!("LOAD-{number}");
                database.exec(
                    "INSERT INTO equipment (asset_id, qr_code, code, name, business_unit, outlet, zone, equipment_type, health_status, last_checked, created_at) VALUES (?, ?, ?, ?, 'Ottotree', 'STP', 'Test', 'AV', 'Operational', '2026-09-14', 0)",
                    params![code, code, code, format!("Test asset {number}")],
                )?;
            }
            let evidence = save_value(&database, &Value::Array(vec![obj([("item", "Test asset".into()), ("passed", true.into())])]))?;
            for number in 0..300 {
                database.exec(
                    "INSERT INTO inspection_sessions(business_unit,outlet,zone,audit_date,auditor,items_data_id,progress,status,created_at,updated_at) VALUES ('Ottotree','STP','Test','2026-09-14','Load test',?,100,'Draft',?,?)",
                    params![evidence, number as i64, number as i64],
                )?;
            }
            let expires = clock::now_secs() + 3600.0;
            for number in 0..users {
                database.exec("INSERT INTO auth_sessions(token_hash, user_id, expires_at) VALUES (?, ?, ?)", params![crate::crypto::sha256_hex(format!("load-{number}").as_bytes()), user_id, expires])?;
            }
            Ok(user_id)
        })();
        let user_id = db::finish(&database, user_id).map_err(|error| error.to_string())?;
        drop(database);
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
        let port = listener.local_addr().map_err(|error| error.to_string())?.port();
        std::thread::spawn(move || crate::http::serve(listener, std::sync::Arc::new(crate::api::handle), 8));
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(users));
        let started = Instant::now();
        let workers: Vec<_> = (0..users)
            .map(|number| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let begin = Instant::now();
                    let mut samples = Vec::new();
                    for path in ["/", "/api/dashboard", "/api/equipment", "/api/inspection-sessions"] {
                        let body = (path == "/api/inspection-sessions").then(|| format!("{{\"outlet\":\"STP\",\"auditor\":\"Load {number}\",\"items\":[{{\"passed\":true}}]}}"));
                        let at = Instant::now();
                        let result = request(port, if body.is_some() { "POST" } else { "GET" }, path, &format!("load-{number}"), body.as_deref());
                        samples.push((path, at.elapsed().as_secs_f64(), result));
                    }
                    (begin.elapsed().as_secs_f64(), samples)
                })
            })
            .collect();
        let results: Vec<(f64, Vec<(&str, f64, Result<(u16, usize), String>)>)> = workers.into_iter().map(|worker| worker.join().unwrap_or((0.0, Vec::new()))).collect();
        let elapsed = started.elapsed().as_secs_f64();
        let database = db::open(&path).map_err(|error| error.to_string())?;
        let saved = database.count("SELECT COUNT(*) FROM inspection_sessions WHERE owner_user_id = ?", params![user_id]).map_err(|error| error.to_string())?;
        let errors: Vec<String> = results
            .iter()
            .flat_map(|(_, samples)| samples.iter())
            .filter_map(|(path, _, result)| match result {
                Ok((200, _)) => None,
                Ok((status, _)) => Some(format!("{path}: {status}")),
                Err(error) => Some(format!("{path}: {error}")),
            })
            .collect();
        let mut endpoints = Value::object();
        for path in ["/", "/api/dashboard", "/api/equipment", "/api/inspection-sessions"] {
            let mut times: Vec<f64> = results.iter().flat_map(|(_, samples)| samples.iter()).filter(|(known, ..)| *known == path).map(|(_, time, _)| *time).collect();
            let sizes: Vec<usize> = results.iter().flat_map(|(_, samples)| samples.iter()).filter(|(known, ..)| *known == path).filter_map(|(_, _, result)| result.as_ref().ok().map(|(_, size)| *size)).collect();
            let mean = if sizes.is_empty() { 0 } else { sizes.iter().sum::<usize>() / sizes.len() };
            endpoints.set(path, obj([("median_ms", Value::Float(percentile(&mut times, 0.5))), ("p95_ms", Value::Float(percentile(&mut times, 0.95))), ("mean_bytes", Value::Int(mean as i64))]));
        }
        let mut flows: Vec<f64> = results.iter().map(|(time, _)| *time).collect();
        let report = obj([
            ("users", Value::Int(users as i64)),
            ("assets_added", Value::Int(2500)),
            ("drafts_seeded", Value::Int(300)),
            ("seconds", Value::Float((elapsed * 100.0).round() / 100.0)),
            ("flow", obj([("median_ms", Value::Float(percentile(&mut flows, 0.5))), ("p95_ms", Value::Float(percentile(&mut flows, 0.95)))])),
            ("requests", Value::Int((users * 4) as i64)),
            ("error_count", Value::Int(errors.len() as i64)),
            ("errors", strings(&errors.iter().take(10).cloned().collect::<Vec<_>>())),
            ("drafts_saved", Value::Int(saved)),
            ("endpoints", endpoints),
        ]);
        if !errors.is_empty() || saved != users as i64 {
            return Err(report.to_json());
        }
        Ok(report.to_json())
    })();
    let _ = std::fs::remove_dir_all(&storage);
    let report = outcome?;
    println!("{report}");
    Ok(())
}
