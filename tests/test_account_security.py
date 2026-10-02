"""Sign-in limits, account validation, database management, and failure responses."""
from datetime import date
import http.client
import json
import sqlite3
import tempfile
import threading
import time
import unittest
from unittest import mock

from test_server import QuietHandler, app
from backend import config, routes, routes_accounts
from backend.login_throttle import LOGIN_THROTTLE, REGISTRATION_THROTTLE, LoginThrottle
from backend.relational_values import save_value
from backend.reminders import deliver_due_reminders


class AccountSecurityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.previous = config.DATA_DIR
        cls.storage = tempfile.TemporaryDirectory(prefix="audit-accounts-")
        app.configure_data_directory(cls.storage.name)
        app.init_db()
        with app.connect() as db:
            # Starter accounts must change their password before using the API; these tests act as them directly.
            db.execute("UPDATE users SET reset_required = 0")
            cls.super_id = db.execute("SELECT id FROM users WHERE role = 'Super'").fetchone()[0]
            override = save_value(db, {"permissions": ["notifications"], "inspectionPermissions": ["verifier"]})
            cls.reviewer_id = db.execute("INSERT INTO users(name,role,email,active,created_at,permission_overrides_data_id) VALUES ('Reviewer','Auditor','reviewer@example.test',1,0,?)", (override,)).lastrowid
        app.SESSION_TOKENS["super"] = {"user_id": cls.super_id, "expires_at": time.time() + 3600}
        app.SESSION_TOKENS["reviewer"] = {"user_id": cls.reviewer_id, "expires_at": time.time() + 3600}
        cls.server = app.AuditHTTPServer(("127.0.0.1", 0), QuietHandler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()
        app.configure_data_directory(cls.previous)
        cls.storage.cleanup()

    def setUp(self):
        LOGIN_THROTTLE.clear()
        REGISTRATION_THROTTLE.clear()

    def request(self, path, method="GET", payload=None, token="super", headers=None):
        connection = http.client.HTTPConnection("127.0.0.1", self.server.server_port, timeout=10)
        request_headers = {"Cookie": f"ottotree_session={token}"} if token else {}
        request_headers.update(headers or {})
        connection.request(method, path, json.dumps(payload) if payload is not None else None, request_headers)
        response = connection.getresponse()
        result = response.status, dict(response.getheaders()), response.read()
        connection.close()
        return result

    def user_row(self, email):
        with app.connect() as db:
            return dict(db.execute("SELECT * FROM users WHERE email = ?", (email,)).fetchone())

    def test_failed_logins_are_limited_per_account_and_reset_by_success(self):
        login = lambda identifier, password: self.request("/api/auth/login", "POST", {"identifier": identifier, "password": password}, token=None)
        for _ in range(5):
            self.assertEqual(login("gavin", "wrong-password")[0], 401)
        status, headers, body = login("gavin", "123456")
        self.assertEqual(status, 429, body)
        self.assertGreater(int(headers["Retry-After"]), 0)
        self.assertIn("Too many", json.loads(body)["error"])
        self.assertEqual(login("jacky", "123456")[0], 200)
        LOGIN_THROTTLE.clear()
        for _ in range(4):
            self.assertEqual(login("gavin", "wrong-password")[0], 401)
        self.assertEqual(login("gavin", "123456")[0], 200)
        self.assertEqual(login("gavin", "wrong-password")[0], 401)
        self.assertEqual(login("gavin", "123456")[0], 200)

    def test_registration_is_limited_per_address(self):
        register = lambda number: self.request("/api/auth/register", "POST", {"name": "Sign Up", "email": f"signup{number}@example.test", "password": "a-long-password"}, token=None)
        with mock.patch.object(routes_accounts, "hash_password", return_value="pbkdf2_sha256$1$salt$digest"):
            for number in range(10):
                self.assertEqual(register(number)[0], 200)
            status, headers, body = register(10)
        self.assertEqual(status, 429, body)
        self.assertGreater(int(headers["Retry-After"]), 0)
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT count(*) FROM users WHERE email LIKE 'signup%@example.test'").fetchone()[0], 10)

    def test_new_databases_start_with_accounts_that_must_change_their_password(self):
        from backend.database_manager import create_database, remove_database
        create_database("fresh")
        try:
            with sqlite3.connect(config.DATA_DIR / "fresh.db") as db:
                flags = {row[0] for row in db.execute("SELECT reset_required FROM users")}
            self.assertEqual(flags, {1})
        finally:
            remove_database("fresh")

    def test_throttle_window_expires_and_memory_is_bounded(self):
        now = [0.0]
        throttle = LoginThrottle(limit=2, window=60, capacity=3, clock=lambda: now[0])
        throttle.failure("a")
        self.assertEqual(throttle.retry_after("a"), 0)
        now[0] = 10
        throttle.failure("a")
        self.assertEqual(throttle.retry_after("a"), 50)
        now[0] = 60
        self.assertEqual(throttle.retry_after("a"), 0)
        for key in "bcde":
            throttle.failure(key)
        self.assertLessEqual(len(throttle.failures), 3)

    def test_unknown_account_login_costs_one_password_hash(self):
        with mock.patch.object(routes_accounts, "verify_password", wraps=routes_accounts.verify_password) as verify:
            self.assertEqual(self.request("/api/auth/login", "POST", {"identifier": "nobody@example.test", "password": "x"}, token=None)[0], 401)
        self.assertEqual(verify.call_count, 1)
        self.assertTrue(verify.call_args[0][1].startswith("pbkdf2_sha256$"))

    def test_unauthenticated_requests_are_refused_before_the_body_is_read(self):
        # No body follows these headers; the server must answer without waiting for one.
        started = time.monotonic()
        status, _, _ = self.request("/api/users", "POST", token=None, headers={"Content-Length": str(5 * 1024 * 1024)})
        self.assertEqual(status, 401)
        status, _, _ = self.request("/api/users/1", "PATCH", token=None, headers={"Content-Length": str(5 * 1024 * 1024)})
        self.assertEqual(status, 401)
        self.assertEqual(self.request("/api/auth/login", "POST", token=None, headers={"Content-Length": "100000"})[0], 413)
        self.assertLess(time.monotonic() - started, 5)
        self.assertEqual(self.request("/api/users", "POST", {"name": "Anonymous"}, token=None)[0], 401)

    def test_responses_carry_browser_protection_headers(self):
        for path in ("/login.html", "/api/branding", "/missing"):
            _, headers, _ = self.request(path, token=None)
            self.assertEqual(headers["X-Content-Type-Options"], "nosniff", path)
            self.assertEqual(headers["X-Frame-Options"], "SAMEORIGIN", path)
            self.assertEqual(headers["Referrer-Policy"], "same-origin", path)

    def test_unexpected_route_failure_returns_json_error(self):
        def broken(handler, parsed, payload=None):
            raise RuntimeError("unexpected fault")
        with mock.patch.dict(routes.ROUTES["POST"], {"/api/schedules": broken}), self.assertLogs(level="ERROR") as logs:
            status, _, body = self.request("/api/schedules", "POST", {})
        self.assertEqual(status, 500)
        self.assertEqual(json.loads(body), {"error": "The server could not complete this request"})
        self.assertIn("unexpected fault", "\n".join(logs.output))

    def test_user_administration_validates_fields(self):
        base = {"name": "Checked User", "email": "checked@example.test", "role": "Auditor"}
        invalid = ({"name": 5}, {"name": " "}, {"email": "not-an-email"}, {"email": ["x"]}, {"role": "No such role"},
                   {"department": "No such department"}, {"password": "short"}, {"title": {"nested": True}})
        for change in invalid:
            status, _, body = self.request("/api/users", "POST", base | change)
            self.assertEqual(status, 400, (change, body))
        self.assertEqual(self.request("/api/users", "POST", base)[0], 200)
        created = self.user_row("checked@example.test")
        self.assertEqual((created["username"], created["reset_required"], created["active"]), ("checked", 1, 1))
        self.assertTrue(app.verify_password(config.DEFAULT_PASSWORD, created["password_hash"]))
        self.assertEqual(self.request("/api/users", "POST", base | {"email": "CHECKED@example.test", "username": "other"})[0], 409)
        self.assertEqual(self.request("/api/users", "POST", base | {"email": "second@example.test", "username": "checked"})[0], 409)
        chosen = base | {"email": "Chosen@Example.Test", "password": "long-enough-password"}
        self.assertEqual(self.request("/api/users", "POST", chosen)[0], 200)
        self.assertEqual(self.user_row("chosen@example.test")["reset_required"], 0)
        target = f"/api/users/{created['id']}"
        self.assertEqual(self.request(target, "PATCH", {"email": "chosen@example.test"})[0], 409)
        self.assertEqual(self.request(target, "PATCH", {"role": "No such role"})[0], 400)
        self.assertEqual(self.request(target, "PATCH", {"username": "renamed", "title": "Lead"})[0], 200)
        renamed = self.user_row("checked@example.test")
        self.assertEqual((renamed["username"], renamed["title"]), ("renamed", "Lead"))

    def test_password_reset_requires_change_and_keeps_account_deactivated(self):
        user = {"name": "Dormant User", "email": "dormant@example.test", "role": "Auditor", "password": "original-password"}
        self.assertEqual(self.request("/api/users", "POST", user)[0], 200)
        target = f"/api/users/{self.user_row(user['email'])['id']}"
        self.assertEqual(self.request(target, "PATCH", {"active": False})[0], 200)
        self.assertEqual(self.request(target, "PATCH", {"resetPassword": True, "password": "temporary-password"})[0], 200)
        row = self.user_row(user["email"])
        self.assertEqual((row["active"], row["reset_required"]), (0, 1))
        self.assertTrue(app.verify_password("temporary-password", row["password_hash"]))

    def test_required_password_change_blocks_the_api_until_done(self):
        user = {"name": "Temporary User", "email": "temporary@example.test", "role": "Auditor"}
        self.assertEqual(self.request("/api/users", "POST", user)[0], 200)
        status, headers, _ = self.request("/api/auth/login", "POST", {"identifier": user["email"], "password": config.DEFAULT_PASSWORD}, token=None)
        self.assertEqual(status, 200)
        token = headers["Set-Cookie"].split(";", 1)[0].split("=", 1)[1]
        status, _, body = self.request("/api/setup", token=token)
        self.assertEqual((status, json.loads(body).get("resetRequired")), (403, True))
        self.assertEqual(self.request("/api/account", "PATCH", {"name": "Renamed"}, token=token)[0], 403)
        self.assertEqual(self.request("/api/equipment", "POST", {"name": "Blocked"}, token=token)[0], 403)
        # The account can still see who it is, change the password, and sign out.
        self.assertTrue(json.loads(self.request("/api/auth/me", token=token)[2])["user"]["resetRequired"])
        self.assertEqual(self.request("/api/account", token=token)[0], 200)
        change = {"oldPassword": config.DEFAULT_PASSWORD, "newPassword": "a-new-long-password"}
        self.assertEqual(self.request("/api/auth/change-password", "POST", change, token=token)[0], 200)
        self.assertEqual(self.request("/api/setup", token=token)[0], 200)

    def test_notifications_can_all_be_marked_read_and_cannot_be_broadcast(self):
        with app.connect() as db:
            for title in ("First", "Second"):
                db.execute("INSERT INTO notifications(title,message,channel,status,created_at,recipient_user_id) VALUES (?, '', 'In-App', 'Unread', 0, ?)", (title, self.reviewer_id))
            other = db.execute("INSERT INTO notifications(title,message,channel,status,created_at,recipient_user_id) VALUES ('Other', '', 'In-App', 'Unread', 0, ?)", (self.super_id,)).lastrowid
        self.assertGreaterEqual(json.loads(self.request("/api/todo", token="reviewer")[2])["unreadNotifications"], 2)
        status, _, body = self.request("/api/notifications/all", "PATCH", {}, token="reviewer")
        self.assertEqual(status, 200, body)
        self.assertGreaterEqual(json.loads(body)["updated"], 2)
        self.assertEqual(json.loads(self.request("/api/todo", token="reviewer")[2])["unreadNotifications"], 0)
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT status FROM notifications WHERE id = ?", (other,)).fetchone()[0], "Unread")
        self.assertEqual(self.request("/api/notifications", "POST", {"title": "To everyone"}, token="reviewer")[0], 404)

    def test_accounts_with_older_addresses_remain_editable(self):
        # The built-in Super address has no dotted domain; a profile save must not be rejected for it.
        status, _, body = self.request("/api/account", "PATCH", {"name": "Super User"})
        self.assertEqual(status, 200, body)
        self.assertEqual(self.request("/api/account", "PATCH", {"email": "still-invalid"})[0], 400)
        with app.connect() as db:
            legacy_id = db.execute("INSERT INTO users(name,role,email,department,active,created_at) VALUES ('Legacy','Retired role','legacy@intranet','GONE',1,0)").lastrowid
        self.assertEqual(self.request(f"/api/users/{legacy_id}", "PATCH", {"active": False})[0], 200)
        self.assertEqual(self.user_row("legacy@intranet")["active"], 0)

    def test_super_manages_every_role_and_others_never_see_the_super_role(self):
        names = lambda token: {row["name"] for row in json.loads(self.request("/api/roles", token=token)[2])["items"]}
        with app.connect() as db:
            admin_id = db.execute("SELECT id FROM users WHERE role = 'Admin'").fetchone()[0]
        app.SESSION_TOKENS["role-admin"] = {"user_id": admin_id, "expires_at": time.time() + 3600}
        self.assertLessEqual({"Super", "Admin", "Auditor"}, names("super"))
        self.assertIn("Auditor", names("role-admin"))
        self.assertNotIn("Super", names("role-admin"))
        setup = json.loads(self.request("/api/setup", token="super")[2])
        self.assertIn("Auditor", {row["name"] for row in setup["roles"]})

    def test_work_orders_become_overdue_without_being_edited(self):
        with app.connect() as db:
            order_id = db.execute("INSERT INTO work_orders(business_unit,outlet,zone,request_type,priority,title,assignee,status,due_date,sla_status,created_at) "
                                  "VALUES ('Ottotree','STP','Room','SSD','High','Stale SLA','Super User','Assigned','2020-01-01','On Track',0)").lastrowid
        listed = next(row for row in json.loads(self.request("/api/work-orders")[2])["items"] if row["id"] == order_id)
        self.assertEqual(listed["sla_status"], "Overdue")
        task = next(row for row in json.loads(self.request("/api/todo")[2])["items"] if row["type"] == "work_order" and row["id"] == order_id)
        self.assertTrue(task["overdue"])
        critical = next(row for row in json.loads(self.request("/api/reports?unit=Ottotree")[2])["criticalIssues"] if row["id"] == order_id)
        self.assertEqual(critical["sla_status"], "Overdue")
        with app.connect() as db:
            db.execute("DELETE FROM work_orders WHERE id = ?", (order_id,))

    def test_open_schedules_are_listed_before_finished_ones(self):
        with app.connect() as db:
            for day, status in (("2020-01-01", "Completed"), ("2031-01-01", "Pending"), ("2020-01-02", "Cancelled")):
                db.execute("INSERT INTO schedules(business_unit,outlet,zone,scheduled_date,auditor,remarks,status,created_at) VALUES ('Ottotree','STP','Order test',?,?,'',?,0)", (day, "Tester", status))
        rows = [row for row in json.loads(self.request("/api/schedules")[2])["items"] if row["zone"] == "Order test"]
        self.assertEqual([row["status"] for row in rows], ["Pending", "Completed", "Cancelled"])

    def test_todo_lists_each_users_next_steps_through_the_workflow(self):
        from test_media_reports import photo_data_url
        todo = lambda token: json.loads(self.request("/api/todo", token=token)[2])
        actions = lambda token: [(row["type"], row["action"]) for row in todo(token)["items"]]
        self.assertEqual(self.request("/api/todo", token=None)[0], 401)
        with app.connect() as db:
            department = db.execute("SELECT code FROM departments ORDER BY code LIMIT 1").fetchone()[0]
            role = db.execute("SELECT name FROM roles WHERE name = 'System Support Executive'").fetchone()[0]
            pic_id = db.execute("INSERT INTO users(name,role,email,department,active,created_at) VALUES ('Todo PIC',?,'todo-pic@example.test',?,1,0)", (role, department)).lastrowid
        app.SESSION_TOKENS["todo-pic"] = {"user_id": pic_id, "expires_at": time.time() + 3600}
        before = len(todo("super")["items"])
        _, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "evidence.png"}})
        image = json.loads(body)["image"]
        items = [{"item": "Works", "passed": True, "images": [image]},
                 {"item": "Clean", "notes": "Dusty", "priority": "High", "assignedDepartment": department, "pic": "Todo PIC", "images": [image]}]
        status, _, body = self.request("/api/inspection-sessions", "POST", {"outlet": "STP", "items": items[:1] + [{"item": "Clean"}]})
        session_id = json.loads(body)["id"]
        self.assertIn(("inspection", "Continue inspection"), actions("super"))
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "PATCH", {"items": items, "complete": True})[0], 200)
        self.assertIn(("inspection", "Sign as auditor, verifier, acknowledger"), actions("super"))
        # The reviewer has the verifier capability but no Inspections page, so nothing is offered to them.
        self.assertEqual(actions("reviewer"), [])
        self.assertEqual(actions("todo-pic"), [("work_order", "Complete corrective action")])
        order_id = todo("todo-pic")["items"][0]["id"]
        self.assertGreaterEqual(todo("todo-pic")["unreadNotifications"], 1)
        done = {"status": "Completed", "actionTaken": "Cleaned", "completionDate": time.strftime("%Y-%m-%d"), "completionRemark": "Done", "completionPhoto": [image]}
        self.assertEqual(self.request(f"/api/work-orders/{order_id}", "PATCH", done, token="todo-pic")[0], 200)
        self.assertEqual(actions("todo-pic"), [])
        self.assertIn(("work_order", "Verify corrective action"), actions("super"))
        # A verifier accepts and closes in one step; the verification is still recorded.
        self.assertEqual(self.request(f"/api/work-orders/{order_id}", "PATCH", {"status": "Closed"}, token="super")[0], 400)
        self.assertEqual(self.request(f"/api/work-orders/{order_id}", "PATCH", {"status": "Closed", "verificationRemark": "Accepted"}, token="todo-pic")[0], 403)
        self.assertEqual(self.request(f"/api/work-orders/{order_id}", "PATCH", {"status": "Closed", "verificationRemark": "Accepted"}, token="super")[0], 200)
        with app.connect() as db:
            order = db.execute("SELECT status, verified_by, verified_at, closed_at FROM work_orders WHERE id = ?", (order_id,)).fetchone()
        self.assertEqual((order["status"], order["verified_by"]), ("Closed", "Super User"))
        self.assertTrue(order["verified_at"] and order["closed_at"])
        signatures = {key: image for key in ("auditedBy", "verifiedBy", "acknowledgedBy")}
        self.assertEqual(self.request(f"/api/inspection-sessions/{session_id}", "PATCH", {"signatures": signatures})[0], 200)
        self.assertIn(("inspection", "Close audit"), actions("super"))
        self.assertEqual(self.request("/api/inspection-sessions/close", "POST", {"id": session_id})[0], 200)
        self.assertEqual(len(todo("super")["items"]), before)

    def test_retired_corrective_actions_permission_becomes_work_orders_on_upgrade(self):
        from backend.permissions import resolve_permissions
        with app.connect() as db:
            role = save_value(db, ["today", "corrective-actions", "work-orders"])
            db.execute("INSERT INTO roles(name, description, permissions_data_id, protected, created_at) VALUES ('Legacy fixer', '', ?, 0, 0)", (role,))
            override = save_value(db, {"permissions": ["corrective-actions"], "inspectionPermissions": []})
            user_id = db.execute("INSERT INTO users(name,role,email,active,created_at,permission_overrides_data_id) VALUES ('Legacy override','Auditor','legacy-override@example.test',1,0,?)", (override,)).lastrowid
        app.init_db()  # What a restart on the new release does.
        with app.connect() as db:
            self.assertEqual(resolve_permissions(db, "Legacy fixer")[0], ["today", "work-orders"])
        app.SESSION_TOKENS["legacy-override"] = {"user_id": user_id, "expires_at": time.time() + 3600}
        status, _, body = self.request("/api/account", token="legacy-override")
        self.assertEqual(status, 200, body)
        self.assertEqual(json.loads(body)["user"]["permissions"], ["work-orders"])
        self.assertEqual(self.request("/api/work-orders", token="legacy-override")[0], 200)
        self.assertEqual(self.request("/api/roles", "POST", {"name": "Uses retired page", "permissions": ["corrective-actions"]})[0], 400)

    def test_fixtures_and_assets_share_one_register_with_generated_codes(self):
        fixture = {"kind": "fixture", "name": "Floor tiles", "outlet": "STP", "location": "R-01", "category": "Building",
                   "serialNumber": "ignored for a fixture", "brand": "ignored"}
        status, _, body = self.request("/api/equipment", "POST", fixture)
        self.assertEqual(status, 200, body)
        created = json.loads(body)
        self.assertEqual(created["code"], f"FXT-{created['id']:05d}")
        status, _, body = self.request("/api/equipment", "POST", {"name": "Amplifier", "outlet": "STP", "location": "R-01"})
        asset = json.loads(body)
        self.assertEqual(asset["code"], f"AST-{asset['id']:05d}")
        rows = {row["id"]: row for row in json.loads(self.request("/api/equipment?outlet=STP")[2])["items"]}
        stored = rows[created["id"]]
        self.assertEqual((stored["kind"], stored["category"], stored["serial_number"], stored["brand"]), ("fixture", "Building", "", ""))
        self.assertEqual(stored["inspection_criteria"], list(config.DEFAULT_FIXTURE_CRITERIA))
        self.assertEqual(rows[asset["id"]]["kind"], "asset")
        compact = {row["id"]: row for row in json.loads(self.request("/api/equipment?outlet=STP&view=inspection")[2])["items"]}
        self.assertEqual((compact[created["id"]]["kind"], compact[created["id"]]["category"]), ("fixture", "Building"))
        # A code already in use is refused; it used to replace the other item.
        self.assertEqual(self.request("/api/equipment", "POST", {"name": "Clash", "code": created["code"]})[0], 409)
        self.assertEqual(self.request("/api/equipment", "POST", {"name": "Bad kind", "kind": "vehicle"})[0], 400)
        self.assertEqual(self.request("/api/equipment", "POST", {"name": "Bad category", "category": "No such category"})[0], 400)
        self.assertEqual(self.request("/api/equipment", "POST", {"kind": "fixture"})[0], 400)
        # Editing keeps the kind and the code unless they are sent.
        self.assertEqual(self.request(f"/api/equipment/{created['id']}", "PATCH", {"name": "Floor tiles (lobby)", "category": "Building"})[0], 200)
        edited = next(row for row in json.loads(self.request("/api/equipment?outlet=STP")[2])["items"] if row["id"] == created["id"])
        self.assertEqual((edited["kind"], edited["code"], edited["name"]), ("fixture", created["code"], "Floor tiles (lobby)"))

    def test_findings_go_to_the_department_responsible_for_their_category(self):
        from test_media_reports import photo_data_url
        _, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "evidence.png"}})
        image = json.loads(body)["image"]
        with app.connect() as db:
            departments = [row[0] for row in db.execute("SELECT code FROM departments ORDER BY code")]
        owner = departments[-1]
        self.assertEqual(self.request("/api/setup/categories", "POST", {"name": "Routed category", "department": "No such department"})[0], 400)
        self.assertEqual(self.request("/api/setup/categories", "POST", {"name": "Routed category", "department": owner, "sequence": 50})[0], 200)
        category = next(row for row in json.loads(self.request("/api/setup")[2])["categories"] if row["name"] == "Routed category")
        self.assertEqual(category["department"], owner)
        self.assertEqual(self.request("/api/equipment", "POST", {"kind": "fixture", "name": "Routed sink", "category": "Routed category"})[0], 200)
        base = {"section": "Routed sink", "notes": "Leaking", "priority": "High", "pic": "Routing Owner", "category": "Routed category", "images": [image]}
        items = [base | {"item": "By category"}, base | {"item": "Chosen", "assignedDepartment": departments[0]}]
        status, _, body = self.request("/api/inspection-sessions", "POST", {"outlet": "STP", "items": items, "complete": True})
        self.assertEqual(status, 200, body)
        with app.connect() as db:
            routed = dict(db.execute("SELECT title, request_type FROM work_orders WHERE category = 'Routed category'").fetchall())
        self.assertEqual({title.rsplit(" - ", 1)[-1]: department for title, department in routed.items()}, {"By category": owner, "Chosen": departments[0]})
        # Renaming the category keeps its items attached; deleting it leaves them uncategorised.
        self.assertEqual(self.request(f"/api/setup/categories/{category['id']}", "PATCH", {"name": "Routed renamed", "department": owner})[0], 200)
        sink = lambda: next(row for row in json.loads(self.request("/api/equipment")[2])["items"] if row["name"] == "Routed sink")
        self.assertEqual(sink()["category"], "Routed renamed")
        self.assertEqual(self.request(f"/api/setup/categories/{category['id']}", "DELETE")[0], 200)
        self.assertEqual(sink()["category"], "")

    def test_completing_an_inspection_requires_photo_evidence(self):
        from test_media_reports import photo_data_url
        _, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "evidence.png"}})
        image = json.loads(body)["image"]
        complete = lambda items: self.request("/api/inspection-sessions", "POST", {"outlet": "STP", "items": items, "complete": True})
        passed, failed = {"section": "Speaker", "item": "Works", "passed": True}, {"section": "Screen", "item": "Clean", "notes": "Cracked", "priority": "High", "pic": "Photo Rule Owner"}
        status, _, body = complete([passed])
        self.assertEqual(status, 409, body)
        self.assertIn("Add a photo for Speaker", json.loads(body)["error"])
        # A draft may be saved without photos.
        self.assertEqual(self.request("/api/inspection-sessions", "POST", {"outlet": "STP", "items": [passed]})[0], 200)
        self.assertEqual(self.request("/api/settings", "POST", {"settings": {"system.requirePhotoEveryAsset": False}})[0], 200)
        try:
            self.assertEqual(complete([passed])[0], 200)
            self.assertIn("Add a photo for Screen", json.loads(complete([passed, failed])[2])["error"])
            self.assertEqual(complete([passed, failed | {"images": [image]}])[0], 200)
        finally:
            self.assertEqual(self.request("/api/settings", "POST", {"settings": {"system.requirePhotoEveryAsset": True}})[0], 200)

    def test_admin_can_change_the_photo_evidence_option(self):
        with app.connect() as db:
            admin_id = db.execute("SELECT id FROM users WHERE role = 'Admin'").fetchone()[0]
        app.SESSION_TOKENS["option-admin"] = {"user_id": admin_id, "expires_at": time.time() + 3600}
        settings = lambda: json.loads(self.request("/api/setup", token="option-admin")[2])["settings"]
        self.assertIsNot(settings().get("system.requirePhotoEveryAsset"), False)
        saved = {"settings": {"system.requirePhotoEveryAsset": False, "report.companyName": "Not allowed for Admin"}}
        self.assertEqual(self.request("/api/settings", "POST", saved, token="option-admin")[0], 200)
        self.assertIs(settings()["system.requirePhotoEveryAsset"], False)
        self.assertNotEqual(settings().get("report.companyName"), "Not allowed for Admin")
        self.assertEqual(self.request("/api/settings", "POST", {"settings": {"system.requirePhotoEveryAsset": True}}, token="option-admin")[0], 200)

    def test_database_creation_switch_and_removal(self):
        original = config.DB_PATH
        outcome = {}
        worker = threading.Thread(target=lambda: outcome.update(result=self.request("/api/account/databases", "POST", {"name": "second"})))
        worker.start()
        statuses = set()
        while worker.is_alive():
            # Creating a database must not redirect other signed-in users to it.
            statuses.add(self.request("/api/auth/me")[0])
            self.assertEqual(config.DB_PATH, original)
        worker.join()
        self.assertEqual(outcome["result"][0], 200, outcome["result"][2])
        self.assertLessEqual(statuses, {200})
        second = config.DATA_DIR / "second.db"
        try:
            (config.DATA_DIR / "notes.db").write_bytes(b"not a database")
            self.assertEqual(self.request("/api/account/databases", "PATCH", {"name": "notes"})[0], 400)
            self.assertEqual(config.DB_PATH, original)
            with sqlite3.connect(second) as other:
                user_id = other.execute("SELECT id FROM users LIMIT 1").fetchone()[0]
                other.execute("INSERT INTO auth_sessions(token_hash,user_id,expires_at) VALUES ('stale',?,?)", (user_id, time.time() + 3600))
            status, _, body = self.request("/api/account/databases", "PATCH", {"name": "second"})
            self.assertEqual(status, 200, body)
            self.assertTrue(json.loads(body)["database"]["requiresLogin"])
            self.assertEqual(config.DB_PATH, second.resolve())
            # The choice is recorded so that a restart returns to it.
            from backend.database_manager import restore_active_database
            config.DB_PATH = original
            self.assertTrue(restore_active_database())
            self.assertEqual(config.DB_PATH, second.resolve())
            self.assertEqual(self.request("/api/auth/me")[0], 401)
            with app.connect() as db:
                self.assertEqual(db.execute("SELECT count(*) FROM auth_sessions").fetchone()[0], 0)
        finally:
            app.configure_data_directory(self.storage.name)
        for suffix in ("-wal", "-shm"):
            second.with_name(second.name + suffix).write_bytes(b"")
        self.assertEqual(self.request("/api/account/databases/second", "DELETE")[0], 200)
        self.assertEqual(sorted(path.name for path in config.DATA_DIR.glob("second.db*")), [])

    def test_repeated_draft_saves_refresh_one_unread_progress_notice(self):
        notices = lambda: json.loads(self.request("/api/notifications", token="reviewer")[2])["items"]
        progress = lambda: [row for row in notices() if row["title"] == "Audit progress updated" and row["related_id"] == session_id]
        payload = {"outlet": "STP", "items": [{"item": "First", "passed": True}, {"item": "Second"}, {"item": "Third"}]}
        status, _, body = self.request("/api/inspection-sessions", "POST", payload)
        self.assertEqual(status, 200, body)
        session_id = json.loads(body)["id"]
        path = f"/api/inspection-sessions/{session_id}"
        payload["items"][1]["passed"] = True
        self.assertEqual(self.request(path, "PATCH", payload)[0], 200)
        unread = progress()
        self.assertEqual(len(unread), 1)
        self.assertIn("66% complete", unread[0]["message"])
        self.assertEqual(self.request(f"/api/notifications/{unread[0]['id']}", "PATCH", {}, token="reviewer")[0], 200)
        payload["items"][2]["notes"] = "Checked"
        self.assertEqual(self.request(path, "PATCH", payload)[0], 200)
        self.assertEqual([row["status"] for row in progress()], ["Unread", "Read"])

    def test_reminder_check_does_not_wait_for_the_write_lock(self):
        writer = sqlite3.connect(config.DB_PATH, timeout=0, isolation_level=None)
        try:
            writer.execute("BEGIN IMMEDIATE")
            started = time.monotonic()
            self.assertEqual(deliver_due_reminders(date(2020, 1, 1)), 0)
            self.assertLess(time.monotonic() - started, 5)
        finally:
            writer.execute("ROLLBACK")
            writer.close()


if __name__ == "__main__":
    unittest.main()
