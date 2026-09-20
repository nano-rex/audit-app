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

SPEC = importlib.util.spec_from_file_location("audit_server", Path(__file__).resolve().parents[1] / "web/server.py")
app = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(app)
from backend.relational_values import save_value, load_value


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
            cls.user_id = db.execute("SELECT id FROM users WHERE role = 'Super'").fetchone()[0]
            db.execute("INSERT INTO users(name, role, email, active, created_at) VALUES ('Restricted', 'Auditor', 'restricted@test', 1, 0)")
            limited_id = db.execute("SELECT id FROM users WHERE email = 'restricted@test'").fetchone()[0]
        app.SESSION_TOKENS["test"] = {"user_id": cls.user_id, "expires_at": time.time() + 3600}
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

    def test_audit_closure_requires_signatures_and_closed_actions_and_is_immutable(self):
        from test_media_reports import photo_data_url
        payload = {"outlet": "STP", "auditDate": "2026-09-18", "items": [{"section": "Safety", "item": "Door", "passed": True}]}
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

    def test_administration_preserves_last_super_and_revokes_deactivated_sessions(self):
        path = f"/api/users/{self.user_id}"
        for payload in ({"active": False}, {"role": "Auditor"}):
            self.assertEqual(self.request(path, "PATCH", payload)[0], 409)
        self.assertEqual(self.request(path, "DELETE")[0], 409)
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
        with app.connect() as db:
            stored = [row[0] for row in db.execute("SELECT page_id FROM user_navigation WHERE user_id = ? ORDER BY position", (self.user_id,))]
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
        self.assertIn('data-history-findings-panel="history"', html)
        self.assertIn('History &amp; Findings', html)

    def test_static_compression_and_revalidation(self):
        status, headers, body = self.request("/", headers={"Accept-Encoding": "gzip"})
        self.assertEqual(status, 200)
        self.assertIn(b"Guided Inspection", gzip.decompress(body))
        status, _, body = self.request("/", headers={"Accept-Encoding": "gzip", "If-None-Match": headers["ETag"]})
        self.assertEqual((status, body), (304, b""))
        _, headers, _ = self.request("/", headers={"Accept-Encoding": "gzip;q=0"})
        self.assertNotIn("Content-Encoding", headers)

    def test_connection_closes_and_rolls_back(self):
        with app.connect() as db:
            self.assertEqual(db.execute("PRAGMA journal_mode").fetchone()[0], "wal")
        with self.assertRaises(sqlite3.ProgrammingError):
            db.execute("SELECT 1")
        with self.assertRaises(RuntimeError):
            with app.connect() as db:
                db.execute("UPDATE users SET name = 'Must roll back' WHERE id = ?", (self.user_id,))
                raise RuntimeError()
        with app.connect() as db:
            self.assertNotEqual(db.execute("SELECT name FROM users WHERE id = ?", (self.user_id,)).fetchone()[0], "Must roll back")

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
        self.assertEqual(self.request("/api/equipment", token=None)[0], 401)
        self.assertEqual(self.request("/api/users", token="limited")[0], 403)
        self.assertEqual(self.request("/api/setup/departments", "POST", {}, token="limited")[0], 403)
        self.assertEqual(self.request("/api/equipment", token="limited")[0], 200)
        self.assertEqual(self.request("/api/setup", token="limited")[0], 200)

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
            self.assertEqual(self.request("/api/work-orders", "POST", {"title": f"Test {number}", "priority": "Priority"})[0], 200)
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
            status, _, body = self.request("/api/inspection-sessions", "POST", {"outlet": "STP", "items": [{"category": "Safety", "passed": True}, {"category": "Other", "passed": False, "notes": "Repair"}], "complete": True})
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
            name = db.execute("SELECT name FROM users WHERE id = ?", (self.user_id,)).fetchone()[0]
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
            self.assertRegex(item["auditRef"], r"^AUD-2026-\d{4,}$")
        record = created[0]
        path = f"/api/inspection-sessions/{record['id']}"
        saved = json.loads(self.request(path)[2])
        self.assertEqual((saved["auditor"], saved["audit_time"], saved["audit_type"], saved["remarks"]), (name, "09:35", audit_type, "Morning review"))
        self.assertEqual(saved["audit_ref"], record["auditRef"])
        self.assertEqual(saved["schedule_id"], record["scheduleId"])
        self.assertEqual(saved["status"], "Draft")
        self.assertEqual(self.request(path, "PATCH", {"auditTime": "99:99"})[0], 400)
        status, _, body = self.request(path, "PATCH", {"items": [{"passed": True}], "complete": True})
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

    def test_z_failed_audit_retains_evidence_and_links_one_work_order(self):
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
            orders = db.execute("SELECT * FROM work_orders WHERE source_finding_id = ?", (findings[0]["id"],)).fetchall()
            self.assertEqual(len(orders), 1)

    def test_z_workflow_rules_identity_history_and_concurrent_close(self):
        from test_media_reports import photo_data_url
        for status, expected in (("Invalid", 400), ("Closed", 409), ("Verified", 409)):
            self.assertEqual(self.request("/api/work-orders", "POST", {"status": status})[0], expected)
        with app.connect() as db:
            cursor = db.execute("INSERT INTO users(name, role, email, department, active, created_at) VALUES ('Workflow PIC', 'Department/PIC', 'workflow@test', 'Technical', 1, 0)")
            app.SESSION_TOKENS["pic"] = {"user_id": cursor.lastrowid, "expires_at": time.time() + 3600}
        self.assertEqual(self.request("/api/work-orders", "POST", {"title": "Workflow test", "pic": "Workflow PIC"})[0], 200)
        with app.connect() as db:
            record_id = db.execute("SELECT id FROM work_orders WHERE title = 'Workflow test'").fetchone()[0]
        path = f"/api/work-orders/{record_id}"
        self.assertEqual(self.request(path, "PATCH", {"status": "Closed"})[0], 409)
        self.assertEqual(self.request(path, "PATCH", {"status": "Completed"}, token="pic")[0], 400)
        self.assertEqual(self.request(path, "PATCH", {"pic": "Someone else"}, token="pic")[0], 403)
        self.assertEqual(self.request(path, "DELETE", token="pic")[0], 403)
        _, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "repair.png"}})
        image = json.loads(body)["image"]
        completion = {"status": "Completed", "actionTaken": "Repaired", "completionDate": "2026-01-01",
                      "completionRemark": "Checked operation", "completionPhoto": [image]}
        self.assertEqual(self.request(path, "PATCH", completion, token="pic")[0], 200)
        self.assertEqual(self.request(path, "PATCH", {"status": "Verified", "verificationRemark": "Fine"}, token="pic")[0], 403)
        self.assertEqual(self.request(path, "PATCH", {"status": "Verified"})[0], 400)
        self.assertEqual(self.request(path, "PATCH", {"status": "In Progress", "verificationRemark": "Needs retest"})[0], 200)
        self.assertEqual(self.request(path, "PATCH", completion, token="pic")[0], 200)
        self.assertEqual(self.request(path, "PATCH", {"status": "Verified", "verifiedBy": "Forged identity", "verifiedAt": "2000-01-01", "verificationRemark": "Retest passed"})[0], 200)
        with app.connect() as db:
            row = db.execute("SELECT * FROM work_orders WHERE id = ?", (record_id,)).fetchone()
            self.assertEqual(row["title"], "Workflow test")  # Partial PATCH preserves omitted fields.
            self.assertNotEqual(row["verified_by"], "Forged identity")
            self.assertNotEqual(row["verified_at"], "2000-01-01")
            self.assertEqual(row["action_taken"], "Repaired")
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            statuses = list(pool.map(lambda _: self.request(path, "PATCH", {"status": "Closed", "verificationRemark": "Accepted"})[0], range(2)))
        self.assertEqual(sorted(statuses), [200, 409])
        self.assertEqual(self.request(path, "PATCH", {"title": "Changed"})[0], 409)
        self.assertEqual(self.request(path, "DELETE")[0], 409)
        with app.connect() as db:
            events = db.execute("SELECT * FROM comments WHERE record_type = 'work_order' AND record_id = ?", (record_id,)).fetchall()
        self.assertEqual(len(events), 5)
        self.assertTrue(all(row["system_generated"] for row in events))
        self.assertEqual(self.request(f"/api/comments/{events[0]['id']}", "DELETE")[0], 409)

    def test_z_workflow_rejects_changes_to_another_pic_assignment(self):
        self.assertEqual(self.request("/api/work-orders", "POST", {"title": "Other PIC", "pic": "Another person"})[0], 200)
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
                {"title": title, "requestType": department, "assignee": department, "pic": pic},
            )
            self.assertEqual(status, 200, body)
        _, _, body = self.request("/api/findings", token="assigned-pic")
        finding_refs = {row["finding_ref"] for row in json.loads(body)["items"]}
        self.assertEqual(finding_refs, {"F-PIC-MATCH", "F-DEPARTMENT"})
        _, _, body = self.request("/api/work-orders", token="assigned-pic")
        titles = {row["title"] for row in json.loads(body)["items"]}
        self.assertTrue({"PIC assignment", "Department assignment"}.issubset(titles))
        self.assertNotIn("Other PIC assignment", titles)
        with app.connect() as db:
            db.execute("DELETE FROM work_orders WHERE title IN ('PIC assignment', 'Department assignment', 'Other PIC assignment')")
            db.execute("DELETE FROM findings WHERE finding_ref IN ('F-PIC-MATCH', 'F-DEPARTMENT', 'F-OTHER-PIC')")
            db.execute("DELETE FROM audits WHERE id = ?", (audit_id,))
            db.execute("DELETE FROM users WHERE id = ?", (user_id,))
        app.SESSION_TOKENS.pop("assigned-pic", None)

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
            user = {"name": f"Test {key}", "email": f"{key}@example.test", "role": role["name"], "active": True}
            self.assertEqual(self.request("/api/users", "POST", user)[0], 200)
            with app.connect() as db:
                user_id = db.execute("SELECT id FROM users WHERE email = ?", (user["email"],)).fetchone()[0]
            app.SESSION_TOKENS[key] = {"user_id": user_id, "expires_at": time.time() + 3600}
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
        self.assertTrue(session["inspection_name"].endswith(f"_{session_id}"))
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "PATCH", {"items": [{"passed": True}], "complete": True})[0], 200)
        _, _, body = self.request("/api/schedules")
        saved = next(row for row in json.loads(body)["items"] if row["id"] == schedule["id"])
        self.assertEqual((saved["inspection_id"], saved["status"], saved["progress"]), (session_id, "Completed", 100))
        self.assertEqual(self.request(f"/api/schedules/{schedule['id']}", "DELETE")[0], 409)
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "DELETE")[0], 409)


if __name__ == "__main__":
    unittest.main()
