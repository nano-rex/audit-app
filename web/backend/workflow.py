"""Server-side work-order transitions, validation, and verification identity."""
import json
from backend.relational_values import load_value
from datetime import date

from backend.common import today_date


class WorkflowError(ValueError):
    def __init__(self, message, status=409):
        super().__init__(message)
        self.status = status


TRANSITIONS = {
    "Open": {"Assigned", "In Progress", "Pending"},
    "Assigned": {"In Progress", "Pending", "Completed"},
    "In Progress": {"Pending", "Completed"},
    "Pending": {"Assigned", "In Progress"},
    "Completed": {"Verified", "In Progress"},
    "Verified": {"Closed", "In Progress"},
    "Closed": set(),
}

FIELDS = {
    "businessUnit": "business_unit", "outlet": "outlet", "zone": "zone",
    "requestType": "request_type", "category": "category", "priority": "priority",
    "title": "title", "description": "description", "assignee": "assignee", "pic": "pic",
    "status": "status", "actionTaken": "action_taken", "completionDate": "completion_date",
    "completionRemark": "completion_remark", "completionPhoto": "completion_photo_data_id",
    "verifiedBy": "verified_by", "verifiedAt": "verified_at", "verificationRemark": "verification_remark",
    "closedAt": "closed_at", "dueDate": "due_date", "vendor": "vendor", "cost": "cost",
}


def can_verify(user):
    return "verifier" in user.get("inspectionPermissions", [])


def assigned_to(user, record):
    identities = {str(user.get(key) or "").strip().casefold() for key in ("name", "email")}
    identities.discard("")
    pic = str(record.get("pic") or "").strip().casefold()
    if pic:
        return pic in identities
    if str(record.get("assignee") or "").strip().casefold() in identities:
        return True
    department = str(user.get("department") or "").strip().casefold()
    return bool(department and department == str(record.get("request_type") or "").strip().casefold())


def validate_update(payload, existing, user):
    existing = dict(existing) if existing else None
    merged = {key: existing[column] for key, column in FIELDS.items()} if existing else {}
    if existing:
        merged["completionPhoto"] = load_value(existing["completion_photo_data_id"]) or []
    merged.update(payload)
    status = merged.get("status", "Assigned")
    if status not in TRANSITIONS:
        raise WorkflowError("Unknown work-order status", 400)
    previous = existing["status"] if existing else None
    if not existing and status not in {"Open", "Assigned"}:
        raise WorkflowError("New work orders must start Open or Assigned")
    if existing:
        if previous == "Closed":
            raise WorkflowError("Closed work orders are read-only")
        if status != previous and status not in TRANSITIONS.get(previous, set()):
            raise WorkflowError(f"Cannot change {previous} directly to {status}")
        if previous == "Verified" and status == previous:
            raise WorkflowError("Reopen a verified work order before editing it")
        if user.get("role") == "Department/PIC":
            if not assigned_to(user, existing):
                raise WorkflowError("This work order is assigned to another person or department", 403)
            for key in ("outlet", "zone", "requestType", "category", "priority", "assignee", "pic", "dueDate"):
                if str(merged.get(key) or "") != str(existing[FIELDS[key]] or ""):
                    raise WorkflowError("Only an auditor or administrator can change the assignment", 403)
    verification_change = status in {"Verified", "Closed"} or previous in {"Completed", "Verified"} and status == "In Progress"
    if verification_change and not can_verify(user):
        raise WorkflowError("An auditor or facilities manager must verify, reject, or close this work order", 403)
    if status in {"Completed", "Verified", "Closed"}:
        for key, label in (("actionTaken", "Action taken"), ("pic", "Person in charge"),
                           ("completionDate", "Completion date"), ("completionRemark", "Completion remark")):
            if not str(merged.get(key) or "").strip():
                raise WorkflowError(f"{label} is required before completion", 400)
        try:
            completed = date.fromisoformat(merged["completionDate"])
        except (TypeError, ValueError):
            raise WorkflowError("Completion date must be a valid YYYY-MM-DD date", 400)
        if completed > date.today():
            raise WorkflowError("Completion date cannot be in the future", 400)
        photos = merged.get("completionPhoto") or []
        if isinstance(photos, str):
            try:
                photos = json.loads(photos)
            except ValueError:
                photos = []
        if not isinstance(photos, list) or not any(isinstance(photo, dict) and (photo.get("url") or photo.get("dataUrl")) for photo in photos):
            raise WorkflowError("Upload a completion photo before completing this work order", 400)
    if verification_change and not str(payload.get("verificationRemark") or "").strip():
        raise WorkflowError("A verification or rejection remark is required", 400)
    # The caller cannot impersonate another verifier or choose verification/closure dates.
    merged["verifiedBy"] = existing.get("verified_by", "") if existing else ""
    merged["verifiedAt"] = existing.get("verified_at", "") if existing else ""
    merged["closedAt"] = ""
    if status == "Verified" and status != previous:
        merged["verifiedBy"] = user["name"]
        merged["verifiedAt"] = today_date()
    if status == "Closed":
        merged["closedAt"] = today_date()
    if status not in {"Verified", "Closed"}:
        merged["verifiedBy"] = merged["verifiedAt"] = ""
    return merged
