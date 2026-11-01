function departmentRow(row) {
  return `
    <article class="department-card">
      <div class="department-card-heading"><span class="department-code">${escapeHtml(row.code)}</span><span class="department-id">Department</span></div>
      <div class="department-card-copy"><strong>${escapeHtml(row.description || "No description")}</strong><p>${escapeHtml(row.responsibilities || "No responsibilities")}</p></div>
      <div class="row-actions">
        <button type="button" class="outline" data-edit-department='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-department="${row.id}">Delete</button>
      </div>
    </article>
  `;
}

function outletRow(row) {
  return `
    <li>
      <b>${escapeHtml(row.code)}</b>
      <span>${escapeHtml(row.location || "No location")} | ${escapeHtml(row.description || "No description")}</span>
      <span class="row-actions">
        <button type="button" class="outline" data-edit-outlet='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-outlet="${row.id}">Delete</button>
      </span>
    </li>
  `;
}

function roleRow(row) {
  return `
    <li>
      <b>${escapeHtml(row.name)}${row.protected ? " - Protected" : ""}</b>
      <span>${escapeHtml(row.description || "No description")}</span>
      <span>Access: ${escapeHtml((row.permissions || []).length ? row.permissions.join(", ") : "none selected")}</span>
      <span>${escapeHtml(row.department || "No department")} | Reports to ${escapeHtml(roleCache.find((role) => role.id === row.reports_to_id)?.name || "nobody")}</span>
      <span class="row-actions">
        <button type="button" class="outline" data-edit-role='${escapeAttr(JSON.stringify(row))}' ${row.protected ? "disabled" : ""}>Edit</button>
        <button type="button" class="danger" data-delete-role="${row.id}" ${row.protected ? "disabled" : ""}>Delete</button>
      </span>
    </li>
  `;
}

function priorityRow(row) {
  return `
    <article>
      <div>
        <b>${escapeHtml(row.name)}</b>
        <span>${escapeHtml(row.classification)} | Due in ${escapeHtml(row.due_days)} day(s) | ${row.active ? "Active" : "Inactive"}</span>
      </div>
      <span class="row-actions">
        <button type="button" class="outline" data-edit-priority='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-priority="${row.id}">Delete</button>
      </span>
    </article>
  `;
}

function auditTypeRow(row) {
  return `
    <article>
      <div>
        <b>${escapeHtml(row.name)}</b>
        <span>${escapeHtml(row.description || "No description")} | ${escapeHtml(row.style || "Detailed")} grading | ${row.active ? "Active" : "Inactive"}</span>
      </div>
      <span class="row-actions">
        <button type="button" class="outline" data-edit-audit-type='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-audit-type="${row.id}">Delete</button>
      </span>
    </article>
  `;
}

// The page a notification is about, when the signed-in user can open it.
function notificationTarget(row) {
  const permissions = currentUser?.permissions || [];
  const id = Number(row.related_id);
  if (!id) return null;
  if (row.related_type === "inspection" && permissions.includes("inspections")) return { type: "inspection", id };
  if (row.related_type === "schedule" && permissions.includes("inspections")) return { type: "schedule", id };
  if (row.related_type === "change") return { type: "change", id };
  if (row.related_type === "work_order" && permissions.includes("work-orders")) return { type: "work_order", id };
  if (row.related_type === "user" && permissions.includes("users")) return { type: "user", id };
  return null;
}

function notificationRow(row) {
  const created = row.created_at ? new Date(row.created_at).toLocaleString() : "No date";
  return `
    <article>
      <div>
        <b>${escapeHtml(row.title)}</b>
        <span>${escapeHtml(row.message || "No message")}</span>
        <span>${escapeHtml(row.channel || "In-App")} | ${escapeHtml(created)} | ${escapeHtml(row.related_type || "General")}</span>
      </div>
      <span class="row-actions">
        <span class="status-pill ${row.status === "Unread" ? "status-progress" : "status-none"}">${escapeHtml(row.status || "Unread")}</span>
        ${notificationTarget(row) ? `<button type="button" class="primary" data-open-notification="${row.id}">Open</button>` : ""}
        <button type="button" class="outline" data-read-notification="${row.id}">Read</button>
        <button type="button" class="danger" data-delete-notification="${row.id}">Delete</button>
      </span>
    </article>
  `;
}

