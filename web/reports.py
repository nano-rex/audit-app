"""Reports for the audit application."""
import json
from io import StringIO
import csv
import html
from datetime import datetime
from media_store import MediaStore
from scoring import summarize as summarize_score, rating_for_score
import config
from accounts import branding_settings
from common import rating, scope, sla_status
from database import connect
from work_orders import finding_items


def dashboard(unit):
    audit_where, audit_params = scope(unit, "audits")
    audit_where = f"{audit_where} AND audits.id IN (SELECT audit_id FROM inspection_sessions WHERE status = 'Completed' AND audit_id IS NOT NULL)"
    schedule_where, schedule_params = scope(unit, "schedules")
    work_order_where, work_order_params = scope(unit, "work_orders")
    equipment_where, equipment_params = scope(unit, "equipment")
    finding_where, finding_params = scope(unit, "findings")
    session_where, session_params = scope(unit, "inspection_sessions")
    with connect() as db:
        settings = {row["key"]: json.loads(row["value"]) for row in db.execute("SELECT key, value FROM app_settings WHERE key LIKE 'scoring.%'")}
        distribution = dict.fromkeys(("Excellent", "Good", "Below Expectation", "Critical"), 0)
        for audit in db.execute(f"SELECT score, scoring_json FROM audits WHERE {audit_where}", audit_params):
            snapshot = json.loads(audit["scoring_json"] or "{}")
            band = snapshot.get("rating") or rating_for_score(audit["score"], settings)
            if band not in distribution:
                band = rating_for_score(audit["score"], settings)
            distribution[band] += 1
        stats = db.execute(
            f"""
            SELECT COUNT(*) total,
                   COALESCE(ROUND(AVG(score)), 0) average,
                   SUM(CASE WHEN score >= 90 THEN 1 ELSE 0 END) excellent,
                   SUM(CASE WHEN score < 60 THEN 1 ELSE 0 END) below60
            FROM audits WHERE {audit_where}
            """,
            audit_params,
        ).fetchone()
        outlets = db.execute(
            f"""
            SELECT outlet, branch,
                   COALESCE(ROUND(AVG(score)), 0) average,
                   COUNT(*) audit_count,
                   (SELECT score FROM audits latest
                    WHERE latest.business_unit = audits.business_unit
                      AND latest.outlet = audits.outlet
                      AND latest.id IN (SELECT audit_id FROM inspection_sessions WHERE status = 'Completed')
                    ORDER BY created_at DESC, id DESC LIMIT 1) latest,
                   (SELECT audit_date FROM audits latest
                    WHERE latest.business_unit = audits.business_unit
                      AND latest.outlet = audits.outlet
                      AND latest.id IN (SELECT audit_id FROM inspection_sessions WHERE status = 'Completed')
                    ORDER BY created_at DESC, id DESC LIMIT 1) audit_date
            FROM audits
            WHERE {audit_where}
            GROUP BY outlet
            ORDER BY outlet
            """,
            audit_params,
        ).fetchall()
        recent = db.execute(
            f"""
            SELECT outlet, branch, audit_date, score
            FROM audits
            WHERE {audit_where}
            ORDER BY created_at DESC, id DESC
            LIMIT 5
            """,
            audit_params,
        ).fetchall()
        kpi = db.execute(
            f"""
            SELECT COUNT(*) assigned,
                   SUM(CASE WHEN status = 'Completed' THEN 1 ELSE 0 END) completed,
                   SUM(CASE WHEN status = 'Pending' THEN 1 ELSE 0 END) pending
            FROM schedules
            WHERE {schedule_where}
            """,
            schedule_params,
        ).fetchone()
        schedules = db.execute(
            f"""
            SELECT id, outlet, zone, scheduled_date, auditor, remarks, status, created_at
            FROM schedules
            WHERE {schedule_where} AND status != 'Completed'
            ORDER BY scheduled_date ASC, created_at DESC, id DESC
            LIMIT 5
            """,
            schedule_params,
        ).fetchall()
        work_orders = db.execute(
            f"""
            SELECT id, work_order_ref, outlet, zone, request_type, category, priority, title,
                   description, assignee, pic, status, action_taken, completion_date,
                   completion_remark, completion_photo, verified_by, verified_at,
                   verification_remark, closed_at, due_date, vendor, sla_status, cost,
                   outlet_confirmed, created_at
            FROM work_orders
            WHERE {work_order_where}
            ORDER BY
                CASE priority WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END,
                created_at DESC
            LIMIT 8
            """,
            work_order_params,
        ).fetchall()
        equipment_rows = db.execute(
            f"""
            SELECT id, asset_id, qr_code, outlet, zone, equipment_type, health_status,
                   last_checked, replacement_flag, notes, name, description, type,
                   operational_status, code, model, serial_number, brand, location,
                   installation_date, temporary_relocation, warranty_date, calibration_date,
                   expiry_date, photos, inverter_model, motor_capacity, source_file,
                   source_sheet, inspection_criteria
            FROM equipment
            WHERE {equipment_where}
            ORDER BY
                CASE health_status WHEN 'Replace' THEN 1 WHEN 'Monitor' THEN 2 ELSE 3 END,
                last_checked DESC
            LIMIT 12
            """,
            equipment_params,
        ).fetchall()
        all_work_orders = [dict(row) for row in db.execute(
            f"""
            SELECT id, outlet, zone, request_type, category, priority, status, due_date, created_at
            FROM work_orders
            WHERE {work_order_where}
            """,
            work_order_params,
        ).fetchall()]
        findings = [dict(row) for row in db.execute(
            f"""
            SELECT findings.outlet, findings.location, findings.category, findings.priority,
                   findings.assigned_department, findings.pic, findings.status, findings.created_at,
                   COALESCE(NULLIF(findings.priority_classification, ''), priority_levels.classification,
                            CASE WHEN findings.priority IN ('High', 'Priority') THEN 'Priority' ELSE 'Non-Priority' END) classification
            FROM findings LEFT JOIN priority_levels ON priority_levels.name = findings.priority
            WHERE {finding_where}
            """, finding_params
        ).fetchall()]
        draft_audits = db.execute(
            f"SELECT COUNT(*) FROM inspection_sessions WHERE {session_where} AND status != 'Completed'", session_params
        ).fetchone()[0]
        unstarted_audits = db.execute(
            f"SELECT COUNT(*) FROM schedules WHERE {schedule_where} AND status != 'Completed' "
            "AND NOT EXISTS (SELECT 1 FROM inspection_sessions WHERE schedule_id = schedules.id)", schedule_params
        ).fetchone()[0]
        monthly_trend = [dict(row) for row in db.execute(
            f"""
            SELECT substr(audit_date, 1, 7) month, COUNT(*) audits, COALESCE(ROUND(AVG(score)), 0) average_score
            FROM audits
            WHERE {audit_where}
            GROUP BY substr(audit_date, 1, 7)
            ORDER BY month
            """,
            audit_params,
        ).fetchall()]

    outlet_rows = [dict(row) for row in outlets]
    recent_rows = [dict(row) | {"rating": rating(row["score"])} for row in recent]
    assigned = kpi["assigned"] or 0
    completed = kpi["completed"] or 0
    response_rate = round((completed * 100 / assigned) if assigned else 0)
    completed_audits = int(stats["total"] or 0)
    pending_audits = draft_audits + unstarted_audits
    closed_statuses = {"Completed", "Verified", "Closed"}
    open_work_orders = [row for row in all_work_orders if row["status"] not in closed_statuses]
    completed_work_orders = [row for row in all_work_orders if row["status"] in closed_statuses]
    priority_findings = [row for row in findings if row["classification"] == "Priority"]
    non_priority_findings = [row for row in findings if row["classification"] != "Priority"]
    overdue_orders = [row for row in all_work_orders if sla_status(row["status"], row.get("due_date")) == "Overdue"]
    due_soon_orders = [row for row in all_work_orders if sla_status(row["status"], row.get("due_date")) == "Due Soon"]
    def grouped(rows, key):
        counts = {}
        for row in rows:
            label = row.get(key) or "Unassigned"
            counts[label] = counts.get(label, 0) + 1
        return [{"label": label, "count": count} for label, count in sorted(counts.items())]
    return {
        "stats": {
            "total": completed_audits + pending_audits,
            "overallAuditScore": stats["average"] if completed_audits else None,
            "outstandingFindings": sum(row["status"] not in closed_statuses for row in findings),
            "average": stats["average"] or 0,
            "excellent": stats["excellent"] or 0,
            "below60": stats["below60"] or 0,
            "auditsCompleted": completed_audits,
            "auditsPending": pending_audits,
            "priorityIssues": len(priority_findings),
            "nonPriorityIssues": len(non_priority_findings),
            "outstandingIssues": len(open_work_orders),
            "completedCorrectiveActions": len(completed_work_orders),
            "overdueFindings": len(overdue_orders),
            "completionRate": round((len(completed_work_orders) * 100 / len(all_work_orders)) if all_work_orders else 0),
        },
        "outlets": outlet_rows,
        "recent": recent_rows,
        "rankings": sorted(outlet_rows, key=lambda item: item["latest"], reverse=True),
        "kpi": {
            "assigned": assigned,
            "completed": completed,
            "pending": kpi["pending"] or 0,
            "responseRate": response_rate,
        },
        "today": {
            "scheduled": [dict(row) for row in schedules],
            "pendingUploads": 0,
            "followUps": len(open_work_orders),
            "dueSoon": len(due_soon_orders),
            "overdue": len(overdue_orders),
        },
        "workOrders": [dict(row) for row in work_orders],
        "equipment": [dict(row) for row in equipment_rows],
        "charts": {
            "auditScores": [{"label": row["outlet"], "score": row["average"]} for row in outlets],
            "performanceDistribution": [{"label": label, "count": count} for label, count in distribution.items()],
            "priorityVsNonPriority": [
                {"label": "Priority", "count": len(priority_findings)},
                {"label": "Non-Priority", "count": len(non_priority_findings)},
            ],
            "findingsByDepartment": grouped(findings, "assigned_department"),
            "findingsByArea": grouped(findings, "location"),
            "findingsByCategory": grouped(findings, "category"),
            "findingsByPriority": grouped(findings, "priority"),
            "monthlyAuditTrend": monthly_trend,
            "findingsTrend": grouped(
                [{"month": datetime.fromtimestamp((row.get("created_at") or 0) / 1000).strftime("%Y-%m")} for row in findings],
                "month",
            ),
            "departmentPerformance": grouped(all_work_orders, "request_type"),
            "locationPerformance": grouped(all_work_orders, "zone"),
            "categoryPerformance": grouped(all_work_orders, "category"),
        },
    }


