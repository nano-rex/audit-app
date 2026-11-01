//! Dashboard figures and the Reports page, with consistent business-unit, outlet, and date filters.

use std::collections::BTreeMap;

use crate::activity::{kpi_report, py_round};
use crate::audits::rating_for_score;
use crate::clock::{self, Date};
use crate::common::{rating, scope, sla_status};
use crate::db::{all_settings, hydrate_many, load_or, read_setting, with_db};
use crate::error::{bad, R};
use crate::inspections::schedule_view;
use crate::json::{obj, Value};

use crate::sqlite::{Row, ToSql};
use crate::work::with_current_sla;

/// The where clause and parameters selecting a table's rows for a report.
pub fn report_scope(unit: &str, table: &str, filters: &Value) -> R<(String, Vec<Value>)> {
    let (mut where_clause, mut values) = scope(unit, table);
    let start = filters.at("from").text();
    let end = filters.at("to").text();
    for value in [&start, &end] {
        if !value.is_empty() && Date::parse_iso(value).is_none() {
            return Err(bad("Report dates must be valid YYYY-MM-DD dates"));
        }
    }
    if !start.is_empty() && !end.is_empty() && start > end {
        return Err(bad("Report start date must not be after the end date"));
    }
    if filters.at("outlet").truthy() {
        where_clause.push_str(&format!(" AND {table}.outlet = ?"));
        values.push(filters.at("outlet").clone());
    }
    if let Some(outlets) = filters.get("outlets").filter(|outlets| !outlets.is_null()) {
        let outlets = outlets.items();
        if outlets.is_empty() {
            where_clause.push_str(" AND 0");
        } else {
            where_clause.push_str(&format!(" AND {table}.outlet IN ({})", crate::db::marks(outlets.len())));
            values.extend(outlets.iter().cloned());
        }
    }
    let date_field = match table {
        "audits" | "inspection_sessions" => Some("audit_date"),
        "schedules" => Some("scheduled_date"),
        _ => None,
    };
    let mut expression = match date_field {
        Some(field) => format!("{table}.{field}"),
        None => format!("date({table}.created_at / 1000, 'unixepoch')"),
    };
    if table == "findings" || table == "work_orders" {
        let link = if table == "findings" { "audit_id" } else { "source_audit_id" };
        expression = format!("COALESCE((SELECT audit_date FROM audits WHERE id = {table}.{link}), {expression})");
    }
    if table != "equipment" {
        for (value, operator) in [(&start, ">="), (&end, "<=")] {
            if !value.is_empty() {
                where_clause.push_str(&format!(" AND {expression} {operator} ?"));
                values.push(Value::str(value.as_str()));
            }
        }
    }
    Ok((where_clause, values))
}

fn bind(values: &[Value]) -> Vec<&dyn ToSql> {
    values.iter().map(|value| value as &dyn ToSql).collect()
}

fn objects(rows: &[Row]) -> Vec<Value> {
    rows.iter().map(Row::to_object).collect()
}

const CLOSED: [&str; 3] = ["Completed", "Verified", "Closed"];

fn is_closed(row: &Value) -> bool {
    row.at("status").as_str().is_some_and(|status| CLOSED.contains(&status))
}

fn label_of(row: &Value, key: &str) -> String {
    if row.at(key).truthy() { row.at(key).py_str() } else { "Unassigned".into() }
}

fn grouped(rows: &[Value], key: &str) -> Value {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for row in rows {
        *counts.entry(label_of(row, key)).or_default() += 1;
    }
    Value::Array(counts.into_iter().map(|(label, count)| obj([("label", label.into()), ("count", Value::Int(count))])).collect())
}

fn performance(rows: &[Value], key: &str) -> Value {
    let mut groups: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for row in rows {
        let entry = groups.entry(label_of(row, key)).or_default();
        entry.0 += 1;
        entry.1 += is_closed(row) as i64;
    }
    Value::Array(
        groups
            .into_iter()
            .map(|(label, (total, completed))| obj([
                ("label", label.into()),
                ("count", Value::Int(total)),
                ("completed", Value::Int(completed)),
                ("score", Value::Int(py_round(completed as f64 * 100.0 / total as f64))),
            ]))
            .collect(),
    )
}

