"""Close completed audits after signatures and corrective actions are finalized."""
import time

from backend.database import connect
from backend.relational_values import load_value
from backend.workflow import WorkflowError


def close_audit(handler, parsed, payload=None):
    user = handler.current_user()
    if "verifier" not in user.get("inspectionPermissions", []):
        raise PermissionError("Verifier permission is required to close an audit")
    session_id = int((payload or {}).get("id") or 0)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        session = db.execute("SELECT * FROM inspection_sessions WHERE id = ?", (session_id,)).fetchone()
        if not session:
            raise WorkflowError("Inspection not found", 404)
        if session["closed_at"]:
            handler.json({"ok": True, "id": session_id, "closedAt": session["closed_at"]})
            return
        if session["status"] != "Completed" or not session["audit_id"]:
            raise WorkflowError("Complete the inspection before closing the audit")
        signatures = load_value(session["signatures_data_id"]) or {}
        if any(not signatures.get(key, {}).get("url") for key in ("auditedBy", "verifiedBy", "acknowledgedBy")):
            raise WorkflowError("Auditor, verifier, and acknowledger signatures are required before closure")
        pending = db.execute("SELECT 1 FROM findings WHERE audit_id = ? AND status != 'Closed' LIMIT 1", (session["audit_id"],)).fetchone()
        pending_order = db.execute("SELECT 1 FROM work_orders WHERE source_audit_id = ? AND status != 'Closed' LIMIT 1", (session["audit_id"],)).fetchone()
        if pending or pending_order:
            raise WorkflowError("Close all linked findings and corrective actions before closing the audit")
        now = int(time.time() * 1000)
        db.execute("UPDATE inspection_sessions SET closed_at = ?, closed_by = ?, updated_at = ? WHERE id = ?", (now, user["name"], now, session_id))
        db.execute("INSERT INTO comments(record_type,record_id,comment,author,created_at,system_generated) VALUES ('inspection',?,'Audit closed',?,?,1)", (session_id, user["name"], now))
    handler.json({"ok": True, "id": session_id, "closedAt": now})
