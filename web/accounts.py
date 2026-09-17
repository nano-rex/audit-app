"""Accounts for the audit application."""
import json
from common import read_setting
from config import ADMIN_ROLE, APP_TABS, DEFAULT_REPORT_SETTINGS, SUPER_ROLE
from database import connect


def public_user(row):
    if not row:
        return None
    permissions = []
    if row["role"] == SUPER_ROLE:
        permissions = [tab[0] for tab in APP_TABS]
    else:
        with connect() as db:
            role = db.execute("SELECT permissions_json FROM roles WHERE name = ?", (row["role"],)).fetchone()
            if role:
                permissions = json.loads(role["permissions_json"] or "[]")
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