fn month_of(created_at: &Value) -> String {
    let at = clock::local_from_ms(created_at.as_i64().unwrap_or(0));
    format!("{:04}-{:02}", at.date.year, at.date.month)
}

pub fn dashboard(unit: &str, filters: &Value, include_room_trends: bool) -> R<Value> {
    let (audit_where, audit_params) = report_scope(unit, "audits", filters)?;
    let audit_where = format!("{audit_where} AND audits.id IN (SELECT audit_id FROM inspection_sessions WHERE status = 'Completed' AND audit_id IS NOT NULL)");
    let (schedule_where, schedule_params) = report_scope(unit, "schedules", filters)?;
    let (order_where, order_params) = report_scope(unit, "work_orders", filters)?;
    let (finding_where, finding_params) = report_scope(unit, "findings", filters)?;
    let (session_where, session_params) = report_scope(unit, "inspection_sessions", filters)?;
    with_db(|db| {
        let settings = all_settings(db, Some("scoring.%"))?;
        let mut distribution: Vec<(&str, i64)> = vec![("Excellent", 0), ("Good", 0), ("Below Expectation", 0), ("Critical", 0)];
        for audit in db.rows(&format!("SELECT score, scoring_data_id FROM audits WHERE {audit_where}"), &bind(&audit_params))? {
            let snapshot = load_or(db, audit.get("scoring_data_id"), Value::object())?;
            let score = audit.get("score").as_f64().unwrap_or(0.0);
            let mut band = snapshot.at("rating").text();
            if band.is_empty() || !distribution.iter().any(|(name, _)| *name == band) {
                band = rating_for_score(score, &settings).to_string();
            }
            if let Some(slot) = distribution.iter_mut().find(|(name, _)| *name == band) {
                slot.1 += 1;
            }
        }
        let stats = db
            .row(
                &format!("SELECT COUNT(*) total, COALESCE(ROUND(AVG(score)), 0) average, SUM(CASE WHEN score >= 90 THEN 1 ELSE 0 END) excellent, SUM(CASE WHEN score < 60 THEN 1 ELSE 0 END) below60 FROM audits WHERE {audit_where}"),
                &bind(&audit_params),
            )?
            .ok_or_else(|| bad("no statistics"))?;
        let outlets = db.rows(
            &format!(
                "WITH filtered_audits AS (SELECT * FROM audits WHERE {audit_where})
                 SELECT outlet, branch, ROUND(AVG(score)) average, COUNT(*) audit_count,
                        (SELECT score FROM filtered_audits latest WHERE latest.outlet = grouped.outlet ORDER BY audit_date DESC, id DESC LIMIT 1) latest,
                        (SELECT audit_date FROM filtered_audits latest WHERE latest.outlet = grouped.outlet ORDER BY audit_date DESC, id DESC LIMIT 1) audit_date
                 FROM filtered_audits grouped GROUP BY outlet ORDER BY outlet"
            ),
            &bind(&audit_params),
        )?;
        let recent = db.rows(&format!("SELECT outlet, branch, audit_date, score FROM audits WHERE {audit_where} ORDER BY created_at DESC, id DESC LIMIT 5"), &bind(&audit_params))?;
        let kpi = db
            .row(
                &format!("SELECT COUNT(*) assigned, SUM(CASE WHEN status = 'Completed' THEN 1 ELSE 0 END) completed, SUM(CASE WHEN status = 'Pending' THEN 1 ELSE 0 END) pending FROM schedules WHERE {schedule_where}"),
                &bind(&schedule_params),
            )?
            .ok_or_else(|| bad("no statistics"))?;
        // The same details as Scheduled Work, so an audit is named and edited the same way here.
        let schedules = db.rows(
            &format!(
                "SELECT schedules.*, inspection_sessions.id AS inspection_id, inspection_sessions.audit_ref, inspection_sessions.inspection_name, inspection_sessions.progress AS progress, inspection_sessions.status AS inspection_status
                 FROM (SELECT * FROM schedules WHERE {schedule_where} AND status NOT IN ('Completed', 'Cancelled')) AS schedules
                 LEFT JOIN inspection_sessions ON inspection_sessions.schedule_id = schedules.id
                 ORDER BY schedules.scheduled_date ASC, schedules.created_at DESC, schedules.id DESC"
            ),
            &bind(&schedule_params),
        )?;
        let all_orders = objects(&db.rows(&format!("SELECT id, outlet, zone, request_type, category, priority, status, due_date, created_at FROM work_orders WHERE {order_where}"), &bind(&order_params))?);
        let findings = objects(&db.rows(
            &format!(
                "SELECT findings.outlet, findings.location, findings.category, findings.priority, findings.assigned_department, findings.pic, findings.status, findings.created_at,
                        COALESCE(NULLIF(findings.priority_classification, ''), priority_levels.classification, CASE WHEN findings.priority IN ('High', 'Priority') THEN 'Priority' ELSE 'Non-Priority' END) classification
                 FROM findings LEFT JOIN priority_levels ON priority_levels.name = findings.priority WHERE {finding_where}"
            ),
            &bind(&finding_params),
        )?);
        let draft_audits = db.count(&format!("SELECT COUNT(*) FROM inspection_sessions WHERE {session_where} AND status != 'Completed'"), &bind(&session_params))?;
        let unstarted_audits = db.count(
            &format!("SELECT COUNT(*) FROM schedules WHERE {schedule_where} AND status != 'Completed' AND NOT EXISTS (SELECT 1 FROM inspection_sessions WHERE schedule_id = schedules.id)"),
            &bind(&schedule_params),
        )?;
        let mut room_groups: BTreeMap<(String, String, String), (i64, i64)> = BTreeMap::new();
        if include_room_trends {
            for session in db.rows(&format!("SELECT outlet, audit_date, items_data_id FROM inspection_sessions WHERE {session_where} AND status = 'Completed'"), &bind(&session_params))? {
                let month: String = session.str("audit_date").chars().take(7).collect();
                for item in load_or(db, session.get("items_data_id"), Value::array())?.items() {
                    if item.at("notApplicable").truthy() {
                        continue;
                    }
                    let room = if item.at("location").truthy() { item.at("location").py_str() } else { "Unassigned".into() };
                    let entry = room_groups.entry((session.str("outlet"), room, month.clone())).or_default();
                    entry.0 += item.at("passed").truthy() as i64;
                    entry.1 += 1;
                }
            }
        }
        let comparison = objects(&db.rows(
            &format!("SELECT outlet, score, position FROM (SELECT outlet, score, ROW_NUMBER() OVER (PARTITION BY outlet ORDER BY audit_date DESC, id DESC) position FROM audits WHERE {audit_where}) WHERE position <= 2 ORDER BY outlet, position DESC"),
            &bind(&audit_params),
        )?);
        let monthly_trend = objects(&db.rows(
            &format!("SELECT substr(audit_date, 1, 7) month, COUNT(*) audits, COALESCE(ROUND(AVG(score)), 0) average_score FROM audits WHERE {audit_where} GROUP BY substr(audit_date, 1, 7) ORDER BY month"),
            &bind(&audit_params),
        )?);

        let outlet_rows = objects(&outlets);
        let recent_rows: Vec<Value> = recent
            .iter()
            .map(|row| {
                let mut item = row.to_object();
                item.set("rating", rating(row.get("score").as_f64().unwrap_or(0.0)));
                item
            })
            .collect();
        let assigned = kpi.int("assigned");
        let completed = kpi.int("completed");
        let response_rate = if assigned != 0 { py_round(completed as f64 * 100.0 / assigned as f64) } else { 0 };
        let completed_audits = stats.int("total");
        let pending_audits = draft_audits + unstarted_audits;
        let open_orders: Vec<&Value> = all_orders.iter().filter(|row| !is_closed(row)).collect();
        let closed_orders: Vec<&Value> = all_orders.iter().filter(|row| is_closed(row)).collect();
        let priority_findings = findings.iter().filter(|row| row.at("classification").as_str() == Some("Priority")).count();
        let other_findings = findings.len() - priority_findings;
        let sla = |row: &Value| sla_status(&row.at("status").py_str(), row.at("due_date"));
        let overdue = all_orders.iter().filter(|row| sla(row) == "Overdue").count();
        let due_soon = all_orders.iter().filter(|row| sla(row) == "Due Soon").count();
        let average = stats.value("average");
        let mut rankings = outlet_rows.clone();
        // sorted(..., key=latest, reverse=True): the latest score, highest first, ties in order.
        rankings.sort_by(|a, b| b.at("latest").as_f64().unwrap_or(f64::MIN).partial_cmp(&a.at("latest").as_f64().unwrap_or(f64::MIN)).unwrap_or(std::cmp::Ordering::Equal));
        let scheduled = schedules.iter().map(|row| schedule_view(db, row)).collect::<R<Vec<_>>>()?;
        let classified: Vec<Value> = findings.iter().map(|row| obj([("label", Value::str(format!("{} · {}", month_of(row.at("created_at")), row.at("classification").py_str())))])).collect();
        let months: Vec<Value> = findings.iter().map(|row| obj([("month", Value::str(month_of(row.at("created_at"))))])).collect();
        Ok(obj([
            ("stats", obj([
                ("total", Value::Int(completed_audits + pending_audits)),
                ("overallAuditScore", if completed_audits != 0 { average.clone() } else { Value::Null }),
                ("outstandingFindings", Value::Int(findings.iter().filter(|row| !is_closed(row)).count() as i64)),
                ("average", if average.truthy() { average.clone() } else { Value::Int(0) }),
                ("excellent", Value::Int(stats.int("excellent"))),
                ("below60", Value::Int(stats.int("below60"))),
                ("auditsCompleted", Value::Int(completed_audits)),
                ("auditsPending", Value::Int(pending_audits)),
                ("priorityIssues", Value::Int(priority_findings as i64)),
                ("nonPriorityIssues", Value::Int(other_findings as i64)),
                ("outstandingIssues", Value::Int(open_orders.len() as i64)),
                ("closedWorkOrders", Value::Int(closed_orders.len() as i64)),
                ("overdueFindings", Value::Int(overdue as i64)),
                ("completionRate", Value::Int(if all_orders.is_empty() { 0 } else { py_round(closed_orders.len() as f64 * 100.0 / all_orders.len() as f64) })),
            ])),
            ("outlets", Value::Array(outlet_rows)),
            ("recent", Value::Array(recent_rows)),
            ("rankings", Value::Array(rankings)),
            ("kpi", obj([
                ("assigned", Value::Int(assigned)),
                ("completed", Value::Int(completed)),
                ("pending", Value::Int(kpi.int("pending"))),
                ("responseRate", Value::Int(response_rate)),
            ])),
            ("today", obj([
                ("scheduled", Value::Array(scheduled)),
                ("followUps", Value::Int(open_orders.len() as i64)),
                ("dueSoon", Value::Int(due_soon as i64)),
                ("overdue", Value::Int(overdue as i64)),
            ])),
            ("charts", obj([
                ("auditScores", Value::Array(outlets.iter().map(|row| obj([("label", row.value("outlet")), ("score", row.value("average"))])).collect())),
                ("performanceDistribution", Value::Array(distribution.iter().map(|(label, count)| obj([("label", (*label).into()), ("count", Value::Int(*count))])).collect())),
                ("priorityVsNonPriority", Value::Array(vec![
                    obj([("label", "Priority".into()), ("count", Value::Int(priority_findings as i64))]),
                    obj([("label", "Non-Priority".into()), ("count", Value::Int(other_findings as i64))]),
                ])),
                ("findingsByDepartment", grouped(&findings, "assigned_department")),
                ("findingsByArea", grouped(&findings, "location")),
                ("findingsByCategory", grouped(&findings, "category")),
                ("findingsByPriority", grouped(&findings, "priority")),
                ("monthlyAuditTrend", Value::Array(monthly_trend.clone())),
                ("roomAuditTrend", Value::Array(room_groups.iter().map(|((outlet, room, month), (passed, total))| obj([
                    ("label", Value::str(format!("{outlet} / {room} / {month}"))),
                    ("score", Value::Int(py_round(*passed as f64 * 100.0 / *total as f64))),
                ])).collect())),
                ("monthlyAuditScores", Value::Array(monthly_trend.iter().map(|row| obj([("label", row.at("month").clone()), ("score", row.at("average_score").clone())])).collect())),
                ("auditComparison", Value::Array(comparison.iter().map(|row| obj([
                    ("label", Value::str(format!("{} · {}", row.at("outlet").py_str(), if row.at("position").as_i64() == Some(1) { "Current" } else { "Previous" }))),
                    ("score", row.at("score").clone()),
                ])).collect())),
                ("priorityTrend", grouped(&classified, "label")),
                ("findingsTrend", grouped(&months, "month")),
                ("departmentPerformance", performance(&all_orders, "request_type")),
                ("locationPerformance", performance(&all_orders, "zone")),
                ("categoryPerformance", performance(&all_orders, "category")),
            ])),
        ]))
    })
}

