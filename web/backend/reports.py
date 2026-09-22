"""Reports for the audit application."""
from backend.relational_values import load_value, hydrate_many
from io import StringIO, BytesIO
import csv
from datetime import datetime
from backend.media_store import MediaStore
from backend.scoring import summarize as summarize_score, rating_for_score
from backend import config
from backend.accounts import branding_settings
from backend.common import rating, sla_status
from backend.report_filters import report_scope
from backend.database import connect
from backend.work_orders import finding_items


def dashboard(unit, filters=None, include_room_trends=False):
    audit_where, audit_params = report_scope(unit, "audits", filters)
    audit_where = f"{audit_where} AND audits.id IN (SELECT audit_id FROM inspection_sessions WHERE status = 'Completed' AND audit_id IS NOT NULL)"
    schedule_where, schedule_params = report_scope(unit, "schedules", filters)
    work_order_where, work_order_params = report_scope(unit, "work_orders", filters)
    equipment_where, equipment_params = report_scope(unit, "equipment", filters)
    finding_where, finding_params = report_scope(unit, "findings", filters)
    session_where, session_params = report_scope(unit, "inspection_sessions", filters)
    with connect() as db:
        settings = {row["key"]: load_value(row["value_data_id"]) for row in db.execute("SELECT key, value_data_id FROM app_settings WHERE key LIKE 'scoring.%'")}
        distribution = dict.fromkeys(("Excellent", "Good", "Below Expectation", "Critical"), 0)
        for audit in db.execute(f"SELECT score, scoring_data_id FROM audits WHERE {audit_where}", audit_params):
            snapshot = load_value(audit["scoring_data_id"] or "{}")
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
            WITH filtered_audits AS (SELECT * FROM audits WHERE {audit_where})
            SELECT outlet, branch, ROUND(AVG(score)) average, COUNT(*) audit_count,
                   (SELECT score FROM filtered_audits latest WHERE latest.outlet = grouped.outlet
                    ORDER BY audit_date DESC, id DESC LIMIT 1) latest,
                   (SELECT audit_date FROM filtered_audits latest WHERE latest.outlet = grouped.outlet
                    ORDER BY audit_date DESC, id DESC LIMIT 1) audit_date
            FROM filtered_audits grouped GROUP BY outlet ORDER BY outlet
            """, audit_params,
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
                   completion_remark, completion_photo_data_id, verified_by, verified_at,
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
                   expiry_date, photos_data_id, inverter_model, motor_capacity, source_file,
                   source_sheet, inspection_criteria_data_id
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
        room_groups = {}
        if include_room_trends:
            for session in db.execute(f"SELECT outlet, audit_date, items_data_id FROM inspection_sessions WHERE {session_where} AND status = 'Completed'", session_params):
                for item in load_value(session["items_data_id"] or "[]"):
                    if item.get("notApplicable"):
                        continue
                    key = (session["outlet"], item.get("location") or "Unassigned", session["audit_date"][:7])
                    values = room_groups.setdefault(key, [0, 0])
                    values[0] += bool(item.get("passed"))
                    values[1] += 1
        comparison = [dict(row) for row in db.execute(
            f"""SELECT outlet, score, position FROM (
                SELECT outlet, score, ROW_NUMBER() OVER (PARTITION BY outlet ORDER BY audit_date DESC, id DESC) position
                FROM audits WHERE {audit_where}) WHERE position <= 2 ORDER BY outlet, position DESC""", audit_params
        ).fetchall()]
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
    def performance(rows, key):
        groups = {}
        for row in rows:
            group = groups.setdefault(row.get(key) or "Unassigned", [0, 0])
            group[0] += 1
            group[1] += row["status"] in closed_statuses
        return [{"label": label, "count": total, "completed": completed, "score": round(completed * 100 / total)} for label, (total, completed) in sorted(groups.items())]
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
        "workOrders": hydrate_many(work_orders),
        "equipment": hydrate_many(equipment_rows),
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
            "roomAuditTrend": [{"label": f"{outlet} / {room} / {month}", "score": round(passed * 100 / total)} for (outlet, room, month), (passed, total) in sorted(room_groups.items())],
            "monthlyAuditScores": [{"label": row["month"], "score": row["average_score"]} for row in monthly_trend],
            "auditComparison": [{"label": f"{row['outlet']} · {'Current' if row['position'] == 1 else 'Previous'}", "score": row["score"]} for row in comparison],
            "priorityTrend": grouped([{"label": datetime.fromtimestamp((row.get("created_at") or 0) / 1000).strftime("%Y-%m") + " · " + row["classification"]} for row in findings], "label"),
            "findingsTrend": grouped(
                [{"month": datetime.fromtimestamp((row.get("created_at") or 0) / 1000).strftime("%Y-%m")} for row in findings],
                "month",
            ),
            "departmentPerformance": performance(all_work_orders, "request_type"),
            "locationPerformance": performance(all_work_orders, "zone"),
            "categoryPerformance": performance(all_work_orders, "category"),
        },
    }


def report(unit, filters=None):
    data = dashboard(unit, filters, include_room_trends=True)
    where, params = report_scope(unit, "work_orders", filters)
    with connect() as db:
        critical_orders = hydrate_many(db.execute(
            f"SELECT * FROM work_orders WHERE {where} AND priority IN ('High', 'Priority') "
            "AND status NOT IN ('Completed', 'Verified', 'Closed') ORDER BY created_at DESC, id DESC", params
        ).fetchall())
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
            "outstandingFindings": data["stats"]["outstandingFindings"],
            "overdueFindings": data["stats"]["overdueFindings"],
            "completionRate": data["stats"]["completionRate"],
        },
        "rankings": data["rankings"],
        "criticalIssues": critical_orders,
        "kpi": data["kpi"],
        "recent": data["recent"],
        "charts": data["charts"],
    }


def report_csv(unit, filters=None):
    data = report(unit, filters)
    brand = branding_settings()
    out = StringIO()
    writer = csv.writer(out)
    def write_row(values):
        writer.writerow(["'" + value if isinstance(value, str) and value.startswith(("=", "+", "-", "@")) else value for value in values])
    write_row([f"{brand['appTitle']} Report", unit])
    write_row([])
    write_row(["Audits", "Completed", "Pending", "Average Score", "Open Work Orders", "Total Findings", "Priority", "Non-Priority", "Completion Rate"])
    summary = data["monthlySummary"]
    write_row([summary["audits"], summary["auditsCompleted"], summary["auditsPending"], summary["averageScore"], summary["openWorkOrders"], summary["totalFindings"], summary["priorityFindings"], summary["nonPriorityFindings"], str(summary["completionRate"]) + "%"])
    write_row([])
    write_row(["KPI"])
    write_row(["Assigned Tasks", "Completed", "Pending", "Response Rate"])
    kpi = data["kpi"]
    write_row([kpi["assigned"], kpi["completed"], kpi["pending"], str(kpi["responseRate"]) + "%"])
    write_row([])
    write_row(["Outlet Rankings"])
    write_row(["Outlet", "Average", "Latest", "Audit Count", "Last Audit Date"])
    for row in data["rankings"]:
        write_row([row["outlet"], row["average"], row["latest"], row["audit_count"], row["audit_date"]])
    write_row([])
    write_row(["Critical Issues"])
    write_row(["ID", "Reference", "Outlet", "Zone", "Category", "Priority", "Title", "Assignee", "Status"])
    for row in data["criticalIssues"]:
        write_row([row["id"], row.get("work_order_ref", ""), row["outlet"], row["zone"], row.get("category", ""), row["priority"], row["title"], row["assignee"], row["status"]])
    write_row([])
    write_row(["Detailed Findings"])
    write_row(["Reference", "Audit", "Outlet", "Location", "Category", "Priority", "Department", "PIC", "Status", "Comment"])
    for row in finding_items(unit, filters)["items"]:
        write_row([row.get("finding_ref", ""), row.get("audit_ref", ""), row.get("outlet", ""), row.get("location", ""), row.get("category", ""), row.get("priority", ""), row.get("assigned_department", ""), row.get("pic", ""), row.get("status", ""), row.get("comment", "")])
    return out.getvalue().encode("utf-8")


def report_xls(unit, filters=None):
    """Native XLSX workbook; the old function name remains for API compatibility."""
    from openpyxl import Workbook
    from openpyxl.styles import Font
    workbook = Workbook()
    summary = workbook.active
    summary.title = "Summary"
    summary.append([branding_settings()["appTitle"], unit])
    for key, value in report(unit, filters)["monthlySummary"].items():
        summary.append([key, value])
    findings = workbook.create_sheet("Findings")
    columns = [("finding_ref", "Finding"), ("audit_ref", "Audit"), ("audit_date", "Audit Date"), ("audit_time", "Audit Time"), ("auditor", "Auditor"), ("outlet", "Outlet"), ("location", "Location"), ("category", "Category"), ("priority", "Priority"), ("assigned_department", "Department"), ("pic", "PIC"), ("status", "Status"), ("comment", "Comment"), ("corrective_action", "Corrective Action"), ("completion_date", "Completed"), ("verified_by", "Verified By")]
    findings.append([label for _, label in columns])
    for row in finding_items(unit, filters)["items"]:
        findings.append([row.get(key) or "" for key, _ in columns])
    for sheet in workbook:
        sheet.freeze_panes = "A2"
        sheet.auto_filter.ref = sheet.dimensions
        for cell in sheet[1]:
            cell.font = Font(bold=True)
        for column in sheet.columns:
            sheet.column_dimensions[column[0].column_letter].width = min(60, max(16, max(len(str(cell.value or "")) for cell in column) + 2))
            for cell in column:
                if isinstance(cell.value, str) and cell.value.startswith(("=", "+", "-", "@")):
                    cell.data_type = "s"
    output = BytesIO()
    workbook.save(output)
    return output.getvalue()


def inspection_pdf(session):
    from backend.pdf_report import build_report
    with connect() as db:
        settings = {row["key"]: load_value(row["value_data_id"]) for row in db.execute("SELECT key, value_data_id FROM app_settings")}
    summary = session.get("scoring") or summarize_score(session.get("items", []), settings)
    return build_report(session, branding_settings(), summary, MediaStore(config.DB_PATH))
