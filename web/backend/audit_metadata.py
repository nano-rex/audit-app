"""Audit header validation and stable references shared by draft and final records."""
import re
from datetime import datetime

from backend.common import OLD_AUDIT_CODE, audit_code
from backend.relational_values import save_value


def allocate_reference(db, audit_date, outlet=""):
    """The next code of the audit's year: AUDIT-<outlet>-<date>-<number>."""
    year = str(audit_date)[:4]
    row = db.execute("SELECT next_number FROM audit_reference_counters WHERE year = ?", (year,)).fetchone()
    if row:
        number = row[0]
    else:
        references = db.execute("SELECT audit_ref FROM audits UNION ALL SELECT audit_ref FROM inspection_sessions").fetchall()
        numbers = [int(match[1]) for row in references
                   if (match := re.fullmatch(rf"AUD-{re.escape(year)}-(\d+)", row[0] or "") or re.fullmatch(rf"AUDIT-.+-{re.escape(year)}\d{{4}}-(\d+)", row[0] or ""))]
        number = max(numbers, default=0) + 1
    db.execute("INSERT INTO audit_reference_counters(year, next_number) VALUES (?, ?) ON CONFLICT(year) DO UPDATE SET next_number = excluded.next_number", (year, number + 1))
    return audit_code(outlet, audit_date, number)


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


def convert_audit_codes(db):
    """Once: old codes (AUD-2026-0004, with a separate name such as STP_2026-10-06_13) become
    AUDIT-STP-20261006-000004, everywhere they were recorded."""
    if db.execute("SELECT 1 FROM app_settings WHERE key = 'system.auditCodesV2'").fetchone():
        return
    renamed = {}
    for table in ("inspection_sessions", "audits"):
        for row in db.execute(f"SELECT audit_ref, outlet, audit_date FROM {table} WHERE audit_ref LIKE 'AUD-%'").fetchall():
            match = OLD_AUDIT_CODE.fullmatch(row["audit_ref"] or "")
            if match and row["audit_ref"] not in renamed:
                renamed[row["audit_ref"]] = audit_code(row["outlet"], row["audit_date"], int(match[1]))
    for old, new in renamed.items():
        db.execute("UPDATE inspection_sessions SET audit_ref = ?, inspection_name = ? WHERE audit_ref = ?", (new, new, old))
        for table in ("audits", "findings", "work_requests"):
            db.execute(f"UPDATE {table} SET audit_ref = ? WHERE audit_ref = ?", (new, old))
        db.execute("UPDATE activity_log SET record_ref = ? WHERE record_ref = ?", (new, old))
        db.execute("UPDATE notifications SET title = REPLACE(title, ?, ?), message = REPLACE(message, ?, ?) WHERE title LIKE ? OR message LIKE ?",
                   (old, new, old, new, f"%{old}%", f"%{old}%"))
    # Every audit is named by its code now.
    db.execute("UPDATE inspection_sessions SET inspection_name = audit_ref WHERE audit_ref IS NOT NULL AND audit_ref != ''")
    db.execute("INSERT INTO app_settings (key, value_data_id) VALUES ('system.auditCodesV2', ?)", (save_value(db, True),))
