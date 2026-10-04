"""Config for the audit application."""
import re
import os
import threading
from collections import OrderedDict
from contextvars import ContextVar
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


DATA_DIR = Path(os.environ.get("AUDIT_DATA_DIR", str(ROOT / "data"))).resolve()


DB_PATH = DATA_DIR / "ottotree_audit_web.db"


# Browsers share cookies between ports of the same host, so instances running side by side
# (one per organization) each need their own session cookie name. The default port keeps the
# original name so existing sign-ins survive.
def _session_cookie_name():
    name = os.environ.get("AUDIT_SESSION_COOKIE", "").strip()
    if not name:
        port = os.environ.get("PORT", "41883").strip()
        name = "ottotree_session" if port == "41883" else f"ottotree_session_{port}"
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", name):
        raise SystemExit("AUDIT_SESSION_COOKIE may use only letters, digits, hyphens, and underscores")
    return name


SESSION_COOKIE = _session_cookie_name()

# Lets one thread initialize another database without redirecting concurrent requests.
DB_PATH_OVERRIDE = ContextVar("audit_db_path_override", default=None)


def active_db_path():
    return DB_PATH_OVERRIDE.get() or DB_PATH


LOUDSPEAKER_OUTLETS = ("STP", "SBA", "TPG", "AQP", "CCS", "SPK", "BSP", "MYT", "DJM", "KPG", "TSU", "TMA", "PGA", "PSC", "PWS")


DEFAULT_CATEGORIES = (
    "AV Equipment",
    "COM Equipment",
    "Facility",
    "F&B Equipment",
    "Electrical",
    "Plumbing",
    "Air Conditioning",
    "Lighting",
    "Furniture",
    "Building",
    "Safety",
    "Cleanliness",
    "IT / Network",
    "KTV Equipment",
    "Others",
)


DEFAULT_INSPECTION_CRITERIA = [
    "Present and correctly placed",
    "Clean and free from visible damage",
    "Operational during inspection",
    "Label, cable, or accessory is complete",
]


# Fixtures & finishes are parts of the building itself (paint, tiles, pipes, sanitary ware).
DEFAULT_FIXTURE_CRITERIA = [
    "Clean and free from stains or marks",
    "Intact with no cracks, leaks, or loose parts",
    "Works as intended",
    "Safe with no hazard to users",
]


ITEM_KINDS = ("asset", "fixture")


DEFAULT_PRIORITY_LEVELS = [
    {"name": "Priority", "classification": "Priority", "dueDays": 3},
    {"name": "Non-Priority", "classification": "Non-Priority", "dueDays": 14},
    {"name": "High", "classification": "Priority", "dueDays": 3},
    {"name": "Medium", "classification": "Non-Priority", "dueDays": 14},
    {"name": "Low", "classification": "Non-Priority", "dueDays": 14},
]


DEFAULT_AUDIT_TYPES = [
    {"name": "Standard", "description": "Full outlet inspection", "active": True},
    {"name": "Quick", "description": "Short follow-up inspection", "active": True},
]


DEFAULT_SCORING_SETTINGS = {
    "passMark": 70,
    "weighting": "Equal",
    "excellentBand": 90,
    "goodBand": 70,
    "belowBand": 60,
}


DEFAULT_REPORT_SETTINGS = {
    "companyName": "Ottotree",
    "departmentHeader": "Facilities Department",
    "logoText": "OTTOTREE",
    "appTitle": "Ottotree Audit",
    "appSubtitle": "Loudspeaker & Mini Studio operations",
    "businessUnitLabel": "Ottotree",
    "reportHeading": "audit report",
    "loginTitle": "Ottotree Audit",
}


DEFAULT_SYSTEM_SETTINGS = {
    "findingsEnabled": True,
    # When off, only assets with a failed check need photo evidence before an inspection can be completed.
    "requirePhotoEveryAsset": True,
}


# An organization's look. Each company database stores its own; the Super account edits it.
THEME_CHOICES = {
    "preset": ("default", "ottotree", "ocean", "plum", "ember", "slate"),
    "font": ("system", "noto-sans-sc", "serif"),
    "corners": ("rounded", "square", "soft"),
    "density": ("comfortable", "compact"),
    "mode": ("system", "light", "dark"),
}


DEFAULT_THEME_SETTINGS = {
    "preset": "default",
    "accent": "",          # "#rrggbb" replaces the preset's accent colour; empty keeps it.
    "font": "system",
    "corners": "rounded",
    "density": "comfortable",
    "mode": "system",      # The organization's default appearance.
    "userChoice": True,    # Whether each user may pick light or dark for themselves.
}


# Accent colour of each palette, used where CSS is not available (the PDF report).
THEME_PRESET_ACCENTS = {"default": "#47735f", "ottotree": "#1e99b4", "ocean": "#2563eb", "plum": "#7c3aed", "ember": "#c2410c", "slate": "#475569"}


# The theme modelled on Ottotree's PM checklist reports: teal accent and Noto Sans SC.
OTTOTREE_THEME = {"preset": "ottotree", "font": "noto-sans-sc"}


APP_TABS = (
    ("today", "Dashboard"),
    ("inspections", "Inspections"),
    ("findings", "History & Findings"),
    ("work-orders", "Maintenance"),
    ("equipment", "Fixed Assets"),
    ("reports", "Reports"),
    ("categories", "Categories"),
    ("departments", "Departments"),
    ("outlets", "Outlets"),
    ("users", "Users"),
    ("roles", "Roles"),
    ("notifications", "Notifications"),
    ("settings", "Settings"),
)

# These pages are intentionally outside the permission model. They are only
# exposed to the built-in Super account and can never be granted to a role.
SUPER_TABS = (
    ("super-dashboard", "Super Dashboard"),
    ("super-settings", "Super Settings"),
)


INCLUDE_PATTERN = re.compile(r"<!--\s*include:\s*([a-zA-Z0-9_./-]+)\s*-->")


from backend.session_store import SessionStore
SESSION_TOKENS = SessionStore()


DEFAULT_PASSWORD = "password123"


SUPER_ROLE = "Super"


ADMIN_ROLE = "Admin"


EQUIPMENT_CACHE = OrderedDict()


EQUIPMENT_CACHE_LOCK = threading.RLock()


STATIC_LOCK = threading.Lock()
