"""Audit header validation and stable references shared by draft and final records."""
import re
from datetime import datetime


def allocate_reference(db, audit_date):
    year = str(audit_date)[:4]
    row = db.execute("SELECT next_number FROM audit_reference_counters WHERE year = ?", (year,)).fetchone()
    if row:
        number = row[0]
    else:
        references = db.execute("SELECT audit_ref FROM audits UNION ALL SELECT audit_ref FROM inspection_sessions").fetchall()
        numbers = [int(match[1]) for row in references if (match := re.fullmatch(rf"AUD-{re.escape(year)}-(\d+)", row[0] or ""))]
        number = max(numbers, default=0) + 1
    db.execute("INSERT INTO audit_reference_counters(year, next_number) VALUES (?, ?) ON CONFLICT(year) DO UPDATE SET next_number = excluded.next_number", (year, number + 1))
    return f"AUD-{year}-{number:04d}"


def validate_metadata(db, payload, existing=None):
    """Validate supplied headers; unchanged legacy setup values remain readable/editable."""
    for key, pattern, fmt, label in (("auditDate", r"\d{4}-\d{2}-\d{2}", "%Y-%m-%d", "date"),
                                     ("auditTime", r"\d{2}:\d{2}", "%H:%M", "time")):
        if key not in payload:
            continue
        value = payload[key]
        try:
            if not isinstance(value, str) or not re.fullmatch(pattern, value):
                raise ValueError()
            datetime.strptime(value, fmt)
        except ValueError:
            raise ValueError(f"Enter a valid audit {label}") from None
    for key, table, column in (("outlet", "outlets", "code"), ("auditType", "audit_types", "name")):
        if key not in payload:
            continue
        value = payload[key]
        old_column = "audit_type" if key == "auditType" else key
        if existing and value == existing[old_column]:
            continue
        query = f"SELECT 1 FROM {table} WHERE {column} = ?" + (" AND active = 1" if key == "auditType" else "")
        if not isinstance(value, str) or not db.execute(query, (value,)).fetchone():
            raise ValueError(f"Select a configured {'audit type' if key == 'auditType' else 'outlet'}")
    if "remarks" in payload and (not isinstance(payload["remarks"], str) or len(payload["remarks"]) > 5000):
        raise ValueError("Remarks must be text of at most 5000 characters")
