//! Who did what, and how long it took: the activity behind the KPI section of Reports.
//!
//! Each step of an audit, work request, and work order is logged with the person, the time, and,
//! where a step answers an earlier one, the time it took to act (duration_ms).

use std::collections::HashMap;

use crate::clock::{self, Date};
use crate::db::{save_value, Db};
use crate::error::R;
use crate::json::{obj, Value};
use crate::params;
use crate::sqlite::ToSql;

pub const ACTIONS: [(&str, &str); 12] = [
    ("audit_assigned", "Assigned scheduled audit"),
    ("audit_started", "Started audit"),
    ("audit_completed", "Completed audit"),
    ("audit_signed", "Signed audit"),
    ("audit_closed", "Closed audit"),
    ("request_raised", "Raised work request"),
    ("request_declined", "Declined work request"),
    ("order_created", "Created work order"),
    ("order_status", "Changed work order status"),
    ("order_closed", "Closed work order"),
    ("finding_edited", "Edited finding"),
    ("finding_deleted", "Removed finding"),
];

/// One step to record.
pub struct Entry<'a> {
    pub user_id: Value,
    pub user_name: Value,
    pub action: &'a str,
    pub record_type: &'a str,
    pub record_id: i64,
    pub record_ref: String,
    pub outlet: String,
    pub started_at: Option<i64>,
    pub detail: String,
    pub business_unit: String,
    pub at: Option<i64>,
}

impl<'a> Entry<'a> {
    pub fn new(user: &Value, action: &'a str, record_type: &'a str, record_id: i64) -> Entry<'a> {
        Entry {
            user_id: user.at("id").clone(),
            user_name: user.at("name").clone(),
            action,
            record_type,
            record_id,
            record_ref: String::new(),
            outlet: String::new(),
            started_at: None,
            detail: String::new(),
            business_unit: "Ottotree".into(),
            at: None,
        }
    }

    pub fn reference(mut self, value: impl Into<String>) -> Self {
        self.record_ref = value.into();
        self
    }

    pub fn outlet(mut self, value: impl Into<String>) -> Self {
        self.outlet = value.into();
        self
    }

    pub fn started(mut self, value: Option<i64>) -> Self {
        self.started_at = value;
        self
    }

    pub fn detail(mut self, value: impl Into<String>) -> Self {
        self.detail = value.into();
        self
    }

    pub fn unit(mut self, value: impl Into<String>) -> Self {
        let value = value.into();
        if !value.is_empty() {
            self.business_unit = value;
        }
        self
    }

    pub fn at(mut self, value: i64) -> Self {
        self.at = Some(value);
        self
    }

    /// Record the step. started_at (ms) is when the thing being answered began; the gap is its time to act.
    pub fn log(self, db: &Db) -> R<()> {
        let at = self.at.filter(|at| *at != 0).unwrap_or_else(clock::now_ms);
        let duration = self.started_at.filter(|started| *started != 0).map(|started| at - started).filter(|gap| *gap >= 0);
        let name = if self.user_name.truthy() { self.user_name.py_str() } else { "Unknown".into() };
        db.exec(
            "INSERT INTO activity_log (created_at, user_id, user_name, action, record_type, record_id, record_ref, outlet, business_unit, duration_ms, detail) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![at, self.user_id, name, self.action, self.record_type, self.record_id, self.record_ref, self.outlet, self.business_unit, duration, self.detail],
        )?;
        Ok(())
    }
}

pub fn last_time(db: &Db, action: &str, record_type: &str, record_id: i64) -> R<Option<i64>> {
    Ok(db.scalar("SELECT MAX(created_at) FROM activity_log WHERE action = ? AND record_type = ? AND record_id = ?", params![action, record_type, record_id])?.as_i64())
}

/// The record's reference, outlet, and business unit on a backfilled entry.
fn place<'a>(entry: Entry<'a>, row: &crate::sqlite::Row, reference: &str) -> Entry<'a> {
    entry.reference(row.str(reference)).outlet(row.str("outlet")).unit(row.str("business_unit"))
}

