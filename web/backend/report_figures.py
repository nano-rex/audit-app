"""The Reports page's figures, prepared once for its PDF and Excel exports so both show what the
page shows (web/js/report-charts.js), in the same order and colours."""
from backend.activity import duration_text

PASS, FAIL, WARN, INFO, MUTED = "1C8F4B", "C9303A", "B96100", "2B6FDB", "B4BDB8"
BANDS = (("Excellent", "24C362"), ("Good", INFO), ("Below Expectation", WARN), ("Critical", FAIL))


def score_colour(score, scoring):
    if score >= scoring.get("goodBand", 70):
        return PASS
    if score >= scoring.get("belowBand", 60):
        return WARN
    return FAIL


def overview(data):
    """The four rings: title, note, centre figure and caption, parts (label, value, colour), footer."""
    summary, kpi = data.get("monthlySummary") or {}, data.get("kpi") or {}
    closed, open_orders = summary.get("closedWorkOrders") or 0, summary.get("openWorkOrders") or 0
    other = max(0, (kpi.get("assigned") or 0) - (kpi.get("completed") or 0) - (kpi.get("pending") or 0))
    return [
        {"title": "Audits", "note": f"{summary.get('audits') or 0} in this report", "centre": f"{round(float(summary.get('averageScore') or 0))}", "caption": "avg score",
         "parts": [("Completed", summary.get("auditsCompleted") or 0, PASS), ("Pending", summary.get("auditsPending") or 0, WARN)], "footer": ""},
        {"title": "Findings", "note": f"{summary.get('totalFindings') or 0} recorded", "centre": f"{summary.get('totalFindings') or 0}", "caption": "findings",
         "parts": [("Priority", summary.get("priorityFindings") or 0, FAIL), ("Non-priority", summary.get("nonPriorityFindings") or 0, INFO)],
         "footer": f"Open {summary.get('outstandingFindings') or 0} · Overdue {summary.get('overdueFindings') or 0}"},
        {"title": "Work orders", "note": f"{open_orders + closed} raised", "centre": f"{summary.get('completionRate') or 0}%", "caption": "completed",
         "parts": [("Closed", closed, PASS), ("Open", open_orders, WARN)], "footer": ""},
        {"title": "Scheduled tasks", "note": f"{kpi.get('assigned') or 0} assigned", "centre": f"{kpi.get('responseRate') or 0}%", "caption": "response",
         "parts": [("Completed", kpi.get("completed") or 0, PASS), ("Pending", kpi.get("pending") or 0, WARN)] + ([("In progress", other, INFO)] if other else []),
         "footer": ""},
    ]


def monthly_audits(charts):
    """(month, audits, average score), oldest first."""
    return sorted(((row.get("month") or "", int(row.get("audits") or 0), round(float(row.get("average_score") or 0)))
                   for row in charts.get("monthlyAuditTrend") or []), key=lambda row: row[0])


def monthly_findings(charts):
    """(month, priority, non-priority), oldest first, from labels such as "2026-10 · Priority"."""
    months = {}
    for row in charts.get("priorityTrend") or []:
        month, _, kind = str(row.get("label") or "").partition(" · ")
        entry = months.setdefault(month, [0, 0])
        entry[0 if (kind or "Priority") == "Priority" else 1] += int(row.get("count") or 0)
    return [(month, *counts) for month, counts in sorted(months.items())]


def outlet_scores(data):
    """(outlet, latest, average, audits, last audit date), as ranked."""
    return [(row.get("outlet") or "", int(row.get("latest") or 0), round(float(row.get("average") or 0)), int(row.get("audit_count") or 0), row.get("audit_date") or "")
            for row in data.get("rankings") or []]


def performance_bands(charts):
    counts = {row.get("label"): int(row.get("count") or 0) for row in charts.get("performanceDistribution") or []}
    return [(label, counts.get(label, 0), colour) for label, colour in BANDS]


def comparison(charts):
    """(outlet, previous score or None, current score or None)."""
    pairs = {}
    for row in charts.get("auditComparison") or []:
        outlet, _, which = str(row.get("label") or "").partition(" · ")
        entry = pairs.setdefault(outlet, [None, None])
        entry[0 if which == "Previous" else 1] = int(row.get("score") or 0)
    return [(outlet, *values) for outlet, values in pairs.items()]


def breakdowns(charts, scoring):
    """The page's breakdown cards: title, note, rows (label, value, colour, note), largest value, suffix."""
    def counts(key, colour):
        rows = sorted(((row.get("label") or "Unassigned", int(row.get("count") or 0), colour, "") for row in charts.get(key) or []), key=lambda row: -row[1])
        return rows

    def shares(key):
        return [(row.get("label") or "Unassigned", int(row.get("score") or 0), score_colour(int(row.get("score") or 0), scoring),
                 f"{row.get('completed') or 0} of {row.get('count') or 0} closed") for row in charts.get(key) or []]
    accent = None  # the organization's accent, filled in by each writer
    return [
        ("Findings by department", "Who the findings were assigned to", counts("findingsByDepartment", accent), None, ""),
        ("Findings by location", "Where the findings were raised", counts("findingsByArea", accent), None, ""),
        ("Findings by asset type", "What kind of item failed", counts("findingsByCategory", accent), None, ""),
        ("Findings by priority", "Priority levels given", counts("findingsByPriority", FAIL), None, ""),
        ("Work closed by department", "Share of work orders closed", shares("departmentPerformance"), 100, "%"),
        ("Work closed by location", "Share of work orders closed", shares("locationPerformance"), 100, "%"),
        ("Work closed by asset type", "Share of work orders closed", shares("categoryPerformance"), 100, "%"),
        ("Location scores by month", "Average audit score of each location",
         [(row.get("label") or "", int(row.get("score") or 0), score_colour(int(row.get("score") or 0), scoring), "") for row in charts.get("roomAuditTrend") or []], 100, "/100"),
    ]


