"""Work requests: what should be fixed, raised from an audit's findings or reported directly.

A request is reviewed on Maintenance > Work Requests. Approving it creates a work order;
declining it closes its findings with the reason. When the work order closes, the request
and its findings close with it.
"""
import time

from backend import activity
from backend.common import record_year, today_date
from backend.database import connect, first_department, first_outlet, insert_record
from backend.relational_values import hydrate_many, load_value, save_value
from backend.workflow import WorkflowError

STATUSES = ("Open", "Ordered", "Declined", "Closed")


def request_ref(record_id, created=None):
    return f"WR-{record_year(created)}-{int(record_id):05d}"


def reviews_requests(user):
    """Turning a request into work, or declining it, assigns work; a Department/PIC account does not."""
    return user.get("role") != "Department/PIC" and (
        user.get("role") == "Super" or "work-orders" in user.get("permissions", []))


def work_request_items(user=None):
    where, params = "", ()
    if user and user.get("role") == "Department/PIC":
        where = "WHERE LOWER(TRIM(COALESCE(work_requests.department, ''))) = ?"
        params = (str(user.get("department") or "").strip().casefold(),)
    with connect() as db:
        rows = db.execute(
            f"""
            SELECT work_requests.*, work_orders.work_order_ref, work_orders.status AS work_order_status,
                   (SELECT COUNT(*) FROM findings WHERE findings.work_request_id = work_requests.id) AS findings_count,
                   (SELECT GROUP_CONCAT(finding_ref, ', ') FROM findings WHERE findings.work_request_id = work_requests.id) AS finding_refs
            FROM work_requests
            LEFT JOIN work_orders ON work_orders.id = work_requests.work_order_id
            {where}
            ORDER BY CASE work_requests.status WHEN 'Open' THEN 0 WHEN 'Ordered' THEN 1 ELSE 2 END,
                     work_requests.created_at DESC, work_requests.id DESC
            """, params).fetchall()
    return {"items": hydrate_many(rows)}


def create_work_request(user, payload):
    now = int(time.time() * 1000)
    finding_ids = [int(value) for value in payload.get("findingIds") or []]
    description = str(payload.get("description") or "").strip()
    if not description:
        raise WorkflowError("Describe the work that is needed", 400)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        if finding_ids:
            marks = ",".join("?" for _ in finding_ids)
            findings = [dict(row) for row in db.execute(f"SELECT * FROM findings WHERE id IN ({marks})", finding_ids)]
            if len(findings) != len(set(finding_ids)):
                raise WorkflowError("A finding in this request no longer exists", 404)
            if any(row["work_request_id"] or row["status"] == "Closed" for row in findings):
                raise WorkflowError("A work request has already been raised for this item")
            if len({(row["outlet"], row["location"]) for row in findings}) > 1:
                raise WorkflowError("A work request covers one item at one location", 400)
            first = findings[0]
            images, seen = [], set()
            for row in findings:
                for image in load_value(row["images_data_id"]) or []:
                    key = (image or {}).get("url") or id(image)
                    if key not in seen:
                        seen.add(key)
                        images.append(image)
            values = {
                "business_unit": first["business_unit"], "outlet": first["outlet"], "location": first["location"],
                "equipment_id": first.get("equipment_id"), "item_name": first.get("item_name") or payload.get("itemName") or "",
                "category": payload.get("category") or first["category"], "priority": payload.get("priority") or first["priority"],
                "department": payload.get("department") or first["assigned_department"], "audit_id": first["audit_id"],
                "audit_ref": first.get("audit_ref") or "", "images_data_id": save_value(db, payload.get("images") or images),
            }
        else:
            for key, label in (("outlet", "Outlet"), ("location", "Location"), ("itemName", "Item")):
                if not str(payload.get(key) or "").strip():
                    raise WorkflowError(f"{label} is required", 400)
            values = {
                "business_unit": payload.get("businessUnit", "Ottotree"), "outlet": payload.get("outlet") or first_outlet(db),
                "location": payload["location"], "equipment_id": payload.get("equipmentId"), "item_name": payload["itemName"].strip(),
                "category": payload.get("category") or "", "priority": payload.get("priority") or "Medium",
                "department": payload.get("department") or first_department(db), "audit_id": None, "audit_ref": "",
                "images_data_id": save_value(db, payload.get("images") or []),
            }
        request_id = insert_record(db, "work_requests", values | {
            "description": description, "status": "Open", "requested_by": user.get("name", ""),
            "requested_by_user_id": user.get("id"), "created_at": now, "updated_at": now,
        })
        db.execute("UPDATE work_requests SET request_ref = ? WHERE id = ?", (request_ref(request_id), request_id))
        if finding_ids:
            db.executemany("UPDATE findings SET work_request_id = ?, status = 'Requested', updated_at = ? WHERE id = ?",
                           [(request_id, now, finding_id) for finding_id in finding_ids])
        activity.log(db, user, "request_raised", "work_request", request_id, request_ref(request_id), values["outlet"],
                     business_unit=values["business_unit"], at=now)
    return {"ok": True, "id": request_id, "requestRef": request_ref(request_id)}