/// Once per database: rebuild what can be known of earlier activity from the records themselves.
pub fn backfill(db: &Db) -> R<()> {
    if db.exists("SELECT 1 FROM app_settings WHERE key = 'system.activityBackfilled'", params![])? {
        return Ok(());
    }
    let names: HashMap<i64, Value> = db.rows("SELECT id, name FROM users", params![])?.iter().map(|row| (row.int("id"), row.value("name"))).collect();
    let ms = |day: &str| -> Option<i64> { Date::parse_iso(day.get(..10).unwrap_or(day)).map(|date| clock::local_midnight(date) * 1000) };
    let person = |id: Value, name: Value| obj([("id", id), ("name", name)]);
    for row in db.rows("SELECT * FROM inspection_sessions", params![])? {
        let owner = row.value("owner_user_id");
        let name = owner.as_i64().and_then(|id| names.get(&id).cloned()).filter(Value::truthy).unwrap_or_else(|| row.value("auditor"));
        let owner = person(owner, name);
        let common = |entry| place(entry, &row, "audit_ref");
        common(Entry::new(&owner, "audit_started", "inspection", row.int("id")).at(row.int("created_at"))).log(db)?;
        if row.str("status") == "Completed" {
            common(Entry::new(&owner, "audit_completed", "inspection", row.int("id")).started(row.opt_int("created_at")).at(row.int("updated_at"))).log(db)?;
        }
        if row.get("closed_at").truthy() {
            let started = if row.int("updated_at") <= row.int("closed_at") { row.opt_int("updated_at") } else { None };
            common(Entry::new(&person(Value::Null, row.value("closed_by")), "audit_closed", "inspection", row.int("id")).started(started).at(row.int("closed_at"))).log(db)?;
        }
    }
    for row in db.rows("SELECT * FROM work_requests", params![])? {
        let common = |entry| place(entry, &row, "request_ref");
        common(Entry::new(&person(row.value("requested_by_user_id"), row.value("requested_by")), "request_raised", "work_request", row.int("id")).at(row.int("created_at"))).log(db)?;
        if row.str("status") == "Declined" {
            common(Entry::new(&person(Value::Null, row.value("reviewed_by")), "request_declined", "work_request", row.int("id"))
                .started(row.opt_int("created_at"))
                .at(row.int("updated_at"))
                .detail(row.str("decline_remark")))
            .log(db)?;
        }
    }
    let requests: HashMap<i64, (i64, String)> =
        db.rows("SELECT id, created_at, request_ref FROM work_requests", params![])?.iter().map(|row| (row.int("id"), (row.int("created_at"), row.str("request_ref")))).collect();
    for row in db.rows("SELECT * FROM work_orders", params![])? {
        let id = row.int("id");
        let common = |entry| place(entry, &row, "work_order_ref");
        let request = row.opt_int("work_request_id").and_then(|key| requests.get(&key));
        common(Entry::new(&person(Value::Null, Value::Null), "order_created", "work_order", id)
            .started(request.map(|(created, _)| *created))
            .at(row.int("created_at"))
            .detail(request.map(|(_, reference)| reference.clone()).unwrap_or_default()))
        .log(db)?;
        // Status changes are kept as system comments with their author and time.
        for change in db.rows(
            "SELECT comment, author, created_at FROM comments WHERE record_type = 'work_order' AND record_id = ? AND system_generated = 1 AND comment LIKE 'Status changed from %' ORDER BY created_at",
            params![id],
        )? {
            let comment = change.str("comment");
            let closed = comment.trim_end().ends_with(" to Closed");
            common(Entry::new(&person(Value::Null, change.value("author")), if closed { "order_closed" } else { "order_status" }, "work_order", id)
                .started(if closed { row.opt_int("created_at") } else { None })
                .at(change.int("created_at"))
                .detail(comment))
            .log(db)?;
        }
        if row.str("status") == "Closed" && !db.exists("SELECT 1 FROM activity_log WHERE action = 'order_closed' AND record_id = ?", params![id])? {
            if let Some(closed_at) = Some(row.str("closed_at")).filter(|text| !text.is_empty()).and_then(|text| ms(&text)) {
                let who = if row.get("pic").truthy() { row.value("pic") } else { row.value("assignee") };
                common(Entry::new(&person(Value::Null, who), "order_closed", "work_order", id).started(row.opt_int("created_at")).at(closed_at)).log(db)?;
            }
        }
    }
    db.exec("INSERT INTO app_settings (key, value_data_id) VALUES ('system.activityBackfilled', ?)", params![save_value(db, &Value::Bool(true))?])?;
    Ok(())
}

/// Time to act: which step answers which, in the reports' words.
pub const TIMINGS: [(&str, &str, &[&str]); 4] = [
    ("audit", "Start to completion of an audit", &["audit_completed"]),
    ("request", "Work request raised to acted on (order created or declined)", &["order_created", "request_declined"]),
    ("order", "Work order opened to closed", &["order_closed"]),
    ("closure", "Audit completed to closed", &["audit_closed"]),
];

pub const COUNTED: [&str; 8] = ["audit_started", "audit_completed", "audit_signed", "audit_closed", "request_raised", "request_declined", "order_created", "order_closed"];

fn round_half_even(value: f64) -> i64 {
    let rounded = value.round();
    if (value - value.trunc()).abs() == 0.5 {
        let floor = value.floor();
        if floor as i64 % 2 == 0 {
            floor as i64
        } else {
            floor as i64 + 1
        }
    } else {
        rounded as i64
    }
}

