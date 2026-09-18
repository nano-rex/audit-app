"""Dashboard counts and charts use actual, consistently scoped records."""
import tempfile
import unittest

from test_server import app
from database import insert_record
from relational_values import save_value
import config


class DashboardTests(unittest.TestCase):
    def setUp(self):
        self.previous_directory = config.DATA_DIR
        self.storage = tempfile.TemporaryDirectory(prefix="audit-dashboard-")
        app.configure_data_directory(self.storage.name)
        app.init_db()
        with app.connect() as db:
            db.execute("DELETE FROM schedules")

    def tearDown(self):
        app.configure_data_directory(self.previous_directory)
        self.storage.cleanup()

    def test_empty_dashboard_does_not_invent_scores(self):
        data = app.dashboard("Mini Studio")
        for key in ("total", "auditsCompleted", "auditsPending", "priorityIssues", "nonPriorityIssues", "outstandingFindings", "completedCorrectiveActions"):
            self.assertEqual(data["stats"][key], 0, key)
        self.assertIsNone(data["stats"]["overallAuditScore"])
        self.assertEqual(data["charts"]["auditScores"], [])
        self.assertEqual(data["charts"]["monthlyAuditTrend"], [])

    def test_counts_classification_scoping_and_completed_scores(self):
        with app.connect() as db:
            common = {"business_unit": "Mini Studio", "outlet": "A", "auditor": "Tester", "created_at": 0}
            audit_ids = []
            for score, date, completed in ((80, "2026-01-01", True), (40, "2026-02-01", True), (100, "2026-02-02", False)):
                audit_id = insert_record(db, "audits", common | {"branch": "Area", "audit_date": date, "audit_type": "Routine", "score": score})
                audit_ids.append(audit_id)
                if completed:
                    insert_record(db, "inspection_sessions", common | {"zone": "Area", "audit_date": date, "items_data_id": save_value(db, []), "progress": 100, "status": "Completed", "audit_id": audit_id, "updated_at": 0})
            for linked in (True, False):
                schedule_id = insert_record(db, "schedules", common | {"zone": "Area", "scheduled_date": "2026-03-01", "status": "Pending"})
                if linked:
                    insert_record(db, "inspection_sessions", common | {"zone": "Area", "audit_date": "2026-03-01", "items_data_id": save_value(db, []), "progress": 0, "status": "Draft", "schedule_id": schedule_id, "updated_at": 0})
            insert_record(db, "inspection_sessions", common | {"zone": "Area", "audit_date": "2026-03-01", "items_data_id": save_value(db, []), "progress": 0, "status": "Draft", "updated_at": 0})
            insert_record(db, "priority_levels", {"name": "Urgent Custom", "classification": "Priority", "due_days": 1, "active": 1, "created_at": 0})
            for index, (priority, classification, status) in enumerate((("Urgent Custom", None, "Assigned"), ("Urgent Custom", "Non-Priority", "Completed"), ("High", "Priority", "In Progress"))):
                insert_record(db, "findings", {"audit_id": audit_ids[0], "business_unit": "Mini Studio", "outlet": "A", "location": "Area", "category": "Safety", "assigned_department": "TECH", "priority": priority, "priority_classification": classification, "status": status, "created_at": index, "updated_at": 0})
            for status in ("Assigned", "Completed", "Verified", "Closed"):
                insert_record(db, "work_orders", {"business_unit": "Mini Studio", "outlet": "A", "zone": "Area", "request_type": "TECH", "priority": "High", "title": "Fix", "assignee": "Tester", "status": status, "created_at": 0})
            # Other business-unit records must not affect any metric or grouping.
            insert_record(db, "findings", {"audit_id": audit_ids[0], "business_unit": "Loudspeaker", "outlet": "Other", "location": "Other", "priority": "High", "status": "Assigned", "created_at": 0, "updated_at": 0})
            insert_record(db, "inspection_sessions", common | {"business_unit": "Loudspeaker", "zone": "Area", "audit_date": "2026-03-01", "items_data_id": save_value(db, []), "progress": 0, "status": "Draft", "updated_at": 0})
        data = app.dashboard("Mini Studio")
        expected = {"total": 5, "auditsCompleted": 2, "auditsPending": 3, "priorityIssues": 2, "nonPriorityIssues": 1, "outstandingFindings": 2, "completedCorrectiveActions": 3, "overallAuditScore": 60}
        for key, value in expected.items():
            self.assertEqual(data["stats"][key], value, key)
        self.assertEqual(data["charts"]["auditScores"], [{"label": "A", "score": 60}])
        self.assertEqual(data["outlets"][0]["latest"], 40)
        for key, label in (("findingsByDepartment", "TECH"), ("findingsByArea", "Area"), ("findingsByCategory", "Safety")):
            self.assertEqual(data["charts"][key], [{"label": label, "count": 3}])
        self.assertEqual(data["charts"]["monthlyAuditTrend"], [{"month": "2026-01", "audits": 1, "average_score": 80}, {"month": "2026-02", "audits": 1, "average_score": 40}])
