"""Paginated audit reports with actual evidence and signature images.

They look like the web app: its font (Noto Sans), its colours (the organization's accent, soft
panels, green / red results), and compact type so more fits on a page.

One audit is printed by build_report (overall) or build_locations_report (chosen locations in
full). A location report prints every audit of one location, each limited to that location.
"""
from datetime import datetime, timezone
from html import escape
from io import BytesIO
from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.enums import TA_RIGHT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.graphics.charts.barcharts import HorizontalBarChart
from reportlab.graphics.charts.piecharts import Pie
from reportlab.graphics.shapes import Circle, Drawing, Line, Rect, String
from reportlab.platypus import CondPageBreak, Image, KeepTogether, PageBreak, Paragraph, SimpleDocTemplate, Spacer, Table, TableStyle
from backend.config import THEME_PRESET_ACCENTS
from backend.scoring import rating_for_score

SIGNATURES = (("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by"))
FONTS = Path(__file__).resolve().parents[1] / "fonts"
FALLBACK_FONT = Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
MARGIN = 36
WIDTH = A4[0] - 2 * MARGIN

# The web app's palette (web/css/base.css).
TEXT = colors.HexColor("#17222b")
MUTED = colors.HexColor("#5f6c66")
BORDER = colors.HexColor("#d3d9d5")
SURFACE_ALT = colors.HexColor("#f7f9f8")
PASS, PASS_SOFT = colors.HexColor("#1c8f4b"), colors.HexColor("#e3f4ea")
FAIL, FAIL_SOFT = colors.HexColor("#c9303a"), colors.HexColor("#fde7e8")
WARN, WARN_SOFT = colors.HexColor("#b96100"), colors.HexColor("#fdefd9")
NEUTRAL_SOFT = colors.HexColor("#eceff0")
RESULT_COLOURS = {"Pass": (PASS, PASS_SOFT), "Fail": (FAIL, FAIL_SOFT), "N/A": (MUTED, NEUTRAL_SOFT)}


def register_fonts():
    """The app's font for reports; DejaVu covers characters outside it (and stands in if it is missing)."""
    names = pdfmetrics.getRegisteredFontNames()
    regular, bold = FONTS / "noto-sans-latin-400.ttf", FONTS / "noto-sans-latin-700.ttf"
    if "AuditSans" not in names and regular.is_file() and bold.is_file():
        pdfmetrics.registerFont(TTFont("AuditSans", str(regular)))
        pdfmetrics.registerFont(TTFont("AuditSans-Bold", str(bold)))
        pdfmetrics.registerFontFamily("AuditSans", normal="AuditSans", bold="AuditSans-Bold", italic="AuditSans", boldItalic="AuditSans-Bold")
    if "AuditFallback" not in names and FALLBACK_FONT.is_file():
        pdfmetrics.registerFont(TTFont("AuditFallback", str(FALLBACK_FONT)))
        pdfmetrics.registerFontFamily("AuditFallback", normal="AuditFallback", bold="AuditFallback", italic="AuditFallback", boldItalic="AuditFallback")
    names = pdfmetrics.getRegisteredFontNames()
    if "AuditSans" in names:
        return "AuditSans", "AuditSans-Bold", set(pdfmetrics.getFont("AuditSans").face.charToGlyph)
    if "AuditFallback" in names:
        return "AuditFallback", "AuditFallback", None
    return "Helvetica", "Helvetica-Bold", None


def asset_details_line(asset, category=""):
    """Code · type · brand model · serial · installed · status, whichever the register has."""
    parts = [asset.get("code"), category or asset.get("category"), asset.get("type"),
             " ".join(value for value in (asset.get("brand"), asset.get("model")) if value),
             asset.get("serial_number") and f"S/N {asset['serial_number']}",
             asset.get("installation_date") and f"Installed {asset['installation_date']}",
             asset.get("warranty_date") and f"Warranty {asset['warranty_date']}",
             asset.get("expiry_date") and f"Expires {asset['expiry_date']}", asset.get("status")]
    return " · ".join(str(part) for part in parts if part)


def mix(colour, amount):
    """The colour faded towards white, as the app's soft tints are."""
    return colors.Color(*(channel + (1 - channel) * amount for channel in colour.rgb()))


def hex_of(colour):
    return "#" + colour.hexval()[2:]