/// Python's round(): halves go to the even neighbour.
pub fn py_round(value: f64) -> i64 {
    round_half_even(value)
}

/// Per-person counts and average time to act, overall averages, and the latest activity.
pub fn kpi_report(db: &Db, where_clause: &str, params: &[Value], limit: usize) -> R<Value> {
    let bound: Vec<&dyn ToSql> = params.iter().map(|value| value as &dyn ToSql).collect();
    let rows: Vec<Value> = db.rows(&format!("SELECT * FROM activity_log WHERE {where_clause} ORDER BY created_at DESC, id DESC"), &bound)?.iter().map(|row| row.to_object()).collect();
    let average = |values: &[i64]| -> Value {
        if values.is_empty() {
            Value::Null
        } else {
            Value::Int(py_round(values.iter().sum::<i64>() as f64 / values.len() as f64 / 1000.0))
        }
    };
    let mut people: Vec<(String, Value, HashMap<&str, Vec<i64>>)> = Vec::new();
    for row in &rows {
        let name = if row.at("user_name").truthy() { row.at("user_name").py_str() } else { "Unknown".into() };
        let index = match people.iter().position(|(key, ..)| *key == name) {
            Some(index) => index,
            None => {
                let mut person = obj([("name", Value::str(&name))]);
                for action in COUNTED {
                    person.set(action, Value::Int(0));
                }
                people.push((name.clone(), person, HashMap::new()));
                people.len() - 1
            }
        };
        let action = row.at("action").py_str();
        let (_, person, times) = &mut people[index];
        if COUNTED.contains(&action.as_str()) {
            let count = person.at(&action).as_i64().unwrap_or(0);
            person.set(&action, Value::Int(count + 1));
        }
        for (key, _, actions) in TIMINGS {
            if actions.contains(&action.as_str()) {
                if let Some(duration) = row.at("duration_ms").as_i64() {
                    times.entry(key).or_default().push(duration);
                }
            }
        }
    }
    let mut ranked: Vec<Value> = people
        .into_iter()
        .map(|(_, mut person, times)| {
            for (key, _, _) in TIMINGS {
                person.set(&format!("{key}Seconds"), average(times.get(key).map(Vec::as_slice).unwrap_or(&[])));
            }
            person
        })
        .collect();
    let total = |person: &Value| -> i64 { COUNTED.iter().map(|action| person.at(action).as_i64().unwrap_or(0)).sum() };
    ranked.sort_by(|a, b| total(b).cmp(&total(a)).then_with(|| a.at("name").py_str().cmp(&b.at("name").py_str())));
    let mut overall = Vec::new();
    for (key, label, actions) in TIMINGS {
        let values: Vec<i64> = rows.iter().filter(|row| actions.contains(&row.at("action").py_str().as_str())).filter_map(|row| row.at("duration_ms").as_i64()).collect();
        overall.push(obj([
            ("key", key.into()),
            ("label", label.into()),
            ("count", Value::Int(values.len() as i64)),
            ("averageSeconds", average(&values)),
            ("longestSeconds", values.iter().max().map(|longest| Value::Int(py_round(*longest as f64 / 1000.0))).unwrap_or(Value::Null)),
        ]));
    }
    let activity: Vec<Value> = rows
        .iter()
        .take(limit)
        .map(|row| {
            let mut entry = row.clone();
            let action = row.at("action").py_str();
            let label = ACTIONS.iter().find(|(name, _)| *name == action).map(|(_, label)| label.to_string()).unwrap_or(action);
            entry.set("label", label);
            entry
        })
        .collect();
    Ok(obj([
        ("people", Value::Array(ranked)),
        ("timeToAct", Value::Array(overall)),
        ("activity", Value::Array(activity)),
        ("activityTotal", Value::Int(rows.len() as i64)),
    ]))
}

/// 3 d 4 h, 2 h 15 m, 12 m, or under a minute.
pub fn duration_text(seconds: &Value) -> String {
    let Some(seconds) = seconds.as_f64() else { return "—".into() };
    if seconds.is_nan() {
        return "—".into();
    }
    let minutes = (seconds.trunc() as i64).div_euclid(60);
    let (hours, minutes) = (minutes.div_euclid(60), minutes.rem_euclid(60));
    let (days, hours) = (hours.div_euclid(24), hours.rem_euclid(24));
    if days != 0 {
        format!("{days} d {hours} h")
    } else if hours != 0 {
        format!("{hours} h {minutes} m")
    } else if minutes != 0 {
        format!("{minutes} m")
    } else {
        "under a minute".into()
    }
}
