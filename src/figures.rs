//! The Reports page's figures, prepared once for its PDF and Excel exports so both show what the
//! page shows (web/js/report-charts.js), in the same order and colours; and an audit's assets
//! counted by their register attributes.

use crate::activity::{duration_text, py_round};
use crate::clock::{self, Date};
use crate::json::Value;

pub const PASS: &str = "1C8F4B";
pub const FAIL: &str = "C9303A";
pub const WARN: &str = "B96100";
pub const INFO: &str = "2B6FDB";
pub const MUTED: &str = "B4BDB8";
pub const BANDS: [(&str, &str); 4] = [("Excellent", "24C362"), ("Good", INFO), ("Below Expectation", WARN), ("Critical", FAIL)];

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

fn int(value: &Value) -> i64 {
    value.as_f64().map(|number| number as i64).unwrap_or(0)
}

pub fn score_colour(score: f64, scoring: &Value) -> &'static str {
    if score >= scoring.get("goodBand").and_then(Value::as_f64).unwrap_or(70.0) {
        PASS
    } else if score >= scoring.get("belowBand").and_then(Value::as_f64).unwrap_or(60.0) {
        WARN
    } else {
        FAIL
    }
}

/// One of the four rings: title, note, centre figure and caption, parts (label, value, colour), footer.
pub struct Ring {
    pub title: &'static str,
    pub note: String,
    pub centre: String,
    pub caption: &'static str,
    pub parts: Vec<(&'static str, i64, &'static str)>,
    pub footer: String,
}

pub fn overview(data: &Value) -> Vec<Ring> {
    let summary = data.at("monthlySummary");
    let kpi = data.at("kpi");
    let get = |source: &Value, key: &str| int(source.at(key));
    let closed = get(summary, "closedWorkOrders");
    let open = get(summary, "openWorkOrders");
    let other = (get(kpi, "assigned") - get(kpi, "completed") - get(kpi, "pending")).max(0);
    let mut tasks = vec![("Completed", get(kpi, "completed"), PASS), ("Pending", get(kpi, "pending"), WARN)];
    if other != 0 {
        tasks.push(("In progress", other, INFO));
    }
    vec![
        Ring {
            title: "Audits",
            note: format!("{} in this report", get(summary, "audits")),
            centre: py_round(number(summary.at("averageScore"))).to_string(),
            caption: "avg score",
            parts: vec![("Completed", get(summary, "auditsCompleted"), PASS), ("Pending", get(summary, "auditsPending"), WARN)],
            footer: String::new(),
        },
        Ring {
            title: "Findings",
            note: format!("{} recorded", get(summary, "totalFindings")),
            centre: get(summary, "totalFindings").to_string(),
            caption: "findings",
            parts: vec![("Priority", get(summary, "priorityFindings"), FAIL), ("Non-priority", get(summary, "nonPriorityFindings"), INFO)],
            footer: format!("Open {} · Overdue {}", get(summary, "outstandingFindings"), get(summary, "overdueFindings")),
        },
        Ring {
            title: "Work orders",
            note: format!("{} raised", open + closed),
            centre: format!("{}%", get(summary, "completionRate")),
            caption: "completed",
            parts: vec![("Closed", closed, PASS), ("Open", open, WARN)],
            footer: String::new(),
        },
        Ring {
            title: "Scheduled tasks",
            note: format!("{} assigned", get(kpi, "assigned")),
            centre: format!("{}%", get(kpi, "responseRate")),
            caption: "response",
            parts: tasks,
            footer: String::new(),
        },
    ]
}

