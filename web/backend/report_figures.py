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
        ("Findings by category", "What kind of item failed", counts("findingsByCategory", accent), None, ""),
        ("Findings by priority", "Priority levels given", counts("findingsByPriority", FAIL), None, ""),
        ("Work closed by department", "Share of work orders closed", shares("departmentPerformance"), 100, "%"),
        ("Work closed by location", "Share of work orders closed", shares("locationPerformance"), 100, "%"),
        ("Work closed by category", "Share of work orders closed", shares("categoryPerformance"), 100, "%"),
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
