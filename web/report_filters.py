"""Consistent business-unit, outlet, and date filters for reports and exports."""
from datetime import date
from common import scope


def report_scope(unit, table, filters=None):
    where, params = scope(unit, table)
    params = list(params)
    filters = filters or {}
    start, end = filters.get("from"), filters.get("to")
    for value in (start, end):
        if value:
            try:
                date.fromisoformat(value)
            except (TypeError, ValueError):
                raise ValueError("Report dates must be valid YYYY-MM-DD dates") from None
    if start and end and start > end:
        raise ValueError("Report start date must not be after the end date")
    if filters.get("outlet"):
        where += f" AND {table}.outlet = ?"
        params.append(filters["outlet"])
    date_field = {"audits": "audit_date", "inspection_sessions": "audit_date", "schedules": "scheduled_date"}.get(table)
    expression = f"{table}.{date_field}" if date_field else f"date({table}.created_at / 1000, 'unixepoch')"
    if table in {"findings", "work_orders"}:
        link = "audit_id" if table == "findings" else "source_audit_id"
        expression = f"COALESCE((SELECT audit_date FROM audits WHERE id = {table}.{link}), {expression})"
    if table != "equipment":
        for value, operator in ((start, ">="), (end, "<=")):
            if value:
                where += f" AND {expression} {operator} ?"
                params.append(value)
    return where, tuple(params)
