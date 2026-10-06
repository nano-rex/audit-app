"""Audit exports beyond the summary report: one audit as Excel (an overall sheet, then a sheet
per location in full), one audit's chosen locations as PDF, and every audit of one location as
PDF or Excel."""
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


def finish_workbook(workbook, report_sheets=()):
    """Bold headings, filters, readable widths; text that looks like a formula stays text. A
    report sheet holds several tables, so it gets no filter and keeps the headings set on it."""
    from openpyxl.styles import Font
    for sheet in workbook:
        if sheet.title not in report_sheets:
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
                   ("status", "Status"), ("comment", "Remark"), ("due_date", "Due"), ("closed_at", "Closed"),
                   ("cause", "Cause"), ("recommendation", "Recommendation"), ("required_action", "Required action"))


def finding_rows(findings):
    return [[finding.get(key) or "" for key, _ in FINDING_COLUMNS] for finding in findings]


def item_location(session, item):
    return item.get("location") or session.get("zone") or "Unassigned"


def session_locations(session):
    """The audit's locations, in the order they were inspected."""
    return list(dict.fromkeys(item_location(session, item) for item in session.get("items", [])))


def location_part(session, location, settings):
    """One location of an audit: its checks, findings, and score."""
    items = [item for item in session.get("items", []) if item_location(session, item) == location]
    findings = [finding for finding in session.get("findings") or [] if (finding.get("location") or session.get("zone")) == location]
    return {"location": location, "items": items, "findings": findings, "summary": summarize_score(items, settings)}


def sheet_title(name, taken):
    """A worksheet name Excel accepts (31 characters, none of []:*?/\\), unique in the workbook."""
    base = "".join("-" if char in '[]:*?/\\' else char for char in str(name)).strip("' ")[:31] or "Location"
    title, number = base, 2
    while title.lower() in taken:
        suffix = f" ({number})"
        title, number = base[:31 - len(suffix)] + suffix, number + 1
    taken.add(title.lower())
    return title


def append_bold(sheet, values):
    from openpyxl.styles import Font
    sheet.append(values)
    for cell in sheet[sheet.max_row]:
        cell.font = Font(bold=True)


def audit_fields(session, summary, location=None):
    signatures = session.get("signatures") or {}
    return (("Audit", session.get("audit_ref") or "Draft"), ("Name", session.get("inspection_name") or ""),
            ("Outlet", session.get("outlet")), ("Location" if location else "Locations", location or session.get("zone")),
            ("Date", session.get("audit_date")), ("Time", session.get("audit_time") or ""), ("Type", session.get("audit_type") or ""),
            ("Auditor", session.get("auditor")), ("Status", "Closed" if session.get("closed_at") else session.get("status")),
            ("Score", summary["score"]), ("Rating", summary["rating"]), ("Checks", summary["total"]), ("Passed", summary["passed"]),
            ("Failed", summary["failed"]), ("N/A", summary["notApplicable"]), ("Remarks", session.get("remarks") or ""),
            ("Audited by", (signatures.get("auditedBy") or {}).get("name", "")),
            ("Verified by", (signatures.get("verifiedBy") or {}).get("name", "")),
            ("Acknowledged by", (signatures.get("acknowledgedBy") or {}).get("name", "")))


def inspection_xlsx(session):
    """The first sheet is the whole audit; each following sheet is one location in full: the audit's
    details, every check with its result and remark, and the findings there."""
    from openpyxl import Workbook
    settings = app_settings()
    summary = session.get("scoring") or summarize_score(session.get("items", []), settings)
    parts = [location_part(session, location, settings) for location in session_locations(session)]
    workbook = Workbook()
    overall = workbook.active
    overall.title = "Overall"
    append_bold(overall, ["Field", "Value"])
    for label, value in audit_fields(session, summary):
        overall.append([label, value])
    overall.append([])
    append_bold(overall, ["Location", "Checks", "Passed", "Failed", "N/A", "Score", "Rating", "Findings"])
    for part in parts:
        score = part["summary"]
        overall.append([part["location"], score["total"], score["passed"], score["failed"], score["notApplicable"],
                        score["score"], score["rating"], len(part["findings"])])
    overall.append([])
    append_bold(overall, [label for _, label in FINDING_COLUMNS])
    for row in finding_rows(session.get("findings") or []):
        overall.append(row)
    taken = {"overall"}
    for part in parts:
        sheet = workbook.create_sheet(sheet_title(part["location"], taken))
        append_bold(sheet, ["Field", "Value"])
        for label, value in audit_fields(session, part["summary"], part["location"]):
            sheet.append([label, value])
        sheet.append([])
        append_bold(sheet, ["Item", "Check", "Result", "Remark", "Category", "Photos"])
        for item in part["items"]:
            sheet.append([item.get("section") or "", item.get("item") or "", result(item), item.get("notes") or "",
                          item.get("category") or "", len(item.get("images") or [])])
        sheet.append([])
        append_bold(sheet, [label for _, label in FINDING_COLUMNS])
        for row in finding_rows(part["findings"]) or [["No findings"]]:
            sheet.append(row)
    return finish_workbook(workbook, report_sheets=set(workbook.sheetnames))


def inspection_locations_pdf(session, locations):
    """The chosen locations of one audit in full: every check and remark, photos, and findings."""
    from backend.pdf_report import build_locations_report
    known = session_locations(session)
    chosen = list(dict.fromkeys(locations))
    if not chosen or any(location not in known for location in chosen):
        raise ValueError("Choose locations from this audit")
    settings = app_settings()
    return build_locations_report(session, [location_part(session, location, settings) for location in chosen],
                                  branding_settings(), MediaStore(config.DB_PATH), settings)


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
