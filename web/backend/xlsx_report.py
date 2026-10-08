"""Excel versions of the audit reports: laid out and coloured like the PDFs (pdf_report.py), with
the same sections, charts, and photos, but in ordinary cells that can be edited.

Every sheet uses eight equal columns (A-H); a table's column may span several of them, which
keeps the sheets aligned like the PDF's page and prints on one A4 width.
"""
from io import BytesIO
from math import ceil

from openpyxl.chart import BarChart, DoughnutChart, Reference
from openpyxl.chart.label import DataLabelList
from openpyxl.chart.series import DataPoint
from openpyxl.drawing.image import Image as SheetImage
from openpyxl.cell.cell import MergedCell
from openpyxl.styles import Alignment, Border, Font, PatternFill, Side
from openpyxl.styles.cell_style import StyleArray
from openpyxl.utils import get_column_letter
from openpyxl.worksheet.merge import MergedCellRange

from backend.config import THEME_PRESET_ACCENTS
from backend.scoring import rating_for_score

FONT = "Noto Sans"
TEXT, MUTED, BORDER = "17222B", "5F6C66", "D3D9D5"
SURFACE_ALT, NEUTRAL_SOFT = "F7F9F8", "ECEFF0"
TONES = {"pass": ("1C8F4B", "E3F4EA"), "fail": ("C9303A", "FDE7E8"), "warn": ("B96100", "FDEFD9"), "neutral": (MUTED, NEUTRAL_SOFT)}
RESULT_TONES = {"Pass": "pass", "Fail": "fail", "N/A": "neutral"}
COLUMNS = 8
COLUMN_WIDTH = 15
SIGNATURES = (("auditedBy", "Audited by"), ("verifiedBy", "Verified by"), ("acknowledgedBy", "Acknowledged by"))


def soft(hex_colour, amount=0.88):
    channels = [int(hex_colour[index:index + 2], 16) for index in (0, 2, 4)]
    return "".join(f"{round(channel + (255 - channel) * amount):02X}" for channel in channels)


def fill(colour):
    return PatternFill("solid", start_color=colour, end_color=colour)


def thin(colour=BORDER):
    return Side(style="thin", color=colour)


def result_of(item):
    return "N/A" if item.get("notApplicable") else "Pass" if item.get("passed") else "Fail"