def edit_work_request(user, request_id, payload):
    """An open request can be corrected by whoever raised it or whoever reviews requests. One raised
    from findings keeps its item and place; one reported directly can change them too."""
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        row = db.execute("SELECT * FROM work_requests WHERE id = ?", (request_id,)).fetchone()
        if not row:
            raise WorkflowError("Work request not found", 404)
        if row["status"] != "Open":
            raise WorkflowError("Only an open request can be edited; edit its work order instead")
        if row["requested_by_user_id"] != user.get("id") and not reviews_requests(user):
            raise WorkflowError("Only whoever raised this request, or someone who assigns work orders, can edit it", 403)
        description = str(payload.get("description", row["description"]) or "").strip()
        if not description:
            raise WorkflowError("Describe the work that is needed", 400)
        values = {"description": description, "updated_at": now}
        for key, column in (("department", "department"), ("category", "category"), ("priority", "priority")):
            if payload.get(key):
                values[column] = str(payload[key])
        if "images" in payload:
            values["images_data_id"] = save_value(db, payload.get("images") or [])
        from_findings = db.execute("SELECT 1 FROM findings WHERE work_request_id = ?", (request_id,)).fetchone()
        if not from_findings:
            for key, column, label in (("outlet", "outlet", "Outlet"), ("location", "location", "Location"), ("itemName", "item_name", "Item")):
                if key in payload:
                    value = str(payload.get(key) or "").strip()
                    if not value:
                        raise WorkflowError(f"{label} is required", 400)
                    values[column] = value
            if "outlet" in values and not db.execute("SELECT 1 FROM outlets WHERE code = ?", (values["outlet"],)).fetchone():
                raise WorkflowError("Select an existing outlet", 400)
        db.execute(f"UPDATE work_requests SET {', '.join(f'{column} = ?' for column in values)} WHERE id = ?", (*values.values(), request_id))
        if from_findings and any(column in values for column in ("department", "category", "priority")):
            # The findings it covers follow the request.
            db.execute("UPDATE findings SET assigned_department = COALESCE(?, assigned_department), category = COALESCE(?, category), "
                       "priority = COALESCE(?, priority), updated_at = ? WHERE work_request_id = ?",
                       (values.get("department"), values.get("category"), values.get("priority"), now, request_id))
            level = db.execute("SELECT classification FROM priority_levels WHERE name = ?", (values.get("priority"),)).fetchone()
            if level:
                db.execute("UPDATE findings SET priority_classification = ? WHERE work_request_id = ?", (level[0], request_id))
    return {"ok": True}


def decline_work_request(user, request_id, remark):
    if not reviews_requests(user):
        raise WorkflowError("Only someone who assigns work orders can decline a request", 403)
    remark = str(remark or "").strip()
    if not remark:
        raise WorkflowError("Say why no work is needed", 400)
    now = int(time.time() * 1000)
    with connect() as db:
        db.execute("BEGIN IMMEDIATE")
        row = db.execute("SELECT status, request_ref, outlet, business_unit, created_at FROM work_requests WHERE id = ?", (request_id,)).fetchone()
        if not row:
            raise WorkflowError("Work request not found", 404)
        if row["status"] != "Open":
            raise WorkflowError("Only an open request can be declined")
        db.execute("UPDATE work_requests SET status = 'Declined', decline_remark = ?, reviewed_by = ?, updated_at = ? WHERE id = ?",
                   (remark, user.get("name", ""), now, request_id))
        # No work is needed, so the findings it covered are settled.
        db.execute("UPDATE findings SET status = 'Closed', closed_at = ?, updated_at = ? WHERE work_request_id = ?",
                   (today_date(), now, request_id))
        activity.log(db, user, "request_declined", "work_request", request_id, row["request_ref"], row["outlet"],
                     started_at=row["created_at"], detail=remark, business_unit=row["business_unit"], at=now)
    return {"ok": True}


def claim_for_work_order(db, user, request_id):
    """Check a request may become a work order; returns it for prefilling."""
    if not reviews_requests(user):
        raise WorkflowError("Only someone who assigns work orders can create one from a request", 403)
    row = db.execute("SELECT * FROM work_requests WHERE id = ?", (request_id,)).fetchone()
    if not row:
        raise WorkflowError("Work request not found", 404)
    if row["status"] != "Open":
        raise WorkflowError("A work order has already been created or the request was declined")
    return dict(row)


def link_work_order(db, request_id, order_id, status, pic):
    now = int(time.time() * 1000)
    db.execute("UPDATE work_orders SET work_request_id = ? WHERE id = ?", (request_id, order_id))
    db.execute("UPDATE work_requests SET status = 'Ordered', work_order_id = ?, updated_at = ? WHERE id = ?", (order_id, now, request_id))
    db.execute("UPDATE findings SET status = ?, pic = ?, updated_at = ? WHERE work_request_id = ?", (status, pic or "", now, request_id))