def report(unit):
    data = dashboard(unit)
    where, params = scope(unit, "work_orders")
    with connect() as db:
        critical_orders = [dict(row) for row in db.execute(
            f"SELECT * FROM work_orders WHERE {where} AND priority IN ('High', 'Priority') "
            "AND status NOT IN ('Completed', 'Verified', 'Closed') ORDER BY created_at DESC, id DESC", params
        ).fetchall()]
    return {
        "unit": unit,
        "monthlySummary": {
            "audits": data["stats"]["total"],
            "auditsCompleted": data["stats"]["auditsCompleted"],
            "auditsPending": data["stats"]["auditsPending"],
            "averageScore": data["stats"]["average"],
            "openWorkOrders": data["stats"]["outstandingIssues"],
            "totalFindings": data["stats"]["priorityIssues"] + data["stats"]["nonPriorityIssues"],
            "priorityFindings": data["stats"]["priorityIssues"],
            "nonPriorityFindings": data["stats"]["nonPriorityIssues"],
            "completedCorrectiveActions": data["stats"]["completedCorrectiveActions"],
            "outstandingFindings": data["stats"]["outstandingIssues"],
            "overdueFindings": data["stats"]["overdueFindings"],
            "completionRate": data["stats"]["completionRate"],
        },
        "rankings": data["rankings"],
        "criticalIssues": critical_orders,
        "kpi": data["kpi"],
        "recent": data["recent"],
        "charts": data["charts"],
    }