function userRow(row) {
  const status = row.active === 0 || row.active === false ? "Inactive" : "Active";
  const login = row.last_login_at || row.lastLoginAt || "Never logged in";
  return `
    <tr>
      <td class="user-name"><strong>${escapeHtml(row.name)}</strong><small>${escapeHtml(row.username || "No username")}</small></td>
      <td class="user-email">${escapeHtml(row.email)}</td>
      <td class="user-department">${escapeHtml(row.department || "No department")}</td>
      <td class="user-role">${escapeHtml(row.role || "No role")}${(row.outlets || []).length ? `<small>${escapeHtml(row.outlets.join(", "))}</small>` : ""}</td>
      <td><span class="status-pill ${status === "Active" ? "status-complete" : "status-untouched"}">${status}</span>${row.reset_requested || row.reset_required || row.resetRequired ? `<small class="user-alert">Password action needed</small>` : ""}</td>
      <td class="user-login">${escapeHtml(login)}</td>
      <td class="row-actions">
        <button type="button" class="outline" data-edit-user='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="outline" data-login-activity="${row.id}" aria-label="Login activity for ${escapeAttr(row.name)}">Activity</button>
        <button type="button" class="danger" data-delete-user="${row.id}">Delete</button>
      </td>
    </tr>
  `;
}

function locationRow(row) {
  return `
    <li>
      <b>${escapeHtml(row.name)}</b>
      <span>${escapeHtml(row.floor || "No floor")} | ${escapeHtml(row.area || "No area")} | Order ${escapeHtml(row.display_order || 0)}</span>
      <span>QR: ${escapeHtml(row.qr_code || "Not assigned")} | ${escapeHtml(row.size || "No size")}</span>
      <span>${escapeHtml(row.equipment || "No fixed assets assigned")}</span>
      <span class="row-actions">
        <button type="button" class="outline" data-edit-location='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-location="${row.id}">Delete</button>
      </span>
    </li>
  `;
}

function zoneRow(row) {
  const locations = row.locations || [];
  return `
    <li>
      <b>${escapeHtml(row.name)}</b>
      <span>${escapeHtml(row.description || "No description")}</span>
      <span class="zone-locations" title="${escapeAttr(locations.join(", "))}">${locations.length} location${locations.length === 1 ? "" : "s"}${locations.length ? `: ${escapeHtml(locations.join(", "))}` : ""}</span>
      <span class="row-actions">
        <button type="button" class="outline" data-edit-zone='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-zone="${row.id}">Delete</button>
      </span>
    </li>
  `;
}

function equipmentRow(row) {
  const status = row.operational_status || row.health_status || "Operational";
  const statusClass = status === "Replace" || status === "Out of Service" ? "warn" : status === "Needs Attention" || status === "Monitor" ? "monitor" : "";
  const fixture = row.kind === "fixture";
  const name = row.name || row.asset_id || row.code || "Fixed Asset";
  const code = row.code || row.asset_id || "";
  const location = row.location || row.zone || "No location";
  // Items sharing this name and asset type, edited together with Bulk Edit.
  const group = typeof equipmentGroup === "function" ? equipmentGroup(row).length : 0;
  const detail = `${row.brand || "No brand"} ${row.model || ""}`.trim() + (row.serial_number ? ` | S/N ${row.serial_number}` : "");
  return `
    <article>
      <div>
        <b>${escapeHtml(name)} <span class="kind-pill">${fixture ? "Variable asset" : "Fixed asset"}</span></b>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(location)} | ${escapeHtml(row.type || row.equipment_type || "No asset type")}${fixture ? "" : ` | Code: ${escapeHtml(code)}`}</span>
        <span>${escapeHtml(detail)}</span>
      </div>
      <span class="row-actions">
        <strong class="${statusClass}">${escapeHtml(status)}<small>${escapeHtml(row.installation_date || row.last_checked || "No date")}</small></strong>
        ${photoSetButton(parseStoredImages(row.photos || "[]"), "Photos")}
        <button type="button" class="outline" data-edit-equipment='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        ${group > 1 ? `<button type="button" class="outline" data-bulk-edit-equipment="${row.id}" title="Edit every ${escapeAttr(name)} of type ${escapeAttr(row.type || row.equipment_type || "unset")}">Edit all ${group}</button>` : ""}
        <button type="button" class="danger" data-delete-equipment="${row.id}">Delete</button>
      </span>
    </article>
  `;
}

