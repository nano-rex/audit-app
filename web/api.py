"""Api for the audit application."""
import json
import mimetypes
import re
import time
import gzip
from http import cookies
from http.server import BaseHTTPRequestHandler
from urllib.parse import parse_qs, urlparse, unquote
from media_store import MediaStore
import config
from accounts import branding_settings, is_super_user, public_user
from catalog import equipment_items, locations, role_items, setup_records, users, zones
from common import checklist, inspection_name
from config import ROOT, SESSION_TOKENS, STATIC_LOCK
from database import connect
from http_support import api_errors, static_content, static_fingerprint
from inspections import inspection_session, inspection_sessions
from reports import dashboard, inspection_pdf, report, report_csv, report_xls
from response_cache import PreparedJson, cached_response
from work_orders import comments, finding_items, notifications, work_order_items
from routes import dispatch


class Handler(BaseHTTPRequestHandler):

    def end_headers(self):
        self.response_started = True
        super().end_headers()

    def setup(self):
        super().setup()
        self.connection.settimeout(30)

    def read_payload(self):
        try:
            if self.headers.get("Transfer-Encoding"):
                raise ValueError("Transfer-Encoding is not supported")
            length = int(self.headers.get("Content-Length", "0"))
            if length < 0:
                raise ValueError("Invalid Content-Length")
            if length > 20 * 1024 * 1024:
                self.json({"error": "Request body exceeds 20 MiB"}, 413)
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
        return jar.get("ottotree_session").value if jar.get("ottotree_session") else ""

    def current_user(self):
        token = self.session_token()
        session = SESSION_TOKENS.get(token)
        if not session or session["expires_at"] < time.time():
            if token:
                SESSION_TOKENS.pop(token, None)
            return None
        with connect() as db:
            row = db.execute(
                """
                SELECT id, name, role, email, department, active, reset_required,
                       last_login_at, login_count, title, responsibilities
                FROM users
                WHERE id = ? AND active = 1
                """,
                (session["user_id"],),
            ).fetchone()
        return public_user(row)

    def require_auth(self, parsed):
        if not parsed.path.startswith("/api/"):
            return True
        if parsed.path == "/api/branding":
            return True
        if parsed.path.startswith("/api/auth/"):
            return True
        user = self.current_user()
        if not user:
            self.json({"ok": False, "error": "Login required"}, status=401)
            return False
        if is_super_user(user):
            return True
        route = parsed.path.removeprefix("/api/").split("/", 1)[0]
        permissions = {
            "dashboard": {"today", "reports"},
            "reports": {"reports"},
            "inspection-sessions": {"inspections"},
            "inspections": {"inspections"},
            "audits": {"inspections"},
            "checklist": {"inspections"},
            "equipment": {"equipment"},
            "users": {"users"},
            "roles": {"roles"},
            "settings": {"settings"},
            "schedules": {"today"},
            "captain-logins": {"today"},
            "findings": {"findings"},
            "work-orders": {"work-orders", "corrective-actions"},
            "notifications": {"notifications"},
            "locations": {"outlets"},
            "zones": {"outlets"},
            "comments": {"findings", "work-orders", "corrective-actions", "inspections"},
        }
        if route == "setup":
            section = parsed.path.split("/")[3:4]
            required = {"priorities": "settings", "audit-types": "settings"}.get(section[0], section[0]) if section else None
            allowed = {required} if required else set()
        else:
            allowed = permissions.get(route, set())
        if self.command == "POST" and route == "work-orders":
            allowed |= {"inspections"}  # Inspectors can raise issues from failed criteria.
        if self.command == "GET":
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
        if parsed.path == "/api/auth/me":
            user = self.current_user()
            if not user:
                self.json({"ok": False, "error": "Login required"}, status=401)
                return
            self.json({"ok": True, "user": user})
            return
        if parsed.path == "/api/branding":
            self.json(branding_settings())
            return
        if parsed.path.startswith("/api/media/"):
            try:
                target = MediaStore(config.DATA_DIR / "media").path(parsed.path.removeprefix("/api/media/"))
            except ValueError:
                self.send_error(404)
                return
            if not target.is_file():
                self.send_error(404)
                return
            body = target.read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", mimetypes.guess_type(target.name)[0])
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "private, max-age=3600")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.end_headers()
            self.wfile.write(body)
            return
        if parsed.path == "/api/dashboard":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.json(cached_response(("dashboard", unit), lambda: dashboard(unit)))
            return
        if parsed.path == "/api/checklist":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.json({"items": checklist(unit)})
            return
        if parsed.path == "/api/work-orders":
            self.json(work_order_items())
            return
        if parsed.path == "/api/findings":
            self.json(finding_items())
            return
        if parsed.path == "/api/equipment":
            outlet = parse_qs(parsed.query).get("outlet", [None])[0]
            self.json(equipment_items(outlet))
            return
        if parsed.path == "/api/inspection-sessions":
            self.json(inspection_sessions())
            return
        if parsed.path.startswith("/api/inspection-sessions/"):
            suffix = parsed.path.rsplit("/", 1)[-1]
            if suffix == "export.pdf":
                session_id = parsed.path.split("/")[-2]
                if not session_id.isdigit():
                    self.send_error(400)
                    return
                session = inspection_session(int(session_id))
                if not session:
                    self.send_error(404)
                    return
                filename = f"{session.get('inspection_name') or inspection_name(session)}.pdf"
                self.download(inspection_pdf(session), "application/pdf", filename)
                return
            if not suffix.isdigit():
                self.send_error(400)
                return
            session = inspection_session(int(suffix))
            if not session:
                self.send_error(404)
                return
            self.json(session)
            return
        if parsed.path == "/api/locations":
            outlet = parse_qs(parsed.query).get("outlet", [""])[0]
            self.json(locations(outlet))
            return
        if parsed.path == "/api/zones":
            outlet = parse_qs(parsed.query).get("outlet", [""])[0]
            self.json(zones(outlet))
            return
        if parsed.path == "/api/reports":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.json(report(unit))
            return
        if parsed.path == "/api/reports/export.json":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.download(json.dumps(report(unit), indent=2).encode("utf-8"), "application/json", "audit-report.json")
            return
        if parsed.path == "/api/reports/export.csv":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.download(report_csv(unit), "text/csv", "audit-report.csv")
            return
        if parsed.path == "/api/reports/export.xls":
            unit = parse_qs(parsed.query).get("unit", ["Ottotree"])[0]
            self.download(report_xls(unit), "application/vnd.ms-excel", "audit-report.xls")
            return
        if parsed.path == "/api/setup":
            self.json(cached_response(("setup",), setup_records))
            return
        if parsed.path == "/api/roles":
            self.json(role_items())
            return
        if parsed.path == "/api/users":
            self.json(users())
            return
        if parsed.path == "/api/notifications":
            self.json(notifications())
            return
        if parsed.path == "/api/comments":
            params = parse_qs(parsed.query)
            self.json(comments(params.get("type", [""])[0], params.get("id", ["0"])[0]))
            return
        self.static_file(parsed.path)

    @api_errors
    def do_POST(self):
        parsed = urlparse(self.path)
        payload = self.read_payload()
        if payload is None:
            return
        if parsed.path.startswith("/api/auth/"):
            if not dispatch("POST", self, parsed, payload):
                self.send_error(404)
            return
        if not self.require_auth(parsed):
            return
        payload = MediaStore(config.DATA_DIR / "media").normalize(payload)
        if parsed.path == "/api/media":
            if not isinstance(payload.get("image"), dict) or not payload["image"].get("url"):
                self.json({"error": "An image is required"}, 400)
                return
            self.json({"image": payload["image"]})
            return
        if not dispatch("POST", self, parsed, payload):
            self.send_error(404)


    @api_errors
    def do_PATCH(self):
        parsed = urlparse(self.path)
        payload = self.read_payload()
        if payload is None or not self.require_auth(parsed):
            return
        payload = MediaStore(config.DATA_DIR / "media").normalize(payload)
        if not dispatch("PATCH", self, parsed, payload):
            self.send_error(404)


    @api_errors
    def do_DELETE(self):
        parsed = urlparse(self.path)
        if self.require_auth(parsed) and not dispatch("DELETE", self, parsed):
            self.send_error(404)


    def static_file(self, request_path):
        path = "index.html" if request_path in ("", "/") else unquote(request_path).lstrip("/")
        target = (ROOT / path).resolve()
        allowed = (path in {"index.html", "login.html", "register.html", "styles.css"}
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
        mime = mimetypes.guess_type(target.name)[0] or "application/octet-stream"
        if etag in [tag.strip() for tag in self.headers.get("If-None-Match", "").split(",")]:
            self.send_response(304)
            self.send_header("ETag", etag)
            self.send_header("Cache-Control", "public, max-age=0, must-revalidate")
            self.send_header("Vary", "Accept-Encoding")
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", mime + "; charset=utf-8")
        self.send_header("Cache-Control", "public, max-age=0, must-revalidate")
        self.send_header("ETag", etag)
        self.send_header("Vary", "Accept-Encoding")
        self.send_header("X-Content-Type-Options", "nosniff")
        if use_gzip:
            self.send_header("Content-Encoding", "gzip")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def json(self, payload, status=200):
        body = payload.body if isinstance(payload, PreparedJson) else json.dumps(payload, separators=(",", ":")).encode("utf-8")
        use_gzip = len(body) >= 1024 and self.accepts_gzip()
        if use_gzip:
            body = payload.compressed if isinstance(payload, PreparedJson) else gzip.compress(body, compresslevel=3, mtime=0)
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Vary", "Accept-Encoding")
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
