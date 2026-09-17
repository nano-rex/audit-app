"""Paginated audit reports with actual evidence and signature images."""
from html import escape
from io import BytesIO
import json
from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle, Image, KeepTogether


def build_report(session, brand, summary, media):
    output = BytesIO()
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
    story = []

    def paragraph(text, style="BodyText"):
        return Paragraph(escape(str(text or "")).replace("\n", "<br/>"), styles[style])

    def images(values, caption):
        for value in values or []:
            if not isinstance(value, dict):
                story.append(paragraph(f"{caption}: {value} (legacy filename only)"))
                continue
            for marked in (False, True):
                content = media.image_bytes(value, marked)
                if not content:
                    continue
                image = Image(BytesIO(content))
                ratio = min(460 / image.imageWidth, 230 / image.imageHeight, 1)
                image.drawWidth, image.drawHeight = image.imageWidth * ratio, image.imageHeight * ratio
                label = f"{'Marked' if marked else caption}: {value.get('markedName' if marked else 'name', 'Photo')}"
                story.append(KeepTogether([paragraph(label, "Caption"), image, Spacer(1, 8)]))

    story += [paragraph(brand.get("companyName") or brand.get("appTitle"), "Title"),
              paragraph(brand.get("departmentHeader")), paragraph("Facilities Audit Report", "Heading1"),
              paragraph(f"{session.get('audit_ref') or 'Draft'} · {session.get('inspection_name', '')}"),
              paragraph(f"Outlet: {session.get('outlet')} | Location: {session.get('zone')}"),
              paragraph(f"Auditor: {session.get('auditor')} | Date: {session.get('audit_date')} {session.get('audit_time') or ''}"),
              paragraph(f"Audit type: {session.get('audit_type') or 'Standard'}"),
              paragraph(session.get("remarks")), Spacer(1, 12)]
    logo = brand.get("logoUrl")
    if logo and logo.startswith("/api/media/"):
        images([{"url": logo, "name": "Company logo"}], "Logo")
    findings = session.get("findings") or []
    completed = sum(row.get("status") in {"Completed", "Verified", "Closed"} for row in findings)
    priority = sum(row.get("priority_classification") == "Priority" or row.get("priority") in {"High", "Priority"} for row in findings)
    rows = [
        ["Score", f"{summary['score']}/100 — {summary['rating']}"],
        ["Pass mark", f"{summary['passMark']} — {'Met' if summary['meetsPassMark'] else 'Not met'}"],
        ["Checklist", f"{summary['total']} total / {summary['passed']} passed / {summary['failed']} failed / {summary['notApplicable']} N/A"],
        ["Findings", f"{len(findings)} total / {priority} priority / {len(findings) - priority} non-priority"],
        ["Corrective actions", f"{completed} completed / {len(findings) - completed} outstanding"],
    ]
    table = Table([[paragraph(cell) for cell in row] for row in rows], colWidths=[125, 355])
    table.setStyle(TableStyle([("BACKGROUND", (0, 0), (0, -1), colors.HexColor("#e5f0eb")), ("GRID", (0, 0), (-1, -1), .4, colors.lightgrey), ("VALIGN", (0, 0), (-1, -1), "TOP"), ("BOTTOMPADDING", (0, 0), (-1, -1), 7)]))
    story += [table, Spacer(1, 14), paragraph("Checklist and evidence", "Heading1")]
    seen_images = set()
    for index, item in enumerate(session.get("items", []), 1):
        status = "N/A" if item.get("notApplicable") else "PASS" if item.get("passed") else "FAIL"
        story += [paragraph(f"{index}. {status} — {item.get('section', '')}: {item.get('item', '')}", "Heading3"),
                  paragraph(f"Location: {item.get('location') or session.get('zone')} | Category: {item.get('category') or 'Unassigned'}"),
                  paragraph(item.get("notes"))]
        for photo in item.get("images") or []:
            key = json.dumps(photo, sort_keys=True)
            if key not in seen_images:
                images([photo], "Original")
                seen_images.add(key)
    story.append(paragraph("Findings and corrective actions", "Heading1"))
    for finding in findings:
        story += [paragraph(f"{finding.get('finding_ref')} — {finding.get('status')}", "Heading2"),
                  paragraph(f"{finding.get('location')} | {finding.get('category')} | {finding.get('priority')}"),
                  paragraph(f"Department: {finding.get('assigned_department')} | PIC: {finding.get('pic') or 'Unassigned'} | Due: {finding.get('due_date') or 'Not set'}")]
        for key, label in (("comment", "Finding"), ("cause", "Cause"), ("recommendation", "Recommendation"), ("required_action", "Required action"), ("corrective_action", "Action taken"), ("completion_remark", "Completion remark"), ("verification_remark", "Verification")):
            if finding.get(key):
                story.append(paragraph(f"{label}: {finding[key]}"))
        story.append(paragraph(f"Completed: {finding.get('completion_date') or 'Pending'} | Verified by: {finding.get('verified_by') or 'Pending'} | Closed: {finding.get('closed_at') or 'Pending'}"))
        photos = finding.get("completion_photo") or "[]"
        images(json.loads(photos) if isinstance(photos, str) else photos, "Completion photo")
    story.append(paragraph("Signatures", "Heading1"))
    for key, label in (("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by")):
        signature = (session.get("signatures") or {}).get(key)
        story.append(paragraph(f"{label}: {(signature or {}).get('name') or 'Unsigned'}", "Heading3"))
        if signature:
            images([signature], label)

    def footer(canvas, document):
        canvas.saveState()
        canvas.setFont("Helvetica", 8)
        canvas.drawString(42, 24, str(session.get("audit_ref") or "Draft audit"))
        canvas.drawRightString(A4[0] - 42, 24, f"Page {document.page}")
        canvas.restoreState()

    document = SimpleDocTemplate(output, pagesize=A4, rightMargin=42, leftMargin=42, topMargin=38, bottomMargin=40,
                                 title=f"Audit {session.get('audit_ref') or session.get('id')}", author=brand.get("companyName", "Ottotree"))
    document.build(story, onFirstPage=footer, onLaterPages=footer)
    return output.getvalue()
