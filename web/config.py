"""Config for the audit application."""
import re
import os
import threading
from collections import OrderedDict
from pathlib import Path


ROOT = Path(__file__).resolve().parent


DATA_DIR = Path(os.environ.get("AUDIT_DATA_DIR", str(ROOT / "data"))).resolve()


DB_PATH = DATA_DIR / "ottotree_audit_web.db"


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
    "todayHeading": "inspections for today",
    "reportHeading": "monthly audit report",
    "loginTitle": "Ottotree Audit",
}


DEFAULT_SYSTEM_SETTINGS = {
    "emailEnabled": False,
    "whatsappEnabled": False,
    "pushEnabled": False,
    "cmmsEnabled": False,
    "preventiveMaintenanceEnabled": False,
    "aiPhotoDetectionEnabled": False,
    "aiSummaryEnabled": False,
    "aiRecommendationEnabled": False,
}


APP_TABS = (
    ("today", "To-do"),
    ("inspections", "Inspections"),
    ("findings", "History & Findings"),
    ("work-orders", "Work Orders"),
    ("equipment", "Fixed Assets"),
    ("reports", "Reports"),
    ("categories", "Categories"),
    ("departments", "Departments"),
    ("outlets", "Outlets"),
    ("users", "Users"),
    ("roles", "Roles"),
    ("corrective-actions", "Corrective Actions"),
    ("notifications", "Notifications"),
    ("settings", "Settings"),
)


INCLUDE_PATTERN = re.compile(r"<!--\s*include:\s*([a-zA-Z0-9_./-]+)\s*-->")


SESSION_TOKENS = {}


DEFAULT_PASSWORD = "password123"


SUPER_ROLE = "Super"


ADMIN_ROLE = "Admin"


EQUIPMENT_CACHE = OrderedDict()


EQUIPMENT_CACHE_LOCK = threading.RLock()


STATIC_LOCK = threading.Lock()
