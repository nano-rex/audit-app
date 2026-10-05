"""Paginated audit reports with actual evidence and signature images.

One audit is printed by build_report. A location report prints every audit of one location,
each as the same audit section limited to that location's checks and findings.
"""
from datetime import datetime, timezone
from html import escape
from io import BytesIO
from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle, Image, KeepTogether, PageBreak
from backend.config import THEME_PRESET_ACCENTS
from backend.scoring import rating_for_score

SIGNATURES = (("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by"))


class ReportWriter:
    """The organization's styles, and the story being written."""

    def __init__(self, brand, media, settings=None):
        self.brand, self.media, self.settings = brand, media, settings or {}
        self.story = []
        styles = getSampleStyleSheet()
        styles.add(ParagraphStyle("Caption", parent=styles["BodyText"], fontSize=8, leading=11))
        font = Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
        if font.is_file():
            if "AuditSans" not in pdfmetrics.getRegisteredFontNames():
                pdfmetrics.registerFont(TTFont("AuditSans", str(font)))
            for style in styles.byName.values():
                style.fontName = "AuditSans"
        styles["BodyText"].fontSize = 9
        styles["BodyText"].leading = 13
        # The organization's accent colours headings and table headings, as in its other printed reports.
        theme = brand.get("theme") or {}
        self.accent = colors.HexColor(theme.get("accent") or THEME_PRESET_ACCENTS.get(theme.get("preset"), THEME_PRESET_ACCENTS["default"]))
        self.soft = colors.Color(*(channel + (1 - channel) * 0.88 for channel in self.accent.rgb()))
        for name in ("Heading1", "Heading2"):
            styles[name].textColor = self.accent
        styles.add(ParagraphStyle("TableHead", parent=styles["BodyText"], textColor=colors.white, fontName=styles["BodyText"].fontName))
        self.styles = styles

    def paragraph(self, text, style="BodyText"):
        # Zero is a value to print; only a missing value is left blank.
        return Paragraph(escape("" if text is None else str(text)).replace("\n", "<br/>"), self.styles[style])

    def add(self, *flowables):
        self.story.extend(flowables)

    def text(self, text, style="BodyText"):
        self.story.append(self.paragraph(text, style))

    def images(self, values, caption, max_height=230):
        for value in values or []:
            if not isinstance(value, dict):
                self.text(f"{caption}: {value} (legacy filename only)")
                continue
            for marked in (False, True):
                content = self.media.image_bytes(value, marked)
                if not content:
                    continue
                image = Image(BytesIO(content))
                ratio = min(460 / image.imageWidth, max_height / image.imageHeight, 1)
                image.drawWidth, image.drawHeight = image.imageWidth * ratio, image.imageHeight * ratio
                label = f"{'Marked' if marked else caption}: {value.get('markedName' if marked else 'name', 'Photo')}"
                self.story.append(KeepTogether([self.paragraph(label, "Caption"), image, Spacer(1, 8)]))

    def table(self, rows, widths, header=True, label_column=False):
        styled = [[self.paragraph(cell, "TableHead" if header and index == 0 else "Caption" if header else "BodyText") for cell in values]
                  for index, values in enumerate(rows)]
        table = Table(styled, colWidths=widths, repeatRows=1 if header else 0)
        commands = [("GRID", (0, 0), (-1, -1), .4, colors.lightgrey), ("VALIGN", (0, 0), (-1, -1), "TOP"), ("BOTTOMPADDING", (0, 0), (-1, -1), 6)]
        if header:
            commands.append(("BACKGROUND", (0, 0), (-1, 0), self.accent))
        if label_column:
            commands.append(("BACKGROUND", (0, 0), (0, -1), self.soft))
        table.setStyle(TableStyle(commands))
        self.story.append(table)

    def letterhead(self, title):
        self.text(self.brand.get("companyName") or self.brand.get("appTitle"), "Title")
        self.text(self.brand.get("departmentHeader"))
        self.text(title, "Heading1")
        logo = self.brand.get("logoUrl")
        if logo and logo.startswith("/api/media/"):
            self.images([{"url": logo, "name": "Company logo"}], "Logo")

    def audit_details(self, session):
        self.add(self.paragraph(f"{session.get('audit_ref') or 'Draft'} · {session.get('inspection_name', '')}", "Heading2"),
                 self.paragraph(f"Outlet: {session.get('outlet')} | Location: {session.get('zone')}"),
                 self.paragraph(f"Auditor: {session.get('auditor')} | Date: {session.get('audit_date')} {session.get('audit_time') or ''}"),
                 self.paragraph(f"Audit type: {session.get('audit_type') or 'Standard'}"))
        if session.get("remarks"):
            self.text(session["remarks"])
        if session.get("closed_at"):
            closed_date = datetime.fromtimestamp(session["closed_at"] / 1000, timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
            self.text(f"Closed: {closed_date} | Closed by: {session.get('closed_by') or ''}")
        self.add(Spacer(1, 8))

    def scorecard(self, summary, findings):
        closed = sum(row.get("status") in {"Completed", "Verified", "Closed"} for row in findings)
        priority = sum(
            (row.get("priority_classification") or ("Priority" if row.get("priority") in {"High", "Priority"} else "Non-Priority")) == "Priority"
            for row in findings
        )
        self.table([
            ["Score", f"{summary['score']}/100 — {summary['rating']}"],
            ["Pass mark", f"{summary['passMark']} — {'Met' if summary['meetsPassMark'] else 'Not met'}"],
            ["Checklist", f"{summary['total']} total / {summary['passed']} passed / {summary['failed']} failed / {summary['notApplicable']} N/A"],
            ["Findings", f"{len(findings)} total / {priority} priority / {len(findings) - priority} non-priority"],
            ["Follow-up", f"{closed} closed / {len(findings) - closed} open"],
        ], [125, 355], header=False, label_column=True)

    def location_grading(self, session):
        location_items = {}
        for item in session.get("items", []):
            location_items.setdefault(item.get("location") or session.get("zone") or "Unassigned", []).append(item)
        rows = [["Location", "Checks", "Passed", "Failed", "N/A", "Score", "Grade"]]
        for location, items in location_items.items():
            applicable = [item for item in items if not item.get("notApplicable")]
            passed = sum(bool(item.get("passed")) for item in applicable)
            score = round(100 * passed / len(applicable)) if applicable else 0
            rows.append([location, len(items), passed, len(applicable) - passed, len(items) - len(applicable),
                         f"{score}/100", rating_for_score(score, self.settings)])
        if len(rows) == 1:
            rows.append([session.get("zone") or "Unassigned", 0, 0, 0, 0, "0/100", "Critical"])
        self.add(Spacer(1, 14), self.paragraph("Location grading", "Heading1"))
        self.table(rows, [145, 48, 48, 48, 42, 55, 85])

    def checklist(self, items, with_photos):
        """Every check, with its result and remark; used where one location is reported in full."""
        rows = [["Item", "Check", "Result", "Remark"]]
        for item in items:
            result = "N/A" if item.get("notApplicable") else "Pass" if item.get("passed") else "Fail"
            rows.append([item.get("section") or "Item", item.get("item") or "", result, item.get("notes") or ""])
        self.add(Spacer(1, 10), self.paragraph("Checklist", "Heading2"))
        self.table(rows, [130, 170, 45, 135])
        if with_photos:
            seen = set()
            for item in items:
                fresh = [image for image in item.get("images") or [] if isinstance(image, dict) and image.get("url") not in seen]
                seen.update(image.get("url") for image in fresh)
                self.images(fresh, item.get("section") or "Photo", max_height=180)

    def findings(self, findings):
        self.text("Findings", "Heading1")
        if not findings:
            self.text("No findings.")
        for finding in findings:
            self.add(self.paragraph(f"{finding.get('finding_ref')} — {finding.get('status')}", "Heading2"),
                     self.paragraph(f"{finding.get('location')} | {finding.get('category')} | {finding.get('priority')}"),
                     self.paragraph(f"Department: {finding.get('assigned_department')} | PIC: {finding.get('pic') or 'Unassigned'} | Due: {finding.get('due_date') or 'Not set'}"))
            for key, label in (("comment", "Finding"), ("cause", "Cause"), ("recommendation", "Recommendation"), ("required_action", "Required action")):
                if finding.get(key):
                    self.text(f"{label}: {finding[key]}")
            self.text(f"Closed: {finding.get('closed_at') or 'Open'}")

    def signatures(self, session):
        self.text("Signatures", "Heading1")
        for key, label in SIGNATURES:
            signature = (session.get("signatures") or {}).get(key)
            self.text(f"{label}: {(signature or {}).get('name') or 'Unsigned'}", "Heading3")
            if signature:
                self.images([{**signature, "name": str(signature.get("signedAt") or "")[:10] or "date not recorded"}], "Signed", max_height=90)

    def build(self, footer_text, title):
        output = BytesIO()

        def footer(canvas, document):
            canvas.saveState()
            canvas.setFont("Helvetica", 8)
            canvas.drawString(42, 24, footer_text)
            canvas.drawRightString(A4[0] - 42, 24, f"Page {document.page}")
            canvas.restoreState()

        document = SimpleDocTemplate(output, pagesize=A4, rightMargin=42, leftMargin=42, topMargin=38, bottomMargin=40,
                                     title=title, author=self.brand.get("companyName", "Ottotree"))
        document.build(self.story, onFirstPage=footer, onLaterPages=footer)
        return output.getvalue()


def build_report(session, brand, summary, media, settings=None):
    writer = ReportWriter(brand, media, settings)
    writer.letterhead("Facilities Audit Report")
    writer.audit_details(session)
    findings = session.get("findings") or []
    writer.scorecard(summary, findings)
    # Keep the PDF compact by reporting one grading row per location rather
    # than repeating every checklist criterion and its evidence.
    writer.location_grading(session)
    writer.findings(findings)
    writer.signatures(session)
    return writer.build(str(session.get("audit_ref") or "Draft audit"), f"Audit {session.get('audit_ref') or session.get('id')}")


def build_location_report(outlet, location, period, audits, brand, media, settings=None):
    """Every audit of one location: an overview, then each audit's checks, photos, and findings there."""
    writer = ReportWriter(brand, media, settings)
    writer.letterhead("Location Audit Report")
    writer.text(f"Outlet: {outlet} | Location: {location}", "Heading2")
    writer.text(f"Period: {period}")
    rows = [["Date", "Audit", "Auditor", "Checks", "Failed", "Score", "Findings"]]
    for audit in audits:
        summary = audit["summary"]
        rows.append([audit["session"].get("audit_date"), audit["session"].get("audit_ref") or "", audit["session"].get("auditor") or "",
                     summary["total"], summary["failed"], f"{summary['score']}/100 {summary['rating']}", len(audit["findings"])])
    writer.add(Spacer(1, 10))
    if len(rows) == 1:
        writer.text("No completed audit covered this location in this period.")
    else:
        writer.table(rows, [62, 92, 92, 45, 42, 90, 57])
    for audit in audits:
        writer.add(PageBreak())
        writer.audit_details(audit["session"])
        writer.scorecard(audit["summary"], audit["findings"])
        writer.checklist(audit["items"], with_photos=True)
        writer.findings(audit["findings"])
        writer.signatures(audit["session"])
    return writer.build(f"{outlet} · {location}", f"Location report {outlet} {location}")
