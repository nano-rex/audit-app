"""Accounts for the audit application."""
from relational_values import load_value
from common import read_setting
from config import ADMIN_ROLE, DEFAULT_REPORT_SETTINGS, SUPER_ROLE
from database import connect
from permissions import resolve_permissions


def public_user(row):
    if not row:
        return None
    with connect() as db:
        record = db.execute("SELECT permission_overrides_data_id FROM users WHERE id = ?", (row["id"],)).fetchone()
        overrides = load_value(record["permission_overrides_data_id"]) if record and record["permission_overrides_data_id"] is not None else None
        permissions, inspection_permissions_data_id = resolve_permissions(db, row["role"], overrides)
    return {
        "id": row["id"],
        "name": row["name"],
        "role": row["role"],
        "email": row["email"],
        "department": row["department"] or "",
        "title": row["title"] or "",
        "responsibilities": row["responsibilities"] or "",
        "active": bool(row["active"]),
        "lastLoginAt": row["last_login_at"] or "",
        "resetRequired": bool(row["reset_required"]),
        "permissions": permissions,
        "inspectionPermissions": inspection_permissions_data_id,
        "permissionOverrides": overrides,
        "permissionSource": "user" if overrides is not None else "role",
        "profilePhoto": load_value(row["profile_photo_data_id"] or "{}") if "profile_photo_data_id" in row.keys() else {},
        "signatureImage": load_value(row["signature_image_data_id"] or "{}") if "signature_image_data_id" in row.keys() else {},
    }


def is_super_user(user):
    return bool(user and user.get("role") == SUPER_ROLE)


def is_company_admin_user(user):
    return bool(user and user.get("role") in (SUPER_ROLE, ADMIN_ROLE))


def branding_settings():
    with connect() as db:
        return {
            "appTitle": read_setting(db, "report.appTitle", DEFAULT_REPORT_SETTINGS["appTitle"]),
            "appSubtitle": read_setting(db, "report.appSubtitle", DEFAULT_REPORT_SETTINGS["appSubtitle"]),
            "companyName": read_setting(db, "report.companyName", DEFAULT_REPORT_SETTINGS["companyName"]),
            "departmentHeader": read_setting(db, "report.departmentHeader", DEFAULT_REPORT_SETTINGS["departmentHeader"]),
            "logoText": read_setting(db, "report.logoText", DEFAULT_REPORT_SETTINGS["logoText"]),
            "logoUrl": read_setting(db, "report.logoUrl", ""),
            "businessUnitLabel": read_setting(db, "report.businessUnitLabel", DEFAULT_REPORT_SETTINGS["businessUnitLabel"]),
            "todayHeading": read_setting(db, "report.todayHeading", DEFAULT_REPORT_SETTINGS["todayHeading"]),
            "reportHeading": read_setting(db, "report.reportHeading", DEFAULT_REPORT_SETTINGS["reportHeading"]),
            "loginTitle": read_setting(db, "report.loginTitle", DEFAULT_REPORT_SETTINGS["loginTitle"]),
        }