class SheetWriter:
    """Writes one sheet top to bottom, as ReportWriter writes a PDF."""

    def __init__(self, sheet, brand, media, settings):
        self.sheet, self.brand, self.media, self.settings = sheet, brand, media, settings or {}
        self.row = 1
        theme = brand.get("theme") or {}
        self.accent = (theme.get("accent") or THEME_PRESET_ACCENTS.get(theme.get("preset"), THEME_PRESET_ACCENTS["default"])).lstrip("#").upper()
        self.accent_soft = soft(self.accent)
        self.images = []
        # Large audits write tens of thousands of cells: each style is registered once and reused.
        self.style_ids = {}
        sheet.sheet_view.showGridLines = False
        for column in range(1, COLUMNS + 1):
            sheet.column_dimensions[get_column_letter(column)].width = COLUMN_WIDTH
        sheet.page_setup.paperSize = sheet.PAPERSIZE_A4
        sheet.page_setup.fitToWidth, sheet.page_setup.fitToHeight = 1, 0
        sheet.sheet_properties.pageSetUpPr.fitToPage = True
        sheet.page_margins.left = sheet.page_margins.right = 0.5

    # ---- Cells ----

    def style_id(self, kind, *key):
        """The workbook's index of a font, fill, border, or alignment, registered once."""
        if (kind, *key) not in self.style_ids:
            workbook = self.sheet.parent
            if kind == "font":
                size, bold, colour = key
                index = workbook._fonts.add(Font(name=FONT, size=size, bold=bold, color=colour))
            elif kind == "fill":
                index = workbook._fills.add(fill(key[0]))
            elif kind == "alignment":
                align, valign, wrap = key
                index = workbook._alignments.add(Alignment(horizontal=align, vertical=valign, wrap_text=wrap))
            else:
                (left, right, top, bottom), colour = key
                index = workbook._borders.add(Border(left=thin(colour) if left else None, right=thin(colour) if right else None,
                                                     top=thin(colour) if top else None, bottom=thin(colour) if bottom else None))
            self.style_ids[(kind, *key)] = index
        return self.style_ids[(kind, *key)]

    def styled(self, cell):
        if not cell.has_style:
            cell._style = StyleArray()
        return cell._style

    def put(self, row, column, value, size=9, bold=False, colour=TEXT, background=None, align="left", wrap=True, span=1, valign="top"):
        """A value in a cell (merged across span columns), styled like the PDF."""
        cell = self.sheet.cell(row=row, column=column, value=value)
        if isinstance(value, str) and value.startswith(("=", "+", "-", "@")):
            cell.data_type = "s"
        style = self.styled(cell)
        style.fontId = self.style_id("font", size, bold, colour)
        style.alignmentId = self.style_id("alignment", align, valign, wrap)
        if span > 1:
            self.merge(row, column, column + span - 1)
        if background:
            fill_id = self.style_id("fill", background)
            for offset in range(span):
                self.styled(self.sheet.cell(row=row, column=column + offset)).fillId = fill_id
        return cell

    def merge(self, row, first, last):
        """Merged cells, added directly: openpyxl's merge_cells checks every earlier merge, which
        is too slow for an audit with thousands of checks."""
        self.sheet.merged_cells.ranges.add(MergedCellRange(self.sheet, f"{get_column_letter(first)}{row}:{get_column_letter(last)}{row}"))
        for column in range(first + 1, last + 1):
            self.sheet._cells[row, column] = MergedCell(self.sheet, row, column)

    def box(self, top, left, bottom, right, colour=BORDER, inner_rows=False):
        """A border round a block, optionally with a rule under each row inside, as the app's panels and lists."""
        for row in range(top, bottom + 1):
            for column in range(left, right + 1):
                sides = (column == left, column == right, row == top, row == bottom or inner_rows)
                self.styled(self.sheet.cell(row=row, column=column)).borderId = self.style_id("border", sides, colour)

    def height(self, row, texts_and_widths, size=9):
        """Rows grow with their wrapped text, as the PDF's do (Excel does not measure on its own)."""
        lines = max([1] + [ceil(len(str(text or "")) * size / 7.5 / max(1, width * COLUMN_WIDTH * 1.05)) + str(text or "").count("\n")
                           for text, width in texts_and_widths])
        self.sheet.row_dimensions[row].height = max(15, lines * size * 1.45 + 4)

    def pill(self, row, column, text, tone, span=1, align="center"):
        foreground, background = TONES.get(tone, TONES["neutral"])
        return self.put(row, column, text, size=8, bold=True, colour=foreground, background=background, align=align, span=span, valign="center")

    def space(self, rows=1):
        self.row += rows

    def rating_tone(self, score):
        mark = float(self.settings.get("scoring.passMark", 70) or 70)
        return "pass" if score >= mark else "warn" if score >= mark - 15 else "fail"

    # ---- Sections ----

    def letterhead(self, title, printed):
        row = self.row
        self.put(row, 1, self.brand.get("companyName") or self.brand.get("appTitle") or "", size=14, bold=True, span=5, wrap=False)
        self.put(row, 6, title, size=11, bold=True, colour=self.accent, align="right", span=3, wrap=False)
        self.put(row + 1, 1, self.brand.get("departmentHeader") or "", size=8, colour=MUTED, span=5, wrap=False)
        self.put(row + 1, 6, f"Printed {printed}", size=8, colour=MUTED, align="right", span=3, wrap=False)
        self.sheet.row_dimensions[row].height = 22
        for column in range(1, COLUMNS + 1):
            self.sheet.cell(row=row + 1, column=column).border = Border(bottom=Side(style="medium", color=self.accent))
        self.row = row + 3

    def heading(self, text):
        """The accent bar and bold title of a section."""
        self.space()
        cell = self.put(self.row, 1, text, size=11, bold=True, span=COLUMNS, wrap=False)
        cell.border = Border(left=Side(style="thick", color=self.accent))
        self.sheet.row_dimensions[self.row].height = 19
        self.row += 2

    def banner(self, text):
        self.space()
        self.put(self.row, 1, text, size=11, bold=True, colour="FFFFFF", background=self.accent, span=COLUMNS, wrap=False, valign="center")
        self.sheet.row_dimensions[self.row].height = 21
        self.row += 2

    def audit_details(self, session, location=None):
        status = "Closed" if session.get("closed_at") else session.get("status") or "Draft"
        self.put(self.row, 1, session.get("audit_ref") or "Draft", size=11, bold=True, span=6, wrap=False)
        self.pill(self.row, 8, status, "pass" if status in ("Completed", "Closed") else "warn")
        self.sheet.row_dimensions[self.row].height = 19
        self.row += 1
        fields = [("Outlet", session.get("outlet")), ("Location", location or session.get("zone") or "All Locations"),
                  ("Date", f"{session.get('audit_date') or ''} {session.get('audit_time') or ''}".strip()),
                  ("Auditor", session.get("auditor")), ("Audit type", session.get("audit_type") or "Standard")]
        if session.get("closed_at"):
            from datetime import datetime, timezone
            closed = datetime.fromtimestamp(session["closed_at"] / 1000, timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
            fields.append(("Closed", f"{closed} by {session.get('closed_by') or ''}".strip()))
        if session.get("remarks"):
            fields.append(("Remarks", session["remarks"]))
        top = self.row
        for start in range(0, len(fields), 4):
            for index, (label, value) in enumerate(fields[start:start + 4]):
                self.put(self.row, 1 + index * 2, label.upper(), size=7, colour=MUTED, background=SURFACE_ALT, span=2)
                self.put(self.row + 1, 1 + index * 2, value or "—", background=SURFACE_ALT, span=2)
            for column in range(1, COLUMNS + 1):
                for row in (self.row, self.row + 1):
                    self.sheet.cell(row=row, column=column).fill = fill(SURFACE_ALT)
            self.height(self.row + 1, [(value, 2) for _, value in fields[start:start + 4]])
            self.row += 2
        self.box(top, 1, self.row - 1, COLUMNS)
        self.row += 1

    def scorecard(self, summary, findings):
        closed = sum(row.get("status") in {"Completed", "Verified", "Closed"} for row in findings)
        priority = sum((row.get("priority_classification") or ("Priority" if row.get("priority") in {"High", "Priority"} else "Non-Priority")) == "Priority"
                       for row in findings)
        tiles = [("Score", summary["score"], TONES["pass" if summary.get("meetsPassMark") else "fail"][0], f"{summary['rating']} · pass mark {summary['passMark']:g}"),
                 ("Checks", summary["total"], TEXT, f"{summary['passed']} passed · {summary['failed']} failed · {summary['notApplicable']} N/A"),
                 ("Findings", len(findings), TEXT, f"{priority} priority · {len(findings) - priority} non-priority"),
                 ("Open follow-up", len(findings) - closed, TEXT, f"{closed} closed")]
        for index, (label, value, colour, note) in enumerate(tiles):
            column = 1 + index * 2
            self.put(self.row, column, label.upper(), size=7, colour=MUTED, span=2)
            self.put(self.row + 1, column, value, size=16, bold=True, colour=colour, span=2, align="left", valign="center")
            self.put(self.row + 2, column, note, size=8, colour=MUTED, span=2)
            self.box(self.row, column, self.row + 2, column + 1)
        self.sheet.row_dimensions[self.row + 1].height = 24
        self.height(self.row + 2, [(note, 2) for *_, note in tiles], size=8)
        self.row += 4

    def table(self, headers, rows, spans, result_column=None, tones=None, size=9):
        """A list like the app's: soft header, rules between rows, results coloured. spans gives each
        column's width in sheet columns; tones colours chosen cells {(row index, column): tone}."""
        starts = [1 + sum(spans[:index]) for index in range(len(spans))]
        top = self.row
        for index, header in enumerate(headers):
            self.put(self.row, starts[index], header, size=8, bold=True, colour=MUTED, background=self.accent_soft, span=spans[index], wrap=False)
        self.row += 1
        for row_index, values in enumerate(rows):
            for index, value in enumerate(values):
                tone = (tones or {}).get((row_index, index)) or (RESULT_TONES.get(str(value)) if index == result_column else None)
                if tone:
                    self.pill(self.row, starts[index], value, tone, span=spans[index], align="left")
                else:
                    self.put(self.row, starts[index], value, size=size, span=spans[index])
            self.height(self.row, [(value, spans[index]) for index, value in enumerate(values)], size)
            self.row += 1
        self.box(top, 1, self.row - 1, sum(spans), inner_rows=True)
        return top

    def photos(self, values, columns=4, label=None):
        """Photos side by side, the marked copy beside its original; returns how many were placed."""
        placed = []
        for value in values or []:
            if not isinstance(value, dict):
                continue
            for marked in (False, True):
                content = self.media.image_bytes(value, marked) if self.media else None
                if content:
                    placed.append((content, "Marked" if marked else label or value.get("name") or "Photo"))
        if not placed:
            return 0
        from PIL import Image as PILImage
        span = COLUMNS // columns
        for start in range(0, len(placed), columns):
            self.sheet.row_dimensions[self.row].height = 82
            for index, (content, caption) in enumerate(placed[start:start + columns]):
                try:
                    picture = PILImage.open(BytesIO(content))
                    picture.thumbnail((int(span * COLUMN_WIDTH * 7) - 6, 106))
                    buffer = BytesIO()
                    picture.convert("RGB").save(buffer, "JPEG", quality=82)
                except Exception:
                    continue
                buffer.seek(0)
                image = SheetImage(buffer)
                self.images.append(buffer)
                self.sheet.add_image(image, f"{get_column_letter(1 + index * span)}{self.row}")
                self.put(self.row + 1, 1 + index * span, caption, size=7.5, colour=MUTED, span=span)
            self.row += 2
        return len(placed)

    def checklist(self, items, assets):
        """Each asset: a bar with its details and how it did, its checks, then its photos."""
        from backend.pdf_report import asset_details_line
        grouped = {}
        for item in items:
            grouped.setdefault(item.get("equipmentId") or item.get("section") or "Item", []).append(item)
        self.heading("Checklist")
        for checks in grouped.values():
            first = checks[0]
            asset = (assets or {}).get(str(first.get("equipmentId") or ""), {})
            failed = sum(result_of(check) == "Fail" for check in checks)
            name = first.get("section") or asset.get("name") or "Item"
            self.put(self.row, 1, name, size=9.5, bold=True, background=SURFACE_ALT, span=6, wrap=False)
            self.put(self.row, 7, f"{len(checks)} check{'s' if len(checks) != 1 else ''}", size=8, colour=MUTED, background=SURFACE_ALT, align="right")
            self.pill(self.row, 8, f"{failed} failed" if failed else "All passed", "fail" if failed else "pass")
            details = asset_details_line(asset, first.get("category"))
            self.put(self.row + 1, 1, details, size=8, colour=MUTED, background=SURFACE_ALT, span=COLUMNS)
            self.height(self.row + 1, [(details, COLUMNS)], 8)
            self.box(self.row, 1, self.row + 1, COLUMNS)
            self.row += 2
            self.table(["Check", "Result", "Remark"], [[check.get("item") or "", result_of(check), check.get("notes") or ""] for check in checks],
                       [4, 1, 3], result_column=1)
            seen, photos = set(), []
            for check in checks:
                for image in check.get("images") or []:
                    if isinstance(image, dict) and image.get("url") not in seen:
                        seen.add(image.get("url"))
                        photos.append(image)
            if not self.photos(photos):
                if not self.photos([image for image in asset.get("photos") or [] if isinstance(image, dict)][:4], label="Registered photo"):
                    self.put(self.row, 1, "No photo.", size=8, colour=MUTED, span=COLUMNS)
                    self.row += 1
            self.row += 1

    def findings(self, findings):
        """Each finding as a card: reference and status, where and who, then what was found."""
        self.heading("Findings")
        if not findings:
            self.put(self.row, 1, "No findings.", size=8, colour=MUTED, span=COLUMNS)
            self.row += 2
            return
        for finding in findings:
            status = finding.get("status") or "Open"
            priority = (finding.get("priority_classification") or "") == "Priority" or finding.get("priority") in {"High", "Priority"}
            top = self.row
            self.put(self.row, 1, finding.get("finding_ref") or "Finding", size=9.5, bold=True, span=6, wrap=False)
            self.pill(self.row, 8, status, "pass" if status == "Closed" else "warn")
            self.row += 1
            meta = " · ".join(str(value) for value in (finding.get("item_name"), finding.get("location"), finding.get("category"), finding.get("priority"),
                                                       finding.get("assigned_department"), f"PIC {finding.get('pic') or 'unassigned'}",
                                                       f"Due {finding.get('due_date') or 'not set'}") if value)
            self.put(self.row, 1, meta, size=8, colour=MUTED, span=COLUMNS)
            self.height(self.row, [(meta, COLUMNS)], 8)
            self.row += 1
            for key, label in (("comment", "Finding"), ("cause", "Cause"), ("recommendation", "Recommendation"),
                               ("required_action", "Required action"), ("closed_at", "Closed")):
                if finding.get(key):
                    self.put(self.row, 1, label, size=8, colour=MUTED)
                    self.put(self.row, 2, finding[key], span=COLUMNS - 1)
                    self.height(self.row, [(finding[key], COLUMNS - 1)])
                    self.row += 1
            self.box(top, 1, self.row - 1, COLUMNS)
            stripe = TONES["fail" if priority else "warn"][0]
            for row in range(top, self.row):
                cell = self.sheet.cell(row=row, column=1)
                cell.border = Border(left=Side(style="thick", color=stripe), top=cell.border.top, bottom=cell.border.bottom)
            self.row += 1

    def signatures(self, session):
        self.heading("Signatures")
        top = self.row
        for index, (key, label) in enumerate(SIGNATURES):
            signature = (session.get("signatures") or {}).get(key) or {}
            column = 1 + index * 3 if index < 2 else 7
            span = 2 if index < 2 else 2
            self.put(top, column, label.upper(), size=7, colour=MUTED, span=span)
            self.put(top + 2, column, signature.get("name") or "—", size=9.5, bold=True, span=span)
            signed = str(signature.get("signedAt") or "")[:10]
            self.put(top + 3, column, f"Signed {signed}" if signed else "Not signed", size=8, colour=MUTED, span=span)
            content = self.media.image_bytes(signature, False) if self.media and signature else None
            if content:
                from PIL import Image as PILImage
                try:
                    picture = PILImage.open(BytesIO(content))
                    picture.thumbnail((200, 52))
                    buffer = BytesIO()
                    picture.convert("RGB").save(buffer, "PNG")
                    buffer.seek(0)
                    self.images.append(buffer)
                    self.sheet.add_image(SheetImage(buffer), f"{get_column_letter(column)}{top + 1}")
                except Exception:
                    pass
            self.box(top, column, top + 3, column + span - 1)
        self.sheet.row_dimensions[top + 1].height = 44
        self.row = top + 5

    # ---- Charts ----

    def charts(self, summary, parts, findings):
        """The PDF's "At a glance": results of every check, findings by category, score by location.
        Each chart reads a small table above it, so editing the numbers redraws it."""
        self.heading("At a glance")
        top = self.row
        self.table(["Checks", "Count"], [["Passed", summary["passed"]], ["Failed", summary["failed"]], ["N/A", summary["notApplicable"]]], [2, 1],
                   tones={(0, 0): "pass", (1, 0): "fail", (2, 0): "neutral"})
        categories = {}
        for finding in findings:
            categories[finding.get("category") or "No category"] = categories.get(finding.get("category") or "No category", 0) + 1
        pairs = sorted(categories.items(), key=lambda pair: -pair[1])[:8]
        self.row = top
        if pairs:
            for index, header in enumerate(("Findings by category", "Count")):
                self.put(self.row, 5 + index * 3, header, size=8, bold=True, colour=MUTED, background=self.accent_soft, span=3 if not index else 1, wrap=False)
            self.row += 1
            for category, count in pairs:
                self.put(self.row, 5, category, span=3)
                self.put(self.row, 8, count)
                self.row += 1
            self.box(top, 5, self.row - 1, 8, inner_rows=True)
        self.row = max(self.row, top + 4) + 1
        doughnut = DoughnutChart(holeSize=55)
        doughnut.title = "Checks"
        doughnut.add_data(Reference(self.sheet, min_col=3, min_row=top + 1, max_row=top + 3), titles_from_data=False)
        doughnut.set_categories(Reference(self.sheet, min_col=1, min_row=top + 1, max_row=top + 3))
        for index, colour in enumerate((TONES["pass"][0], TONES["fail"][0], "B4BDB8")):
            point = DataPoint(idx=index)
            point.graphicalProperties.solidFill = colour
            point.graphicalProperties.line.solidFill = "FFFFFF"
            doughnut.series[0].dPt.append(point)
        doughnut.height, doughnut.width = 6.2, 7.6
        self.sheet.add_chart(doughnut, f"A{self.row}")
        if pairs:
            bars = self.bar_chart("Findings by category", Reference(self.sheet, min_col=8, min_row=top + 1, max_row=top + len(pairs)),
                                  Reference(self.sheet, min_col=5, min_row=top + 1, max_row=top + len(pairs)), [TONES["fail"][0]] * len(pairs))
            bars.height, bars.width = 6.2, 13.5
            self.sheet.add_chart(bars, f"D{self.row}")
        self.row += 13
        return top

    def bar_chart(self, title, data, labels, colours, maximum=None):
        chart = BarChart()
        chart.type, chart.style, chart.title = "bar", 10, title
        chart.add_data(data, titles_from_data=False)
        chart.set_categories(labels)
        chart.legend = None
        chart.y_axis.scaling.min = 0
        if maximum:
            chart.y_axis.scaling.max = maximum
        chart.y_axis.majorGridlines = None
        chart.x_axis.delete = False
        chart.y_axis.delete = False
        series = chart.series[0]
        series.graphicalProperties.line.noFill = True
        for index, colour in enumerate(colours):
            point = DataPoint(idx=index)
            point.graphicalProperties.solidFill = colour
            series.dPt.append(point)
        series.dLbls = DataLabelList()
        series.dLbls.showVal = True
        return chart

    def location_grading(self, parts):
        """One row per location, its grade coloured; the score column feeds the chart below."""
        self.heading("Location grading")
        rows, tones = [], {}
        for index, part in enumerate(parts):
            summary = part["summary"]
            rows.append([part["location"], summary["total"], summary["passed"], summary["failed"], summary["notApplicable"],
                         summary["score"], rating_for_score(summary["score"], self.settings)])
            tones[(index, 6)] = self.rating_tone(summary["score"])
        top = self.table(["Location", "Checks", "Passed", "Failed", "N/A", "Score", "Grade"], rows, [2, 1, 1, 1, 1, 1, 1], tones=tones)
        if len(parts) > 1:
            self.row += 1
            chart = self.bar_chart("Score by location", Reference(self.sheet, min_col=7, min_row=top + 1, max_row=top + len(parts)),
                                   Reference(self.sheet, min_col=1, min_row=top + 1, max_row=top + len(parts)),
                                   [TONES[self.rating_tone(part["summary"]["score"])][0] for part in parts], maximum=100)
            chart.height, chart.width = max(5, 0.55 * len(parts) + 2), 21.5
            self.sheet.add_chart(chart, f"A{self.row}")
            self.row += int(chart.height / 0.53) + 2

    def asset_register(self, parts, assets):
        """Every asset inspected, by location, with its register details and how it did."""
        self.heading("Assets inspected")
        for part in parts:
            grouped = {}
            for item in part["items"]:
                grouped.setdefault(item.get("equipmentId") or item.get("section") or "Item", []).append(item)
            self.put(self.row, 1, f"{part['location']} · {len(grouped)} asset{'s' if len(grouped) != 1 else ''} · {part['summary']['score']}/100",
                     size=9.5, bold=True, span=COLUMNS, wrap=False)
            self.row += 1
            rows, tones = [], {}
            for index, checks in enumerate(grouped.values()):
                first = checks[0]
                asset = (assets or {}).get(str(first.get("equipmentId") or ""), {})
                failed = sum(result_of(check) == "Fail" for check in checks)
                rows.append([first.get("section") or asset.get("name") or "Item", asset.get("code") or "", asset.get("type") or "",
                             " ".join(value for value in (asset.get("brand"), asset.get("model")) if value), asset.get("serial_number") or "",
                             asset.get("installation_date") or "", asset.get("status") or "",
                             f"{failed} failed" if failed else f"{len(checks)}/{len(checks)} passed"])
                tones[(index, 7)] = "fail" if failed else "pass"
            self.table(["Asset", "Code", "Type", "Brand / model", "Serial", "Installed", "Status", "Result"], rows, [1] * 8, tones=tones, size=8)
            self.row += 1


def new_sheet(workbook, title, brand, media, settings, first=False):
    sheet = workbook.active if first else workbook.create_sheet(title)
    sheet.title = title
    return SheetWriter(sheet, brand, media, settings)


def save(workbook):
    output = BytesIO()
    workbook.save(output)
    return output.getvalue()