class ReportWriter:
    """The organization's styles, and the story being written."""

    def __init__(self, brand, media, settings=None):
        self.brand, self.media, self.settings = brand, media, settings or {}
        self.story = []
        self.font, self.bold, self.glyphs = register_fonts()
        theme = brand.get("theme") or {}
        self.accent = colors.HexColor(theme.get("accent") or THEME_PRESET_ACCENTS.get(theme.get("preset"), THEME_PRESET_ACCENTS["default"]))
        self.soft = mix(self.accent, 0.88)
        base = getSampleStyleSheet()
        body = ParagraphStyle("BodyText", parent=base["BodyText"], fontName=self.font, fontSize=8.5, leading=11.5, textColor=TEXT, spaceBefore=0, spaceAfter=0)
        self.styles = {
            "BodyText": body,
            "Caption": ParagraphStyle("Caption", parent=body, fontSize=7.5, leading=9.5, textColor=MUTED),
            "Title": ParagraphStyle("Title", parent=body, fontName=self.bold, fontSize=13, leading=16),
            "Heading1": ParagraphStyle("Heading1", parent=body, fontName=self.bold, fontSize=11, leading=14, spaceBefore=10, spaceAfter=5),
            "Heading2": ParagraphStyle("Heading2", parent=body, fontName=self.bold, fontSize=9.5, leading=12.5, spaceBefore=4, spaceAfter=3),
            "Heading3": ParagraphStyle("Heading3", parent=body, fontName=self.bold, fontSize=9, leading=11.5),
            "TableHead": ParagraphStyle("TableHead", parent=body, fontName=self.bold, fontSize=7.5, leading=9.5, textColor=MUTED),
            "Label": ParagraphStyle("Label", parent=body, fontSize=7, leading=9, textColor=MUTED),
            "Value": ParagraphStyle("Value", parent=body, fontName=self.bold, fontSize=13, leading=16),
            "Right": ParagraphStyle("Right", parent=body, alignment=TA_RIGHT),
            "RightCaption": ParagraphStyle("RightCaption", parent=body, fontSize=7.5, leading=9.5, textColor=MUTED, alignment=TA_RIGHT),
            "ReportTitle": ParagraphStyle("ReportTitle", parent=body, fontName=self.bold, fontSize=10.5, leading=13, textColor=self.accent, alignment=TA_RIGHT),
        }
        self.fallbacks = {}

    # ---- Building blocks ----

    def style(self, name, text=""):
        """The named style, or its fallback-font copy when the text has characters the app's font lacks."""
        style = self.styles.get(name, self.styles["BodyText"])
        if self.glyphs is None or all(ord(char) in self.glyphs or char in "\n\r\t" for char in text):
            return style
        if name not in self.fallbacks:
            self.fallbacks[name] = ParagraphStyle(f"{name}Fallback", parent=style, fontName="AuditFallback")
        return self.fallbacks[name] if "AuditFallback" in pdfmetrics.getRegisteredFontNames() else style

    def paragraph(self, text, style="BodyText"):
        # Zero is a value to print; only a missing value is left blank.
        text = "" if text is None else str(text)
        return Paragraph(escape(text).replace("\n", "<br/>"), self.style(style, text))

    def rich(self, markup, style="BodyText", plain=""):
        """Markup built here from escaped values (bold, colour); plain is its text, to choose the font."""
        return Paragraph(markup, self.style(style, plain or markup))

    def add(self, *flowables):
        self.story.extend(flowables)

    def text(self, text, style="BodyText"):
        self.story.append(self.paragraph(text, style))

    def heading(self, text):
        """A section heading with the accent bar the app's panels use."""
        bar = Table([["", self.paragraph(text, "Heading2")]], colWidths=[3, WIDTH - 3])
        bar.setStyle(TableStyle([("BACKGROUND", (0, 0), (0, 0), self.accent), ("LEFTPADDING", (1, 0), (1, 0), 7),
                                 ("TOPPADDING", (0, 0), (-1, -1), 1), ("BOTTOMPADDING", (0, 0), (-1, -1), 1),
                                 ("VALIGN", (0, 0), (-1, -1), "MIDDLE")]))
        # A heading never ends a page on its own.
        self.add(CondPageBreak(110), Spacer(1, 10), bar, Spacer(1, 6))

    def panel_style(self, background=None, padding=6):
        commands = [("BOX", (0, 0), (-1, -1), 0.6, BORDER), ("ROUNDEDCORNERS", [4, 4, 4, 4]), ("VALIGN", (0, 0), (-1, -1), "TOP"),
                    ("LEFTPADDING", (0, 0), (-1, -1), padding), ("RIGHTPADDING", (0, 0), (-1, -1), padding),
                    ("TOPPADDING", (0, 0), (-1, -1), padding - 1), ("BOTTOMPADDING", (0, 0), (-1, -1), padding - 1)]
        if background is not None:
            commands.append(("BACKGROUND", (0, 0), (-1, -1), background))
        return commands

    def photo(self, value, marked, width, height):
        content = self.media.image_bytes(value, marked) if self.media else None
        if not content:
            return None
        image = Image(BytesIO(content))
        ratio = min(width / image.imageWidth, height / image.imageHeight, 1)
        image.drawWidth, image.drawHeight = image.imageWidth * ratio, image.imageHeight * ratio
        return image

    def images(self, values, caption, max_height=230):
        for value in values or []:
            if not isinstance(value, dict):
                self.text(f"{caption}: {value} (legacy filename only)", "Caption")
                continue
            for marked in (False, True):
                image = self.photo(value, marked, WIDTH, max_height)
                if image:
                    label = f"{'Marked' if marked else caption}: {value.get('markedName' if marked else 'name', 'Photo')}"
                    self.story.append(KeepTogether([self.paragraph(label, "Caption"), image, Spacer(1, 6)]))

    def image_grid(self, values, columns=4, height=95):
        """Photos side by side, the marked copy beside its original; returns how many were placed."""
        width = (WIDTH - (columns - 1) * 6) / columns
        cells = []
        for value in values or []:
            if not isinstance(value, dict):
                continue
            for marked in (False, True):
                image = self.photo(value, marked, width - 2, height)
                if image:
                    label = "Marked" if marked else (value.get("caption") or value.get("name") or "Photo")
                    cells.append([image, self.paragraph(label, "Caption")])
        if not cells:
            return 0
        rows = [cells[index:index + columns] for index in range(0, len(cells), columns)]
        rows[-1] += [""] * (columns - len(rows[-1]))
        table = Table(rows, colWidths=[width + 6] * (columns - 1) + [width])
        table.setStyle(TableStyle([("VALIGN", (0, 0), (-1, -1), "TOP"), ("LEFTPADDING", (0, 0), (-1, -1), 0),
                                   ("RIGHTPADDING", (0, 0), (-1, -1), 6), ("TOPPADDING", (0, 0), (-1, -1), 4), ("BOTTOMPADDING", (0, 0), (-1, -1), 2)]))
        self.story.append(table)
        return len(cells)

    def table(self, rows, widths, header=True, label_column=False, result_column=None, markup=None, style="BodyText"):
        self.story.append(self.table_flowable(rows, widths, header, label_column, result_column, markup, style))

    def table_flowable(self, rows, widths, header=True, label_column=False, result_column=None, markup=None, style="BodyText", width=WIDTH):
        """A table like the app's lists: soft header, light rules, results coloured. markup gives
        cells (row, column) their own formatted content, such as a coloured grade."""
        scale = width / sum(widths)
        widths = [column * scale for column in widths]
        markup = dict(markup or {})
        commands = []
        if result_column is not None:
            for index, values in enumerate(rows[1 if header else 0:], 1 if header else 0):
                colour = RESULT_COLOURS.get(str(values[result_column]))
                if colour:
                    markup[(index, result_column)] = f'<font color="{hex_of(colour[0])}"><b>{escape(str(values[result_column]))}</b></font>'
                    commands.append(("BACKGROUND", (result_column, index), (result_column, index), colour[1]))
        styled = []
        for index, values in enumerate(rows):
            line = []
            for column, cell in enumerate(values):
                name = "TableHead" if (header and index == 0) or (label_column and column == 0) else style
                line.append(self.rich(markup[(index, column)], name, str(cell)) if (index, column) in markup else self.paragraph(cell, name))
            styled.append(line)
        table = Table(styled, colWidths=widths, repeatRows=1 if header else 0)
        padding = 3 if style == "Caption" else 5
        commands = commands + [("BOX", (0, 0), (-1, -1), 0.6, BORDER), ("LINEBELOW", (0, 0), (-1, -2), 0.4, BORDER),
                    ("VALIGN", (0, 0), (-1, -1), "TOP"), ("ROUNDEDCORNERS", [4, 4, 4, 4]),
                    ("LEFTPADDING", (0, 0), (-1, -1), padding), ("RIGHTPADDING", (0, 0), (-1, -1), padding),
                    ("TOPPADDING", (0, 0), (-1, -1), 3), ("BOTTOMPADDING", (0, 0), (-1, -1), 4)]
        if header:
            commands.append(("BACKGROUND", (0, 0), (-1, 0), self.soft))
        if label_column:
            commands.append(("BACKGROUND", (0, 0), (0, -1), SURFACE_ALT))
        table.setStyle(TableStyle(commands))
        return table

    def pill(self, text, tone):
        foreground, background = {"pass": (PASS, PASS_SOFT), "fail": (FAIL, FAIL_SOFT), "warn": (WARN, WARN_SOFT)}.get(tone, (MUTED, NEUTRAL_SOFT))
        return f'<font backColor="{hex_of(background)}" color="{hex_of(foreground)}"><b>&nbsp;{escape(str(text))}&nbsp;</b></font>'

    def rating_tone(self, score):
        mark = float(self.settings.get("scoring.passMark", 70) or 70)
        return "pass" if score >= mark else "warn" if score >= mark - 15 else "fail"

    # ---- Report sections ----

    def letterhead(self, title):
        """The organization and the report's name, over the accent rule."""
        company = self.brand.get("companyName") or self.brand.get("appTitle") or ""
        left = [self.paragraph(company, "Title"), self.paragraph(self.brand.get("departmentHeader") or "", "Caption")]
        right = [self.paragraph(title, "ReportTitle"), self.paragraph(f"Printed {datetime.now().strftime('%Y-%m-%d %H:%M')}", "RightCaption")]
        logo = self.brand.get("logoUrl")
        image = self.photo({"url": logo}, False, 90, 34) if logo and logo.startswith("/api/media/") else None
        cells, widths = ([image, left, right], [100, WIDTH - 280, 180]) if image else ([left, right], [WIDTH - 200, 200])
        table = Table([cells], colWidths=widths)
        table.setStyle(TableStyle([("VALIGN", (0, 0), (-1, -1), "MIDDLE"), ("LINEBELOW", (0, 0), (-1, 0), 1.5, self.accent),
                                   ("LEFTPADDING", (0, 0), (-1, -1), 0), ("RIGHTPADDING", (0, 0), (-1, -1), 0), ("BOTTOMPADDING", (0, 0), (-1, -1), 7)]))
        self.add(table, Spacer(1, 10))

    def audit_details(self, session, location=None):
        """The audit's name, then its details in a soft panel, as the app shows an audit."""
        status = "Closed" if session.get("closed_at") else session.get("status") or "Draft"
        title = f"<b>{escape(str(session.get('audit_ref') or 'Draft'))}</b> &nbsp;{self.pill(status, 'pass' if status in ('Completed', 'Closed') else 'warn')}"
        self.add(self.rich(title, "Heading2", str(session.get("audit_ref"))))
        fields = [("Outlet", session.get("outlet")), ("Location", location or session.get("zone") or "All Locations"),
                  ("Date", f"{session.get('audit_date') or ''} {session.get('audit_time') or ''}".strip()),
                  ("Auditor", session.get("auditor")), ("Audit type", session.get("audit_type") or "Standard")]
        if session.get("closed_at"):
            closed = datetime.fromtimestamp(session["closed_at"] / 1000, timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
            fields.append(("Closed", f"{closed} by {session.get('closed_by') or ''}".strip()))
        cells = [[self.paragraph(label.upper(), "Label"), self.paragraph(value or "—")] for label, value in fields]
        per_row = 3
        rows = [cells[index:index + per_row] for index in range(0, len(cells), per_row)]
        rows[-1] += [""] * (per_row - len(rows[-1]))
        table = Table(rows, colWidths=[WIDTH / per_row] * per_row)
        table.setStyle(TableStyle(self.panel_style(SURFACE_ALT)))
        self.add(table)
        if session.get("remarks"):
            self.add(Spacer(1, 4), self.rich(f'<font color="{hex_of(MUTED)}">Remarks:</font> {escape(str(session["remarks"]))}', plain=str(session["remarks"])))
        self.add(Spacer(1, 8))

    def scorecard(self, summary, findings):
        """Four tiles like the dashboard's figures: score, checks, findings, follow-up."""
        closed = sum(row.get("status") in {"Completed", "Verified", "Closed"} for row in findings)
        priority = sum(
            (row.get("priority_classification") or ("Priority" if row.get("priority") in {"High", "Priority"} else "Non-Priority")) == "Priority"
            for row in findings
        )
        tone = "pass" if summary.get("meetsPassMark") else "fail"
        colour = PASS if tone == "pass" else FAIL
        tiles = [
            ("Score", f'<font color="{hex_of(colour)}">{summary["score"]}</font><font size="8" color="{hex_of(MUTED)}">/100</font>',
             f'{summary["rating"]} · pass mark {summary["passMark"]:g}'),
            ("Checks", str(summary["total"]), f'{summary["passed"]} passed · {summary["failed"]} failed · {summary["notApplicable"]} N/A'),
            ("Findings", str(len(findings)), f"{priority} priority · {len(findings) - priority} non-priority"),
            ("Follow-up", f"{len(findings) - closed} open", f"{closed} closed"),
        ]
        # One table, so the tiles share their height; empty columns keep them apart.
        gap = 6
        width = (WIDTH - gap * 3) / 4
        rows = [[], [], []]
        for index, (label, value, note) in enumerate(tiles):
            if index:
                for row in rows:
                    row.append("")
            rows[0].append(self.paragraph(label.upper(), "Label"))
            rows[1].append(self.rich(value, "Value"))
            rows[2].append(self.paragraph(note, "Caption"))
        table = Table(rows, colWidths=[width, gap] * 3 + [width])
        commands = [("VALIGN", (0, 0), (-1, -1), "TOP"), ("LEFTPADDING", (0, 0), (-1, -1), 7), ("RIGHTPADDING", (0, 0), (-1, -1), 7),
                    ("TOPPADDING", (0, 0), (-1, -1), 1), ("BOTTOMPADDING", (0, 0), (-1, -1), 1),
                    ("TOPPADDING", (0, 0), (-1, 0), 6), ("BOTTOMPADDING", (0, 2), (-1, 2), 6)]
        for column in range(0, 7, 2):
            commands.append(("BOX", (column, 0), (column, 2), 0.6, BORDER))
        table.setStyle(TableStyle(commands))
        self.add(table, Spacer(1, 4))

    def location_grading(self, session):
        location_items = {}
        for item in session.get("items", []):
            location_items.setdefault(item.get("location") or session.get("zone") or "Unassigned", []).append(item)
        rows = [["Location", "Checks", "Passed", "Failed", "N/A", "Score", "Grade"]]
        tones = []
        for location, items in location_items.items():
            applicable = [item for item in items if not item.get("notApplicable")]
            passed = sum(bool(item.get("passed")) for item in applicable)
            score = round(100 * passed / len(applicable)) if applicable else 0
            rows.append([location, len(items), passed, len(applicable) - passed, len(items) - len(applicable),
                         f"{score}/100", rating_for_score(score, self.settings)])
            tones.append(self.rating_tone(score))
        if len(rows) == 1:
            rows.append([session.get("zone") or "Unassigned", 0, 0, 0, 0, "0/100", "Critical"])
            tones.append("fail")
        self.heading("Location grading")
        self.table(rows, [170, 50, 50, 50, 40, 60, 100], markup={(index, 6): self.pill(rows[index][6], tone) for index, tone in enumerate(tones, 1)})

    def checklist(self, items, with_photos, assets=None):
        """Every check, one asset at a time: the asset's details and own table of checks, then its
        photos (those taken in the audit, or its registered photo when none was taken)."""
        grouped = {}
        for item in items:
            key = item.get("equipmentId") or item.get("section") or "Item"
            grouped.setdefault(key, []).append(item)
        self.heading("Checklist")
        for key, checks in grouped.items():
            first = checks[0]
            failed = sum(not check.get("passed") and not check.get("notApplicable") for check in checks)
            count = f"{len(checks)} check{'s' if len(checks) != 1 else ''}"
            status = self.pill(f"{failed} failed" if failed else "All passed", "fail" if failed else "pass")
            name = first.get("section") or "Item"
            asset = (assets or {}).get(str(first.get("equipmentId") or ""), {})
            details = asset_details_line(asset, first.get("category"))
            bar = Table([[[self.rich(f"<b>{escape(name)}</b>", plain=name), self.paragraph(details, "Caption")] if details else self.rich(f"<b>{escape(name)}</b>", plain=name),
                          self.rich(f'<font size="7.5" color="{hex_of(MUTED)}">{count}</font> &nbsp;{status}', "Right")]],
                        colWidths=[WIDTH - 150, 150])
            bar.setStyle(TableStyle(self.panel_style(SURFACE_ALT, 5) + [("VALIGN", (0, 0), (-1, -1), "MIDDLE")]))
            rows = [["Check", "Result", "Remark"]]
            for check in checks:
                result = "N/A" if check.get("notApplicable") else "Pass" if check.get("passed") else "Fail"
                rows.append([check.get("item") or "", result, check.get("notes") or ""])
            self.story.append(KeepTogether([bar, Spacer(1, 3), self.table_flowable(rows, [250, 50, 223], result_column=1)]))
            if with_photos:
                seen, photos = set(), []
                for check in checks:
                    for image in check.get("images") or []:
                        if isinstance(image, dict) and image.get("url") not in seen:
                            seen.add(image.get("url"))
                            photos.append(image)
                if not self.image_grid(photos):
                    registered = [{**image, "caption": "Registered photo"} for image in asset.get("photos") or [] if isinstance(image, dict)]
                    if not self.image_grid(registered[:4]):
                        self.text("No photo.", "Caption")
            self.add(Spacer(1, 10))

    def charts(self, summary, parts, findings):
        """The audit at a glance: results of every check, each location's score, and findings by category."""
        self.heading("At a glance")
        mark = float(summary.get("passMark") or 70)
        results = Drawing(170, 118)
        pie = Pie()
        pie.x, pie.y, pie.width, pie.height = 6, 14, 92, 92
        values = [summary["passed"], summary["failed"], summary["notApplicable"]]
        pie.data = values if any(values) else [1]
        pie.innerRadiusFraction = 0.55
        pie.slices.strokeColor = colors.white
        pie.slices.strokeWidth = 1
        for index, colour in enumerate((PASS, FAIL, colors.HexColor("#b4bdb8")) if any(values) else (NEUTRAL_SOFT,)):
            pie.slices[index].fillColor = colour
        results.add(pie)
        results.add(String(52, 56, f"{summary['score']}", fontName=self.bold, fontSize=13, fillColor=TEXT, textAnchor="middle"))
        results.add(String(52, 46, "score", fontName=self.font, fontSize=6.5, fillColor=MUTED, textAnchor="middle"))
        for index, (label, value, colour) in enumerate((("Passed", values[0], PASS), ("Failed", values[1], FAIL), ("N/A", values[2], colors.HexColor("#b4bdb8")))):
            y = 84 - index * 16
            results.add(Rect(108, y, 7, 7, fillColor=colour, strokeColor=None))
            results.add(String(119, y, f"{label} {value}", fontName=self.font, fontSize=7.5, fillColor=TEXT))
        results.add(String(6, 4, "Checks", fontName=self.bold, fontSize=7.5, fillColor=MUTED))
        categories = {}
        for finding in findings:
            categories[finding.get("category") or "No category"] = categories.get(finding.get("category") or "No category", 0) + 1
        side = self.bar_chart(sorted(categories.items(), key=lambda pair: -pair[1])[:8], WIDTH - 180, "Findings by category",
                              lambda value: FAIL, maximum=None) if categories else self.paragraph("No findings in this audit.", "Caption")
        row = Table([[results, side]], colWidths=[180, WIDTH - 180])
        row.setStyle(TableStyle(self.panel_style(colors.white, 6) + [("LINEBEFORE", (1, 0), (1, 0), 0.4, BORDER)]))
        self.add(KeepTogether([row]), Spacer(1, 6))
        scores = [(part["location"], part["summary"]["score"]) for part in parts]
        if len(scores) > 1:
            shown = sorted(scores, key=lambda pair: pair[1])[:20]
            title = "Score by location" + (" (the 20 lowest)" if len(scores) > 20 else "")
            chart = self.bar_chart(shown, WIDTH - 12, title, lambda value: {"pass": PASS, "warn": WARN}.get(self.rating_tone(value), FAIL), maximum=100, mark=mark)
            box = Table([[chart]], colWidths=[WIDTH])
            box.setStyle(TableStyle(self.panel_style(colors.white, 6)))
            self.add(KeepTogether([box]))

    def bar_chart(self, pairs, width, title, colour_for, maximum=None, mark=None):
        """Horizontal bars, one per label, coloured by their value."""
        height = 26 + 13 * len(pairs)
        drawing = Drawing(width, height)
        chart = HorizontalBarChart()
        label_width = min(150, max(60, 4.2 * max(len(str(label)) for label, _ in pairs)))
        chart.x, chart.y, chart.width, chart.height = label_width + 4, 6, width - label_width - 30, 13 * len(pairs)
        chart.data = [[value for _, value in reversed(pairs)]]
        chart.categoryAxis.categoryNames = [str(label)[:32] for label, _ in reversed(pairs)]
        chart.categoryAxis.labels.fontName, chart.categoryAxis.labels.fontSize = self.font, 7
        chart.categoryAxis.labels.fillColor = TEXT
        chart.categoryAxis.strokeColor = BORDER
        chart.valueAxis.valueMin = 0
        chart.valueAxis.valueMax = maximum or max(1, max(value for _, value in pairs))
        chart.valueAxis.valueStep = 25 if maximum == 100 else max(1, round(chart.valueAxis.valueMax / 4))
        chart.valueAxis.labels.fontName, chart.valueAxis.labels.fontSize = self.font, 6.5
        chart.valueAxis.labels.fillColor = MUTED
        chart.valueAxis.strokeColor = BORDER
        chart.valueAxis.visibleGrid = True
        chart.valueAxis.gridStrokeColor = colors.HexColor("#eceff0")
        chart.bars.strokeColor = None
        chart.barWidth = 8
        chart.barLabelFormat = "%d"
        chart.barLabels.fontName, chart.barLabels.fontSize = self.font, 6.5
        chart.barLabels.fillColor = TEXT
        chart.barLabels.boxAnchor = "w"
        chart.barLabels.dx = 3
        for index, (_, value) in enumerate(reversed(pairs)):
            chart.bars[(0, index)].fillColor = colour_for(value)
        drawing.add(chart)
        if mark is not None:
            x = chart.x + chart.width * mark / chart.valueAxis.valueMax
            drawing.add(Line(x, chart.y - 2, x, chart.y + chart.height + 2, strokeColor=MUTED, strokeDashArray=[2, 2], strokeWidth=0.6))
            drawing.add(String(x, chart.y + chart.height + 5, f"pass mark {mark:g}", fontName=self.font, fontSize=6.5, fillColor=MUTED, textAnchor="middle"))
        drawing.add(String(0, height - 9, title, fontName=self.bold, fontSize=7.5, fillColor=MUTED))
        return drawing

    def asset_register(self, parts, assets):
        """Every asset inspected, by location, with its register details and how it did."""
        self.heading("Assets inspected")
        for part in parts:
            grouped = {}
            for item in part["items"]:
                grouped.setdefault(item.get("equipmentId") or item.get("section") or "Item", []).append(item)
            rows = [["Asset", "Code", "Type", "Brand / model", "Serial", "Installed", "Status", "Result"]]
            markup = {}
            for index, checks in enumerate(grouped.values(), 1):
                first = checks[0]
                asset = (assets or {}).get(str(first.get("equipmentId") or ""), {})
                failed = sum(not check.get("passed") and not check.get("notApplicable") for check in checks)
                result = f"{failed} failed" if failed else f"{len(checks)}/{len(checks)} passed"
                rows.append([first.get("section") or asset.get("name") or "Item", asset.get("code") or "", asset.get("type") or "",
                             " ".join(value for value in (asset.get("brand"), asset.get("model")) if value), asset.get("serial_number") or "",
                             asset.get("installation_date") or "", asset.get("status") or "", result])
                markup[(index, 7)] = self.pill(result, "fail" if failed else "pass")
            count = len(rows) - 1
            label = self.rich(f'<b>{escape(part["location"])}</b> <font color="{hex_of(MUTED)}" size="7.5">· {count} asset{"s" if count != 1 else ""} · {part["summary"]["score"]}/100</font>', "Heading3", part["location"])
            self.add(CondPageBreak(60), label, Spacer(1, 3))
            self.table(rows, [100, 84, 62, 70, 70, 50, 60, 62], markup=markup, style="Caption")
            self.add(Spacer(1, 7))

    def findings(self, findings):
        """Each finding as a card: reference and status, where and who, then what was found."""
        self.heading("Findings")
        if not findings:
            self.text("No findings.", "Caption")
            return
        for finding in findings:
            status = finding.get("status") or "Open"
            priority = (finding.get("priority_classification") or "") == "Priority" or finding.get("priority") in {"High", "Priority"}
            head = f"<b>{escape(str(finding.get('finding_ref') or 'Finding'))}</b> &nbsp;{self.pill(status, 'pass' if status == 'Closed' else 'warn')}"
            meta = " · ".join(str(value) for value in (finding.get("item_name"), finding.get("location"), finding.get("category"),
                                                       finding.get("priority"), finding.get("assigned_department"),
                                                       f"PIC {finding.get('pic') or 'unassigned'}", f"Due {finding.get('due_date') or 'not set'}") if value)
            rows = [[self.rich(head, plain=str(finding.get("finding_ref"))), ""], [self.paragraph(meta, "Caption"), ""]]
            for key, label in (("comment", "Finding"), ("cause", "Cause"), ("recommendation", "Recommendation"), ("required_action", "Required action")):
                if finding.get(key):
                    rows.append([self.paragraph(label, "Label"), self.paragraph(finding[key])])
            if finding.get("closed_at"):
                rows.append([self.paragraph("Closed", "Label"), self.paragraph(finding["closed_at"])])
            card = Table(rows, colWidths=[85, WIDTH - 85])
            card.setStyle(TableStyle(self.panel_style(colors.white, 6) + [
                ("SPAN", (0, 0), (-1, 0)), ("SPAN", (0, 1), (-1, 1)), ("TOPPADDING", (0, 1), (-1, -1), 1), ("BOTTOMPADDING", (0, 0), (-1, -2), 1),
                ("LINEBEFORE", (0, 0), (0, -1), 2.5, FAIL if priority else WARN)]))
            self.add(KeepTogether([card]), Spacer(1, 5))

    def signatures(self, session):
        """The three signatures side by side."""
        self.heading("Signatures")
        cells = []
        width = (WIDTH - 12) / 3
        for key, label in SIGNATURES:
            signature = (session.get("signatures") or {}).get(key) or {}
            image = self.photo(signature, False, width - 16, 42) if signature else None
            signed = str(signature.get("signedAt") or "")[:10]
            cells.append([self.paragraph(label.upper(), "Label"), image or self.paragraph("Not signed", "Caption"),
                          self.paragraph(signature.get("name") or "—", "Heading3"), self.paragraph(signed and f"Signed {signed}", "Caption")])
        inner = []
        for cell in cells:
            box = Table([[part] for part in cell], colWidths=[width])
            box.setStyle(TableStyle(self.panel_style(colors.white, 7) + [("TOPPADDING", (0, 0), (-1, -1), 2), ("BOTTOMPADDING", (0, 0), (-1, -1), 2)]))
            inner.append(box)
        row = Table([inner], colWidths=[width + 6, width + 6, width])
        row.setStyle(TableStyle([("LEFTPADDING", (0, 0), (-1, -1), 0), ("RIGHTPADDING", (0, 0), (-1, -1), 6), ("VALIGN", (0, 0), (-1, -1), "TOP")]))
        self.add(KeepTogether([row]))

    def build(self, footer_text, title):
        output = BytesIO()
        company = self.brand.get("companyName", "Ottotree")

        def footer(canvas, document):
            canvas.saveState()
            canvas.setStrokeColor(BORDER)
            canvas.setLineWidth(0.6)
            canvas.line(MARGIN, 30, A4[0] - MARGIN, 30)
            canvas.setFillColor(MUTED)
            canvas.setFont(self.font, 7)
            canvas.drawString(MARGIN, 20, footer_text)
            canvas.drawCentredString(A4[0] / 2, 20, company)
            canvas.drawRightString(A4[0] - MARGIN, 20, f"Page {document.page}")
            canvas.restoreState()

        document = SimpleDocTemplate(output, pagesize=A4, rightMargin=MARGIN, leftMargin=MARGIN, topMargin=MARGIN, bottomMargin=44,
                                     title=title, author=company)
        document.build(self.story, onFirstPage=footer, onLaterPages=footer)
        return output.getvalue()


def build_report(session, brand, summary, media, settings=None, parts=None, assets=None):
    """One audit overall: details, score, charts, grading by location, every asset inspected with
    its register details, findings, and signatures."""
    writer = ReportWriter(brand, media, settings)
    writer.letterhead("Facilities Audit Report")
    writer.audit_details(session)
    findings = session.get("findings") or []
    writer.scorecard(summary, findings)
    parts = parts or []
    writer.charts(summary, parts, findings)
    # One grading row per location, then each asset in a row; the location report has every check.
    writer.location_grading(session)
    if parts:
        writer.asset_register(parts, assets)
    writer.findings(findings)
    writer.add(CondPageBreak(130))
    writer.signatures(session)
    return writer.build(str(session.get("audit_ref") or "Draft audit"), f"Audit {session.get('audit_ref') or session.get('id')}")


def location_banner(writer, text):
    """A location's section starts with a solid accent banner, so each stands apart."""
    banner = Table([[writer.rich(f'<font color="white"><b>{escape(text)}</b></font>', "Heading2", text)]], colWidths=[WIDTH])
    banner.setStyle(TableStyle([("BACKGROUND", (0, 0), (-1, -1), writer.accent), ("ROUNDEDCORNERS", [4, 4, 4, 4]),
                                ("LEFTPADDING", (0, 0), (-1, -1), 8), ("TOPPADDING", (0, 0), (-1, -1), 2), ("BOTTOMPADDING", (0, 0), (-1, -1), 3)]))
    writer.add(banner, Spacer(1, 6))


def build_locations_report(session, parts, brand, media, settings=None):
    """One audit, limited to the chosen locations, each in full: its score, every check and
    remark, photos, and findings."""
    writer = ReportWriter(brand, media, settings)
    writer.letterhead("Audit Report — Selected Locations")
    writer.audit_details(session)
    rows = [["Location", "Checks", "Failed", "Score", "Findings"]]
    for part in parts:
        summary = part["summary"]
        rows.append([part["location"], summary["total"], summary["failed"], f"{summary['score']}/100 · {summary['rating']}", len(part["findings"])])
    writer.heading("Locations in this report")
    writer.table(rows, [200, 60, 60, 130, 70])
    for index, part in enumerate(parts):
        writer.add(PageBreak() if index == 0 else CondPageBreak(260))
        location_banner(writer, f"Location: {part['location']}")
        writer.scorecard(part["summary"], part["findings"])
        writer.checklist(part["items"], with_photos=True, assets=part.get("assets"))
        writer.findings(part["findings"])
    writer.add(CondPageBreak(130))
    writer.signatures(session)
    reference = session.get("audit_ref") or "Draft audit"
    return writer.build(f"{reference} · {', '.join(part['location'] for part in parts)}"[:90], f"Audit {reference} locations")


def build_location_report(outlet, location, period, audits, brand, media, settings=None):
    """Every audit of one location: an overview, then each audit's checks, photos, and findings there."""
    writer = ReportWriter(brand, media, settings)
    writer.letterhead("Location Audit Report")
    writer.add(writer.rich(f"<b>{escape(outlet)} · {escape(location)}</b>", "Heading2", outlet + location), writer.paragraph(f"Period: {period}", "Caption"))
    rows = [["Date", "Audit", "Auditor", "Checks", "Failed", "Score", "Findings"]]
    for audit in audits:
        summary = audit["summary"]
        rows.append([audit["session"].get("audit_date"), audit["session"].get("audit_ref") or "", audit["session"].get("auditor") or "",
                     summary["total"], summary["failed"], f"{summary['score']}/100 · {summary['rating']}", len(audit["findings"])])
    writer.heading("Audits of this location")
    if len(rows) == 1:
        writer.text("No completed audit covered this location in this period.", "Caption")
    else:
        writer.table(rows, [62, 92, 92, 45, 42, 90, 57])
    for audit in audits:
        writer.add(PageBreak())
        location_banner(writer, f"{audit['session'].get('audit_ref') or 'Audit'} · {audit['session'].get('audit_date') or ''}")
        writer.audit_details(audit["session"], location)
        writer.scorecard(audit["summary"], audit["findings"])
        writer.checklist(audit["items"], with_photos=True, assets=audit.get("assets"))
        writer.findings(audit["findings"])
        writer.add(CondPageBreak(130))
        writer.signatures(audit["session"])
    return writer.build(f"{outlet} · {location}", f"Location report {outlet} {location}")


def build_summary_report(data, findings, scope, brand, settings=None):
    """The Reports page on paper, in its order and colours: the four rings, audits and findings by
    month, outlets, breakdowns, time to act, people, then the findings. The activity log is its own
    page and is not part of the report."""
    from backend import report_figures as figures
    from backend.activity import duration_text
    writer = ReportWriter(brand, None, settings)
    scoring = data.get("scoring") or {"passMark": 70, "goodBand": 70, "belowBand": 60}
    charts = data.get("charts") or {}
    accent_hex = writer.accent.hexval()[2:].upper()
    colour = lambda value: colors.HexColor(f"#{value or accent_hex}")
    writer.letterhead("Audit Report")
    writer.add(writer.paragraph(scope, "Heading2"), Spacer(1, 4))

    # The four rings.
    cells = []
    for ring in figures.overview(data):
        total = sum(value for _, value, _ in ring["parts"])
        ring_width = (WIDTH - 24) / 4 - 14
        legend = Table([[writer.rich(f'<font color="{hex_of(colour(code))}">●</font> {escape(label)}', "Caption", label), writer.paragraph(value, "Right"),
                         writer.paragraph(f"{round(value * 100 / total) if total else 0}%", "RightCaption")] for label, value, code in ring["parts"]],
                       colWidths=[ring_width - 52, 22, 30])
        legend.setStyle(TableStyle([("LEFTPADDING", (0, 0), (-1, -1), 0), ("RIGHTPADDING", (0, 0), (-1, -1), 0),
                                    ("TOPPADDING", (0, 0), (-1, -1), 1), ("BOTTOMPADDING", (0, 0), (-1, -1), 1)]))
        cell = [writer.paragraph(ring["title"], "Heading3"), writer.paragraph(ring["note"], "Caption"), Spacer(1, 4),
                ring_drawing(writer, [(value, colour(code)) for _, value, code in ring["parts"]], ring["centre"], ring["caption"]), Spacer(1, 4), legend]
        if ring["footer"]:
            cell.append(writer.paragraph(ring["footer"], "Caption"))
        cells.append(cell)
    writer.add(card_grid(writer, cells, 4))

    # Trends.
    monthly = figures.monthly_audits(charts)
    writer.heading("Audits by month")
    writer.text("Completed audits each month, and their average score against the pass mark.", "Caption")
    writer.add(Spacer(1, 4), column_drawing(writer, [(month, [count]) for month, count, _ in monthly], [writer.accent], ["Audits"],
                                           line=[score for *_, score in monthly], mark=scoring.get("passMark")) if monthly else writer.paragraph("No data yet", "Caption"))
    found = figures.monthly_findings(charts)
    writer.heading("Findings by month")
    writer.text("Priority and non-priority findings raised each month.", "Caption")
    writer.add(Spacer(1, 4), column_drawing(writer, [(month, [priority, other]) for month, priority, other in found], [colour(figures.FAIL), colour(figures.INFO)],
                                           ["Priority", "Non-priority"]) if found else writer.paragraph("No data yet", "Caption"))

    # Outlets.
    outlets = figures.outlet_scores(data)
    bands = figures.performance_bands(charts)
    total_bands = sum(count for _, count, _ in bands)
    pairs = figures.comparison(charts)
    half = (WIDTH - 8) / 2
    latest = hbar_drawing(writer, [name for name, *_ in outlets], [("Latest", [score for _, score, *_ in outlets], [colour(figures.score_colour(score, scoring)) for _, score, *_ in outlets])],
                          half - 14, maximum=100, mark=scoring.get("passMark")) if outlets else writer.paragraph("No completed audits", "Caption")
    distribution = [ring_drawing(writer, [(count, colour(code)) for _, count, code in bands], total_bands, "audits"),
                    writer.paragraph(" · ".join(f"{label} {count}" for label, count, _ in bands), "Caption")] if total_bands else writer.paragraph("No completed audits", "Caption")
    compared = hbar_drawing(writer, [name for name, *_ in pairs], [("Previous", [value or 0 for _, value, _ in pairs], colour(figures.MUTED)),
                                                                   ("Current", [value or 0 for *_, value in pairs], writer.accent)],
                            half - 14, maximum=100, legend=True) if pairs else writer.paragraph("No outlet has two audits yet", "Caption")
    writer.heading("Outlets")
    writer.add(card_grid(writer, [
        [writer.paragraph("Latest Scores by Outlet", "Heading3"), writer.paragraph("Each outlet's latest audit score, coloured by grade", "Caption"), Spacer(1, 4), latest],
        [writer.paragraph("Performance Distribution", "Heading3"), writer.paragraph(f"{total_bands} completed audits", "Caption"), Spacer(1, 4),
         *(distribution if isinstance(distribution, list) else [distribution])],
        [writer.paragraph("Previous vs Current Audit", "Heading3"), writer.paragraph("Each outlet's last two audit scores", "Caption"), Spacer(1, 4), compared],
        [writer.paragraph("Outlet Rankings", "Heading3"), Spacer(1, 4),
         writer.table_flowable([["#", "Outlet", "Latest", "Average", "Audits"]] + [[index, name, score, average, count] for index, (name, score, average, count, _) in enumerate(outlets, 1)],
                               [20, 70, 45, 45, 40], markup={(index, 2): writer.pill(score, writer.rating_tone(score)) for index, (_, score, *_) in enumerate(outlets, 1)},
                               style="Caption", width=half - 14)
         if outlets else writer.paragraph("No completed audits", "Caption")],
    ], 2))
    writer.heading("Critical issues")
    if data.get("criticalIssues"):
        writer.table([["Work order", "Outlet", "Location", "Title", "Status", "Due"]]
                     + [[row.get("work_order_ref") or row["id"], row["outlet"], row.get("zone") or "", row.get("title") or "",
                         row["status"], row.get("due_date") or ""] for row in data["criticalIssues"]], [80, 55, 70, 155, 60, 60], style="Caption")
    else:
        writer.text("No open high-priority work orders.", "Caption")

    # Breakdowns, two to a row.
    writer.heading("Breakdowns")
    cells = []
    for title, note, rows, maximum, suffix in figures.breakdowns(charts, scoring):
        chart = hbar_drawing(writer, [label for label, *_ in rows[:12]], [("", [value for _, value, *_ in rows[:12]], [colour(code) for _, _, code, _ in rows[:12]])],
                             half - 14, maximum=maximum, suffix=suffix,
                             mark=scoring.get("passMark") if suffix == "/100" else None) if rows else writer.paragraph("No data yet", "Caption")
        cells.append([writer.paragraph(title, "Heading3"), writer.paragraph(note + (" (the first 12)" if len(rows) > 12 else ""), "Caption"), Spacer(1, 4), chart])
    writer.add(card_grid(writer, cells, 2))

    # Time to act and people.
    writer.heading("Time to act")
    writer.text("Average time each step took in this period; the faint bar behind is the longest.", "Caption")
    steps = [row for row in figures.time_to_act(data) if row[3]]
    writer.add(Spacer(1, 4))
    if steps:
        writer.add(hbar_drawing(writer, [label for label, *_ in steps], [("Longest", [longest for _, _, longest, _ in steps], writer.soft),
                                                                         ("Average", [average for _, average, _, _ in steps], writer.accent)],
                                WIDTH, overlap=True, label_format=duration_text))
    writer.table([["Step", "Average", "Longest", "Times"]] + [[label, duration_text(average), duration_text(longest), count]
                                                             for label, average, longest, count in figures.time_to_act(data)], [260, 90, 90, 50], style="Caption")
    writer.heading("People")
    writer.text("Who completed audits, acted on work requests, and closed work orders.", "Caption")
    workers = figures.people(data)
    stack_colours = [writer.accent, colour(figures.INFO), colour(figures.PASS)]
    if workers:
        writer.add(Spacer(1, 4), hbar_drawing(writer, [name for name, *_ in workers],
                                              [(label, [row[index + 1] for row in workers], stack_colours[index]) for index, (label, _) in enumerate(figures.PEOPLE_STACKS)],
                                              WIDTH, stacked=True, legend=True))
    header, rows = figures.people_table(data)
    if rows:
        writer.add(Spacer(1, 6))
        writer.table([header] + rows, [58, 40, 46, 44, 34, 40, 42, 44, 44, 42, 40, 49], style="Caption")
    else:
        writer.text("No activity in this period.", "Caption")

    # The lists.
    writer.heading("Findings")
    if findings:
        writer.table([["Finding", "Audit", "Outlet", "Location", "Item / check", "Priority", "Status"]]
                     + [[row.get("finding_ref") or "", row.get("audit_ref") or "", row.get("outlet") or "", row.get("location") or "",
                         " · ".join(value for value in (row.get("item_name"), row.get("criterion")) if value) or row.get("comment") or "",
                         row.get("priority") or "", row.get("status") or ""] for row in findings], [58, 118, 36, 50, 121, 40, 50], style="Caption",
                     markup={(index, 6): writer.pill(row.get("status") or "", "pass" if row.get("status") == "Closed" else "warn") for index, row in enumerate(findings, 1)})
    else:
        writer.text("No findings in this period.", "Caption")
    return writer.build(scope, "Audit report")


def card_grid(writer, cells, columns):
    """Cards side by side, as the page's chart grid; each card is a list of flowables."""
    gap = 8
    width = (WIDTH - gap * (columns - 1)) / columns
    rows = [cells[index:index + columns] for index in range(0, len(cells), columns)]
    rows[-1] += [""] * (columns - len(rows[-1]))
    table = Table([sum(([cell, ""] for cell in row), [])[:-1] for row in rows], colWidths=([width, gap] * columns)[:-1])
    commands = [("VALIGN", (0, 0), (-1, -1), "TOP"), ("LEFTPADDING", (0, 0), (-1, -1), 7), ("RIGHTPADDING", (0, 0), (-1, -1), 7),
                ("TOPPADDING", (0, 0), (-1, -1), 7), ("BOTTOMPADDING", (0, 0), (-1, -1), 8)]
    for row_index, row in enumerate(rows):
        for index, cell in enumerate(row):
            if cell != "":
                commands.append(("BOX", (index * 2, row_index), (index * 2, row_index), 0.6, BORDER))
        if row_index:
            commands.append(("TOPPADDING", (0, row_index), (-1, row_index), 9))
    table.setStyle(TableStyle(commands))
    return table


def ring_drawing(writer, parts, centre, caption, size=96):
    """A ring of parts (value, colour) with a figure in its middle, as the page draws them."""
    drawing = Drawing(size, size)
    pie = Pie()
    pie.x = pie.y = 2
    pie.width = pie.height = size - 4
    values = [value for value, _ in parts]
    pie.data = values if any(values) else [1]
    pie.innerRadiusFraction = 0.66
    pie.slices.strokeColor = colors.white
    pie.slices.strokeWidth = 1
    for index, (_, colour) in enumerate(parts if any(values) else [(1, NEUTRAL_SOFT)]):
        pie.slices[index].fillColor = colour
    drawing.add(pie)
    drawing.add(String(size / 2, size / 2 - 1, str(centre), fontName=writer.bold, fontSize=15, fillColor=TEXT, textAnchor="middle"))
    drawing.add(String(size / 2, size / 2 - 12, caption, fontName=writer.font, fontSize=7, fillColor=MUTED, textAnchor="middle"))
    holder = Table([[drawing]], colWidths=[None])
    holder.setStyle(TableStyle([("ALIGN", (0, 0), (-1, -1), "CENTER"), ("LEFTPADDING", (0, 0), (-1, -1), 0), ("RIGHTPADDING", (0, 0), (-1, -1), 0)]))
    return holder


def column_drawing(writer, points, colours, names, line=None, mark=None, width=WIDTH, height=170):
    """Columns per label (stacked when several), with an optional 0-100 line such as the score."""
    drawing = Drawing(width, height)
    left, right, top, bottom = 26, 30 if line else 8, 16, 34
    plot_width, plot_height = width - left - right, height - top - bottom
    totals = [sum(values) for _, values in points]
    maximum = max([1] + totals)
    step = plot_width / max(1, len(points))
    bar = min(40, step * 0.6)
    y_of = lambda value: bottom + value * plot_height / maximum
    for tick in sorted({0, (maximum + 1) // 2, maximum}):
        drawing.add(Line(left, y_of(tick), width - right, y_of(tick), strokeColor=NEUTRAL_SOFT, strokeWidth=0.6))
        drawing.add(String(left - 4, y_of(tick) - 2.5, str(tick), fontName=writer.font, fontSize=6.5, fillColor=MUTED, textAnchor="end"))
    for index, (label, values) in enumerate(points):
        x = left + step * index + (step - bar) / 2
        base = bottom
        for value, colour in zip(values, colours):
            if value:
                drawing.add(Rect(x, base, bar, value * plot_height / maximum, fillColor=colour, strokeColor=None))
                base += value * plot_height / maximum
        drawing.add(String(x + bar / 2, base + 3, str(totals[index]), fontName=writer.bold, fontSize=7, fillColor=TEXT, textAnchor="middle"))
        drawing.add(String(x + bar / 2, bottom - 10, str(label), fontName=writer.font, fontSize=6.5, fillColor=MUTED, textAnchor="middle"))
    if line:
        line_y = lambda value: bottom + value * plot_height / 100
        coords = [(left + step * index + step / 2, line_y(value)) for index, value in enumerate(line)]
        if mark is not None:
            drawing.add(Line(left, line_y(mark), width - right, line_y(mark), strokeColor=MUTED, strokeDashArray=[3, 3], strokeWidth=0.6))
        for (x1, y1), (x2, y2) in zip(coords, coords[1:]):
            drawing.add(Line(x1, y1, x2, y2, strokeColor=TEXT, strokeWidth=1.4))
        for (x, y), value in zip(coords, line):
            tone = PASS if value >= 70 else WARN if value >= 55 else FAIL
            if mark is not None:
                tone = PASS if value >= mark else WARN if value >= mark - 15 else FAIL
            drawing.add(Circle(x, y, 3, fillColor=tone, strokeColor=colors.white, strokeWidth=1))
            drawing.add(String(x, y + 5, str(value), fontName=writer.bold, fontSize=6.5, fillColor=TEXT, textAnchor="middle"))
        for tick in (0, 50, 100):
            drawing.add(String(width - right + 4, line_y(tick) - 2.5, str(tick), fontName=writer.font, fontSize=6.5, fillColor=MUTED))
    # The legend, under the chart.
    x = left
    entries = list(zip(names, colours)) + ([("Average score (0–100)", TEXT)] if line else []) + ([(f"Pass mark {mark:g}", MUTED)] if line and mark is not None else [])
    for name, colour in entries:
        drawing.add(Rect(x, 4, 7, 7, fillColor=colour, strokeColor=None))
        drawing.add(String(x + 10, 4.5, name, fontName=writer.font, fontSize=7, fillColor=TEXT))
        x += 16 + 4.1 * len(name)
    return drawing


def hbar_drawing(writer, labels, series, width, maximum=None, mark=None, stacked=False, overlap=False, legend=False, suffix="", label_format=None):
    """Horizontal bars per label: one series (each bar its own colour), several side by side,
    stacked, or overlapping (the longest behind the average)."""
    count = len(labels)
    per = 12 if (stacked or overlap or len(series) == 1) else 7 * len(series) + 4
    height = 18 + per * count + (14 if legend else 0)
    drawing = Drawing(width, height)
    chart = HorizontalBarChart()
    label_width = min(width * 0.42, max(50, 4.1 * max([len(str(label)) for label in labels] + [4])))
    chart.x, chart.y = label_width + 4, 12 + (14 if legend else 0)
    chart.width, chart.height = width - label_width - 34, per * count
    chart.data = [list(reversed(values)) for _, values, _ in series]
    chart.categoryAxis.categoryNames = [str(label)[:int(label_width / 3.6)] for label in reversed(labels)]
    chart.categoryAxis.labels.fontName, chart.categoryAxis.labels.fontSize = writer.font, 7
    chart.categoryAxis.labels.fillColor = TEXT
    chart.categoryAxis.strokeColor = BORDER
    if stacked:
        chart.categoryAxis.style = "stacked"
    peak = max([1] + ([sum(values) for values in zip(*[values for _, values, _ in series])] if stacked else [value for _, values, _ in series for value in values]))
    chart.valueAxis.valueMin = 0
    chart.valueAxis.valueMax = maximum or peak
    chart.valueAxis.valueStep = 25 if maximum == 100 else max(1, round(chart.valueAxis.valueMax / 4))
    chart.valueAxis.visible = not label_format
    chart.valueAxis.labels.fontName, chart.valueAxis.labels.fontSize = writer.font, 6.5
    chart.valueAxis.labels.fillColor = MUTED
    chart.valueAxis.strokeColor = BORDER
    chart.valueAxis.visibleGrid = True
    chart.valueAxis.gridStrokeColor = NEUTRAL_SOFT
    chart.bars.strokeColor = None
    chart.groupSpacing = 3
    chart.barWidth = 8
    if overlap:
        # The average as the bar; the longest is drawn faint behind it below.
        chart.data = [list(reversed(series[1][1]))]
        chart.valueAxis.valueMax = maximum or max([1] + list(series[0][1]) + list(series[1][1]))
    for index, (_, values, colour) in enumerate(series if not overlap else series[1:]):
        target = index
        if isinstance(colour, list):
            for bar_index, bar_colour in enumerate(reversed(colour)):
                chart.bars[(target, bar_index)].fillColor = bar_colour
        else:
            chart.bars[target].fillColor = colour
    chart.barLabelFormat = (lambda value: label_format(value)) if label_format else (lambda value: f"{value:g}{suffix}")
    chart.barLabels.fontName, chart.barLabels.fontSize = writer.font, 6.5
    chart.barLabels.fillColor = TEXT
    chart.barLabels.boxAnchor = "w"
    chart.barLabels.dx = 3
    if stacked:
        chart.barLabelFormat = None
    if overlap:
        # The longest, faint, under the average.
        longest = series[0][1]
        scale = chart.width / chart.valueAxis.valueMax
        for index, value in enumerate(reversed(longest)):
            y = chart.y + per * index + (per - 8) / 2
            drawing.add(Rect(chart.x, y, value * scale, 8, fillColor=series[0][2], strokeColor=None))
    drawing.add(chart)
    if mark is not None:
        x = chart.x + chart.width * mark / chart.valueAxis.valueMax
        drawing.add(Line(x, chart.y - 2, x, chart.y + chart.height + 2, strokeColor=MUTED, strokeDashArray=[2, 2], strokeWidth=0.6))
        drawing.add(String(x, chart.y + chart.height + 4, f"pass mark {mark:g}", fontName=writer.font, fontSize=6.5, fillColor=MUTED, textAnchor="middle"))
    if stacked:
        totals = [sum(values) for values in zip(*[values for _, values, _ in series])]
        scale = chart.width / chart.valueAxis.valueMax
        for index, total in enumerate(reversed(totals)):
            drawing.add(String(chart.x + total * scale + 3, chart.y + per * index + per / 2 - 2.5, str(total), fontName=writer.bold, fontSize=6.5, fillColor=TEXT))
    if legend:
        x = chart.x
        for name, _, colour in series:
            drawing.add(Rect(x, 2, 7, 7, fillColor=colour if not isinstance(colour, list) else colour[0], strokeColor=None))
            drawing.add(String(x + 10, 2.5, name, fontName=writer.font, fontSize=7, fillColor=TEXT))
            x += 18 + 4.1 * len(name)
    return drawing