// Evidence recorded with the finding.
function workOrderPhotos(row) {
  return parseStoredImages(row.images_json || "[]").map((image) => ({ ...image, caption: "Finding evidence" }));
}

function workOrderRow(row) {
  const closed = row.status === "Closed";
  const reference = row.work_order_ref || `#${row.id}`;
  const pic = row.pic || row.assignee || "No PIC";
  const closedOn = row.closed_at ? ` | Closed ${row.closed_at}` : "";
  const sla = row.sla_status || "No SLA";
  const due = row.due_date ? `Due ${row.due_date}` : "No due date";
  return `
    <article>
      <div>
        <b>${escapeHtml(reference)} ${escapeHtml(row.title)}</b>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(row.zone)} | ${escapeHtml(row.request_type)} | ${escapeHtml(row.category || "No asset type")} | ${escapeHtml(row.assignee)}</span>
        <span>${escapeHtml(pic)} | ${escapeHtml(due)} | ${escapeHtml(closed ? closedOn.replace(" | ", "") : sla)}</span>
      </div>
      <span class="row-actions">
        <strong class="${row.priority === "High" || sla === "Overdue" ? "warn" : ""}">${escapeHtml(row.priority)}<small>${escapeHtml(row.status)}</small></strong>
        ${photoSetButton(workOrderPhotos(row), "Photos")}
        <button type="button" class="outline" data-edit-work-order='${escapeAttr(JSON.stringify(row))}'>${closed ? "View" : "Edit"}</button>
        ${closed ? "" : `<button type="button" class="danger" data-delete-work-order="${row.id}">Delete</button>`}
      </span>
    </article>
  `;
}

// One inspected item with the checks it failed, and the work request raised for it.
function findingItemRow(group) {
  const request = group.findings.find((row) => row.request_ref);
  // Findings recorded before work requests existed already have their own work order.
  const order = group.findings.find((row) => row.order_ref);
  const untouched = group.findings.every((row) => row.status === "Open");
  const kind = group.item_kind === "fixture" ? "Variable asset" : "Fixed asset";
  const status = request ? `${request.request_ref} · ${request.request_status || ""}` : order ? order.order_ref : untouched ? "No work request yet" : "";
  return `
    <article class="finding-item">
      <div>
        <b>${escapeHtml(group.item_name || "Item")} <small class="muted">${escapeHtml(kind)}</small></b>
        <span>${escapeHtml(group.audit_ref || `Audit ${group.audit_id}`)} | ${escapeHtml(group.outlet)} | ${escapeHtml(group.location)} | ${escapeHtml(group.department || "No department")} | ${escapeHtml(group.priority || "")}</span>
        <ul class="finding-checks">${group.findings.map((row) => `<li><span class="status-pill ${row.status === "Closed" ? "status-complete" : "status-untouched"}">${escapeHtml(row.status)}</span>
          <span class="finding-check-text">${escapeHtml(row.criterion || "Check")}${row.comment && row.comment !== row.criterion ? ` — ${escapeHtml(row.comment)}` : ""}
            <small class="muted">${escapeHtml([row.priority, row.category, row.assigned_department, row.pic && `PIC ${row.pic}`, row.due_date && `Due ${row.due_date}`].filter(Boolean).join(" · "))}</small></span>
          ${typeof canEditFindings === "function" && canEditFindings() && row.status !== "Closed" ? `<span class="finding-actions">
            <button type="button" class="outline" data-edit-finding="${row.id}">Edit</button>
            ${findingIsSettled(row) ? "" : `<button type="button" class="danger" data-delete-finding="${row.id}">Delete</button>`}</span>` : ""}</li>`).join("")}</ul>
      </div>
      <span class="row-actions">
        <span class="muted">${escapeHtml(status)}</span>
        ${photoSetButton(group.images, "Photos")}
        ${!request && !order && untouched && canRequestWork() ? `<button type="button" class="primary" data-request-work="${escapeAttr(group.key)}">Create work request</button>` : ""}
      </span>
    </article>
  `;
}

