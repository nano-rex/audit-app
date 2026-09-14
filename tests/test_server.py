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


class QuietHandler(app.Handler):
    def log_message(self, *args):
        pass


class ServerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.storage = tempfile.TemporaryDirectory(prefix="audit-tests-")
        app.DATA_DIR = Path(cls.storage.name)
        app.DB_PATH = app.DATA_DIR / "test.db"
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

    def test_static_allowlist(self):
        for path in ("/data/ottotree_audit_web.db", "/server.py", "/README.md", "/js/../server.py", "/js/%2e%2e/server.py"):
            self.assertEqual(self.request(path, token=None)[0], 404, path)
        self.assertEqual(self.request("/", token=None)[0], 200)

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


if __name__ == "__main__":
    unittest.main()
