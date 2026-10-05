"""Audit exports beyond the summary report: one audit as Excel, and every audit of one location
as PDF or Excel."""
from io import BytesIO

from backend import config
from backend.accounts import branding_settings
from backend.database import connect
from backend.inspections import inspection_session
from backend.media_store import MediaStore
from backend.relational_values import load_value
from backend.scoring import summarize as summarize_score


def app_settings():
    with connect() as db:
        return {row["key"]: load_value(row["value_data_id"]) for row in db.execute("SELECT key, value_data_id FROM app_settings")}


def result(item):
    return "N/A" if item.get("notApplicable") else "Pass" if item.get("passed") else "Fail"


def finish_workbook(workbook):
    """Bold headings, filters, readable widths; text that looks like a formula stays text."""
    from openpyxl.styles import Font
    for sheet in workbook:
        if sheet.max_row > 1:
            sheet.freeze_panes = "A2"
            sheet.auto_filter.ref = sheet.dimensions
        for cell in sheet[1]:
            cell.font = Font(bold=True)
        for column in sheet.columns:
            sheet.column_dimensions[column[0].column_letter].width = min(60, max(14, max(len(str(cell.value or "")) for cell in column) + 2))
            for cell in column:
                if isinstance(cell.value, str) and cell.value.startswith(("=", "+", "-", "@")):
                    cell.data_type = "s"
    output = BytesIO()
    workbook.save(output)
    return output.getvalue()


FINDING_COLUMNS = (("finding_ref", "Finding"), ("location", "Location"), ("item_name", "Item"), ("criterion", "Check"),
                   ("category", "Category"), ("priority", "Priority"), ("assigned_department", "Department"), ("pic", "PIC"),
                   ("status", "Status"), ("comment", "Remark"), ("due_date", "Due"), ("closed_at", "Closed"))


def finding_rows(findings):
    return [[finding.get(key) or "" for key, _ in FINDING_COLUMNS] for finding in findings]


def inspection_xlsx(session):
    from openpyxl import Workbook
    settings = app_settings()
    summary = session.get("scoring") or summarize_score(session.get("items", []), settings)
    workbook = Workbook()
    sheet = workbook.active
    sheet.title = "Audit"
    signatures = session.get("signatures") or {}
    for label, value in (("Field", "Value"), ("Audit", session.get("audit_ref") or "Draft"), ("Name", session.get("inspection_name") or ""),
                         ("Outlet", session.get("outlet")), ("Locations", session.get("zone")), ("Date", session.get("audit_date")),
                         ("Time", session.get("audit_time") or ""), ("Type", session.get("audit_type") or ""), ("Auditor", session.get("auditor")),
                         ("Status", "Closed" if session.get("closed_at") else session.get("status")), ("Score", summary["score"]),
                         ("Rating", summary["rating"]), ("Checks", summary["total"]), ("Passed", summary["passed"]),
                         ("Failed", summary["failed"]), ("N/A", summary["notApplicable"]), ("Remarks", session.get("remarks") or ""),
                         ("Audited by", (signatures.get("auditedBy") or {}).get("name", "")),
                         ("Verified by", (signatures.get("verifiedBy") or {}).get("name", "")),
                         ("Acknowledged by", (signatures.get("acknowledgedBy") or {}).get("name", ""))):
        sheet.append([label, value])
    checklist = workbook.create_sheet("Checklist")
    checklist.append(["Location", "Item", "Check", "Result", "Remark", "Category", "Photos"])
    for item in session.get("items", []):
        checklist.append([item.get("location") or session.get("zone"), item.get("section") or "", item.get("item") or "", result(item),
                          item.get("notes") or "", item.get("category") or "", len(item.get("images") or [])])
    findings = workbook.create_sheet("Findings")
    findings.append([label for _, label in FINDING_COLUMNS])
    for row in finding_rows(session.get("findings") or []):
        findings.append(row)
    return finish_workbook(workbook)


def location_audits(outlet, location, date_from="", date_to="", outlets=None):
    """Completed audits of an outlet that covered the location, each limited to it."""
    if outlets is not None and outlet not in outlets:
        return []
    clauses, params = ["outlet = ?", "status = 'Completed'"], [outlet]
    if date_from:
        clauses.append("audit_date >= ?")
        params.append(date_from)
    if date_to:
        clauses.append("audit_date <= ?")
        params.append(date_to)
    with connect() as db:
        ids = [row[0] for row in db.execute(f"SELECT id FROM inspection_sessions WHERE {' AND '.join(clauses)} ORDER BY audit_date, id", params)]
    settings = app_settings()
    audits = []
    for session_id in ids:
        session = inspection_session(session_id)
        items = [item for item in session.get("items", []) if (item.get("location") or session.get("zone")) == location]
        if not items:
            continue
        findings = [finding for finding in session.get("findings") or [] if finding.get("location") == location]
        audits.append({"session": session, "items": items, "findings": findings, "summary": summarize_score(items, settings)})
    return audits


def period_label(date_from, date_to):
    if date_from and date_to:
        return f"{date_from} to {date_to}"
    if date_from:
        return f"From {date_from}"
    if date_to:
        return f"Up to {date_to}"
    return "All dates"


def location_pdf(outlet, location, date_from="", date_to="", outlets=None):
    from backend.pdf_report import build_location_report
    audits = location_audits(outlet, location, date_from, date_to, outlets)
    return build_location_report(outlet, location, period_label(date_from, date_to), audits, branding_settings(),
                                 MediaStore(config.DB_PATH), app_settings())


def location_xlsx(outlet, location, date_from="", date_to="", outlets=None):
    from openpyxl import Workbook
    audits = location_audits(outlet, location, date_from, date_to, outlets)
    workbook = Workbook()
    overview = workbook.active
    overview.title = "Audits"
    overview.append(["Date", "Audit", "Name", "Auditor", "Checks", "Passed", "Failed", "N/A", "Score", "Rating", "Findings", "Status"])
    checklist = workbook.create_sheet("Checklist")
    checklist.append(["Date", "Audit", "Item", "Check", "Result", "Remark", "Category", "Photos"])
    findings = workbook.create_sheet("Findings")
    findings.append(["Date", "Audit", *[label for _, label in FINDING_COLUMNS]])
    for audit in audits:
        session, summary = audit["session"], audit["summary"]
        date, reference = session.get("audit_date"), session.get("audit_ref") or ""
        overview.append([date, reference, session.get("inspection_name") or "", session.get("auditor") or "", summary["total"],
                         summary["passed"], summary["failed"], summary["notApplicable"], summary["score"], summary["rating"],
                         len(audit["findings"]), "Closed" if session.get("closed_at") else session.get("status")])
        for item in audit["items"]:
            checklist.append([date, reference, item.get("section") or "", item.get("item") or "", result(item), item.get("notes") or "",
                              item.get("category") or "", len(item.get("images") or [])])
        for row in finding_rows(audit["findings"]):
            findings.append([date, reference, *row])
    info = workbook.create_sheet("Report", 0)
    for label, value in (("Field", "Value"), ("Outlet", outlet), ("Location", location), ("Period", period_label(date_from, date_to)),
                         ("Audits", len(audits))):
        info.append([label, value])
    return finish_workbook(workbook)