pub fn report(unit: &str, filters: &Value) -> R<Value> {
    let data = dashboard(unit, filters, true)?;
    let (where_clause, values) = report_scope(unit, "work_orders", filters)?;
    let (activity_where, activity_params) = report_scope(unit, "activity_log", filters)?;
    with_db(|db| {
        let critical = db.rows(
            &format!("SELECT * FROM work_orders WHERE {where_clause} AND priority IN ('High', 'Priority') AND status NOT IN ('Completed', 'Verified', 'Closed') ORDER BY created_at DESC, id DESC"),
            &bind(&values),
        )?;
        let critical = with_current_sla(hydrate_many(db, &critical)?);
        // People and time to act come from the activity log; the log itself is its own page.
        let mut performance = kpi_report(db, &activity_where, &activity_params, 0)?;
        performance.remove("activity");
        performance.remove("activityTotal");
        // The charts colour scores by these bands and mark the pass mark.
        let mut scoring = Value::object();
        for (key, fallback) in [("passMark", 70.0), ("excellentBand", 90.0), ("goodBand", 70.0), ("belowBand", 60.0)] {
            let value = read_setting(db, &format!("scoring.{key}"), Value::Float(fallback))?;
            let number = if value.truthy() { value.py_float().unwrap_or(fallback) } else { fallback };
            scoring.set(key, Value::Float(number));
        }
        let stats = data.at("stats");
        let stat = |key: &str| stats.at(key).clone();
        let total_findings = stats.at("priorityIssues").as_i64().unwrap_or(0) + stats.at("nonPriorityIssues").as_i64().unwrap_or(0);
        let mut result = performance;
        for (key, value) in [
            ("unit", Value::str(unit)),
            ("scoring", scoring),
            ("monthlySummary", obj([
                ("audits", stat("total")),
                ("auditsCompleted", stat("auditsCompleted")),
                ("auditsPending", stat("auditsPending")),
                ("averageScore", stat("average")),
                ("openWorkOrders", stat("outstandingIssues")),
                ("totalFindings", Value::Int(total_findings)),
                ("priorityFindings", stat("priorityIssues")),
                ("nonPriorityFindings", stat("nonPriorityIssues")),
                ("closedWorkOrders", stat("closedWorkOrders")),
                ("outstandingFindings", stat("outstandingFindings")),
                ("overdueFindings", stat("overdueFindings")),
                ("completionRate", stat("completionRate")),
            ])),
            ("rankings", data.at("rankings").clone()),
            ("criticalIssues", Value::Array(critical)),
            ("kpi", data.at("kpi").clone()),
            ("recent", data.at("recent").clone()),
            ("charts", data.at("charts").clone()),
        ] {
            result.set(key, value);
        }
        Ok(result)
    })
}

/// The Activity Log page: the latest entries in the period (newest first), and how many there are.
pub fn activity_items(unit: &str, filters: &Value) -> R<Value> {
    let (where_clause, values) = report_scope(unit, "activity_log", filters)?;
    with_db(|db| {
        let performance = kpi_report(db, &where_clause, &values, 500)?;
        Ok(obj([("items", performance.at("activity").clone()), ("total", performance.at("activityTotal").clone())]))
    })
}
