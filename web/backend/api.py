"""Api for the audit application."""
import json
import mimetypes
import re
import time
import gzip
from http import cookies
from http.server import BaseHTTPRequestHandler
from urllib.parse import parse_qs, urlparse, unquote
from backend.media_store import MediaStore
from backend import config
from backend.accounts import is_company_admin_user, branding_settings, is_super_user, public_user
from backend.catalog import user_login_activity, equipment_items, locations, role_items, setup_records, users, zones
from backend.database_manager import list_databases
from backend.common import inspection_name, read_setting
from backend.config import ROOT, SESSION_TOKENS, STATIC_LOCK
from backend.database import connect
from backend.http_support import api_errors, static_content, static_fingerprint
from backend.inspections import inspection_session, inspection_sessions, schedule_items
from backend.reports import dashboard, inspection_pdf, report, report_pdf, report_xls
from backend.response_cache import PreparedJson, cached_response
from backend.work_orders import comments, finding_items, notifications, work_order_items
from backend.work_requests import work_request_items
from backend import change_requests, outlet_access
from backend.permissions import CHANGE_RECORDS
from backend.audit_exports import inspection_locations_pdf, inspection_xlsx, location_pdf, location_xlsx
from backend.schedule_assignment import assignable_people
from backend.routes import dispatch
from backend import control
from backend.todo import todo_items


BODY_LIMIT = 20 * 1024 * 1024
# Sign-in forms are small; unauthenticated callers cannot make the server buffer uploads.
UNAUTHENTICATED_BODY_LIMIT = 64 * 1024


