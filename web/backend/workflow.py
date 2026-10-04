"""Server-side work-order transitions and validation."""
from backend.common import today_date


class WorkflowError(ValueError):
    def __init__(self, message, status=409):
        super().__init__(message)
        self.status = status


# A work order is raised, assigned, worked on, and closed. There is no separate completion
# or verification step: whoever it is assigned to (or an auditor/administrator) closes it.
TRANSITIONS = {
    "Open": {"Assigned", "In Progress", "Pending", "Closed"},
    "Assigned": {"In Progress", "Pending", "Closed"},
    "In Progress": {"Assigned", "Pending", "Closed"},
    "Pending": {"Assigned", "In Progress", "Closed"},
    "Closed": set(),
}

FIELDS = {
    "businessUnit": "business_unit", "outlet": "outlet", "zone": "zone",
    "requestType": "request_type", "category": "category", "priority": "priority",
    "title": "title", "description": "description", "assignee": "assignee", "pic": "pic",
    "status": "status", "closedAt": "closed_at", "dueDate": "due_date", "vendor": "vendor", "cost": "cost",
}


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
    merged.update({key: value for key, value in payload.items() if key in FIELDS or key in ("cause", "recommendation", "requiredAction", "images")})
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
        if user.get("role") == "Department/PIC":
            if not assigned_to(user, existing):
                raise WorkflowError("This work order is assigned to another person or department", 403)
            for key in ("outlet", "zone", "requestType", "category", "priority", "assignee", "pic", "dueDate"):
                if str(merged.get(key) or "") != str(existing[FIELDS[key]] or ""):
                    raise WorkflowError("Only an auditor or administrator can change the assignment", 403)
    # The closing date is recorded by the server, not chosen by the caller.
    merged["closedAt"] = today_date() if status == "Closed" else ""
    return merged