def time_to_act(data):
    """(step, average seconds, longest seconds, times) for the steps that happened."""
    return [(row["label"], row.get("averageSeconds") or 0, row.get("longestSeconds") or 0, row.get("count") or 0)
            for row in data.get("timeToAct") or []]


def people(data):
    """(person, audits completed, requests acted on, orders closed) for those who did any."""
    rows = [(row["name"], row.get("audit_completed") or 0, (row.get("order_created") or 0) + (row.get("request_declined") or 0), row.get("order_closed") or 0)
            for row in data.get("people") or []]
    return [row for row in rows if sum(row[1:])]


PEOPLE_STACKS = (("Audits completed", None), ("Requests acted on", INFO), ("Orders closed", PASS))


def people_table(data):
    header = ["Person", "Audits started", "Audits completed", "Avg. audit time", "Signed", "Audits closed", "Requests raised",
              "Requests acted on", "Avg. time to act", "Orders created", "Orders closed", "Avg. time to close"]
    rows = [[row["name"], row["audit_started"], row["audit_completed"], duration_text(row["auditSeconds"]), row["audit_signed"], row["audit_closed"],
             row["request_raised"], row["order_created"] + row["request_declined"], duration_text(row["requestSeconds"]), row["order_created"],
             row["order_closed"], duration_text(row["orderSeconds"])] for row in data.get("people") or []]
    return header, rows


# ---- An audit's assets by their register attributes ----

def _year(value):
    import re
    match = re.search(r"(19|20)\d{2}", str(value or ""))
    return match[0] if match else "Not recorded"


def _date_status(value):
    """Expired, Valid, or Not recorded, for a date typed as 2026-10-09, 09.10.2026, or 9/10/2026."""
    import re
    from datetime import date
    text = str(value or "").strip()
    match = re.fullmatch(r"(\d{4})[-./](\d{1,2})[-./](\d{1,2})", text)
    parts = (match[1], match[2], match[3]) if match else None
    if not parts:
        match = re.fullmatch(r"(\d{1,2})[-./](\d{1,2})[-./](\d{2}|\d{4})", text)
        parts = (match[3] if len(match[3]) == 4 else f"20{match[3]}", match[2], match[1]) if match else None
    try:
        day = date(int(parts[0]), int(parts[1]), int(parts[2])) if parts else None
    except ValueError:
        day = None
    if not day:
        return "Not recorded"
    return "Expired" if day < date.today() else "Valid"


ASSET_ATTRIBUTES = (
    ("Asset type", lambda asset, first: asset.get("type") or "Not recorded"),
    ("Brand", lambda asset, first: asset.get("brand") or "Not recorded"),
    ("Installation year", lambda asset, first: _year(asset.get("installation_date"))),
    ("Operational status", lambda asset, first: asset.get("status") or "Not recorded"),
    ("Warranty", lambda asset, first: _date_status(asset.get("warranty_date"))),
    ("Expiry", lambda asset, first: _date_status(asset.get("expiry_date"))),
)


def asset_attributes(items, assets, limit=12):
    """The audit's assets counted by each register attribute, split into those whose checks all
    passed and those with a failure: [(attribute, [(value, passed, failed)])], the largest first
    (years oldest first), with the smallest values over the limit gathered as "Other". An
    attribute nobody recorded is left out."""
    grouped = {}
    for item in items:
        grouped.setdefault(str(item.get("equipmentId") or item.get("section") or "Item"), []).append(item)
    inspected = []
    for key, checks in grouped.items():
        failed = any(not check.get("passed") and not check.get("notApplicable") for check in checks)
        # A casual audit's group stands for every asset in it.
        weight = max(1, int(checks[0].get("groupCount") or 1))
        inspected.append(((assets or {}).get(key, {}), checks[0], failed, weight))
    result = []
    for name, value_of in ASSET_ATTRIBUTES:
        counts = {}
        for asset, first, failed, weight in inspected:
            entry = counts.setdefault(value_of(asset, first), [0, 0])
            entry[1 if failed else 0] += weight
        if set(counts) <= {"Not recorded"}:
            continue
        rows = [(value, passed, failed) for value, (passed, failed) in counts.items()]
        rows.sort(key=(lambda row: (row[0] == "Not recorded", row[0])) if name == "Installation year" else (lambda row: -(row[1] + row[2])))
        if len(rows) > (limit if name != "Installation year" else 30):
            rest = rows[limit - 1:]
            rows = rows[:limit - 1] + [("Other", sum(row[1] for row in rest), sum(row[2] for row in rest))]
        result.append((name, rows))
    return sum(weight for *_, weight in inspected), sum(weight for *_, failed, weight in inspected if failed), result
