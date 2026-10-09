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
            override = save_value(db, {"permissions": ["notifications"], "inspectionPermissions": ["verifier"]})
            cls.reviewer_id = db.execute("INSERT INTO users(name,role,email,active,created_at,permission_overrides_data_id) VALUES ('Reviewer','Auditor','reviewer@example.test',1,0,?)", (override,)).lastrowid
        cls.super_id = app.first_super_id()
        from backend.control import connect_control
        with connect_control() as control_db:
            control_db.execute("UPDATE super_users SET reset_required = 0")
        app.SUPER_SESSIONS["super"] = {"user_id": cls.super_id, "expires_at": time.time() + 3600}
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
        self.assertLessEqual({"Admin", "Auditor"}, names("super"))
        self.assertNotIn("Super", names("super"), "the Super role is not an organization role")
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
        # Nothing is assigned until the finding is requested and a work order is made from the request.
        self.assertEqual(actions("todo-pic"), [])
        # The tabs count what waits: a sign-off, and an item still needing a work request.
        counts = todo("super")["counts"]
        self.assertGreaterEqual(counts["signoff"], 1)
        findings_before = counts["findings"]
        self.assertGreaterEqual(findings_before, 1)
        with app.connect() as db:
            finding_ids = [row[0] for row in db.execute("SELECT id FROM findings WHERE audit_id = (SELECT audit_id FROM inspection_sessions WHERE id = ?)", (session_id,))]
        request_id = json.loads(self.request("/api/work-requests", "POST", {"findingIds": finding_ids, "description": "Clean it"})[2])["id"]
        self.assertIn(("work_request", "Review work request"), actions("super"))
        counts = todo("super")["counts"]
        self.assertEqual(counts["findings"], findings_before - 1)
        self.assertGreaterEqual(counts["requests"], 1)
        self.assertEqual(self.request("/api/work-orders", "POST", {"title": "Clean", "requestType": department, "pic": "Todo PIC", "workRequestId": request_id})[0], 200)
        self.assertNotIn(("work_request", "Review work request"), actions("super"))
        self.assertEqual(actions("todo-pic"), [("work_order", "Resolve work order")])
        order_id = todo("todo-pic")["items"][0]["id"]
        self.assertGreaterEqual(todo("todo-pic")["unreadNotifications"], 1)
        # The person in charge closes the work order when it is done; there is no separate verification.
        self.assertEqual(self.request(f"/api/work-orders/{order_id}", "PATCH", {"status": "Closed"}, token="todo-pic")[0], 200)
        self.assertEqual(actions("todo-pic"), [])
        with app.connect() as db:
            order = db.execute("SELECT status, closed_at FROM work_orders WHERE id = ?", (order_id,)).fetchone()
            finding = db.execute("SELECT status, closed_at FROM findings WHERE work_request_id = (SELECT work_request_id FROM work_orders WHERE id = ?)", (order_id,)).fetchone()
        self.assertEqual(order["status"], "Closed")
        self.assertTrue(order["closed_at"])
        self.assertEqual((finding["status"], finding["closed_at"]), ("Closed", order["closed_at"]))
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

    def test_variable_and_fixed_assets_share_one_register_with_generated_codes(self):
        # A variable asset has every detail of a fixed asset, but no code label of its own.
        variable = {"kind": "fixture", "name": "Floor tiles", "outlet": "STP", "location": "R-01", "type": "Building",
                    "serialNumber": "SN-TILES-1", "brand": "Tilecraft", "installationDate": "2024-01-02"}
        status, _, body = self.request("/api/equipment", "POST", variable)
        self.assertEqual(status, 200, body)
        created = json.loads(body)
        self.assertEqual(created["code"], f"VAR-{created['id']:05d}")
        status, _, body = self.request("/api/equipment", "POST", {"name": "Amplifier", "outlet": "STP", "location": "R-01"})
        asset = json.loads(body)
        self.assertEqual(asset["code"], f"AST-{asset['id']:05d}")
        rows = {row["id"]: row for row in json.loads(self.request("/api/equipment?outlet=STP")[2])["items"]}
        stored = rows[created["id"]]
        self.assertEqual((stored["kind"], stored["type"], stored["serial_number"], stored["brand"], stored["installation_date"]),
                         ("fixture", "Building", "SN-TILES-1", "Tilecraft", "2024-01-02"))
        self.assertEqual(stored["inspection_criteria"], list(config.DEFAULT_FIXTURE_CRITERIA))
        self.assertEqual(rows[asset["id"]]["kind"], "asset")
        # Without a type a variable asset is Others.
        status, _, body = self.request("/api/equipment", "POST", {"kind": "fixture", "name": "Odd thing", "outlet": "STP", "location": "R-01"})
        self.assertEqual(next(row for row in json.loads(self.request("/api/equipment?outlet=STP")[2])["items"] if row["id"] == json.loads(body)["id"])["type"], "Others")
        # A code already in use is refused; it used to replace the other item.
        self.assertEqual(self.request("/api/equipment", "POST", {"name": "Clash", "code": created["code"]})[0], 409)
        self.assertEqual(self.request("/api/equipment", "POST", {"name": "Bad kind", "kind": "vehicle"})[0], 400)
        self.assertEqual(self.request("/api/equipment", "POST", {"kind": "fixture"})[0], 400)
        # Editing keeps the kind and the code unless they are sent.
        self.assertEqual(self.request(f"/api/equipment/{created['id']}", "PATCH", {"name": "Floor tiles (lobby)"})[0], 200)
        edited = next(row for row in json.loads(self.request("/api/equipment?outlet=STP")[2])["items"] if row["id"] == created["id"])
        self.assertEqual((edited["kind"], edited["code"], edited["name"], edited["brand"]), ("fixture", created["code"], "Floor tiles (lobby)", "Tilecraft"))
        # The asset types: the variable types, and every type in use.
        setup = json.loads(self.request("/api/setup")[2])
        self.assertNotIn("categories", setup)
        self.assertIn("Building", setup["variableTypes"])
        self.assertTrue({"Building", "Others"}.issubset(setup["assetTypes"]))

    def test_findings_are_filed_under_their_asset_type(self):
        from test_media_reports import photo_data_url
        _, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(), "name": "evidence.png"}})
        image = json.loads(body)["image"]
        with app.connect() as db:
            departments = [row[0] for row in db.execute("SELECT code FROM departments ORDER BY code")]
        # Categories are retired.
        self.assertEqual(self.request("/api/setup/categories", "POST", {"name": "Routed category"})[0], 404)
        base = {"section": "Routed sink", "notes": "Leaking", "priority": "High", "pic": "Routing Owner", "category": "Plumbing", "images": [image]}
        items = [base | {"item": "Default department"}, base | {"item": "Chosen", "assignedDepartment": departments[-1]}]
        status, _, body = self.request("/api/inspection-sessions", "POST", {"outlet": "STP", "items": items, "complete": True})
        self.assertEqual(status, 200, body)
        with app.connect() as db:
            routed = dict(db.execute("SELECT criterion, assigned_department FROM findings WHERE category = 'Plumbing' AND item_name = 'Routed sink'").fetchall())
        self.assertEqual(routed, {"Default department": departments[0], "Chosen": departments[-1]})

    def test_organization_theme_is_validated_saved_and_public(self):
        theme = lambda: json.loads(self.request("/api/branding", token=None)[2])["theme"]
        self.assertEqual(theme()["preset"], "ottotree", "an Ottotree organization starts on its own theme")
        self.assertEqual(theme()["font"], "noto-sans-sc")
        for key, value in (("theme.preset", "neon"), ("theme.font", "comic"), ("theme.accent", "teal"), ("theme.mode", "dim"),
                           ("theme.userChoice", "yes"), ("theme.background", "#000000")):
            self.assertEqual(self.request("/api/settings", "POST", {"settings": {key: value}})[0], 400, key)
        saved = {"theme.preset": "plum", "theme.accent": "#123abc", "theme.corners": "square", "theme.density": "compact", "theme.mode": "dark", "theme.userChoice": False}
        self.assertEqual(self.request("/api/settings", "POST", {"settings": saved})[0], 200)
        try:
            current = theme()
            self.assertEqual({f"theme.{key}": value for key, value in current.items() if f"theme.{key}" in saved}, saved)
            with app.connect() as db:
                admin_id = db.execute("SELECT id FROM users WHERE role = 'Admin'").fetchone()[0]
            app.SESSION_TOKENS["theme-admin"] = {"user_id": admin_id, "expires_at": time.time() + 3600}
            self.assertEqual(self.request("/api/settings", "POST", {"settings": {"theme.preset": "ember"}}, token="theme-admin")[0], 200)
            self.assertEqual(theme()["preset"], "plum", "only the Super account changes the theme")
            status, headers, _ = self.request("/fonts/noto-sans-sc-latin-400.woff2", token=None)
            self.assertEqual((status, headers["Content-Type"]), (200, "font/woff2"))
            self.assertIn("immutable", headers["Cache-Control"])
            self.assertEqual(self.request("/fonts/OFL-NotoSansSC.txt", token=None)[0], 404)
            # The logo is public so the sign-in page can show it; other photos are not.
            from test_media_reports import photo_data_url
            image = lambda colour: json.loads(self.request("/api/media", "POST", {"image": {"dataUrl": photo_data_url(colour), "name": "x.png"}})[2])["image"]["url"]
            logo, evidence = image("navy"), image("orange")
            self.assertEqual(self.request("/api/settings", "POST", {"settings": {"report.logoUrl": logo}})[0], 200)
            self.assertEqual(self.request(logo, token=None)[0], 200)
            self.assertEqual(json.loads(self.request("/api/branding", token=None)[2])["logoUrl"], logo)
            self.assertEqual(self.request(evidence, token=None)[0], 401)
        finally:
            restore = {"report.logoUrl": "", "theme.preset": "ottotree", "theme.accent": "", "theme.corners": "rounded", "theme.density": "comfortable", "theme.mode": "system", "theme.userChoice": True}
            self.assertEqual(self.request("/api/settings", "POST", {"settings": restore})[0], 200)

    def test_each_instance_port_gets_its_own_session_cookie(self):
        import os
        name = lambda **env: mock.patch.dict(os.environ, env, clear=False)
        for env, expected in (({"PORT": "41883"}, "ottotree_session"), ({"PORT": "41991"}, "ottotree_session_41991"),
                              ({"PORT": "41991", "AUDIT_SESSION_COOKIE": "company_b"}, "company_b")):
            with name(**env):
                os.environ.pop("AUDIT_SESSION_COOKIE", None) if "AUDIT_SESSION_COOKIE" not in env else None
                self.assertEqual(config._session_cookie_name(), expected)
        with name(AUDIT_SESSION_COOKIE="bad name;"), self.assertRaises(SystemExit):
            config._session_cookie_name()
        # The server reads and writes the configured name only.
        with mock.patch.object(config, "SESSION_COOKIE", "company_b"):
            status, headers, _ = self.request("/api/auth/login", "POST", {"identifier": "jacky", "password": "123456"}, token=None)
            self.assertEqual(status, 200)
            self.assertTrue(headers["Set-Cookie"].startswith("company_b="))
            token = headers["Set-Cookie"].split(";", 1)[0].split("=", 1)[1]
            self.assertEqual(self.request("/api/auth/me", token=None, headers={"Cookie": f"company_b={token}"})[0], 200)
            self.assertEqual(self.request("/api/auth/me", token=token)[0], 401, "another instance's cookie is ignored")

    def test_super_accounts_live_in_their_own_database(self):
        from backend.control import connect_control, control_path
        from backend.database_manager import list_databases
        self.assertNotIn(control_path().stem, {row["name"] for row in list_databases()}, "never listed as an organization")
        # An older organization database that still holds a Super account gives it up on upgrade.
        with app.connect() as db:
            legacy = db.execute("INSERT INTO users(name, username, role, email, password_hash, active, reset_required, created_at) VALUES "
                                "('Legacy Super', 'legacy-super', 'Super', 'legacy-super@example.test', ?, 1, 0, 0)", (app.hash_password("legacy-password"),)).lastrowid
            db.execute("INSERT INTO user_login_activity(user_id, email, logged_at) VALUES (?, 'legacy-super@example.test', '2026-09-01 08:00:00')", (legacy,))
            owned = db.execute("INSERT INTO inspection_sessions(business_unit,outlet,zone,audit_date,auditor,progress,status,created_at,updated_at,owner_user_id) "
                               "VALUES ('Ottotree','STP','Room','2026-09-01','Legacy Super',0,'Draft',0,0,?)", (legacy,)).lastrowid
            db.execute("INSERT OR IGNORE INTO roles(name, description, protected, created_at) VALUES ('Super', 'old', 1, 0)")
        app.init_db()
        with app.connect() as db:
            self.assertIsNone(db.execute("SELECT 1 FROM users WHERE id = ? OR role = 'Super'", (legacy,)).fetchone())
            self.assertIsNone(db.execute("SELECT 1 FROM roles WHERE name = 'Super'").fetchone())
            self.assertIsNone(db.execute("SELECT 1 FROM user_login_activity WHERE user_id = ?", (legacy,)).fetchone())
            owner = db.execute("SELECT owner_user_id FROM inspection_sessions WHERE id = ?", (owned,)).fetchone()[0]
        with connect_control() as db:
            moved = db.execute("SELECT * FROM super_users WHERE username = 'legacy-super'").fetchone()
            self.assertEqual(db.execute("SELECT count(*) FROM super_login_activity WHERE user_id = ?", (moved["id"],)).fetchone()[0], 1)
        self.assertEqual(owner, -moved["id"], "the draft stays with its owner")
        # The moved account signs in with its existing password and works in the organization.
        status, headers, body = self.request("/api/auth/login", "POST", {"identifier": "legacy-super", "password": "legacy-password"}, token=None)
        self.assertEqual(status, 200, body)
        token = headers["Set-Cookie"].split(";", 1)[0].split("=", 1)[1]
        user = json.loads(body)["user"]
        self.assertEqual((user["role"], user["accountScope"], user["id"]), ("Super", "control", -moved["id"]))
        self.assertEqual(self.request("/api/users", token=token)[0], 200)
        self.assertNotIn("legacy-super", {row["username"] for row in json.loads(self.request("/api/users", token=token)[2])["items"]})
        # Its own details, password, and page order are kept in the control database.
        self.assertEqual(self.request("/api/account", "PATCH", {"name": "Renamed Super"}, token=token)[0], 200)
        self.assertEqual(self.request("/api/account", "PATCH", {"department": "SSD", "profilePhoto": {"url": "/api/media/x.png"}}, token=token)[0], 400)
        self.assertEqual(self.request("/api/account/navigation", "PATCH", {"order": ["super-settings", "today"]}, token=token)[0], 200)
        change = {"oldPassword": "legacy-password", "newPassword": "a-new-super-password"}
        self.assertEqual(self.request("/api/auth/change-password", "POST", change, token=token)[0], 200)
        with connect_control() as db:
            row = db.execute("SELECT * FROM super_users WHERE id = ?", (moved["id"],)).fetchone()
            pages = [item[0] for item in db.execute("SELECT page_id FROM super_navigation WHERE user_id = ? ORDER BY position", (moved["id"],))]
        self.assertEqual(row["name"], "Renamed Super")
        self.assertTrue(app.verify_password("a-new-super-password", row["password_hash"]))
        self.assertEqual(pages, ["super-settings", "today"])
        with app.connect() as db:
            self.assertIsNone(db.execute("SELECT 1 FROM users WHERE name = 'Renamed Super'").fetchone())
        self.assertEqual(self.request("/api/auth/logout", "POST", {}, token=token)[0], 200)
        self.assertEqual(self.request("/api/auth/me", token=token)[0], 401)

    def test_uploads_are_stored_small_and_need_a_session(self):
        from io import BytesIO
        import base64
        from PIL import Image
        picture = BytesIO()
        Image.effect_noise((1600, 900), 60).convert("RGB").save(picture, "PNG")
        data_url = "data:image/png;base64," + base64.b64encode(picture.getvalue()).decode()
        _, _, body = self.request("/api/media", "POST", {"image": {"dataUrl": data_url, "name": "wide.png"}})
        url = json.loads(body)["image"]["url"]
        # Every upload is stored as a small WebP (a few kilobytes), which lists show as it is.
        status, headers, full = self.request(url)
        self.assertEqual((status, headers["Content-Type"]), (200, "image/webp"))
        self.assertLessEqual(len(full), 8 * 1024)
        self.assertLess(len(full), len(picture.getvalue()) // 50)
        status, headers, thumb = self.request(url + "?thumb=1")
        self.assertEqual((status, headers["Content-Type"], thumb), (200, "image/webp", full))
        with Image.open(BytesIO(thumb)) as small:
            # The noisiest picture is stepped down in size until it fits, keeping its shape.
            self.assertLessEqual(max(small.size), 1280)
            self.assertAlmostEqual(small.size[0] / small.size[1], 1600 / 900, delta=0.02)
        self.assertEqual(self.request(url + "?thumb=1")[2], thumb)
        self.assertEqual(self.request(url + "?thumb=1", token=None)[0], 401)
        self.assertEqual(self.request("/api/media/" + "0" * 64 + ".png?thumb=1")[0], 404)

    def test_location_item_list_is_its_full_membership(self):
        ids = [json.loads(self.request("/api/equipment", "POST", {"name": f"Member {n}", "outlet": "STP"})[2])["id"] for n in range(3)]
        self.assertEqual(self.request("/api/locations", "POST", {"outlet": "STP", "name": "Membership room", "equipmentIds": ids})[0], 200)
        location = next(row for row in json.loads(self.request("/api/locations?outlet=STP")[2])["items"] if row["name"] == "Membership room")
        placed = lambda: {row["id"]: row["location"] for row in json.loads(self.request("/api/equipment?outlet=STP")[2])["items"] if row["id"] in ids}
        self.assertEqual(set(placed().values()), {"Membership room"})
        path = f"/api/locations/{location['id']}"
        # Leaving an item out of the list removes it from the location.
        self.assertEqual(self.request(path, "PATCH", {"name": "Membership room", "equipmentIds": ids[:2]})[0], 200)
        self.assertEqual(placed(), {ids[0]: "Membership room", ids[1]: "Membership room", ids[2]: ""})
        # An edit that does not mention the items leaves them where they are.
        self.assertEqual(self.request(path, "PATCH", {"name": "Membership room", "floor": "Level 2"})[0], 200)
        self.assertEqual(placed()[ids[0]], "Membership room")

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
            # Organization sessions in the selected database are ended; the Super account, kept in its
            # own database, stays signed in and now works in the selected organization.
            with app.connect() as db:
                self.assertEqual(db.execute("SELECT count(*) FROM auth_sessions").fetchone()[0], 0)
                self.assertIsNone(db.execute("SELECT 1 FROM users WHERE role = 'Super'").fetchone())
            status, _, body = self.request("/api/auth/me")
            self.assertEqual((status, json.loads(body)["user"]["accountScope"]), (200, "control"))
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