// Scheduled Work and History name an audit the same way: its audit number and inspection name
// first, then the schedule it came from, so one audit reads identically in both lists.
// An audit is named by its code (AUDIT-STP-20261006-000004).
function auditTitle(session, fallbackName) {
  if (!session?.audit_ref) return fallbackName ? `Not started · ${fallbackName}` : "Not started";
  return session.audit_ref;
}

function lastSaved(row) {
  const time = row?.updated_at || row?.created_at;
  return time ? `Saved ${new Date(time).toLocaleString()}` : "Not saved yet";
}

function scheduleLabel(scheduleId) {
  return scheduleId ? `Schedule SCH-${String(scheduleId).padStart(5, "0")}` : "No schedule";
}

function scheduleRow(row) {
  const matchingSession = inspectionHistoryCache.find((session) =>
    session.schedule_id === row.id
  );
  const session = matchingSession || (row.audit_ref ? { ...row, audit_date: row.scheduled_date, id: row.inspection_id } : null);
  // Once started, the audit's own location, auditor and save time are what History shows too.
  const shown = matchingSession || row;
  const status = row.inspection_id || matchingSession
    ? inspectionHistoryProgressStatus(matchingSession || { status: row.inspection_status, progress: row.progress })
    : { className: "status-untouched", label: "Not Started (0%)" };
  // A visit's priority, and its due date once it has passed without the audit being completed.
  const done = (matchingSession?.status || row.inspection_status) === "Completed";
  const overdue = row.due_date && !done && row.due_date < todayIsoDate();
  const urgent = typeof priorityCache !== "undefined" && priorityCache.some((level) => level.name === row.priority && level.classification === "Priority");
  return `
    <article data-schedule-id="${row.id}">
      <div data-open-schedule='${escapeAttr(JSON.stringify(row))}'>
        <b>${escapeHtml(auditTitle(session, `${row.outlet} · ${row.scheduled_date}`))}</b>
        <span>${escapeHtml(row.scheduled_date)}${row.due_date ? ` | <span class="${overdue ? "warn" : ""}">Due ${escapeHtml(row.due_date)}${overdue ? " (overdue)" : ""}</span>` : ""} | ${escapeHtml(matchingSession ? lastSaved(matchingSession) : "Not started")}</span>
        <span>${escapeHtml(shown.outlet)} | ${escapeHtml(shown.zone || "No location")}${row.audit_type ? ` | ${escapeHtml(row.audit_type)}${typeof auditStyleOf === "function" && auditStyleOf(row.audit_type) === "Casual" && row.audit_type !== "Casual" ? " (casual)" : ""}` : ""} | ${escapeHtml(matchingSession ? `Audited by ${shown.auditor || "nobody"}` : `Assigned to ${row.auditor && row.auditor !== "Unassigned" ? row.auditor : "nobody yet"}`)} | ${escapeHtml(scheduleLabel(row.id))}</span>
      </div>
      <span class="row-actions">
        ${row.priority ? `<span class="status-pill ${urgent ? "status-untouched" : "priority-pill"}">${escapeHtml(row.priority)}</span>` : ""}
        <span class="status-pill ${status.className}">${escapeHtml(status.label)}</span>
        <button type="button" class="primary" data-open-schedule='${escapeAttr(JSON.stringify(row))}'>Open</button>
        <button type="button" class="outline" data-edit-schedule='${escapeAttr(JSON.stringify(row))}'>Edit</button>
      </span>
    </article>
  `;
}

function rankingRow(row, rank) {
  return `
    <article>
      <em>${rank}</em>
      <div><b>${escapeHtml(row.outlet)}</b><span>${escapeHtml(row.branch)} - ${escapeHtml(row.audit_date)}</span></div>
      <strong class="${row.latest >= 90 ? "excellent" : ""}">${row.latest}<small>${row.latest >= 90 ? "Excellent" : "Good"}</small></strong>
    </article>
  `;
}