def report_csv(unit):
    data = report(unit)
    brand = branding_settings()
    out = StringIO()
    writer = csv.writer(out)
    writer.writerow([f"{brand['appTitle']} Report", unit])
    writer.writerow([])
    writer.writerow(["Audits", "Completed", "Pending", "Average Score", "Open Work Orders", "Total Findings", "Priority", "Non-Priority", "Completion Rate"])
    summary = data["monthlySummary"]
    writer.writerow([summary["audits"], summary["auditsCompleted"], summary["auditsPending"], summary["averageScore"], summary["openWorkOrders"], summary["totalFindings"], summary["priorityFindings"], summary["nonPriorityFindings"], str(summary["completionRate"]) + "%"])
    writer.writerow([])
    writer.writerow(["KPI"])
    writer.writerow(["Assigned Tasks", "Completed", "Pending", "Response Rate"])
    kpi = data["kpi"]
    writer.writerow([kpi["assigned"], kpi["completed"], kpi["pending"], str(kpi["responseRate"]) + "%"])
    writer.writerow([])
    writer.writerow(["Outlet Rankings"])
    writer.writerow(["Outlet", "Average", "Latest", "Audit Count", "Last Audit Date"])
    for row in data["rankings"]:
        writer.writerow([row["outlet"], row["average"], row["latest"], row["audit_count"], row["audit_date"]])
    writer.writerow([])
    writer.writerow(["Critical Issues"])
    writer.writerow(["ID", "Reference", "Outlet", "Zone", "Category", "Priority", "Title", "Assignee", "Status"])
    for row in data["criticalIssues"]:
        writer.writerow([row["id"], row.get("work_order_ref", ""), row["outlet"], row["zone"], row.get("category", ""), row["priority"], row["title"], row["assignee"], row["status"]])
    writer.writerow([])
    writer.writerow(["Detailed Findings"])
    writer.writerow(["Reference", "Audit", "Outlet", "Location", "Category", "Priority", "Department", "PIC", "Status", "Comment"])
    for row in finding_items()["items"]:
        writer.writerow([row.get("finding_ref", ""), row.get("audit_ref", ""), row.get("outlet", ""), row.get("location", ""), row.get("category", ""), row.get("priority", ""), row.get("assigned_department", ""), row.get("pic", ""), row.get("status", ""), row.get("comment", "")])
    return out.getvalue().encode("utf-8")


def report_xls(unit):
    data = report(unit)
    brand = branding_settings()
    rows = [
        "<table>",
        f"<tr><th colspan='2'>{html.escape(brand['appTitle'])} Report</th></tr>",
    ]
    for key, value in data["monthlySummary"].items():
        rows.append(f"<tr><td>{key}</td><td>{value}</td></tr>")
    rows.append("</table>")
    return "\n".join(rows).encode("utf-8")


def inspection_pdf(session):
    from pdf_report import build_report
    with connect() as db:
        settings = {row["key"]: json.loads(row["value"]) for row in db.execute("SELECT key, value FROM app_settings")}
    summary = session.get("scoring") or summarize_score(session.get("items", []), settings)
    return build_report(session, branding_settings(), summary, MediaStore(config.DATA_DIR / "media"))
