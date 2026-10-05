"""Which outlets an account may see and change.

A role either covers every outlet ("all"), or limits its users to one outlet ("one") or to
the several outlets assigned to them ("several"). Lists are filtered to those outlets, and a
change or a single record outside them is refused.
"""
from backend.config import SUPER_ROLE

SCOPES = {"all": "All outlets", "one": "One outlet", "several": "Selected outlets"}

# Records reached by /api/<route>/<id>, and the column naming their outlet.
RECORDS = {
    "schedules": ("schedules", "outlet"),
    "inspection-sessions": ("inspection_sessions", "outlet"),
    "work-orders": ("work_orders", "outlet"),
    "work-requests": ("work_requests", "outlet"),
    "equipment": ("equipment", "outlet"),
    "locations": ("locations", "outlet_code"),
    "zones": ("zones", "outlet_code"),
}
DENIED = "That outlet is not assigned to your account"


def role_scope(db, role_name):
    if role_name == SUPER_ROLE:
        return "all"
    row = db.execute("SELECT outlet_scope FROM roles WHERE name = ?", (role_name,)).fetchone()
    return (row["outlet_scope"] if row else None) or "all"


def validate_scope(value):
    value = value or "all"
    if value not in SCOPES:
        raise ValueError("Choose all outlets, one outlet, or selected outlets")
    return value


def validate_user_outlets(db, scope, outlets):
    """The outlets kept on a user: none for an all-outlet role, otherwise existing outlet codes."""
    if scope == "all":
        return []
    if not isinstance(outlets, list) or any(not isinstance(code, str) for code in outlets):
        raise ValueError("Outlets must be a list of outlet codes")
    outlets = list(dict.fromkeys(outlets))
    known = {row[0] for row in db.execute("SELECT code FROM outlets")}
    if any(code not in known for code in outlets):
        raise ValueError("Select existing outlets")
    if scope == "one" and len(outlets) != 1:
        raise ValueError("This role works at exactly one outlet; choose it")
    if scope == "several" and not outlets:
        raise ValueError("Choose the outlets this person covers")
    return outlets


def allowed(user):
    """None when the account sees every outlet, otherwise the set of outlet codes it may see."""
    if not user or user.get("outletScope", "all") == "all":
        return None
    return set(user.get("outlets") or [])


def require(user, outlet):
    scope = allowed(user)
    if scope is not None and outlet not in scope:
        raise PermissionError(DENIED)


def keep(user, rows, key="outlet"):
    scope = allowed(user)
    return rows if scope is None else [row for row in rows if row.get(key) in scope]


def record_outlet(db, route, record_id):
    table, column = RECORDS[route]
    row = db.execute(f"SELECT {column} FROM {table} WHERE id = ?", (int(record_id),)).fetchone()
    return row[0] if row else None


def guard_mutation(db, user, method, path, payload):
    """Refuse a change that would read or write a record at an outlet the account may not see."""
    scope = allowed(user)
    if scope is None:
        return
    payload = payload or {}
    parts = path.removeprefix("/api/").split("/")
    route = parts[0]
    if route == "setup" and parts[1:2] == ["outlets"]:
        raise PermissionError("Outlets are managed by an account that covers every outlet")
    if route == "users" or route == "roles":
        return
    record_id = parts[1] if len(parts) > 1 and parts[1].isdigit() else None
    if record_id and route in RECORDS:
        outlet = record_outlet(db, route, record_id)
        if outlet is not None and outlet not in scope:
            raise PermissionError(DENIED)
    if "outlet" in payload and payload["outlet"] not in scope:
        raise PermissionError(DENIED)
    linked = []
    if path == "/api/schedules/start":
        linked.append(record_outlet(db, "schedules", payload.get("scheduleId") or 0))
    elif path == "/api/inspection-sessions/close":
        linked.append(record_outlet(db, "inspection-sessions", payload.get("id") or 0))
    elif path == "/api/work-orders" and method == "POST":
        linked.append(record_outlet(db, "work-requests", payload.get("workRequestId") or 0))
    elif path == "/api/work-requests" and method == "POST":
        ids = [int(value) for value in payload.get("findingIds") or [] if str(value).isdigit()]
        if ids:
            marks = ",".join("?" for _ in ids)
            linked += [row[0] for row in db.execute(f"SELECT outlet FROM findings WHERE id IN ({marks})", ids)]
    elif path == "/api/comments" and method == "POST" and payload.get("recordType") == "work_order":
        linked.append(record_outlet(db, "work-orders", payload.get("recordId") or 0))
    if any(outlet is not None and outlet not in scope for outlet in linked):
        raise PermissionError(DENIED)
    # Creating a record without naming an outlet would fall back to the first outlet, which may be another's.
    if method == "POST" and route in RECORDS and record_id is None and path == f"/api/{route}" \
            and route not in ("work-orders", "work-requests") and not payload.get("outlet"):
        raise PermissionError("Choose one of your outlets")
    if path == "/api/work-requests" and method == "POST" and not payload.get("findingIds") and payload.get("outlet") not in scope:
        raise PermissionError(DENIED)
