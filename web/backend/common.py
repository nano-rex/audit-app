"""Common for the audit application."""
from backend.relational_values import load_value
import json
import re
import time
import hashlib
import secrets
from datetime import datetime


def today_date():
    return datetime.now().strftime("%Y-%m-%d")


def normalize_audit_date(value):
    return today_date() if not value or value == "Today" else value


def parse_date(value):
    try:
        return datetime.strptime(normalize_audit_date(value), "%Y-%m-%d")
    except (TypeError, ValueError):
        return datetime.now()


def record_year(value=None):
    return normalize_audit_date(value).split("-", 1)[0]


def audit_ref(record_id, audit_date=None):
    return f"AUD-{record_year(audit_date)}-{int(record_id):04d}"


def finding_ref(record_id, audit_date=None):
    return f"F-{record_year(audit_date)}-{int(record_id):05d}"


def work_order_ref(record_id, audit_date=None):
    return f"WO-{record_year(audit_date)}-{int(record_id):05d}"


def location_qr_code(outlet, name):
    outlet_code = re.sub(r"[^A-Z0-9]+", "-", (outlet or "OUTLET").upper()).strip("-") or "OUTLET"
    location_code = re.sub(r"[^A-Z0-9]+", "-", (name or "LOCATION").upper()).strip("-") or "LOCATION"
    return f"LOC-{outlet_code}-{location_code}"


def hash_password(password, salt=None):
    salt = salt or secrets.token_hex(16)
    digest = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt.encode("ascii"), 600000).hex()
    return f"pbkdf2_sha256$600000${salt}${digest}"


def verify_password(password, stored_hash):
    if not stored_hash or "$" not in stored_hash:
        return False
    if stored_hash.startswith("pbkdf2_sha256$"):
        try:
            _, iterations, salt, digest = stored_hash.split("$")
            actual = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt.encode("ascii"), int(iterations)).hex()
            return secrets.compare_digest(actual, digest)
        except (ValueError, UnicodeError):
            return False
    salt, digest = stored_hash.split("$", 1)
    actual = hashlib.sha256(f"{salt}:{password}".encode("utf-8")).hexdigest()
    return secrets.compare_digest(actual, digest)


def workflow_dates(payload):
    status = payload.get("status", "Assigned")
    verified_at = payload.get("verifiedAt", "")
    closed_at = payload.get("closedAt", "")
    if status in ("Verified", "Closed") and not verified_at:
        verified_at = today_date()
    if status == "Closed" and not closed_at:
        closed_at = today_date()
    if status not in ("Verified", "Closed"):
        closed_at = ""
    return status, verified_at, closed_at


def read_setting(db, key, fallback=None):
    row = db.execute("SELECT value_data_id FROM app_settings WHERE key = ?", (key,)).fetchone()
    if not row:
        return fallback
    try:
        return load_value(row["value_data_id"])
    except json.JSONDecodeError:
        return row["value_data_id"]


def calculate_due_date(created_at, due_days):
    base = datetime.fromtimestamp((created_at or int(time.time() * 1000)) / 1000)
    return datetime.fromtimestamp(base.timestamp() + int(due_days) * 86400).strftime("%Y-%m-%d")


def priority_due_date(db, priority, created_at):
    row = db.execute("SELECT due_days FROM priority_levels WHERE name = ? AND active = 1", (priority,)).fetchone()
    days = int(row["due_days"]) if row else (3 if priority in ("High", "Priority") else 14)
    return calculate_due_date(created_at, days)


def sla_status(status, due_date):
    if status in ("Completed", "Verified", "Closed"):
        return "Completed"
    if not due_date:
        return "No due date"
    days = (parse_date(due_date) - parse_date(today_date())).days
    if days < 0:
        return "Overdue"
    if days <= 3:
        return "Due Soon"
    return "On Track"


def create_notification(db, title, message, channel="In-App", related_type=None, related_id=None):
    db.execute(
        """
        INSERT INTO notifications (title, message, channel, status, related_type, related_id, created_at)
        VALUES (?, ?, ?, 'Unread', ?, ?, ?)
        """,
        (title, message, channel, related_type, related_id, int(time.time() * 1000)),
    )


def image_label(image):
    if isinstance(image, dict):
        return image.get("markedName") or image.get("name") or "Image"
    return str(image)


def image_labels(images):
    return [image_label(image) for image in (images or [])]


def parse_image_list(value):
    if not value:
        return []
    if isinstance(value, list):
        return value
    try:
        images = load_value(value)
        return images if isinstance(images, list) else [images]
    except (TypeError, json.JSONDecodeError):
        return [value]


def rating(score):
    if score >= 90:
        return "Excellent"
    if score >= 70:
        return "Good"
    if score >= 60:
        return "Below Expectation"
    return "Critical"


def checklist(unit):
    loudspeaker = [
        {"section": "Loudspeaker Display", "item": "Main speaker display is present, clean, and powered"},
        {"section": "Loudspeaker Display", "item": "Price tags and product cards are accurate"},
        {"section": "Loudspeaker Demo", "item": "Demo audio source and cables are working"},
        {"section": "Loudspeaker Safety", "item": "Power socket, cable routing, and fixture are safe"},
    ]
    mini_studio = [
        {"section": "Main Entrance", "item": "Big headphone display is present and in good condition"},
        {"section": "Studio Area", "item": "Demo headphones are clean, working, and correctly placed"},
        {"section": "Counter", "item": "F&B counter cabinet and cashier drawer area are clean"},
        {"section": "Safety", "item": "Emergency exit and walkway are clear and usable"},
    ]
    if unit == "Loudspeaker":
        return loudspeaker
    if unit == "Mini Studio":
        return mini_studio
    return mini_studio + loudspeaker


def scope(unit, table_name):
    if unit in ("Mini Studio", "Loudspeaker"):
        return f"{table_name}.business_unit = ?", (unit,)
    return "1 = 1", ()


def inspection_progress(items):
    if not items:
        return 0
    complete = 0
    for item in items:
        passed = bool(item.get("passed"))
        not_applicable = bool(item.get("notApplicable"))
        notes = (item.get("notes") or "").strip()
        if passed or not_applicable or notes:
            complete += 1
    return complete * 100 // len(items)


def inspection_name(session):
    outlet = session.get("outlet") or "Outlet"
    date = normalize_audit_date(session.get("audit_date"))
    return f"{outlet}_{date}_{session.get('id')}"


def normalized_inspection_name(session):
    name = session.get("inspection_name") or inspection_name(session)
    if "_Today_" in name:
        return inspection_name(session)
    return name