/// (month, audits, average score), oldest first.
pub fn monthly_audits(charts: &Value) -> Vec<(String, i64, i64)> {
    let mut rows: Vec<(String, i64, i64)> =
        charts.at("monthlyAuditTrend").items().iter().map(|row| (row.at("month").text(), int(row.at("audits")), py_round(number(row.at("average_score"))))).collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

/// (month, priority, non-priority), oldest first, from labels such as "2026-10 · Priority".
pub fn monthly_findings(charts: &Value) -> Vec<(String, i64, i64)> {
    let mut months: std::collections::BTreeMap<String, (i64, i64)> = std::collections::BTreeMap::new();
    for row in charts.at("priorityTrend").items() {
        let label = row.at("label").text();
        let (month, kind) = label.split_once(" · ").unwrap_or((&label, ""));
        let entry = months.entry(month.to_string()).or_default();
        if kind.is_empty() || kind == "Priority" {
            entry.0 += int(row.at("count"));
        } else {
            entry.1 += int(row.at("count"));
        }
    }
    months.into_iter().map(|(month, (priority, other))| (month, priority, other)).collect()
}

/// (outlet, latest, average, audits, last audit date), as ranked.
pub fn outlet_scores(data: &Value) -> Vec<(String, i64, i64, i64, String)> {
    data.at("rankings")
        .items()
        .iter()
        .map(|row| (row.at("outlet").text(), int(row.at("latest")), py_round(number(row.at("average"))), int(row.at("audit_count")), row.at("audit_date").text()))
        .collect()
}

pub fn performance_bands(charts: &Value) -> Vec<(&'static str, i64, &'static str)> {
    let counts = charts.at("performanceDistribution");
    BANDS
        .iter()
        .map(|(label, colour)| {
            let count = counts.items().iter().find(|row| row.at("label").as_str() == Some(label)).map(|row| int(row.at("count"))).unwrap_or(0);
            (*label, count, *colour)
        })
        .collect()
}

/// (outlet, previous score, current score).
pub fn comparison(charts: &Value) -> Vec<(String, Option<i64>, Option<i64>)> {
    let mut pairs: Vec<(String, Option<i64>, Option<i64>)> = Vec::new();
    for row in charts.at("auditComparison").items() {
        let label = row.at("label").text();
        let (outlet, which) = label.split_once(" · ").unwrap_or((&label, ""));
        let index = match pairs.iter().position(|(name, ..)| name == outlet) {
            Some(index) => index,
            None => {
                pairs.push((outlet.to_string(), None, None));
                pairs.len() - 1
            }
        };
        if which == "Previous" {
            pairs[index].1 = Some(int(row.at("score")));
        } else {
            pairs[index].2 = Some(int(row.at("score")));
        }
    }
    pairs
}

/// A breakdown card: title, note, rows (label, value, colour or the accent, note), largest value, suffix.
pub struct Breakdown {
    pub title: &'static str,
    pub note: &'static str,
    pub rows: Vec<(String, i64, Option<&'static str>, String)>,
    pub maximum: Option<f64>,
    pub suffix: &'static str,
}

pub fn breakdowns(charts: &Value, scoring: &Value) -> Vec<Breakdown> {
    let counts = |key: &str, colour: Option<&'static str>| -> Vec<(String, i64, Option<&'static str>, String)> {
        let mut rows: Vec<(String, i64, Option<&'static str>, String)> = charts
            .at(key)
            .items()
            .iter()
            .map(|row| (if row.at("label").truthy() { row.at("label").text() } else { "Unassigned".into() }, int(row.at("count")), colour, String::new()))
            .collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        rows
    };
    let shares = |key: &str| -> Vec<(String, i64, Option<&'static str>, String)> {
        charts
            .at(key)
            .items()
            .iter()
            .map(|row| {
                let score = int(row.at("score"));
                (
                    if row.at("label").truthy() { row.at("label").text() } else { "Unassigned".into() },
                    score,
                    Some(score_colour(score as f64, scoring)),
                    format!("{} of {} closed", int(row.at("completed")), int(row.at("count"))),
                )
            })
            .collect()
    };
    let rooms = charts
        .at("roomAuditTrend")
        .items()
        .iter()
        .map(|row| {
            let score = int(row.at("score"));
            (row.at("label").text(), score, Some(score_colour(score as f64, scoring)), String::new())
        })
        .collect();
    vec![
        Breakdown { title: "Findings by department", note: "Who the findings were assigned to", rows: counts("findingsByDepartment", None), maximum: None, suffix: "" },
        Breakdown { title: "Findings by location", note: "Where the findings were raised", rows: counts("findingsByArea", None), maximum: None, suffix: "" },
        Breakdown { title: "Findings by asset type", note: "What kind of item failed", rows: counts("findingsByCategory", None), maximum: None, suffix: "" },
        Breakdown { title: "Findings by priority", note: "Priority levels given", rows: counts("findingsByPriority", Some(FAIL)), maximum: None, suffix: "" },
        Breakdown { title: "Work closed by department", note: "Share of work orders closed", rows: shares("departmentPerformance"), maximum: Some(100.0), suffix: "%" },
        Breakdown { title: "Work closed by location", note: "Share of work orders closed", rows: shares("locationPerformance"), maximum: Some(100.0), suffix: "%" },
        Breakdown { title: "Work closed by asset type", note: "Share of work orders closed", rows: shares("categoryPerformance"), maximum: Some(100.0), suffix: "%" },
        Breakdown { title: "Location scores by month", note: "Average audit score of each location", rows: rooms, maximum: Some(100.0), suffix: "/100" },
    ]
}

/// (step, average seconds, longest seconds, times) for each step.
pub fn time_to_act(data: &Value) -> Vec<(String, i64, i64, i64)> {
    data.at("timeToAct").items().iter().map(|row| (row.at("label").text(), int(row.at("averageSeconds")), int(row.at("longestSeconds")), int(row.at("count")))).collect()
}

/// (person, audits completed, requests acted on, orders closed) for those who did any.
pub fn people(data: &Value) -> Vec<(String, i64, i64, i64)> {
    data.at("people")
        .items()
        .iter()
        .map(|row| (row.at("name").text(), int(row.at("audit_completed")), int(row.at("order_created")) + int(row.at("request_declined")), int(row.at("order_closed"))))
        .filter(|row| row.1 + row.2 + row.3 != 0)
        .collect()
}

pub const PEOPLE_STACKS: [(&str, Option<&str>); 3] = [("Audits completed", None), ("Requests acted on", Some(INFO)), ("Orders closed", Some(PASS))];

pub fn people_table(data: &Value) -> (Vec<&'static str>, Vec<Vec<Value>>) {
    let header = vec!["Person", "Audits started", "Audits completed", "Avg. audit time", "Signed", "Audits closed", "Requests raised", "Requests acted on", "Avg. time to act", "Orders created", "Orders closed", "Avg. time to close"];
    let rows = data
        .at("people")
        .items()
        .iter()
        .map(|row| {
            let count = |key: &str| Value::Int(int(row.at(key)));
            vec![
                row.at("name").clone(),
                count("audit_started"),
                count("audit_completed"),
                Value::str(duration_text(row.at("auditSeconds"))),
                count("audit_signed"),
                count("audit_closed"),
                count("request_raised"),
                Value::Int(int(row.at("order_created")) + int(row.at("request_declined"))),
                Value::str(duration_text(row.at("requestSeconds"))),
                count("order_created"),
                count("order_closed"),
                Value::str(duration_text(row.at("orderSeconds"))),
            ]
        })
        .collect();
    (header, rows)
}

// ---- An audit's assets by their register attributes ----

fn year_of(value: &Value) -> String {
    let text = value.text();
    let bytes = text.as_bytes();
    for start in 0..bytes.len().saturating_sub(3) {
        let window = &bytes[start..start + 4];
        if window.iter().all(u8::is_ascii_digit) && (window.starts_with(b"19") || window.starts_with(b"20")) {
            return text[start..start + 4].to_string();
        }
    }
    "Not recorded".into()
}

/// Expired, Valid, or Not recorded, for a date typed as 2026-10-09, 09.10.2026, or 9/10/2026.
fn date_status(value: &Value) -> &'static str {
    let text = value.text().trim().to_string();
    let parts: Vec<&str> = text.split(['-', '.', '/']).collect();
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let day = if parts.len() == 3 && parts.iter().all(|part| digits(part)) {
        let (a, b, c) = (parts[0], parts[1], parts[2]);
        if a.len() == 4 && b.len() <= 2 && c.len() <= 2 {
            Date::new(a.parse().unwrap_or(0), b.parse().unwrap_or(0), c.parse().unwrap_or(0))
        } else if a.len() <= 2 && b.len() <= 2 && (c.len() == 2 || c.len() == 4) {
            let year = if c.len() == 4 { c.parse().unwrap_or(0) } else { 2000 + c.parse::<i64>().unwrap_or(0) };
            Date::new(year, b.parse().unwrap_or(0), a.parse().unwrap_or(0))
        } else {
            None
        }
    } else {
        None
    };
    match day {
        None => "Not recorded",
        Some(day) if day < clock::today() => "Expired",
        Some(_) => "Valid",
    }
}

fn attribute_value(name: &str, asset: &Value) -> String {
    let or_missing = |key: &str| if asset.at(key).truthy() { asset.at(key).text() } else { "Not recorded".into() };
    match name {
        "Asset type" => or_missing("type"),
        "Brand" => or_missing("brand"),
        "Installation year" => year_of(asset.at("installation_date")),
        "Operational status" => or_missing("status"),
        "Warranty" => date_status(asset.at("warranty_date")).into(),
        _ => date_status(asset.at("expiry_date")).into(),
    }
}

const ATTRIBUTES: [&str; 6] = ["Asset type", "Brand", "Installation year", "Operational status", "Warranty", "Expiry"];

/// The key a check's asset is grouped by: its asset id, else its name.
pub fn group_key(item: &Value) -> String {
    let id = item.at("equipmentId");
    if id.truthy() {
        return id.py_str();
    }
    let section = item.at("section");
    if section.truthy() { section.py_str() } else { "Item".into() }
}

/// The audit's assets counted by each register attribute, split into those whose checks all
/// passed and those with a failure: (assets, with failures, [(attribute, [(value, passed, failed)])]),
/// the largest first (years oldest first), the smallest values over the limit gathered as "Other".
/// An attribute nobody recorded is left out.
pub fn asset_attributes(items: &[Value], assets: &Value) -> (i64, i64, Vec<(&'static str, Vec<(String, i64, i64)>)>) {
    let mut grouped: Vec<(String, Vec<&Value>)> = Vec::new();
    for item in items {
        let key = group_key(item);
        match grouped.iter_mut().find(|(name, _)| *name == key) {
            Some((_, checks)) => checks.push(item),
            None => grouped.push((key, vec![item])),
        }
    }
    let empty = Value::object();
    let mut inspected: Vec<(&Value, bool, i64)> = Vec::new();
    for (key, checks) in &grouped {
        let failing: Vec<&&Value> = checks.iter().filter(|check| !check.at("passed").truthy() && !check.at("notApplicable").truthy()).collect();
        // A casual audit's group stands for every asset in it; the assets given its issues failed.
        let weight = checks[0].at("groupCount").or(&Value::Int(1)).py_int().unwrap_or(1).max(1);
        let mut affected: Vec<String> = Vec::new();
        for check in &failing {
            for value in check.at("affectedIds").items() {
                let value = value.py_str();
                if !affected.contains(&value) {
                    affected.push(value);
                }
            }
        }
        let asset = assets.get(key).unwrap_or(&empty);
        if weight > 1 && !affected.is_empty() {
            inspected.push((asset, true, (affected.len() as i64).min(weight)));
            if weight > affected.len() as i64 {
                inspected.push((asset, false, weight - affected.len() as i64));
            }
        } else {
            inspected.push((asset, !failing.is_empty(), weight));
        }
    }
    let mut result = Vec::new();
    for name in ATTRIBUTES {
        let mut counts: Vec<(String, i64, i64)> = Vec::new();
        for (asset, failed, weight) in &inspected {
            let value = attribute_value(name, asset);
            let index = match counts.iter().position(|(known, ..)| *known == value) {
                Some(index) => index,
                None => {
                    counts.push((value, 0, 0));
                    counts.len() - 1
                }
            };
            if *failed {
                counts[index].2 += weight;
            } else {
                counts[index].1 += weight;
            }
        }
        if counts.iter().all(|(value, ..)| value == "Not recorded") {
            continue;
        }
        if name == "Installation year" {
            counts.sort_by(|a, b| (a.0 == "Not recorded", &a.0).cmp(&(b.0 == "Not recorded", &b.0)));
        } else {
            counts.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2)));
        }
        let limit = if name == "Installation year" { 30 } else { 12 };
        if counts.len() > limit {
            let rest = counts.split_off(11);
            counts.push(("Other".into(), rest.iter().map(|row| row.1).sum(), rest.iter().map(|row| row.2).sum()));
        }
        result.push((name, counts));
    }
    let total = inspected.iter().map(|(_, _, weight)| weight).sum();
    let failing = inspected.iter().filter(|(_, failed, _)| *failed).map(|(_, _, weight)| weight).sum();
    (total, failing, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_and_dates() {
        assert_eq!(year_of(&Value::str("installed 2019-04")), "2019");
        assert_eq!(year_of(&Value::str("n/a")), "Not recorded");
        assert_eq!(date_status(&Value::str("2000-01-01")), "Expired");
        assert_eq!(date_status(&Value::str("9/10/2099")), "Valid");
        assert_eq!(date_status(&Value::str("someday")), "Not recorded");
    }
}