class Handler(BaseHTTPRequestHandler):

    def end_headers(self):
        self.response_started = True
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("X-Frame-Options", "SAMEORIGIN")
        self.send_header("Referrer-Policy", "same-origin")
        super().end_headers()

    def setup(self):
        super().setup()
        self.connection.settimeout(30)

    def handle_one_request(self):
        self.response_started = False
        self._current_user_loaded = False
        self._current_user_value = None
        return super().handle_one_request()

    def read_payload(self, limit=BODY_LIMIT):
        try:
            if self.headers.get("Transfer-Encoding"):
                raise ValueError("Transfer-Encoding is not supported")
            length = int(self.headers.get("Content-Length", "0"))
            if length < 0:
                raise ValueError("Invalid Content-Length")
            if length > limit:
                self.close_connection = True
                self.json({"error": f"Request body exceeds {limit // 1024 // 1024} MiB" if limit >= 1024 * 1024 else "Request body is too large"}, 413)
                return None
            payload = json.loads(self.rfile.read(length) or b"{}")
            if not isinstance(payload, dict):
                raise ValueError("JSON body must be an object")
            if "items" in payload and (not isinstance(payload["items"], list) or
                    any(not isinstance(item, dict) for item in payload["items"])):
                raise ValueError("items must be an array of objects")
            return payload
        except (ValueError, UnicodeError) as error:
            self.json({"error": str(error)}, 400)
            return None

    def discard_body(self):
        """Finish a refused request: drain a small unread body so the refusal is delivered, never a large one."""
        self.close_connection = True
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            return
        if 0 < length <= UNAUTHENTICATED_BODY_LIMIT:
            self.rfile.read(length)

    def accepts_gzip(self):
        for entry in self.headers.get("Accept-Encoding", "").split(","):
            parts = entry.strip().split(";")
            if parts[0].strip() == "gzip":
                return not any(re.fullmatch(r"q\s*=\s*0(?:\.0*)?", part.strip()) for part in parts[1:])
        return False

    def session_token(self):
        header = self.headers.get("Cookie", "")
        jar = cookies.SimpleCookie()
        try:
            jar.load(header)
        except cookies.CookieError:
            return ""
        cookie = jar.get(config.SESSION_COOKIE)
        return cookie.value if cookie else ""

    def current_user(self):
        if self._current_user_loaded:
            return self._current_user_value
        self._current_user_loaded = True
        token = self.session_token()
        if not token:
            return None
        with connect() as db:
            session = db.execute("SELECT user_id, expires_at FROM auth_sessions WHERE token_hash = ?", (SESSION_TOKENS.key(token),)).fetchone()
            if not session or session["expires_at"] < time.time():
                if session:
                    db.execute("DELETE FROM auth_sessions WHERE token_hash = ?", (SESSION_TOKENS.key(token),))
                    return None
                # Not an organization session: a Super account signs in through the control database.
                self._current_user_value = control.user_for_session(SESSION_TOKENS.key(token))
                return self._current_user_value
            row = db.execute(
                """
                SELECT id, name, username, role, email, department, active, reset_required,
                       last_login_at, login_count, title, responsibilities, profile_photo_data_id, signature_image_data_id
                FROM users
                WHERE id = ? AND active = 1
                """,
                (session["user_id"],),
            ).fetchone()
            self._current_user_value = public_user(row, db)
            return self._current_user_value

    def guard_outlets(self, parsed, payload):
        user = self.current_user()
        if outlet_access.allowed(user) is not None:
            with connect() as db:
                outlet_access.guard_mutation(db, user, self.command, parsed.path, payload)
        return True

    def hold_for_approval(self, parsed, payload):
        """Changes to managed records need the person's permission; without approval rights they wait for an approver."""
        record, record_id = change_requests.record_for(self.command, parsed.path)
        if not record:
            return False
        user = self.current_user()
        if not change_requests.allowed(user, record, "manage"):
            raise PermissionError(f"You cannot add, edit, or delete {CHANGE_RECORDS[record][0].lower()}")
        if change_requests.allowed(user, record, "approve"):
            return False
        self.json(change_requests.hold(user, self.command, parsed.path, payload, record, record_id))
        return True

    def require_auth(self, parsed):
        if not parsed.path.startswith("/api/"):
            return True
        if parsed.path == "/api/branding":
            return True
        if parsed.path.startswith("/api/auth/"):
            return True
        if parsed.path.startswith("/api/media/") and self.command == "GET":
            # The organization's logo is shown on the sign-in page, before anyone has a session.
            with connect() as db:
                if read_setting(db, "report.logoUrl", "") == parsed.path:
                    return True
        user = self.current_user()
        if not user:
            self.json({"ok": False, "error": "Login required"}, status=401)
            return False
        if user.get("resetRequired") and not (self.command == "GET" and parsed.path == "/api/account"):
            # The account holds a temporary or default password; nothing else is available until it is replaced.
            self.json({"ok": False, "error": "Change your password to continue", "resetRequired": True}, status=403)
            return False
        route = parsed.path.removeprefix("/api/").split("/", 1)[0]
        if ((route == "audits" and self.command == "POST") or
                (route == "inspection-sessions" and self.command == "DELETE")) and "auditor" not in user.get("inspectionPermissions", []):
            self.json({"error": "Auditor permission is required"}, 403)
            return False
        if is_super_user(user):
            return True
        route = parsed.path.removeprefix("/api/").split("/", 1)[0]
        if route == "settings" and is_company_admin_user(user):
            return True
        if route == "findings":
            with connect() as db:
                if read_setting(db, "system.findingsEnabled", True) is False:
                    self.json({"ok": False, "error": "Findings is turned off"}, 403)
                    return False
        permissions = {
            "dashboard": {"today", "reports"},
            "reports": {"reports"},
            "inspection-sessions": {"inspections"},
            "audits": {"inspections"},
            "equipment": {"equipment"},
            "users": {"users"},
            "roles": {"roles"},
            "settings": {"settings"},
            "schedules": {"today", "inspections"},
            "findings": {"findings", "inspections"},
            "work-orders": {"work-orders"},
            "work-requests": {"work-orders", "findings"},
            "notifications": {"notifications"},
            "locations": {"outlets"},
            "zones": {"outlets"},
            "comments": {"findings", "work-orders", "inspections"},
            "location-report.pdf": {"reports"},
            "location-report.xlsx": {"reports"},
        }
        if route == "setup":
            section = parsed.path.split("/")[3:4]
            required = {"priorities": "settings", "audit-types": "settings"}.get(section[0], section[0]) if section else None
            allowed = {required} if required else set()
        else:
            allowed = permissions.get(route, set())
        if route == "notifications" and self.command in {"GET", "PATCH", "DELETE"}:
            allowed = set()  # Each user can access their own addressed notifications.
        if self.command == "POST" and route == "work-requests":
            allowed |= {"inspections"}  # Inspectors can request work for what failed.
        if self.command == "GET":
            if route == "inspection-sessions":
                allowed |= {"findings", "history"}
            if route in {"setup", "locations", "zones"}:
                return True  # Shared selection lists used by the permitted workflows.
            if route == "equipment":
                allowed |= {"inspections", "outlets"}
        if allowed and not allowed.intersection(user.get("permissions", [])):
            self.json({"ok": False, "error": "You do not have access to this section"}, 403)
            return False
        return True

    @api_errors
    def do_GET(self):
        parsed = urlparse(self.path)
        if not self.require_auth(parsed):
            return
        if parsed.path in {"/api/auth/me", "/api/account"}:
            user = self.current_user()
            if not user:
                self.json({"ok": False, "error": "Login required"}, status=401)
                return
            self.json({"ok": True, "user": user})
            return
        if parsed.path == "/api/account/databases":
            if not is_super_user(self.current_user()):
                self.json({"error": "Super access required"}, 403)
                return
            self.json({"databases": list_databases()})
            return
        if parsed.path == "/api/branding":
            self.json(branding_settings())
            return
        if parsed.path.startswith("/api/media/"):
            try:
                store, identifier = MediaStore(config.DB_PATH), parsed.path.removeprefix("/api/media/")
                if parse_qs(parsed.query).get("thumb"):
                    thumbnail = store.thumbnail(identifier)
                    stored = (thumbnail, "image/jpeg") if thumbnail is not None else None
                else:
                    stored = store.read(identifier)
            except ValueError:
                self.send_error(404)
                return
            if stored is None:
                self.send_error(404)
                return
            body, mime = stored
            self.send_response(200)
            self.send_header("Content-Type", mime)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "private, max-age=3600")
            self.end_headers()
            self.wfile.write(body)
            return
        viewer = self.current_user()
        scope = outlet_access.allowed(viewer)
        # Outlet-limited accounts get their own cached copy; everyone else shares one.
        scope_key = tuple(sorted(scope)) if scope is not None else ("*",)
        listed = lambda data, key="outlet": {**data, "items": outlet_access.keep(viewer, data["items"], key)}
        if parsed.path == "/api/dashboard":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            filters = {"outlets": sorted(scope)} if scope is not None else None
            self.json(cached_response(("dashboard", unit, *scope_key), lambda: dashboard(unit, filters)))
            return
        if parsed.path == "/api/todo":
            self.json(listed(todo_items(viewer)))
            return
        if parsed.path == "/api/schedules":
            self.json(listed(schedule_items()))
            return
        if parsed.path == "/api/schedules/assignees":
            outlet = parse_qs(parsed.query).get("outlet", [""])[0]
            if outlet:
                outlet_access.require(viewer, outlet)
            with connect() as db:
                self.json({"items": assignable_people(db, outlet)})
            return
        if parsed.path == "/api/work-orders":
            self.json(listed(work_order_items(viewer)))
            return
        if parsed.path == "/api/work-requests":
            self.json(listed(work_request_items(viewer)))
            return
        if parsed.path == "/api/changes":
            self.json(change_requests.change_items(viewer))
            return
        if parsed.path == "/api/findings":
            self.json(listed(finding_items(user=viewer)))
            return
        if parsed.path == "/api/equipment":
            outlet = parse_qs(parsed.query).get("outlet", [None])[0]
            if outlet:
                outlet_access.require(viewer, outlet)
            compact = parse_qs(parsed.query).get("view", [""])[0] == "inspection"
            data = equipment_items(outlet, compact)
            self.json(data if scope is None else listed(data))
            return
        if parsed.path == "/api/inspection-sessions":
            self.json(listed(inspection_sessions()))
            return
        if parsed.path.startswith("/api/inspection-sessions/"):
            suffix = parsed.path.rsplit("/", 1)[-1]
            if suffix == "export.xlsx":
                session_id = parsed.path.split("/")[-2]
                if not session_id.isdigit():
                    self.send_error(400)
                    return
                session = inspection_session(int(session_id))
                if not session:
                    self.send_error(404)
                    return
                outlet_access.require(viewer, session["outlet"])
                filename = f"{session.get('inspection_name') or inspection_name(session)}.xlsx"
                self.download(inspection_xlsx(session), "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", filename)
                return
            if suffix == "export.pdf":
                session_id = parsed.path.split("/")[-2]
                if not session_id.isdigit():
                    self.send_error(400)
                    return
                session = inspection_session(int(session_id))
                if not session:
                    self.send_error(404)
                    return
                outlet_access.require(viewer, session["outlet"])
                # Overall, or the chosen locations (?location=A&location=B) in full.
                chosen = parse_qs(parsed.query).get("location", [])
                name = session.get('inspection_name') or inspection_name(session)
                if chosen:
                    suffix = chosen[0] if len(chosen) == 1 else f"{len(chosen)} locations"
                    self.download(inspection_locations_pdf(session, chosen), "application/pdf", f"{name} - {suffix}.pdf")
                else:
                    self.download(inspection_pdf(session), "application/pdf", f"{name}.pdf")
                return
            if not suffix.isdigit():
                self.send_error(400)
                return
            session = inspection_session(int(suffix))
            if not session:
                self.send_error(404)
                return
            outlet_access.require(viewer, session["outlet"])
            self.json(session)
            return
        if parsed.path == "/api/locations":
            outlet = parse_qs(parsed.query).get("outlet", [""])[0]
            if outlet:
                outlet_access.require(viewer, outlet)
            self.json(listed(locations(outlet, brief=parse_qs(parsed.query).get("brief") == ["1"]), "outlet_code"))
            return
        if parsed.path == "/api/zones":
            outlet = parse_qs(parsed.query).get("outlet", [""])[0]
            if outlet:
                outlet_access.require(viewer, outlet)
            self.json(listed(zones(outlet), "outlet_code"))
            return
        if parsed.path in {"/api/location-report.pdf", "/api/location-report.xlsx"}:
            query = {key: parse_qs(parsed.query).get(key, [""])[0] for key in ("outlet", "location", "from", "to")}
            if not query["outlet"] or not query["location"]:
                raise ValueError("Choose an outlet and a location for the location report")
            outlet_access.require(viewer, query["outlet"])
            name = f"{query['outlet']}_{query['location']}".replace("/", "-")
            if parsed.path.endswith(".pdf"):
                self.download(location_pdf(query["outlet"], query["location"], query["from"], query["to"]), "application/pdf", f"{name}.pdf")
            else:
                self.download(location_xlsx(query["outlet"], query["location"], query["from"], query["to"]),
                              "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", f"{name}.xlsx")
            return
        report_filters = {key: parse_qs(parsed.query).get(key, [""])[0] for key in ("outlet", "from", "to")}
        if parsed.path.startswith("/api/reports"):
            if report_filters["outlet"]:
                outlet_access.require(viewer, report_filters["outlet"])
            if scope is not None:
                report_filters["outlets"] = sorted(scope)
        if parsed.path == "/api/reports":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.json(cached_response(("report", unit, report_filters["outlet"], report_filters["from"], report_filters["to"], *scope_key), lambda: report(unit, report_filters)))
            return
        if parsed.path == "/api/reports/export.pdf":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.download(report_pdf(unit, report_filters), "application/pdf", "audit-report.pdf")
            return
        if parsed.path in {"/api/reports/export.xls", "/api/reports/export.xlsx"}:
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.download(report_xls(unit, report_filters), "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "audit-report.xlsx")
            return
        if parsed.path == "/api/setup":
            data = cached_response(("setup", is_super_user(viewer)), lambda: setup_records(is_super_user(viewer)))
            if scope is not None:
                data = {**data, "outlets": [row for row in data["outlets"] if row["code"] in scope],
                        "zones": [row for row in data["zones"] if row["outlet_code"] in scope]}
            self.json(data)
            return
        if parsed.path == "/api/roles":
            self.json(role_items(is_super_user(self.current_user())))
            return
        activity = re.fullmatch(r"/api/users/(\d+)/activity", parsed.path)
        if activity:
            if not is_company_admin_user(self.current_user()):
                raise PermissionError("Admin access required")
            offset = parse_qs(parsed.query).get("offset", ["0"])[0]
            self.json(user_login_activity(int(activity.group(1)), offset))
            return
        if parsed.path == "/api/users":
            self.json(users(is_super_user(self.current_user())))
            return
        if parsed.path == "/api/notifications":
            self.json(notifications(self.current_user()["id"]))
            return
        if parsed.path == "/api/comments":
            params = parse_qs(parsed.query)
            if scope is not None and params.get("type", [""])[0] == "work_order" and params.get("id", [""])[0].isdigit():
                with connect() as db:
                    outlet_access.require(viewer, outlet_access.record_outlet(db, "work-orders", params["id"][0]))
            self.json(comments(params.get("type", [""])[0], params.get("id", ["0"])[0]))
            return
        self.static_file(parsed.path)

    @api_errors
    def do_POST(self):
        parsed = urlparse(self.path)
        if parsed.path.startswith("/api/auth/"):
            payload = self.read_payload(UNAUTHENTICATED_BODY_LIMIT)
            if payload is not None and not dispatch("POST", self, parsed, payload):
                self.send_error(404)
            return
        if not self.require_auth(parsed):
            self.discard_body()
            return
        payload = self.read_payload()
        if payload is None:
            return
        payload = MediaStore(config.DB_PATH).normalize(payload)
        if parsed.path == "/api/media":
            if not isinstance(payload.get("image"), dict) or not payload["image"].get("url"):
                self.json({"error": "An image is required"}, 400)
                return
            self.json({"image": payload["image"]})
            return
        self.guard_outlets(parsed, payload)
        if self.hold_for_approval(parsed, payload):
            return
        if not dispatch("POST", self, parsed, payload):
            self.send_error(404)


    @api_errors
    def do_PATCH(self):
        parsed = urlparse(self.path)
        if not self.require_auth(parsed):
            self.discard_body()
            return
        payload = self.read_payload()
        if payload is None:
            return
        payload = MediaStore(config.DB_PATH).normalize(payload)
        self.guard_outlets(parsed, payload)
        if self.hold_for_approval(parsed, payload):
            return
        if not dispatch("PATCH", self, parsed, payload):
            self.send_error(404)


    @api_errors
    def do_DELETE(self):
        parsed = urlparse(self.path)
        if self.require_auth(parsed) and self.guard_outlets(parsed, None) and not self.hold_for_approval(parsed, None) and not dispatch("DELETE", self, parsed):
            self.send_error(404)


    def static_file(self, request_path):
        path = "index.html" if request_path in ("", "/") else unquote(request_path).lstrip("/")
        target = (ROOT / path).resolve()
        allowed = (path in {"index.html", "login.html", "register.html"}
                   or (path.startswith("fonts/") and target.is_relative_to(ROOT / "fonts") and target.suffix == ".woff2")
                   or (path.startswith("js/") and target.is_relative_to(ROOT / "js") and target.suffix == ".js")
                   or (path.startswith("css/") and target.is_relative_to(ROOT / "css") and target.suffix == ".css"))
        if not allowed or not target.is_relative_to(ROOT) or not target.is_file():
            self.send_error(404)
            return
        with STATIC_LOCK:
            fingerprint = static_fingerprint(target, int(time.monotonic()))
            body, compressed, etag = static_content(target, fingerprint)
        use_gzip = self.accepts_gzip()
        if use_gzip:
            body = compressed
            etag = etag[:-1] + '-gzip"'
        mime = "font/woff2" if target.suffix == ".woff2" else mimetypes.guess_type(target.name)[0] or "application/octet-stream"
        if etag in [tag.strip() for tag in self.headers.get("If-None-Match", "").split(",")]:
            self.send_response(304)
            self.send_header("ETag", etag)
            self.send_header("Cache-Control", "public, max-age=0, must-revalidate")
            self.send_header("Vary", "Accept-Encoding")
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", mime if mime.startswith("font/") else mime + "; charset=utf-8")
        # Fonts never change in place; everything else is revalidated so edits show at once.
        self.send_header("Cache-Control", "public, max-age=604800, immutable" if mime.startswith("font/") else "public, max-age=0, must-revalidate")
        self.send_header("ETag", etag)
        self.send_header("Vary", "Accept-Encoding")
        if use_gzip:
            self.send_header("Content-Encoding", "gzip")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def json(self, payload, status=200, headers=()):
        body = payload.body if isinstance(payload, PreparedJson) else json.dumps(payload, separators=(",", ":")).encode("utf-8")
        use_gzip = len(body) >= 1024 and self.accepts_gzip()
        if use_gzip:
            body = payload.compressed if isinstance(payload, PreparedJson) else gzip.compress(body, compresslevel=3, mtime=0)
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Vary", "Accept-Encoding")
        for name, value in headers:
            self.send_header(name, value)
        if use_gzip:
            self.send_header("Content-Encoding", "gzip")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def download(self, body, mime, filename):
        filename = re.sub(r"[^A-Za-z0-9._ -]", "_", filename)
        self.send_response(200)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Disposition", f'attachment; filename="{filename}"')
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
