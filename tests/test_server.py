"""Regression tests against an isolated database and loopback HTTP server."""
import concurrent.futures
import gzip
import hashlib
import http.client
import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import threading
import time
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("audit_server", Path(__file__).resolve().parents[1] / "web/server.py")
app = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(app)
from backend.relational_values import save_value, load_value


from backend.relational_values import save_value as app_save_value


class QuietHandler(app.Handler):
    def log_message(self, *args):
        pass


class ServerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.storage = tempfile.TemporaryDirectory(prefix="audit-tests-")
        app.configure_data_directory(cls.storage.name)
        app.init_db()
        with app.connect() as db:
            # Starter accounts must change their password before using the API; these tests act as them directly.
            db.execute("UPDATE users SET reset_required = 0")
            db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Restricted', 'Auditor', 'restricted@test', 1, 0)")
            limited_id = db.execute("SELECT id FROM users WHERE email = 'restricted@test'").fetchone()[0]
            cls.admin_id = db.execute("SELECT id FROM users WHERE role = 'Admin'").fetchone()[0]
        # The Super account lives in the control database, not in the organization.
        cls.user_id = app.first_super_id()
        from backend.control import connect_control
        with connect_control() as control_db:
            control_db.execute("UPDATE super_users SET reset_required = 0")
        app.SUPER_SESSIONS["test"] = {"user_id": cls.user_id, "expires_at": time.time() + 3600}
        app.SESSION_TOKENS["limited"] = {"user_id": limited_id, "expires_at": time.time() + 3600}
        cls.server = app.AuditHTTPServer(("127.0.0.1", 0), QuietHandler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()
        cls.storage.cleanup()

    def outlet_role(self, name, permissions=("today", "inspections", "findings", "work-orders", "equipment", "notifications"), capabilities=("auditor",)):
        status, _, body = self.request("/api/roles", "POST", {"name": name, "permissions": list(permissions), "inspectionPermissions": list(capabilities)})
        self.assertEqual(status, 200, body)
        return name

    @staticmethod
    def limit_to(user_id, outlets):
        """The person sees only these outlets."""
        with app.connect() as db:
            db.execute("UPDATE users SET outlets_data_id = ? WHERE id = ?", (app_save_value(db, outlets), user_id))

    def from_request(self, order=None, **request):
        """A work order is made from a work request: raise one, then point the order at it."""
        fields = {"outlet": "STP", "location": "Room", "itemName": "Test item", "description": "Needs work"} | request
        status, _, body = self.request("/api/work-requests", "POST", fields)
        self.assertEqual(status, 200, body)
        return (order or {}) | {"workRequestId": json.loads(body)["id"]}

    def request(self, path, method="GET", payload=None, token="test", headers=None, raw=None):
        connection = http.client.HTTPConnection("127.0.0.1", self.server.server_port, timeout=30)
        request_headers = {"Cookie": f"ottotree_session={token}"} if token else {}
        request_headers.update(headers or {})
        body = raw if raw is not None else (json.dumps(payload) if payload is not None else None)
        connection.request(method, path, body, request_headers)
        response = connection.getresponse()
        result = response.status, dict(response.getheaders()), response.read()
        connection.close()
        return result

    def evidence(self):
        """One stored photo, reused wherever a completed inspection needs evidence."""
        if not getattr(type(self), "_evidence", None):
            from test_media_reports import photo_data_url
            _, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "evidence.png"}})
            type(self)._evidence = json.loads(body)["image"]
        return dict(type(self)._evidence)

    def test_audit_closure_requires_signatures_and_closed_actions_and_is_immutable(self):
        from test_media_reports import photo_data_url
        payload = {"outlet": "STP", "auditDate": "2026-09-18", "items": [{"section": "Safety", "item": "Door", "passed": True, "images": [self.evidence()]}]}
        status, _, body = self.request("/api/inspection-sessions", "POST", payload)
        self.assertEqual(status, 200, body)
        session_id = json.loads(body)["id"]
        path = f"/api/inspection-sessions/{session_id}"
        close = lambda token="test": self.request("/api/inspection-sessions/close", "POST", {"id": session_id}, token=token)
        with app.connect() as db:
            override = save_value(db, {"permissions": ["inspections"], "inspectionPermissions": ["auditor"]})
            user_id = db.execute("INSERT INTO users(name,role,email,active,created_at,permission_overrides_data_id) VALUES ('Closure auditor','Auditor','closure@example.test',1,0,?)", (override,)).lastrowid
        app.SESSION_TOKENS["closure-auditor"] = {"user_id": user_id, "expires_at": time.time() + 3600}
        self.assertEqual(close("closure-auditor")[0], 403)
        self.assertEqual(close()[0], 409)
        self.assertEqual(self.request(path, "PATCH", {"complete": True})[0], 200)
        self.assertEqual(close()[0], 409)
        _, _, body = self.request("/api/media", "POST", {"image": {"name": "signature.png", "dataUrl": photo_data_url()}})
        signature = json.loads(body)["image"]
        signatures = {key: signature for key in ("auditedBy", "verifiedBy", "acknowledgedBy")}
        self.assertEqual(self.request(path, "PATCH", {"signatures": signatures})[0], 200)
        with app.connect() as db:
            audit_id = db.execute("SELECT audit_id FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()[0]
            order_id = db.execute("INSERT INTO work_orders(business_unit,outlet,zone,request_type,priority,title,assignee,status,created_at,source_audit_id) VALUES ('Ottotree','STP','Room','TECH','High','Repair','Tester','Assigned',0,?)", (audit_id,)).lastrowid
        self.assertEqual(close()[0], 409)
        with app.connect() as db:
            db.execute("UPDATE work_orders SET status = 'Closed' WHERE id = ?", (order_id,))
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(lambda _: close(), range(2)))
        self.assertTrue(all(result[0] == 200 for result in results), results)
        self.assertEqual(self.request(path, "PATCH", {"signatures": {}})[0], 409)
        self.assertEqual(self.request(path, "DELETE")[0], 409)
        _, _, body = self.request(path)
        session = json.loads(body)
        self.assertTrue(session["closed_at"])
        self.assertTrue(session["closed_by"])
        with app.connect() as db:
            count = db.execute("SELECT COUNT(*) FROM comments WHERE record_type = 'inspection' AND record_id = ? AND comment = 'Audit closed'", (session_id,)).fetchone()[0]
        self.assertEqual(count, 1)

    def test_administration_keeps_super_out_and_revokes_deactivated_sessions(self):
        with app.connect() as db:
            self.assertIsNone(db.execute("SELECT 1 FROM users WHERE role = 'Super'").fetchone(), "no Super account inside an organization")
        self.assertEqual(self.request("/api/users", "POST", {"name": "Would be super", "email": "would-be@example.test", "role": "Super"})[0], 400)
        self.assertEqual(self.request(f"/api/users/{self.admin_id}", "PATCH", {"role": "Super"})[0], 400)
        self.assertEqual(self.request("/api/users", "POST", {"name": "Copycat", "email": "copycat@example.test", "username": "super"})[0], 409)
        # The starter Super address is also not a valid organization address, so it may fail either check.
        self.assertIn(self.request("/api/users", "POST", {"name": "Copycat", "email": "super@sudo"})[0], {400, 409})
        user = {"name": "Lifecycle User", "email": "lifecycle@example.test", "role": "Auditor", "active": True}
        self.assertEqual(self.request("/api/users", "POST", user)[0], 200)
        with app.connect() as db:
            identifier = db.execute("SELECT id FROM users WHERE email = ?", (user["email"],)).fetchone()[0]
        app.SESSION_TOKENS["lifecycle"] = {"user_id": identifier, "expires_at": time.time() + 3600}
        target = f"/api/users/{identifier}"
        self.assertEqual(self.request(target, "PATCH", {"active": False})[0], 200)
        self.assertIsNone(app.SESSION_TOKENS.get("lifecycle"))
        self.assertEqual(self.request(target, "PATCH", {"active": True})[0], 200)
        self.assertEqual(self.request("/api/account", token="lifecycle")[0], 401)
        with app.connect() as db:
            row = db.execute("SELECT name, email, role FROM users WHERE id = ?", (identifier,)).fetchone()
            self.assertEqual(tuple(row), (user["name"], user["email"], user["role"]))
            db.execute("INSERT INTO user_login_activity(user_id,email,logged_at) VALUES (?,?,'2026-09-20')", (identifier, user["email"]))
        self.assertEqual(self.request(target, "DELETE")[0], 409)
        self.assertEqual(self.request(target, "PATCH", {"active": False})[0], 200)

    def test_location_integrity_duplicates_rename_rollback_and_history(self):
        for code in ("HIER-A", "HIER-B"):
            self.assertEqual(self.request("/api/setup/outlets", "POST", {"code": code})[0], 200)
        location = {"outlet": "HIER-A", "name": "Room One", "floor": "1", "area": "East", "displayOrder": 7}
        self.assertEqual(self.request("/api/locations", "POST", location)[0], 200)
        with app.connect() as db:
            identifier = db.execute("SELECT id FROM locations WHERE outlet_code = 'HIER-A' AND name = 'Room One'").fetchone()[0]
        target = f"/api/locations/{identifier}"
        self.assertEqual(self.request("/api/locations", "POST", location)[0], 409)
        self.assertEqual(self.request("/api/zones", "POST", {"outlet": "HIER-B", "name": "Invalid", "locations": ["Room One"]})[0], 400)
        self.assertEqual(self.request("/api/zones", "POST", {"outlet": "HIER-A", "name": "Custom", "locations": ["Room One"]})[0], 200)
        asset = {"outlet": "HIER-A", "location": "Room One", "zone": "Room One", "name": "Hierarchy asset", "code": "HIER-ASSET"}
        self.assertEqual(self.request("/api/equipment", "POST", asset)[0], 200)
        self.assertEqual(self.request(target, "PATCH", {"name": "Room Two"})[0], 200)
        with app.connect() as db:
            room = db.execute("SELECT * FROM locations WHERE id = ?", (identifier,)).fetchone()
            self.assertEqual((room["floor"], room["area"], room["display_order"]), ("1", "East", 7))
            zone = db.execute("SELECT locations_data_id FROM zones WHERE outlet_code = 'HIER-A' AND name = 'Custom'").fetchone()
            self.assertEqual(load_value(zone[0]), ["Room Two"])
            equipment = db.execute("SELECT id,location,zone FROM equipment WHERE code = 'HIER-ASSET'").fetchone()
            self.assertEqual(tuple(equipment)[1:], ("Room Two", "Room Two"))
            asset_id = equipment["id"]
        self.assertEqual(self.request(target, "DELETE")[0], 409)
        self.assertEqual(self.request("/api/locations", "POST", {"outlet": "HIER-B", "name": "Wrong asset", "equipmentIds": [asset_id]})[0], 400)
        with app.connect() as db:
            self.assertIsNone(db.execute("SELECT id FROM locations WHERE name = 'Wrong asset'").fetchone())
            outlet_id = db.execute("SELECT id FROM outlets WHERE code = 'HIER-A'").fetchone()[0]
        self.assertEqual(self.request(f"/api/setup/outlets/{outlet_id}", "PATCH", {"code": "HIER-C"})[0], 200)
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT outlet FROM equipment WHERE id = ?", (asset_id,)).fetchone()[0], "HIER-C")
        self.assertEqual(self.request("/api/schedules", "POST", {"outlet": "HIER-C", "zone": "Room Two", "scheduledDate": "2026-09-20"})[0], 200)
        self.assertEqual(self.request(target, "PATCH", {"name": "Lost history"})[0], 409)
        self.assertEqual(self.request(f"/api/setup/outlets/{outlet_id}", "DELETE")[0], 409)
        self.assertEqual(self.request(target, "PATCH", {"floor": "2"})[0], 200)

    def test_login_activity_is_admin_only_and_paginated(self):
        with app.connect() as db:
            identifier = db.execute("INSERT INTO users(name,role,email,active,created_at) VALUES ('Activity Test','Auditor','activity@example.test',1,0)").lastrowid
            db.executemany("INSERT INTO user_login_activity(user_id,email,logged_at,remember_me,user_agent) VALUES (?,?,'2026-09-20',1,'Test browser')", [(identifier, 'activity@example.test')] * 55)
        path = f"/api/users/{identifier}/activity"
        self.assertEqual(self.request(path, token="limited")[0], 403)
        status, _, body = self.request(path)
        self.assertEqual(status, 200)
        first = json.loads(body)
        self.assertEqual(len(first["items"]), 50)
        self.assertTrue(first["hasMore"])
        _, _, body = self.request(path + "?offset=50")
        second = json.loads(body)
        self.assertEqual(len(second["items"]), 5)
        self.assertFalse(second["hasMore"])
        self.assertGreater(first["items"][-1]["id"], second["items"][0]["id"])
        self.assertEqual(self.request(path + "?offset=invalid")[0], 400)

    def test_navigation_order_is_persistent_and_account_scoped(self):
        path = "/api/account/navigation"
        order = ["account", "reports", "today", "notifications"]
        self.assertEqual(self.request(path, "PATCH", {"order": order}, token=None)[0], 401)
        self.assertEqual(self.request(path, "PATCH", {"order": order})[0], 200)
        _, _, body = self.request("/api/auth/me")
        self.assertEqual(json.loads(body)["user"]["navigationOrder"], order)
        _, _, body = self.request("/api/auth/me", token="limited")
        self.assertEqual(json.loads(body)["user"]["navigationOrder"], [])
        for invalid in (["today", "today"], ["unknown"], ["roles"], "today", [1], [{}]):
            self.assertEqual(self.request(path, "PATCH", {"order": invalid})[0], 400)
        from backend.control import connect_control
        with connect_control() as db:
            stored = [row[0] for row in db.execute("SELECT page_id FROM super_navigation WHERE user_id = ? ORDER BY position", (self.user_id,))]
        self.assertEqual(stored, order)
        self.assertEqual(self.request(path, "PATCH", {"order": ["notifications", "account"], "userId": self.user_id}, token="limited")[0], 200)
        _, _, body = self.request("/api/auth/me")
        self.assertEqual(json.loads(body)["user"]["navigationOrder"], order)
        self.assertEqual(self.request(path, "PATCH", {"order": []})[0], 200)

    def test_static_allowlist(self):
        for path in ("/data/ottotree_audit_web.db", "/server.py", "/README.md", "/js/../server.py", "/js/%2e%2e/server.py"):
            self.assertEqual(self.request(path, token=None)[0], 404, path)
        self.assertEqual(self.request("/", token=None)[0], 200)

    def test_users_page_contains_nested_management_sections(self):
        status, _, body = self.request("/", token=None)
        self.assertEqual(status, 200)
        html = body.decode()
        self.assertNotIn("<!-- include:", html)
        self.assertIn('id="department-search"', html)
        self.assertIn('id="role-search"', html)
        self.assertIn('data-user-panel="departments"', html)
        self.assertIn('data-user-panel="roles"', html)
        self.assertNotIn('data-tab="departments"', html)
        self.assertNotIn('data-tab="roles"', html)
        self.assertIn('data-guided-content hidden', html)
        self.assertIn('data-guided-schedules-panel', html)
        self.assertNotIn('data-inspection-subtab="history"', html)
        # History and Findings are separate pages under Inspections.
        self.assertIn('id="history" class="tab-panel"', html)
        self.assertIn('id="findings" class="tab-panel"', html)
        self.assertNotIn('History &amp; Findings', html)

    def test_static_compression_and_revalidation(self):
        status, headers, body = self.request("/", headers={"Accept-Encoding": "gzip"})
        self.assertEqual(status, 200)
        self.assertIn(b"Guided Inspection", gzip.decompress(body))
        status, _, body = self.request("/", headers={"Accept-Encoding": "gzip", "If-None-Match": headers["ETag"]})
        self.assertEqual((status, body), (304, b""))
        _, headers, _ = self.request("/", headers={"Accept-Encoding": "gzip;q=0"})
        self.assertNotIn("Content-Encoding", headers)

    def test_database_connections_enforce_valid_references(self):
        with app.connect() as db:
            self.assertEqual(db.execute("PRAGMA foreign_keys").fetchone()[0], 1)
            self.assertEqual(db.execute("PRAGMA foreign_key_check").fetchall(), [])

    def test_authentication_context_is_loaded_once_per_request(self):
        from unittest.mock import patch
        with patch("backend.api.connect", wraps=app.connect) as connect_spy:
            status, _, body = self.request("/api/account")
        self.assertEqual(status, 200, body)
        self.assertEqual(connect_spy.call_count, 1)

    def test_connection_closes_and_rolls_back(self):
        with app.connect() as db:
            self.assertEqual(db.execute("PRAGMA journal_mode").fetchone()[0], "wal")
        with self.assertRaises(sqlite3.ProgrammingError):
            db.execute("SELECT 1")
        with self.assertRaises(RuntimeError):
            with app.connect() as db:
                db.execute("UPDATE users SET name = 'Must roll back' WHERE id = ?", (self.admin_id,))
                raise RuntimeError()
        with app.connect() as db:
            self.assertNotEqual(db.execute("SELECT name FROM users WHERE id = ?", (self.admin_id,)).fetchone()[0], "Must roll back")

    def test_seed_preserves_credentials_and_deactivation(self):
        with app.connect() as db:
            db.execute("UPDATE users SET password_hash = 'custom', active = 0, reset_required = 1 WHERE email = 'admin@ottotree.local'")
        app.init_db()
        with app.connect() as db:
            row = db.execute("SELECT password_hash, active, reset_required FROM users WHERE email = 'admin@ottotree.local'").fetchone()
        self.assertEqual(tuple(row), ("custom", 0, 1))

    def test_password_compatibility(self):
        modern = app.hash_password("test-password")
        self.assertTrue(app.verify_password("test-password", modern))
        self.assertFalse(app.verify_password("wrong", modern))
        legacy = "salt$" + hashlib.sha256(b"salt:test-password").hexdigest()
        self.assertTrue(app.verify_password("test-password", legacy))

    def test_incomplete_inspection_cannot_round_up_to_complete(self):
        items = [{"passed": True} for _ in range(199)] + [{}]
        self.assertEqual(app.inspection_progress(items), 99)
        self.assertEqual(self.request("/api/inspection-sessions", "POST", {"items": items, "complete": True})[0], 409)

    def test_cached_responses_are_invalidated_after_commit(self):
        before = app.cached_response(("setup",), app.setup_records)
        with app.connect() as db:
            db.execute("INSERT INTO departments(code, description, created_at) VALUES ('CACHE-TEST', 'Test', 0)")
        after = app.cached_response(("setup",), app.setup_records)
        self.assertNotEqual(before.body, after.body)
        self.assertTrue(any(row["code"] == "CACHE-TEST" for row in after["departments"]))
        self.assertEqual(json.loads(gzip.decompress(after.compressed)), after)

    def test_auth_and_role_permissions(self):
        status, _, body = self.request("/api/auth/login", "POST", {"identifier": "gavin", "password": "123456"}, token=None)
        self.assertEqual(status, 200, body)
        self.assertEqual(json.loads(body)["user"]["username"], "gavin")
        self.assertEqual(self.request("/api/equipment", token=None)[0], 401)
        self.assertEqual(self.request("/api/users", token="limited")[0], 403)
        self.assertEqual(self.request("/api/setup/departments", "POST", {}, token="limited")[0], 403)
        self.assertEqual(self.request("/api/equipment", token="limited")[0], 200)
        status, _, body = self.request("/api/setup", token="limited")
        self.assertEqual(status, 200)
        self.assertNotIn("Super", {row["name"] for row in json.loads(body)["roles"]})

        with app.connect() as db:
            admin_id = db.execute("SELECT id FROM users WHERE role = 'Admin' LIMIT 1").fetchone()[0]
        app.SESSION_TOKENS["admin"] = {"user_id": admin_id, "expires_at": time.time() + 3600}
        status, _, body = self.request("/api/users", token="admin")
        self.assertEqual(status, 200)
        self.assertNotIn("Super", {row["role"] for row in json.loads(body)["items"]})

    def test_account_profile_updates_permissions_and_picture(self):
        from test_media_reports import photo_data_url
        with app.connect() as db:
            department = db.execute("SELECT code FROM departments ORDER BY id LIMIT 1").fetchone()[0]
            cursor = db.execute("INSERT INTO users(name, role, email, department, active, created_at) VALUES ('Profile User', 'Auditor', 'profile@example.test', ?, 1, 0)", (department,))
            user_id = cursor.lastrowid
        app.SESSION_TOKENS["profile"] = {"user_id": user_id, "expires_at": time.time() + 3600}
        self.assertEqual(self.request("/api/account", token=None)[0], 401)
        self.assertEqual(self.request("/api/account", "PATCH", {"role": "Super"}, token="profile")[0], 403)
        self.assertEqual(self.request("/api/account", "PATCH", {"department": "Another department"}, token="profile")[0], 403)
        self.assertEqual(self.request("/api/account", "PATCH", {"email": "invalid"}, token="profile")[0], 400)
        _, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "profile.png"}}, token="profile")
        photo = json.loads(body)["image"]
        payload = {"name": "Updated Profile", "email": "PROFILE-UPDATED@EXAMPLE.TEST", "profilePhoto": photo}
        status, _, body = self.request("/api/account", "PATCH", payload, token="profile")
        self.assertEqual(status, 200, body)
        user = json.loads(body)["user"]
        self.assertEqual((user["name"], user["email"], user["role"]), ("Updated Profile", "profile-updated@example.test", "Auditor"))
        _, _, body = self.request("/api/auth/me", token="profile")
        self.assertEqual(json.loads(body)["user"]["profilePhoto"]["url"], photo["url"])
        with app.connect() as db:
            other_email = "duplicate@example.test"
            db.execute("INSERT INTO users(name,role,email,active,created_at) VALUES ('Duplicate','Auditor',?,1,0)", (other_email,))
        self.assertEqual(self.request("/api/account", "PATCH", {"email": other_email.upper()}, token="profile")[0], 409)
        self.assertEqual(self.request("/api/account", "PATCH", {"profilePhoto": {}}, token="profile")[0], 200)
        _, _, body = self.request("/api/account", token="profile")
        self.assertEqual(json.loads(body)["user"]["profilePhoto"], {})

    def test_invalid_bodies(self):
        for body in ("[]", "null", "{", '{"items":[1]}'):
            self.assertEqual(self.request("/api/inspection-sessions", "POST", raw=body)[0], 400)
        self.assertEqual(self.request("/api/inspection-sessions", "POST", headers={"Content-Length": "-1"})[0], 400)
        self.assertEqual(self.request("/api/inspection-sessions", "POST", headers={"Content-Length": "999999999"})[0], 413)

    def test_concurrent_completion_creates_one_audit(self):
        payload = {"outlet": "STP", "auditDate": "2026-09-14", "items": [{"item": "Test asset", "passed": True}]}
        status, _, body = self.request("/api/inspection-sessions", "POST", payload)
        self.assertEqual(status, 200)
        session_id = json.loads(body)["id"]
        payload["complete"] = True
        for item in payload["items"]:
            item["images"] = [self.evidence()]
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            statuses = list(pool.map(lambda _: self.request(f"/api/inspection-sessions/{session_id}", "PATCH", payload)[0], range(8)))
        self.assertEqual(statuses.count(200), 1, statuses)
        self.assertEqual(statuses.count(409), 7, statuses)
        with app.connect() as db:
            audit_id = db.execute("SELECT audit_id FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()[0]
            self.assertIsNotNone(audit_id)
            self.assertEqual(db.execute("SELECT COUNT(*) FROM audits WHERE id = ?", (audit_id,)).fetchone()[0], 1)
        payload["complete"] = False
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "PATCH", payload)[0], 409)

    def test_report_counts_not_limited_to_preview(self):
        for number in range(10):
            self.assertEqual(self.request("/api/work-orders", "POST", self.from_request({"title": f"Test {number}", "priority": "Priority"}))[0], 200)
        data = app.report("Ottotree")
        self.assertEqual(data["monthlySummary"]["openWorkOrders"], 10)
        self.assertEqual(len(data["criticalIssues"]), 10)
        self.assertEqual(app.dashboard("Ottotree")["today"]["followUps"], 10)

    def test_scoring_settings_validation_and_saved_snapshot(self):
        with app.connect() as db:
            original = {row["key"]: load_value(row["value_data_id"]) for row in db.execute("SELECT * FROM app_settings WHERE key LIKE 'scoring.%'")}
        try:
            for invalid in ({"scoring.weights": {"Safety": 0}}, {"scoring.passMark": 101}, {"scoring.goodBand": 95}, {"scoring.weighting": "Unknown"}):
                self.assertEqual(self.request("/api/settings", "POST", {"settings": invalid})[0], 400)
            configured = {"scoring.weighting": "Weighted", "scoring.weights": {"Safety": 3, "Other": 1}, "scoring.passMark": 0}
            self.assertEqual(self.request("/api/settings", "POST", {"settings": configured})[0], 200)
            # A failed criterion requires notes but retains the weighted score snapshot.
            status, _, body = self.request("/api/inspection-sessions", "POST", {"outlet": "STP", "items": [{"category": "Safety", "passed": True, "images": [self.evidence()]}, {"category": "Other", "passed": False, "notes": "Repair", "images": [self.evidence()]}], "complete": True})
            self.assertEqual(status, 200, body)
            session = json.loads(self.request(f"/api/inspection-sessions/{json.loads(body)['id']}")[2])
            self.assertEqual(session["scoring"]["score"], 75)
            self.assertEqual(session["scoring"]["passMark"], 0)
            self.assertTrue(session["scoring"]["meetsPassMark"])
        finally:
            self.assertEqual(self.request("/api/settings", "POST", {"settings": original})[0], 200)

    def test_recovery_requests_and_durable_sessions(self):
        from backend.session_store import SessionStore
        with app.connect() as db:
            user_id = db.execute("INSERT INTO users(name, role, email, password_hash, active, created_at) VALUES ('Recovery User', 'Auditor', 'recovery@example.com', ?, 1, 0)", (app.hash_password("TestPassword123"),)).lastrowid
        for email in ("recovery@example.com", "nobody@example.com", "recovery@example.com"):
            self.assertEqual(self.request("/api/auth/forgot-password", "POST", {"email": email}, token=None)[0], 200)
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT count(*) FROM password_reset_requests WHERE user_id = ?", (user_id,)).fetchone()[0], 1)
            self.assertEqual(db.execute("SELECT count(*) FROM notifications WHERE related_type = 'user' AND related_id = ?", (user_id,)).fetchone()[0], 1)
        status, headers, body = self.request("/api/auth/login", "POST", {"email": "recovery@example.com", "password": "TestPassword123", "remember": True}, token=None)
        self.assertEqual(status, 200, body)
        token = headers["Set-Cookie"].split(";", 1)[0].split("=", 1)[1]
        self.assertEqual(SessionStore().get(token)["user_id"], user_id)
        self.assertGreater(SessionStore().get(token)["expires_at"], time.time() + 29 * 86400)
        with app.connect() as db:
            self.assertIsNone(db.execute("SELECT 1 FROM auth_sessions WHERE token_hash = ?", (token,)).fetchone())
        self.assertEqual(self.request("/api/auth/logout", "POST", {}, token=token)[0], 200)
        self.assertIsNone(SessionStore().get(token))

    def test_new_audit_header_reference_and_completion(self):
        with app.connect() as db:
            audit_type = db.execute("SELECT name FROM audit_types WHERE active = 1 LIMIT 1").fetchone()[0]
            pass
        name = "Super User"
        payload = {"outlet": "STP", "auditDate": "2026-09-18", "auditTime": "09:35", "auditType": audit_type,
                   "remarks": "Morning review", "auditor": "Forged name"}
        for change in ({"outlet": "Missing"}, {"auditDate": "2026-02-30"}, {"auditTime": "25:70"}, {"auditType": "Missing"}, {"remarks": "x" * 5001}):
            self.assertEqual(self.request("/api/audits/start", "POST", payload | change)[0], 400, change.keys())
        with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
            responses = list(pool.map(lambda _: self.request("/api/audits/start", "POST", payload), range(5)))
        self.assertTrue(all(response[0] == 200 for response in responses), responses)
        created = [json.loads(response[2]) for response in responses]
        self.assertEqual(len({item["auditRef"] for item in created}), 5)
        for item in created:
            self.assertRegex(item["auditRef"], r"^AUDIT-STP-20260918-\d{6}$")
        record = created[0]
        path = f"/api/inspection-sessions/{record['id']}"
        saved = json.loads(self.request(path)[2])
        self.assertEqual((saved["auditor"], saved["audit_time"], saved["audit_type"], saved["remarks"]), (name, "09:35", audit_type, "Morning review"))
        self.assertEqual(saved["audit_ref"], record["auditRef"])
        self.assertEqual(saved["schedule_id"], record["scheduleId"])
        self.assertEqual(saved["status"], "Draft")
        self.assertEqual(self.request(path, "PATCH", {"auditTime": "99:99"})[0], 400)
        status, _, body = self.request(path, "PATCH", {"items": [{"passed": True, "images": [self.evidence()]}], "complete": True})
        self.assertEqual(status, 200, body)
        with app.connect() as db:
            audit = db.execute("SELECT * FROM audits WHERE id = ?", (json.loads(body)["auditId"],)).fetchone()
            self.assertEqual((audit["audit_ref"], audit["audit_time"], audit["audit_type"], audit["remarks"]), (record["auditRef"], "09:35", audit_type, "Morning review"))
        self.assertEqual(self.request(path, "PATCH", {"remarks": "Changed"})[0], 409)

    def test_z_domain_route_round_trips(self):
        cases = [
            ("/api/equipment", "equipment", "code", {"code": "ROUTE-TEST", "name": "Route test"}),
            ("/api/setup/departments", "departments", "code", {"code": "ROUTE-TEST"}),
            ("/api/setup/categories", "categories", "name", {"name": "ROUTE-TEST"}),
            ("/api/setup/outlets", "outlets", "code", {"code": "ROUTE-TEST"}),
            ("/api/locations", "locations", "name", {"name": "ROUTE-TEST", "outlet": "STP"}),
            ("/api/zones", "zones", "name", {"name": "ROUTE-TEST", "outlet": "STP"}),
            ("/api/schedules", "schedules", "auditor", {"auditor": "ROUTE-TEST"}),
            ("/api/roles", "roles", "name", {"name": "ROUTE-TEST", "tabs": ["today"]}),
            ("/api/work-orders", "work_orders", "title", {"title": "ROUTE-TEST", "cause": "Broken", "requiredAction": "Repair"}),
        ]
        for path, table, field, payload in cases:
            with self.subTest(path=path):
                if table == "work_orders":
                    payload = self.from_request(payload)
                self.assertEqual(self.request(path, "POST", payload)[0], 200)
                with app.connect() as db:
                    row = db.execute(f"SELECT * FROM {table} WHERE {field} = ?", ("ROUTE-TEST",)).fetchone()
                    self.assertIsNotNone(row)
                    record_id = row["id"]
                    if table == "work_orders":
                        self.assertEqual(row["cause"], "Broken")
                self.assertEqual(self.request(f"{path}/{record_id}", "PATCH", payload)[0], 200)
                self.assertEqual(self.request(f"{path}/{record_id}", "DELETE")[0], 200)
                with app.connect() as db:
                    self.assertIsNone(db.execute(f"SELECT id FROM {table} WHERE id = ?", (record_id,)).fetchone())

    def test_z_failed_audit_becomes_request_then_work_order(self):
        from test_media_reports import photo_data_url
        status, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "evidence.png"}})
        self.assertEqual(status, 200)
        image = json.loads(body)["image"]
        self.assertEqual(self.request(image["url"], token=None)[0], 401)
        self.assertEqual(self.request(image["url"])[0], 200)
        payload = {"outlet": "STP", "auditTime": "12:30", "auditType": "Quick", "remarks": "Follow-up",
                   "items": [{"item": "Broken fixture", "notes": "Repair needed", "priority": "Non-Priority",
                              "pic": "Tester", "cause": "Loose screw", "images": [image]}], "complete": True}
        status, _, body = self.request("/api/inspection-sessions", "POST", payload)
        self.assertEqual(status, 200, body)
        session_id = json.loads(body)["id"]
        with app.connect() as db:
            session = db.execute("SELECT * FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()
            self.assertEqual((session["audit_time"], session["audit_type"], session["remarks"]), ("12:30", "Quick", "Follow-up"))
            findings = db.execute("SELECT * FROM findings WHERE audit_id = ?", (session["audit_id"],)).fetchall()
            self.assertEqual(len(findings), 1)
            self.assertEqual((findings[0]["pic"], findings[0]["cause"]), ("Tester", "Loose screw"))
            self.assertEqual(load_value(findings[0]["images_data_id"])[0]["url"], image["url"])
            # Completing an inspection records the finding; no work is ordered until someone requests it.
            self.assertEqual(findings[0]["status"], "Open")
            self.assertIsNone(db.execute("SELECT 1 FROM work_orders WHERE source_audit_id = ?", (session["audit_id"],)).fetchone())
        finding_id = findings[0]["id"]
        status, _, body = self.request("/api/work-requests", "POST", {"findingIds": [finding_id], "description": "Fix the fixture"})
        self.assertEqual(status, 200, body)
        request_id = json.loads(body)["id"]
        self.assertEqual(self.request("/api/work-requests", "POST", {"findingIds": [finding_id], "description": "Again"})[0], 409)
        listed = next(row for row in json.loads(self.request("/api/findings")[2])["items"] if row["id"] == finding_id)
        self.assertEqual((listed["status"], listed["request_status"]), ("Requested", "Open"))
        status, _, body = self.request("/api/work-orders", "POST", {"title": "Fix it", "pic": "Tester", "workRequestId": request_id})
        self.assertEqual(status, 200, body)
        order_id = json.loads(body)["id"]
        self.assertEqual(self.request("/api/work-orders", "POST", {"title": "Twice", "workRequestId": request_id})[0], 409)
        self.assertEqual(self.request(f"/api/work-orders/{order_id}", "PATCH", {"status": "Closed"})[0], 200)
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT status FROM findings WHERE id = ?", (finding_id,)).fetchone()[0], "Closed")
            self.assertEqual(db.execute("SELECT status, work_order_id FROM work_requests WHERE id = ?", (request_id,)).fetchone()[:], ("Closed", order_id))
            self.assertEqual(db.execute("SELECT source_audit_id FROM work_orders WHERE id = ?", (order_id,)).fetchone()[0], session["audit_id"])
            # Each step is logged with who took it, and how long the answering steps took.
            steps = {row["action"]: row for row in db.execute(
                "SELECT action, user_name, duration_ms FROM activity_log WHERE (record_type = 'inspection' AND record_id = ?) "
                "OR (record_type = 'work_request' AND record_id = ?) OR (record_type = 'work_order' AND record_id = ?)", (session_id, request_id, order_id))}
        self.assertEqual(set(steps), {"audit_started", "audit_completed", "request_raised", "order_created", "order_closed"})
        self.assertTrue(all(row["user_name"] == "Super User" for row in steps.values()))
        self.assertIsNotNone(steps["order_created"]["duration_ms"])
        self.assertIsNotNone(steps["order_closed"]["duration_ms"])
        data = json.loads(self.request("/api/reports")[2])
        me = next(row for row in data["people"] if row["name"] == "Super User")
        self.assertGreaterEqual((me["request_raised"], me["order_created"], me["order_closed"]), (1, 1, 1))
        self.assertEqual([row["key"] for row in data["timeToAct"]], ["audit", "request", "order", "closure"])
        self.assertTrue(any(row["action"] == "order_closed" for row in json.loads(self.request("/api/activity")[2])["items"]))
        from io import BytesIO
        from openpyxl import load_workbook
        workbook = load_workbook(BytesIO(self.request("/api/reports/export.xlsx")[2]))
        # The activity log is its own page and is not exported with the report.
        self.assertEqual(workbook.sheetnames, ["Report", "People", "Findings"])
        activity = json.loads(self.request("/api/activity")[2])
        self.assertGreater(activity["total"], 0)
        self.assertNotIn("activity", json.loads(self.request("/api/reports")[2]))

    def test_z_declined_request_closes_its_findings(self):
        with app.connect() as db:
            audit_id = db.execute("INSERT INTO audits(business_unit, outlet, branch, audit_date, auditor, audit_type, score, created_at) VALUES ('Ottotree', 'STP', 'Room', '2026-09-20', 'Auditor', 'Standard', 0, 0)").lastrowid
            finding_id = db.execute("INSERT INTO findings(finding_ref, audit_id, business_unit, outlet, location, priority, comment, status, created_at, updated_at) VALUES ('F-DECLINE', ?, 'Ottotree', 'STP', 'Room', 'High', 'Scuffed', 'Open', 0, 0)", (audit_id,)).lastrowid
        request_id = json.loads(self.request("/api/work-requests", "POST", {"findingIds": [finding_id], "description": "Repaint"})[2])["id"]
        self.assertEqual(self.request(f"/api/work-requests/{request_id}", "PATCH", {"action": "decline"})[0], 400)
        self.assertEqual(self.request(f"/api/work-requests/{request_id}", "PATCH", {"action": "decline", "remark": "Cosmetic only"})[0], 200)
        self.assertEqual(self.request("/api/work-orders", "POST", {"title": "Late", "workRequestId": request_id})[0], 409)
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT status FROM findings WHERE id = ?", (finding_id,)).fetchone()[0], "Closed")
            self.assertEqual(db.execute("SELECT status, decline_remark FROM work_requests WHERE id = ?", (request_id,)).fetchone()[:], ("Declined", "Cosmetic only"))

    def test_z_workflow_rules_identity_history_and_concurrent_close(self):
        # Completed and Verified are no longer steps; a new order cannot start Closed.
        for status, expected in (("Invalid", 400), ("Completed", 400), ("Verified", 400), ("Closed", 409)):
            self.assertEqual(self.request("/api/work-orders", "POST", self.from_request({"status": status}))[0], expected)
        self.assertEqual(self.request("/api/work-orders", "POST", {"title": "No request"})[0], 400)
        with app.connect() as db:
            cursor = db.execute("INSERT INTO users(name, role, email, department, active, created_at) VALUES ('Workflow PIC', 'Department/PIC', 'workflow@test', 'Technical', 1, 0)")
            app.SESSION_TOKENS["pic"] = {"user_id": cursor.lastrowid, "expires_at": time.time() + 3600}
        self.assertEqual(self.request("/api/work-orders", "POST", self.from_request({"title": "Workflow test", "pic": "Workflow PIC"}))[0], 200)
        with app.connect() as db:
            record_id = db.execute("SELECT id FROM work_orders WHERE title = 'Workflow test'").fetchone()[0]
        path = f"/api/work-orders/{record_id}"
        self.assertEqual(self.request(path, "PATCH", {"status": "Completed"}, token="pic")[0], 400)
        self.assertEqual(self.request(path, "PATCH", {"pic": "Someone else"}, token="pic")[0], 403)
        self.assertEqual(self.request(path, "DELETE", token="pic")[0], 403)
        self.assertEqual(self.request(path, "PATCH", {"status": "In Progress"}, token="pic")[0], 200)
        # The closing date is the server's, not the caller's.
        self.assertEqual(self.request(path, "PATCH", {"status": "Pending", "closedAt": "2000-01-01"}, token="pic")[0], 200)
        with app.connect() as db:
            row = db.execute("SELECT * FROM work_orders WHERE id = ?", (record_id,)).fetchone()
            self.assertEqual(row["title"], "Workflow test")  # Partial PATCH preserves omitted fields.
            self.assertFalse(row["closed_at"])
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            statuses = list(pool.map(lambda _: self.request(path, "PATCH", {"status": "Closed"}, token="pic")[0], range(2)))
        self.assertEqual(sorted(statuses), [200, 409])
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT closed_at FROM work_orders WHERE id = ?", (record_id,)).fetchone()[0], time.strftime("%Y-%m-%d"))
        self.assertEqual(self.request(path, "PATCH", {"title": "Changed"})[0], 409)
        self.assertEqual(self.request(path, "DELETE")[0], 409)
        with app.connect() as db:
            events = db.execute("SELECT * FROM comments WHERE record_type = 'work_order' AND record_id = ?", (record_id,)).fetchall()
        self.assertEqual(len(events), 3)
        self.assertTrue(all(row["system_generated"] for row in events))
        self.assertEqual(self.request(f"/api/comments/{events[0]['id']}", "DELETE")[0], 409)

    def test_z_workflow_rejects_changes_to_another_pic_assignment(self):
        self.assertEqual(self.request("/api/work-orders", "POST", self.from_request({"title": "Other PIC", "pic": "Another person"}))[0], 200)
        with app.connect() as db:
            record_id = db.execute("SELECT id FROM work_orders WHERE title = 'Other PIC'").fetchone()[0]
            cursor = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Unassigned PIC', 'Department/PIC', 'unassigned@test', 1, 0)")
        app.SESSION_TOKENS["unassigned-pic"] = {"user_id": cursor.lastrowid, "expires_at": time.time() + 3600}
        self.assertEqual(self.request(f"/api/work-orders/{record_id}", "PATCH", {"status": "In Progress"}, token="unassigned-pic")[0], 403)

    def test_department_pic_lists_only_assigned_findings_and_work_orders(self):
        with app.connect() as db:
            user_id = db.execute(
                "INSERT INTO users(name, role, email, department, active, created_at) VALUES ('Assigned PIC', 'Department/PIC', 'assigned-pic@test', 'Facilities', 1, 0)"
            ).lastrowid
            audit_id = db.execute(
                "INSERT INTO audits(business_unit, outlet, branch, audit_date, auditor, audit_type, score, created_at) VALUES ('Ottotree', 'STP', 'Room', '2026-09-20', 'Auditor', 'Standard', 0, 0)"
            ).lastrowid
            for reference, pic, department in (
                ("F-PIC-MATCH", "Assigned PIC", "Other"),
                ("F-DEPARTMENT", "", "Facilities"),
                ("F-OTHER-PIC", "Another PIC", "Facilities"),
            ):
                db.execute(
                    "INSERT INTO findings(finding_ref, audit_id, audit_ref, business_unit, outlet, location, category, priority, assigned_department, pic, comment, status, created_at, updated_at) VALUES (?, ?, 'AUD-TEST', 'Ottotree', 'STP', 'Room', 'Safety', 'High', ?, ?, 'Issue', 'Assigned', 0, 0)",
                    (reference, audit_id, department, pic),
                )
        app.SESSION_TOKENS["assigned-pic"] = {"user_id": user_id, "expires_at": time.time() + 3600}
        for title, department, pic in (
            ("PIC assignment", "Other", "Assigned PIC"),
            ("Department assignment", "Facilities", ""),
            ("Other PIC assignment", "Facilities", "Another PIC"),
        ):
            status, _, body = self.request(
                "/api/work-orders", "POST",
                self.from_request({"title": title, "requestType": department, "assignee": department, "pic": pic}),
            )
            self.assertEqual(status, 200, body)
        _, _, body = self.request("/api/findings", token="assigned-pic")
        finding_refs = {row["finding_ref"] for row in json.loads(body)["items"]}
        self.assertEqual(finding_refs, {"F-PIC-MATCH", "F-DEPARTMENT"})
        _, _, body = self.request("/api/work-orders", token="assigned-pic")
        titles = {row["title"] for row in json.loads(body)["items"]}
        self.assertTrue({"PIC assignment", "Department assignment"}.issubset(titles))
        self.assertNotIn("Other PIC assignment", titles)
        app.SESSION_TOKENS.pop("assigned-pic", None)
        with app.connect() as db:
            db.execute("DELETE FROM work_orders WHERE title IN ('PIC assignment', 'Department assignment', 'Other PIC assignment')")
            db.execute("DELETE FROM findings WHERE finding_ref IN ('F-PIC-MATCH', 'F-DEPARTMENT', 'F-OTHER-PIC')")
            db.execute("DELETE FROM audits WHERE id = ?", (audit_id,))
            db.execute("DELETE FROM users WHERE id = ?", (user_id,))

    def test_z_report_distribution_uses_completed_audits_and_saved_ratings(self):
        empty = app.report("Mini Studio")["charts"]["performanceDistribution"]
        self.assertEqual(sum(row["count"] for row in empty), 0)
        with app.connect() as db:
            for score, snapshot, status in ((95, {"rating": "Good"}, "Completed"), (80, {}, "Completed"), (65, {}, "Completed"), (40, {}, "Completed"), (100, {}, "Draft")):
                cursor = db.execute("INSERT INTO audits(business_unit,outlet,branch,audit_date,auditor,audit_type,score,created_at,scoring_data_id) VALUES ('Mini Studio','MST','Test','2026-01-01','Distribution test','Standard',?,0,?)", (score, save_value(db, snapshot)))
                db.execute("INSERT INTO inspection_sessions(business_unit,outlet,zone,audit_date,auditor,items_data_id,progress,status,audit_id,created_at,updated_at) VALUES ('Mini Studio','MST','Test','2026-01-01','Distribution test',?,100,?,?,0,0)", (save_value(db, []), status, cursor.lastrowid))
        data = app.report("Mini Studio")
        counts = {row["label"]: row["count"] for row in data["charts"]["performanceDistribution"]}
        self.assertEqual(counts, {"Excellent": 0, "Good": 2, "Below Expectation": 1, "Critical": 1})
        self.assertEqual(sum(counts.values()), data["monthlySummary"]["auditsCompleted"])
        self.assertEqual(data["monthlySummary"]["audits"], data["monthlySummary"]["auditsCompleted"] + data["monthlySummary"]["auditsPending"])

    def test_z_permissions_inheritance_signatures_and_targeted_notifications(self):
        from test_media_reports import photo_data_url
        people = {}
        for key, capabilities in (("author", ["auditor"]), ("reviewer", ["verifier"]), ("ack", ["acknowledger"])):
            role = {"name": f"Test {key}", "permissions": ["inspections", "notifications"], "inspectionPermissions": capabilities}
            self.assertEqual(self.request("/api/roles", "POST", role)[0], 200)
            # A chosen password: accounts left on the default one must change it before using the API.
            user = {"name": f"Test {key}", "email": f"{key}@example.test", "role": role["name"], "active": True, "password": "TestPassword123"}
            self.assertEqual(self.request("/api/users", "POST", user)[0], 200)
            with app.connect() as db:
                user_id = db.execute("SELECT id FROM users WHERE email = ?", (user["email"],)).fetchone()[0]
            app.SESSION_TOKENS[key] = {"user_id": user_id, "expires_at": time.time() + 3600}
            del user["password"]  # Later edits reuse this record; resending a password would revoke the session.
            people[key] = (user_id, user)
            _, _, body = self.request("/api/account", token=key)
            effective = json.loads(body)["user"]
            self.assertEqual(effective["permissionSource"], "role")
            self.assertEqual(effective["inspectionPermissions"], capabilities)

        self.assertEqual(self.request("/api/inspection-sessions", "POST", {"items": [{"passed": True}]}, token="reviewer")[0], 403)
        payload = {"outlet": "STP", "auditor": "Forged auditor", "items": [{"item": "First", "passed": True}, {"item": "Second"}]}
        status, _, body = self.request("/api/inspection-sessions", "POST", payload, token="author")
        self.assertEqual(status, 200, body)
        session_id = json.loads(body)["id"]
        path = f"/api/inspection-sessions/{session_id}"
        _, _, body = self.request(path, token="reviewer")
        session = json.loads(body)
        self.assertEqual(session["auditor"], "Test author")
        self.assertEqual(session["owner_user_id"], people["author"][0])

        def notices(token):
            status, _, body = self.request("/api/notifications", token=token)
            self.assertEqual(status, 200)
            return [row for row in json.loads(body)["items"] if row["related_type"] == "inspection" and row["related_id"] == session_id]

        self.assertEqual(len(notices("reviewer")), 1)
        self.assertEqual(len(notices("ack")), 1)
        self.assertEqual(len(notices("author")), 0)
        self.assertEqual(self.request(path, "PATCH", payload, token="author")[0], 200)
        self.assertEqual(len(notices("reviewer")), 1)  # An unchanged save is not another progress event.
        self.assertEqual(self.request(path, "PATCH", {"items": []}, token="reviewer")[0], 403)
        payload["items"][1]["passed"] = True
        payload["complete"] = True
        for item in payload["items"]:
            item["images"] = [self.evidence()]
        self.assertEqual(self.request(path, "PATCH", payload, token="author")[0], 200)
        _, _, body = self.request("/api/media", "POST", {"image": {"name": "signature.png", "dataUrl": photo_data_url()}}, token="reviewer")
        signature = json.loads(body)["image"]
        self.assertEqual(self.request("/api/account", "PATCH", {"signatureImage": signature}, token="reviewer")[0], 200)
        _, _, body = self.request("/api/account", token="reviewer")
        self.assertEqual(json.loads(body)["user"]["signatureImage"]["url"], signature["url"])
        self.assertEqual(self.request(path, "PATCH", {"signatures": {"verifiedBy": signature}}, token="author")[0], 403)
        self.assertEqual(self.request(path, "PATCH", {"signatures": {"verifiedBy": signature}}, token="reviewer")[0], 200)
        _, _, body = self.request(path, token="reviewer")
        verified_session = json.loads(body)
        self.assertEqual(verified_session["auditor"], "Test author")
        self.assertEqual(verified_session["signatures"]["verifiedBy"]["signedByUserId"], people["reviewer"][0])
        self.assertEqual(verified_session["status"], "Completed")
        author_notifications = notices("author")
        self.assertEqual([row["title"] for row in author_notifications], ["Audit verified"])
        notice_path = f"/api/notifications/{author_notifications[0]['id']}"
        self.assertEqual(self.request(notice_path, "PATCH", {}, token="reviewer")[0], 404)
        self.assertEqual(self.request(notice_path, "DELETE", token="reviewer")[0], 404)
        self.assertEqual(self.request(notice_path, "PATCH", {}, token="author")[0], 200)
        signatures = verified_session["signatures"] | {"acknowledgedBy": signature}
        self.assertEqual(self.request(path, "PATCH", {"signatures": signatures}, token="ack")[0], 200)

        user_id, user = people["reviewer"]
        overridden = user | {"permissionOverrides": {"permissions": ["notifications"], "inspectionPermissions": ["acknowledger"]}}
        self.assertEqual(self.request(f"/api/users/{user_id}", "PATCH", overridden)[0], 200)
        _, _, body = self.request("/api/account", token="reviewer")
        effective = json.loads(body)["user"]
        self.assertEqual(effective["permissionSource"], "user")
        self.assertEqual(effective["inspectionPermissions"], ["acknowledger"])
        self.assertEqual(self.request(path, token="reviewer")[0], 403)
        with app.connect() as db:
            role_id = db.execute("SELECT id FROM roles WHERE name = ?", (user["role"],)).fetchone()[0]
        self.assertEqual(self.request(f"/api/roles/{role_id}", "PATCH", {"name": user["role"], "permissions": ["inspections"], "inspectionPermissions": []})[0], 200)
        _, _, body = self.request("/api/account", token="reviewer")
        self.assertEqual(json.loads(body)["user"]["inspectionPermissions"], ["acknowledger"])
        self.assertEqual(self.request(f"/api/users/{user_id}", "PATCH", user | {"permissionOverrides": None})[0], 200)
        _, _, body = self.request("/api/account", token="reviewer")
        self.assertEqual(json.loads(body)["user"]["inspectionPermissions"], [])

    def test_z_schedule_ids_resume_one_inspection_and_track_completion(self):
        status, _, body = self.request("/api/schedules", "POST", {"outlet": "STP", "scheduledDate": "2026-09-18", "auditor": "Assigned auditor"})
        self.assertEqual(status, 200)
        schedule = json.loads(body)
        self.assertGreater(schedule["id"], 0)
        self.assertEqual(schedule["scheduleRef"], f"SCH-{schedule['id']:05d}")
        with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
            responses = list(pool.map(lambda _: self.request("/api/schedules/start", "POST", {"scheduleId": schedule["id"]}), range(5)))
        self.assertTrue(all(response[0] == 200 for response in responses))
        ids = {json.loads(response[2])["id"] for response in responses}
        self.assertEqual(len(ids), 1)
        session_id = ids.pop()
        _, _, body = self.request(f"/api/inspection-sessions/{session_id}")
        session = json.loads(body)
        self.assertEqual(session["schedule_id"], schedule["id"])
        self.assertNotEqual(session["auditor"], "Assigned auditor")
        # An audit is named by its code: AUDIT-<outlet>-<date>-<number>.
        self.assertRegex(session["audit_ref"], r"^AUDIT-.+-\d{8}-\d{6}$")
        self.assertEqual(session["inspection_name"], session["audit_ref"])
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "PATCH", {"items": [{"passed": True, "images": [self.evidence()]}], "complete": True})[0], 200)
        _, _, body = self.request("/api/schedules")
        saved = next(row for row in json.loads(body)["items"] if row["id"] == schedule["id"])
        self.assertEqual((saved["inspection_id"], saved["status"], saved["progress"]), (session_id, "Completed", 100))
        self.assertEqual(self.request(f"/api/schedules/{schedule['id']}", "DELETE")[0], 409)
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "DELETE")[0], 409)


    def test_z_visit_covers_chosen_locations_and_starts_pending(self):
        with app.connect() as db:
            names = [row[0] for row in db.execute("SELECT name FROM locations WHERE outlet_code = 'STP' ORDER BY name LIMIT 2")]
        self.assertEqual(len(names), 2)
        # A new visit is Pending whatever status is sent, and covers every location by default.
        status, _, body = self.request("/api/schedules", "POST", {"outlet": "STP", "scheduledDate": "2026-09-21", "status": "Completed"})
        self.assertEqual(status, 200)
        everywhere = json.loads(body)["id"]
        status, _, body = self.request("/api/schedules", "POST", {"outlet": "STP", "scheduledDate": "2026-09-21", "locations": names})
        chosen = json.loads(body)["id"]
        self.assertEqual(self.request("/api/schedules", "POST", {"outlet": "STP", "scheduledDate": "2026-09-21", "locations": ["Not a location"]})[0], 400)
        rows = {row["id"]: row for row in json.loads(self.request("/api/schedules")[2])["items"]}
        self.assertEqual((rows[everywhere]["status"], rows[everywhere]["zone"], rows[everywhere]["visit_locations"]), ("Pending", "All Locations", []))
        self.assertEqual((rows[chosen]["zone"], rows[chosen]["visit_locations"]), (", ".join(names), names))
        # The audit started from the visit covers the same locations.
        session_id = json.loads(self.request("/api/schedules/start", "POST", {"scheduleId": chosen})[2])["id"]
        session = json.loads(self.request(f"/api/inspection-sessions/{session_id}")[2])
        self.assertEqual(session["visit_locations"], names)
        # The audit stays at the outlet it was scheduled for.
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "PATCH", {"outlet": "MAM", "items": []})[0], 409)
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "PATCH", {"items": []})[0], 200)
        self.assertEqual(json.loads(self.request(f"/api/inspection-sessions/{session_id}")[2])["outlet"], "STP")
        # Editing the visit keeps the status the audit gave it, and a chosen location cannot be renamed.
        self.assertEqual(self.request(f"/api/schedules/{chosen}", "PATCH", {"outlet": "STP", "scheduledDate": "2026-09-22", "locations": names[:1]})[0], 200)
        rows = {row["id"]: row for row in json.loads(self.request("/api/schedules")[2])["items"]}
        self.assertEqual((rows[chosen]["status"], rows[chosen]["visit_locations"]), ("In Progress", names[:1]))
        with app.connect() as db:
            location_id = db.execute("SELECT id FROM locations WHERE outlet_code = 'STP' AND name = ?", (names[1],)).fetchone()[0]
        self.assertEqual(self.request(f"/api/locations/{location_id}", "PATCH", {"name": "Renamed visit location"})[0], 409)


    def test_z_outlet_limited_accounts_see_only_their_outlets(self):
        roles = {row["name"]: row for row in json.loads(self.request("/api/roles")[2])["items"]}
        self.assertEqual({name: roles[name]["department"] for name in ("Regional Manager", "Operation Manager", "PIC", "Captain")},
                         {name: "Operation" for name in ("Regional Manager", "Operation Manager", "PIC", "Captain")})
        # Outlets are set on each person; people sharing a role can cover different outlets.
        manager = {"name": "Outlet Manager", "email": "outlet-manager@example.test", "role": "Operation Manager", "department": "Operation", "password": "a-long-password"}
        self.assertEqual(self.request("/api/users", "POST", manager | {"outlets": ["NOPE"]})[0], 400)
        self.assertEqual(self.request("/api/users", "POST", manager | {"outlets": ["MST"]})[0], 200)
        regional = {"name": "Region Lead", "email": "region-lead@example.test", "role": "Operation Manager", "department": "Operation", "password": "a-long-password"}
        self.assertEqual(self.request("/api/users", "POST", regional | {"outlets": ["MST", "MAM"]})[0], 200)
        with app.connect() as db:
            ids = {row["email"]: row["id"] for row in db.execute("SELECT id, email FROM users WHERE email IN ('outlet-manager@example.test', 'region-lead@example.test')")}
            db.execute("UPDATE users SET reset_required = 0 WHERE id IN (?, ?)", tuple(ids.values()))
        app.SESSION_TOKENS["outlet-manager"] = {"user_id": ids["outlet-manager@example.test"], "expires_at": time.time() + 3600}
        app.SESSION_TOKENS["region-lead"] = {"user_id": ids["region-lead@example.test"], "expires_at": time.time() + 3600}
        me = json.loads(self.request("/api/account", token="outlet-manager")[2])["user"]
        self.assertEqual((me["outletScope"], me["outlets"]), ("selected", ["MST"]))
        get = lambda path, token: json.loads(self.request(path, token=token)[2])
        self.assertEqual([row["code"] for row in get("/api/setup", "outlet-manager")["outlets"]], ["MST"])
        self.assertEqual(sorted(row["code"] for row in get("/api/setup", "region-lead")["outlets"]), ["MAM", "MST"])
        # Records at another outlet are neither listed nor reachable.
        here = json.loads(self.request("/api/schedules", "POST", {"outlet": "MST", "scheduledDate": "2026-10-05", "auditor": "Here"})[2])["id"]
        there = json.loads(self.request("/api/schedules", "POST", {"outlet": "MAM", "scheduledDate": "2026-10-05", "auditor": "There"})[2])["id"]
        listed = {row["id"] for row in get("/api/schedules", "outlet-manager")["items"]}
        self.assertIn(here, listed)
        self.assertNotIn(there, listed)
        self.assertIn(there, {row["id"] for row in get("/api/schedules", "region-lead")["items"]})
        self.assertTrue(all(row["outlet"] == "MST" for row in get("/api/equipment", "outlet-manager")["items"]))
        self.assertEqual(self.request("/api/equipment?outlet=MAM", token="outlet-manager")[0], 403)
        self.assertEqual(self.request("/api/locations?outlet=MAM", token="outlet-manager")[0], 403)
        self.assertEqual(self.request("/api/reports?outlet=MAM", token="outlet-manager")[0], 403)
        # Changes outside the outlet are refused; inside it they go through.
        self.assertEqual(self.request("/api/schedules", "POST", {"outlet": "MAM", "scheduledDate": "2026-10-06"}, token="outlet-manager")[0], 403)
        self.assertEqual(self.request("/api/schedules", "POST", {"scheduledDate": "2026-10-06"}, token="outlet-manager")[0], 403)
        self.assertEqual(self.request(f"/api/schedules/{there}", "PATCH", {"outlet": "MST", "scheduledDate": "2026-10-07"}, token="outlet-manager")[0], 403)
        self.assertEqual(self.request(f"/api/schedules/{here}", "PATCH", {"outlet": "MAM", "scheduledDate": "2026-10-07"}, token="outlet-manager")[0], 403)
        self.assertEqual(self.request("/api/schedules/start", "POST", {"scheduleId": there}, token="outlet-manager")[0], 403)
        self.assertEqual(self.request("/api/setup/outlets/1", "PATCH", {"code": "X"}, token="region-lead")[0], 403)
        self.assertEqual(self.request(f"/api/schedules/{here}", "PATCH", {"outlet": "MST", "scheduledDate": "2026-10-08"}, token="outlet-manager")[0], 200)
        # The dashboard counts only the account's outlets.
        everything = get("/api/dashboard", "test")
        mine = get("/api/dashboard", "outlet-manager")
        self.assertLessEqual(mine["today"]["kpi"]["assigned"] if "kpi" in mine.get("today", {}) else 0, everything["today"]["kpi"]["assigned"] if "kpi" in everything.get("today", {}) else 0)
        self.assertTrue(all(row["outlet"] == "MST" for row in mine["today"]["scheduled"]))
        for token in ("outlet-manager", "region-lead"):
            app.SESSION_TOKENS.pop(token, None)


    def test_z_audit_and_location_exports(self):
        from io import BytesIO
        from openpyxl import load_workbook
        from pypdf import PdfReader
        from test_media_reports import photo_data_url
        image = json.loads(self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "room.png"}})[2])["image"]
        items = [{"section": "Export sink", "item": "No leaks", "location": "Export Room", "passed": False, "notes": "Dripping", "priority": "High", "images": [image]},
                 {"section": "Export sink", "item": "Clean", "location": "Export Room", "passed": True, "images": [image]},
                 {"section": "Other lamp", "item": "Works", "location": "Elsewhere", "passed": True, "images": [image]}]
        status, _, body = self.request("/api/inspection-sessions", "POST", {"outlet": "MST", "auditDate": "2026-10-05", "items": items, "complete": True})
        self.assertEqual(status, 200, body)
        session_id = json.loads(body)["id"]
        status, headers, body = self.request(f"/api/inspection-sessions/{session_id}/export.xlsx")
        self.assertEqual(status, 200)
        workbook = load_workbook(BytesIO(body))
        # The whole audit first, then each location in full, laid out like the PDF.
        self.assertEqual(workbook.sheetnames, ["Overall", "Export Room", "Elsewhere"])
        cells = lambda name: [value for row in workbook[name].iter_rows(values_only=True) for value in row if value is not None]
        overall = cells("Overall")
        self.assertIn("Facilities Audit Report", overall)
        self.assertIn("Export Room", overall)
        self.assertIn("Dripping", overall)
        self.assertGreaterEqual(len(workbook["Overall"]._charts), 1)
        # The assets as counts by attribute (with their charts), not a list of every asset.
        self.assertTrue({"Assets inspected", "All passed", "With failures", "Category"}.issubset(overall))
        self.assertNotIn("Brand / model", overall)
        room = cells("Export Room")
        self.assertIn("Location: Export Room", room)
        self.assertTrue({"No leaks", "Clean", "Dripping", "Fail", "Pass"}.issubset(room))
        self.assertNotIn("Other lamp", room)
        self.assertGreaterEqual(len(workbook["Export Room"]._images), 1)
        self.assertIn("No findings.", cells("Elsewhere"))
        # The PDF: overall, or chosen locations in full.
        status, headers, body = self.request(f"/api/inspection-sessions/{session_id}/export.pdf?location=Export%20Room")
        self.assertEqual(status, 200, body[:200])
        self.assertIn("Export Room.pdf", headers["Content-Disposition"])
        text = "\n".join(page.extract_text() for page in PdfReader(BytesIO(body)).pages)
        self.assertIn("Selected Locations", text)
        self.assertIn("No leaks", text)
        self.assertNotIn("Other lamp", text)
        status, _, body = self.request(f"/api/inspection-sessions/{session_id}/export.pdf?location=Export%20Room&location=Elsewhere")
        self.assertIn("Other lamp", "\n".join(page.extract_text() for page in PdfReader(BytesIO(body)).pages))
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}/export.pdf?location=Nowhere")[0], 400)
        status, _, body = self.request(f"/api/inspection-sessions/{session_id}/export.pdf")
        self.assertIn("Location grading", "\n".join(page.extract_text() for page in PdfReader(BytesIO(body)).pages))
        # The location report holds only that location's checks.
        status, _, body = self.request("/api/location-report.xlsx?outlet=MST&location=Export%20Room&from=2026-10-01&to=2026-10-31")
        self.assertEqual(status, 200)
        workbook = load_workbook(BytesIO(body))
        self.assertEqual(workbook.sheetnames[0], "Overview")
        audit_sheets = workbook.sheetnames[1:]
        self.assertTrue(audit_sheets)
        checks = [value for name in audit_sheets for row in workbook[name].iter_rows(values_only=True) for value in row if value is not None]
        self.assertTrue({"No leaks", "Clean"}.issubset(checks))
        self.assertNotIn("Works", checks)
        status, headers, body = self.request("/api/location-report.pdf?outlet=MST&location=Export%20Room")
        self.assertEqual(status, 200)
        text = "\n".join(page.extract_text() for page in PdfReader(BytesIO(body)).pages)
        self.assertIn("Location Audit Report", text)
        self.assertIn("No leaks", text)
        self.assertNotIn("Other lamp", text)
        self.assertEqual(self.request("/api/location-report.pdf?outlet=MST")[0], 400)
        empty = self.request("/api/location-report.xlsx?outlet=MST&location=Export%20Room&from=2030-01-01")
        self.assertEqual(empty[0], 200)
        # An account limited to other outlets cannot export this one.
        self.outlet_role("Exporter", permissions=("today", "findings", "reports"))
        with app.connect() as db:
            user_id = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Export PIC', 'Exporter', 'export-pic@example.test', 1, 0)").lastrowid
        self.limit_to(user_id, ["MAM"])
        app.SESSION_TOKENS["export-pic"] = {"user_id": user_id, "expires_at": time.time() + 3600}
        self.assertEqual(self.request("/api/location-report.pdf?outlet=MST&location=Export%20Room", token="export-pic")[0], 403)
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}/export.xlsx", token="export-pic")[0], 403)
        app.SESSION_TOKENS.pop("export-pic", None)

    def test_missing_export_package_is_named(self):
        import builtins
        real_import = builtins.__import__
        def without_reportlab(name, *args, **kwargs):
            if name.startswith("reportlab") or name == "backend.pdf_report":
                raise ModuleNotFoundError("No module named 'reportlab'", name="reportlab")
            return real_import(name, *args, **kwargs)
        with mock.patch("builtins.__import__", without_reportlab), self.assertLogs(level="ERROR"):
            status, _, body = self.request("/api/location-report.pdf?outlet=MST&location=Anywhere")
        self.assertEqual(status, 503)
        self.assertIn("reportlab", json.loads(body)["error"])


    def test_reports_need_the_reports_permission(self):
        self.outlet_role("Without Reports", permissions=("today", "findings", "work-orders", "notifications"), capabilities=())
        with app.connect() as db:
            user_id = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('No Reports', 'Without Reports', 'no-reports@example.test', 1, 0)").lastrowid
        app.SESSION_TOKENS["no-reports"] = {"user_id": user_id, "expires_at": time.time() + 3600}
        for path in ("/api/reports", "/api/reports/export.pdf", "/api/reports/export.xlsx", "/api/location-report.pdf?outlet=MST&location=Room"):
            self.assertEqual(self.request(path, token="no-reports")[0], 403, path)
        app.SESSION_TOKENS.pop("no-reports", None)

    def test_roles_form_a_chain_of_command_without_loops(self):
        roles = lambda: {row["name"]: row for row in json.loads(self.request("/api/roles")[2])["items"]}
        for name in ("Chain Top", "Chain Middle", "Chain Bottom"):
            self.assertEqual(self.request("/api/roles", "POST", {"name": name, "permissions": ["today"]})[0], 200)
        ids = {name: row["id"] for name, row in roles().items() if name.startswith("Chain")}
        body = lambda name, parent: {"name": name, "permissions": ["today"], "reportsTo": ids[parent] if parent else None}
        self.assertEqual(self.request(f"/api/roles/{ids['Chain Middle']}", "PATCH", body("Chain Middle", "Chain Top"))[0], 200)
        self.assertEqual(self.request(f"/api/roles/{ids['Chain Bottom']}", "PATCH", body("Chain Bottom", "Chain Middle"))[0], 200)
        self.assertEqual(roles()["Chain Bottom"]["reports_to_id"], ids["Chain Middle"])
        # A role cannot report to itself or to anyone below it.
        self.assertEqual(self.request(f"/api/roles/{ids['Chain Top']}", "PATCH", body("Chain Top", "Chain Top"))[0], 400)
        self.assertEqual(self.request(f"/api/roles/{ids['Chain Top']}", "PATCH", body("Chain Top", "Chain Bottom"))[0], 400)
        self.assertEqual(self.request(f"/api/roles/{ids['Chain Top']}", "PATCH", {"name": "Chain Top", "permissions": ["today"], "reportsTo": 999999})[0], 400)
        # Editing a role without naming its manager keeps it; removing a role keeps the chain whole.
        self.assertEqual(self.request(f"/api/roles/{ids['Chain Bottom']}", "PATCH", {"name": "Chain Bottom", "permissions": ["today"]})[0], 200)
        self.assertEqual(roles()["Chain Bottom"]["reports_to_id"], ids["Chain Middle"])
        self.assertEqual(self.request(f"/api/roles/{ids['Chain Middle']}", "DELETE")[0], 200)
        self.assertEqual(roles()["Chain Bottom"]["reports_to_id"], ids["Chain Top"])

    def test_z_opened_visit_can_be_deleted_with_its_draft(self):
        schedule_id = json.loads(self.request("/api/schedules", "POST", {"outlet": "MST", "scheduledDate": "2026-10-06", "auditor": "Delete me"})[2])["id"]
        session_id = json.loads(self.request("/api/schedules/start", "POST", {"scheduleId": schedule_id})[2])["id"]
        self.assertEqual(self.request(f"/api/schedules/{schedule_id}", "DELETE")[0], 200)
        with app.connect() as db:
            self.assertIsNone(db.execute("SELECT 1 FROM schedules WHERE id = ?", (schedule_id,)).fetchone())
            self.assertIsNone(db.execute("SELECT 1 FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone())

    def test_z_superior_assigns_scheduled_work(self):
        self.outlet_role("Outlet Staff")
        with app.connect() as db:
            here = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Staff Here', 'Outlet Staff', 'staff-here@example.test', 1, 0)").lastrowid
            there = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Staff There', 'Outlet Staff', 'staff-there@example.test', 1, 0)").lastrowid
            viewer = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Report Reader', 'Management', 'reader@example.test', 1, 0)").lastrowid
        self.limit_to(here, ["MST"])
        self.limit_to(there, ["MAM"])
        # Only people who can audit, at this outlet, are offered.
        offered = {row["id"] for row in json.loads(self.request("/api/schedules/assignees?outlet=MST")[2])["items"]}
        self.assertIn(here, offered)
        self.assertNotIn(there, offered)
        self.assertNotIn(viewer, offered)
        base = {"outlet": "MST", "scheduledDate": "2026-10-09", "remarks": "Assigned visit"}
        self.assertEqual(self.request("/api/schedules", "POST", base | {"assignees": [there]})[0], 400)
        status, _, body = self.request("/api/schedules", "POST", base | {"assignees": [here]})
        self.assertEqual(status, 200, body)
        schedule_id = json.loads(body)["id"]
        row = next(item for item in json.loads(self.request("/api/schedules")[2])["items"] if item["id"] == schedule_id)
        self.assertEqual((row["assignees"], row["auditor"]), ([here], "Staff Here"))
        app.SESSION_TOKENS["staff-here"] = {"user_id": here, "expires_at": time.time() + 3600}
        with app.connect() as db:
            db.execute("UPDATE users SET reset_required = 0 WHERE id = ?", (here,))
            notified = db.execute("SELECT title FROM notifications WHERE recipient_user_id = ? AND related_type = 'schedule' AND related_id = ?", (here, schedule_id)).fetchone()
            logged = db.execute("SELECT user_name, detail FROM activity_log WHERE action = 'audit_assigned' AND record_id = ?", (schedule_id,)).fetchone()
        self.assertEqual(notified["title"], "Scheduled audit assigned to you")
        self.assertEqual((logged["user_name"], logged["detail"]), ("Super User", "Staff Here"))
        todo = json.loads(self.request("/api/todo", token="staff-here")[2])
        self.assertIn(("schedule", schedule_id), {(item["type"], item["id"]) for item in todo["items"]})
        self.assertGreaterEqual(todo["counts"]["guided"], 1)
        # Editing without naming assignees keeps them; starting the visit takes it off the waiting list.
        self.assertEqual(self.request(f"/api/schedules/{schedule_id}", "PATCH", base | {"remarks": "Changed"})[0], 200)
        row = next(item for item in json.loads(self.request("/api/schedules")[2])["items"] if item["id"] == schedule_id)
        self.assertEqual((row["assignees"], row["auditor"]), ([here], "Staff Here"))
        self.assertEqual(self.request("/api/schedules/start", "POST", {"scheduleId": schedule_id}, token="staff-here")[0], 200)
        todo = json.loads(self.request("/api/todo", token="staff-here")[2])
        self.assertNotIn(("schedule", schedule_id), {(item["type"], item["id"]) for item in todo["items"]})
        app.SESSION_TOKENS.pop("staff-here", None)

    def test_z_restart_keeps_edited_people_and_removed_roles(self):
        # A starter account whose email was changed, and a default role that was removed,
        # must neither stop the next start nor come back.
        with app.connect() as db:
            db.execute("UPDATE users SET email = 'gavin@company.example' WHERE username = 'gavin'")
            db.execute("UPDATE users SET role = 'Auditor' WHERE role = 'Facilities Officer'")
            db.execute("DELETE FROM roles WHERE name = 'Facilities Officer'")
            users_before = db.execute("SELECT count(*) FROM users").fetchone()[0]
        app.init_db()
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT count(*) FROM users").fetchone()[0], users_before)
            self.assertIsNone(db.execute("SELECT 1 FROM roles WHERE name = 'Facilities Officer'").fetchone())
            self.assertIsNone(db.execute("SELECT 1 FROM users WHERE email = 'gavin@audit.local'").fetchone())

    def test_z_outlets_move_to_people_once(self):
        # Before: a role covered every outlet or limited its people to theirs.
        with app.connect() as db:
            db.execute("INSERT INTO roles(name, permissions_data_id, protected, created_at, outlet_scope) VALUES ('Legacy Limited', ?, 0, 0, 'several')", (app_save_value(db, ["today"]),))
            limited = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Legacy Limited One', 'Legacy Limited', 'legacy.limited@example.test', 1, 0)").lastrowid
            everyone = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Legacy Everywhere', 'Auditor', 'legacy.everywhere@example.test', 1, 0)").lastrowid
            # A person in an all-outlet role was stored with an empty list.
            db.execute("UPDATE users SET outlets_data_id = ? WHERE id = ?", (app_save_value(db, ["MAM", "MST"]), limited))
            db.execute("UPDATE users SET outlets_data_id = ? WHERE id = ?", (app_save_value(db, []), everyone))
            db.execute("DELETE FROM app_settings WHERE key = 'system.outletsOnPeople'")
        app.init_db()
        people = {row["id"]: row for row in json.loads(self.request("/api/users")[2])["items"]}
        self.assertEqual(sorted(people[limited]["outlets"]), ["MAM", "MST"])
        self.assertIsNone(people[everyone]["outlets"])
        # Editing a person without naming outlets keeps them; All outlets clears them.
        self.assertEqual(self.request(f"/api/users/{limited}", "PATCH", {"title": "Still limited"})[0], 200)
        self.assertEqual(sorted(next(row for row in json.loads(self.request("/api/users")[2])["items"] if row["id"] == limited)["outlets"]), ["MAM", "MST"])
        self.assertEqual(self.request(f"/api/users/{limited}", "PATCH", {"outlets": None})[0], 200)
        self.assertIsNone(next(row for row in json.loads(self.request("/api/users")[2])["items"] if row["id"] == limited)["outlets"])

    def test_z_history_split_from_findings_keeps_access(self):
        with app.connect() as db:
            db.execute("INSERT INTO roles(name, permissions_data_id, protected, created_at) VALUES ('Findings Reader', ?, 0, 0)", (app_save_value(db, ["today", "findings"]),))
            user_id = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Own Pages', 'Management', 'own.pages@example.test', 1, 0)").lastrowid
            db.execute("UPDATE users SET permission_overrides_data_id = ? WHERE id = ?",
                       (app_save_value(db, {"permissions": ["findings"], "inspectionPermissions": []}), user_id))
            db.execute("DELETE FROM app_settings WHERE key = 'system.historySplit'")
        app.init_db()
        role = next(row for row in json.loads(self.request("/api/roles")[2])["items"] if row["name"] == "Findings Reader")
        self.assertEqual(sorted(role["permissions"]), ["findings", "history", "today"])
        person = next(row for row in json.loads(self.request("/api/users")[2])["items"] if row["id"] == user_id)
        self.assertEqual(sorted(person["permissionOverrides"]["permissions"]), ["findings", "history"])

    def test_z_changes_wait_for_approval(self):
        roles = {row["name"]: row for row in json.loads(self.request("/api/roles")[2])["items"]}
        # Everyone keeps what they could do: a role that opens Assets may change and approve assets.
        self.assertTrue({"assets.manage", "assets.approve"}.issubset(roles["Auditor"]["actions"]))
        self.assertNotIn("users.manage", roles["Auditor"]["actions"])
        self.assertTrue({"users.manage", "users.approve", "roles.approve"}.issubset(roles["Admin"]["actions"]))
        for name, actions in (("Asset Clerk", ["assets.manage", "users.manage"]), ("Asset Viewer", [])):
            status, _, body = self.request("/api/roles", "POST", {"name": name, "permissions": ["today", "equipment", "users"], "actions": actions})
            self.assertEqual(status, 200, body)
        with app.connect() as db:
            clerk = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Clerk', 'Asset Clerk', 'clerk@example.test', 1, 0)").lastrowid
            viewer = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Viewer', 'Asset Viewer', 'viewer@example.test', 1, 0)").lastrowid
        app.SESSION_TOKENS["clerk"] = {"user_id": clerk, "expires_at": time.time() + 3600}
        app.SESSION_TOKENS["viewer"] = {"user_id": viewer, "expires_at": time.time() + 3600}
        asset = {"kind": "asset", "name": "Approval Lamp", "outlet": "MST", "location": "Room"}
        self.assertEqual(self.request("/api/equipment", "POST", asset, token="viewer")[0], 403)
        # Without approval rights, the change waits.
        status, _, body = self.request("/api/equipment", "POST", asset, token="clerk")
        self.assertEqual(status, 200, body)
        held = json.loads(body)
        self.assertTrue(held["pending"])
        with app.connect() as db:
            self.assertIsNone(db.execute("SELECT 1 FROM equipment WHERE name = 'Approval Lamp'").fetchone())
        mine = next(row for row in json.loads(self.request("/api/changes", token="clerk")[2])["items"] if row["id"] == held["changeId"])
        self.assertEqual((mine["status"], mine["mine"], mine["canDecide"]), ("Pending", True, False))
        self.assertEqual(self.request(f"/api/changes/{held['changeId']}", "PATCH", {"decision": "approve"}, token="clerk")[0], 403)
        theirs = next(row for row in json.loads(self.request("/api/changes")[2])["items"] if row["id"] == held["changeId"])
        self.assertTrue(theirs["canDecide"])
        self.assertGreaterEqual(json.loads(self.request("/api/todo")[2])["counts"]["approvals"], 1)
        self.assertEqual(self.request(f"/api/changes/{held['changeId']}", "PATCH", {"decision": "approve"})[0], 200)
        with app.connect() as db:
            lamp = db.execute("SELECT id, notes FROM equipment WHERE name = 'Approval Lamp'").fetchone()
            told = db.execute("SELECT title FROM notifications WHERE recipient_user_id = ? AND related_type = 'change' AND related_id = ?", (clerk, held["changeId"])).fetchone()
        self.assertIsNotNone(lamp)
        self.assertEqual(told["title"], "Your change was approved")
        self.assertEqual(self.request(f"/api/changes/{held['changeId']}", "PATCH", {"decision": "approve"})[0], 409)
        # A rejected edit changes nothing.
        held = json.loads(self.request(f"/api/equipment/{lamp['id']}", "PATCH", {"name": "Renamed Lamp"}, token="clerk")[2])
        self.assertEqual(self.request(f"/api/changes/{held['changeId']}", "PATCH", {"decision": "reject", "remark": "Keep the name"})[0], 200)
        with app.connect() as db:
            self.assertIsNotNone(db.execute("SELECT 1 FROM equipment WHERE name = 'Approval Lamp'").fetchone())
        # A password never waits in a request.
        held = json.loads(self.request("/api/users", "POST", {"name": "New Starter", "email": "starter@example.test", "role": "Asset Viewer", "password": "secret-password-1"}, token="clerk")[2])
        with app.connect() as db:
            stored = db.execute("SELECT payload_data_id FROM change_requests WHERE id = ?", (held["changeId"],)).fetchone()[0]
        from backend.relational_values import load_value
        self.assertNotIn("password", load_value(stored))
        self.assertEqual(self.request(f"/api/changes/{held['changeId']}", "PATCH", {"decision": "approve"})[0], 200)
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT reset_required FROM users WHERE email = 'starter@example.test'").fetchone()[0], 1)
        for token in ("clerk", "viewer"):
            app.SESSION_TOKENS.pop(token, None)

    def test_z_bulk_add_and_edit_assets(self):
        for code in ("BULK-A", "BULK-B"):
            self.assertEqual(self.request("/api/setup/outlets", "POST", {"code": code})[0], 200)
        for outlet, name in (("BULK-A", "Hall"), ("BULK-A", "Bar"), ("BULK-B", "Hall")):
            self.assertEqual(self.request("/api/locations", "POST", {"outlet": outlet, "name": name})[0], 200)
        lamp = {"kind": "asset", "name": "Bulk Lamp", "brand": "Lumo", "code": "IGNORED", "serialNumber": "IGNORED"}
        # Every location of the chosen outlets; a named location only where it exists.
        status, _, body = self.request("/api/equipment/bulk", "POST", lamp | {"outlets": ["BULK-A", "BULK-B"], "locations": "all"})
        self.assertEqual(status, 200, body)
        self.assertEqual(json.loads(body)["count"], 3)
        status, _, body = self.request("/api/equipment/bulk", "POST", lamp | {"name": "Bulk Sign", "outlets": ["BULK-A", "BULK-B"], "locations": ["Bar"]})
        self.assertEqual(json.loads(body)["count"], 1, body)
        self.assertEqual(self.request("/api/equipment/bulk", "POST", lamp | {"outlets": ["BULK-B"], "locations": ["Bar"]})[0], 400)
        with app.connect() as db:
            rows = db.execute("SELECT id, code, serial_number, outlet, location FROM equipment WHERE name = 'Bulk Lamp' ORDER BY id").fetchall()
        self.assertEqual([(row["outlet"], row["location"]) for row in rows], [("BULK-A", "Bar"), ("BULK-A", "Hall"), ("BULK-B", "Hall")])
        self.assertTrue(all(row["code"].startswith("AST-") and not row["serial_number"] for row in rows))
        ids = [row["id"] for row in rows]
        # Shared fields go to every item; code and serial number are each item's own.
        edit = {"ids": ids, "fields": {"brand": "Brighta", "outlet": "BULK-B", "code": "SAME"},
                "items": [{"id": ids[0], "code": "LAMP-1", "serialNumber": "SN-1"}, {"id": ids[1], "code": "LAMP-2", "serialNumber": "SN-2"}]}
        status, _, body = self.request("/api/equipment/bulk", "PATCH", edit)
        self.assertEqual(status, 200, body)
        with app.connect() as db:
            rows = db.execute("SELECT id, code, qr_code, serial_number, brand, outlet FROM equipment WHERE id IN (?, ?, ?) ORDER BY id", ids).fetchall()
        self.assertEqual([row["brand"] for row in rows], ["Brighta"] * 3)
        self.assertEqual([row["outlet"] for row in rows], ["BULK-A", "BULK-A", "BULK-B"])
        self.assertEqual([(row["code"], row["qr_code"], row["serial_number"]) for row in rows[:2]], [("LAMP-1", "LAMP-1", "SN-1"), ("LAMP-2", "LAMP-2", "SN-2")])
        # Codes may be swapped, but not repeated or taken from another item; serial numbers neither.
        swap = {"ids": ids[:2], "items": [{"id": ids[0], "code": "LAMP-2"}, {"id": ids[1], "code": "LAMP-1"}]}
        self.assertEqual(self.request("/api/equipment/bulk", "PATCH", swap)[0], 200)
        for items in ([{"id": ids[0], "code": "X"}, {"id": ids[1], "code": "X"}], [{"id": ids[2], "code": "LAMP-1"}],
                      [{"id": ids[2], "serialNumber": "SN-1"}]):
            status, _, body = self.request("/api/equipment/bulk", "PATCH", {"ids": [ids[2]] if len(items) == 1 else ids[:2], "items": items})
            self.assertEqual(status, 400, body)
        self.assertEqual(self.request("/api/equipment", "POST", {"name": "Copy", "outlet": "BULK-A", "serialNumber": "SN-1"})[0], 400)
        # Someone covering one outlet adds only there and edits only items there.
        self.outlet_role("Bulk Keeper", ("today", "equipment"))
        with app.connect() as db:
            keeper = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Keeper', 'Bulk Keeper', 'keeper@example.test', 1, 0)").lastrowid
            db.execute("UPDATE roles SET action_permissions_data_id = ? WHERE name = 'Bulk Keeper'", (app_save_value(db, ["assets.manage", "assets.approve"]),))
        self.limit_to(keeper, ["BULK-B"])
        app.SESSION_TOKENS["keeper"] = {"user_id": keeper, "expires_at": time.time() + 3600}
        status, _, body = self.request("/api/equipment/bulk", "POST", lamp | {"name": "Keeper Lamp", "outlets": "all", "locations": "all"}, token="keeper")
        self.assertEqual((status, json.loads(body)["count"]), (200, 1), body)
        self.assertEqual(self.request("/api/equipment/bulk", "POST", lamp | {"outlets": ["BULK-A"], "locations": "all"}, token="keeper")[0], 403)
        self.assertEqual(self.request("/api/equipment/bulk", "PATCH", {"ids": ids, "fields": {"brand": "No"}}, token="keeper")[0], 403)
        self.assertEqual(self.request("/api/equipment/bulk", "PATCH", {"ids": ids[2:], "fields": {"brand": "Yes"}}, token="keeper")[0], 200)
        # Bulk changes wait for approval like single ones.
        from backend import change_requests
        for method in ("POST", "PATCH"):
            self.assertEqual(change_requests.record_for(method, "/api/equipment/bulk"), ("assets", None))

    def test_z_work_request_edit(self):
        status, _, body = self.request("/api/work-requests", "POST", {"outlet": "STP", "location": "Room", "itemName": "Door", "description": "Sticks"})
        self.assertEqual(status, 200, body)
        request_id = json.loads(body)["id"]
        path = f"/api/work-requests/{request_id}"
        status, _, body = self.request(path, "PATCH", {"action": "edit", "outlet": "MST", "location": "Hall", "itemName": "Front door",
                                                       "priority": "High", "description": "Sticks when wet"})
        self.assertEqual(status, 200, body)
        with app.connect() as db:
            row = db.execute("SELECT outlet, location, item_name, priority, description FROM work_requests WHERE id = ?", (request_id,)).fetchone()
        self.assertEqual(tuple(row), ("MST", "Hall", "Front door", "High", "Sticks when wet"))
        self.assertEqual(self.request(path, "PATCH", {"action": "edit", "description": " "})[0], 400)
        # Someone who neither raised it nor assigns work cannot change it.
        self.outlet_role("Request Reader", ("today", "findings"))
        with app.connect() as db:
            reader = db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Request Reader', 'Request Reader', 'request-reader@example.test', 1, 0)").lastrowid
        app.SESSION_TOKENS["reader"] = {"user_id": reader, "expires_at": time.time() + 3600}
        self.assertEqual(self.request(path, "PATCH", {"action": "edit", "description": "Mine now"}, token="reader")[0], 403)
        app.SESSION_TOKENS.pop("reader", None)
        # Once a work order is made, the request is settled.
        self.assertEqual(self.request(path, "PATCH", {"action": "decline", "remark": "Fixed already"})[0], 200)
        self.assertEqual(self.request(path, "PATCH", {"action": "edit", "description": "Later"})[0], 409)

    def test_z_checks_keep_their_asset_name(self):
        status, _, body = self.request("/api/equipment", "POST", {"name": "Named Chiller", "outlet": "MST", "location": "Plant Room", "code": "NAME-CHILLER"})
        self.assertEqual(status, 200, body)
        asset = json.loads(body)["id"]
        # Checks sent without the asset's name or location (as a page left earlier sent them).
        items = [{"equipmentId": str(asset), "item": "Cold", "passed": False, "notes": "Warm", "priority": "High", "images": [{"name": "x.png"}]},
                 {"equipmentId": str(asset), "item": "Quiet", "passed": True, "images": [{"name": "y.png"}]}]
        status, _, body = self.request("/api/inspection-sessions", "POST", {"outlet": "MST", "auditDate": "2026-10-08", "items": items, "complete": True})
        self.assertEqual(status, 200, body)
        session = json.loads(self.request(f"/api/inspection-sessions/{json.loads(body)['id']}")[2])
        self.assertEqual({(item["section"], item["location"]) for item in session["items"]}, {("Named Chiller", "Plant Room")})
        with app.connect() as db:
            finding = db.execute("SELECT item_name, location FROM findings WHERE equipment_id = ?", (asset,)).fetchone()
        self.assertEqual(tuple(finding), ("Named Chiller", "Plant Room"))

    def test_z_old_audit_codes_are_converted(self):
        from backend.audit_metadata import convert_audit_codes
        with app.connect() as db:
            db.execute("DELETE FROM app_settings WHERE key = 'system.auditCodesV2'")
            session = db.execute("INSERT INTO inspection_sessions(business_unit, outlet, zone, audit_date, auditor, items_data_id, progress, status, "
                                 "created_at, updated_at, audit_ref, inspection_name) VALUES ('Ottotree', 'STP', 'All Locations', '2026-10-06', 'A', ?, 0, "
                                 "'Draft', 0, 0, 'AUD-2026-0942', 'STP_2026-10-06_13')", (app_save_value(db, []),)).lastrowid
            request = db.execute("INSERT INTO work_requests(business_unit, outlet, location, item_name, category, priority, department, audit_ref, "
                                 "description, status, created_at, updated_at) VALUES ('Ottotree', 'STP', 'Room', 'Sink', 'Safety', 'High', 'AVC', "
                                 "'AUD-2026-0942', 'x', 'Open', 0, 0)").lastrowid
            convert_audit_codes(db)
            row = db.execute("SELECT audit_ref, inspection_name FROM inspection_sessions WHERE id = ?", (session,)).fetchone()
            linked = db.execute("SELECT audit_ref FROM work_requests WHERE id = ?", (request,)).fetchone()
        self.assertEqual(tuple(row), ("AUDIT-STP-20261006-000942", "AUDIT-STP-20261006-000942"))
        self.assertEqual(linked[0], "AUDIT-STP-20261006-000942")
        # A draft's code follows its date.
        status, _, body = self.request(f"/api/inspection-sessions/{session}", "PATCH", {"auditDate": "2026-10-09", "items": []})
        self.assertEqual(status, 200, body)
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT audit_ref FROM inspection_sessions WHERE id = ?", (session,)).fetchone()[0], "AUDIT-STP-20261009-000942")

    def test_z_visit_covers_zones_or_assets(self):
        self.assertEqual(self.request("/api/setup/outlets", "POST", {"code": "SCOPE"})[0], 200)
        for name in ("Hall", "Bar", "Store"):
            self.assertEqual(self.request("/api/locations", "POST", {"outlet": "SCOPE", "name": name})[0], 200)
        self.assertEqual(self.request("/api/zones", "POST", {"outlet": "SCOPE", "name": "Front", "locations": ["Hall", "Bar"]})[0], 200)
        visit = {"outlet": "SCOPE", "scheduledDate": "2026-10-20"}
        # Zones: their locations.
        status, _, body = self.request("/api/schedules", "POST", visit | {"scope": "zones", "zones": ["Front"]})
        self.assertEqual(status, 200, body)
        row = next(item for item in json.loads(self.request("/api/schedules")[2])["items"] if item["id"] == json.loads(body)["id"])
        self.assertEqual((row["visit_scope"], sorted(row["visit_locations"]), row["zone"]), ({"by": "zones", "zones": ["Front"]}, ["Bar", "Hall"], "Zone: Front"))
        self.assertEqual(self.request("/api/schedules", "POST", visit | {"scope": "zones", "zones": []})[0], 400)
        self.assertEqual(self.request("/api/schedules", "POST", visit | {"scope": "zones", "zones": ["Elsewhere"]})[0], 400)
        # Assets: those assets, and their locations; the audit started from it keeps them.
        ids = []
        for name, location in (("Scope fridge", "Store"), ("Scope lamp", "Hall")):
            status, _, body = self.request("/api/equipment", "POST", {"name": name, "outlet": "SCOPE", "location": location})
            ids.append(json.loads(body)["id"])
        status, _, body = self.request("/api/schedules", "POST", visit | {"scope": "assets", "assets": ids[:1]})
        self.assertEqual(status, 200, body)
        schedule_id = json.loads(body)["id"]
        row = next(item for item in json.loads(self.request("/api/schedules")[2])["items"] if item["id"] == schedule_id)
        self.assertEqual((row["visit_scope"], row["visit_locations"]), ({"by": "assets", "assets": ids[:1]}, ["Store"]))
        session_id = json.loads(self.request("/api/schedules/start", "POST", {"scheduleId": schedule_id})[2])["id"]
        session = json.loads(self.request(f"/api/inspection-sessions/{session_id}")[2])
        self.assertEqual((session["visit_scope"], session["visit_locations"]), ({"by": "assets", "assets": ids[:1]}, ["Store"]))
        self.assertEqual(self.request("/api/schedules", "POST", visit | {"scope": "assets", "assets": [999999]})[0], 400)
        # Editing a visit back to locations clears the asset choice.
        status, _, body = self.request(f"/api/schedules/{schedule_id}", "PATCH", visit | {"scope": "locations", "locations": []})
        self.assertEqual(status, 200, body)
        row = next(item for item in json.loads(self.request("/api/schedules")[2])["items"] if item["id"] == schedule_id)
        self.assertEqual((row["visit_scope"], row["visit_locations"], row["zone"]), ({"by": "locations"}, [], "All Locations"))

    def test_z_visit_priority_and_due_date(self):
        with app.connect() as db:
            level = db.execute("SELECT name FROM priority_levels WHERE active = 1 ORDER BY id LIMIT 1").fetchone()[0]
        visit = {"outlet": "STP", "scheduledDate": "2026-11-02"}
        status, _, body = self.request("/api/schedules", "POST", visit | {"priority": level, "dueDate": "2026-11-05"})
        self.assertEqual(status, 200, body)
        schedule_id = json.loads(body)["id"]
        row = next(item for item in json.loads(self.request("/api/schedules")[2])["items"] if item["id"] == schedule_id)
        self.assertEqual((row["priority"], row["due_date"]), (level, "2026-11-05"))
        # Both are optional; an edit can clear them.
        self.assertEqual(self.request(f"/api/schedules/{schedule_id}", "PATCH", visit)[0], 200)
        row = next(item for item in json.loads(self.request("/api/schedules")[2])["items"] if item["id"] == schedule_id)
        self.assertEqual((row["priority"], row["due_date"]), ("", ""))
        for wrong in ({"priority": "Not a level"}, {"dueDate": "2026-11-01"}, {"dueDate": "soon"}):
            self.assertEqual(self.request("/api/schedules", "POST", visit | wrong)[0], 400, wrong)

if __name__ == "__main__":
    unittest.main()
