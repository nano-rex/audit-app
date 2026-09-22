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

function categoryRow(row) {
  return `
    <li>
      <b>${escapeHtml(row.sequence || 0)}. ${escapeHtml(row.name)}</b>
      <span>${escapeHtml(row.description || "No description")} | ${row.active ? "Active" : "Inactive"}</span>
      <span class="row-actions">
        <button type="button" class="outline" data-edit-category='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-category="${row.id}">Delete</button>
      </span>
    </li>
  `;
}

function roleRow(row) {
  return `
    <li>
      <div>
        <b>${escapeHtml(row.name)}${row.protected ? " - Protected" : ""}</b>
        <span>${escapeHtml(row.description || "No description")}</span>
        <span>${escapeHtml((row.permissions || []).length ? row.permissions.join(", ") : "No access selected")}</span>
      </div>
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
        <span>${escapeHtml(row.description || "No description")} | ${row.active ? "Active" : "Inactive"}</span>
      </div>
      <span class="row-actions">
        <button type="button" class="outline" data-edit-audit-type='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-audit-type="${row.id}">Delete</button>
      </span>
    </article>
  `;
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
        <strong class="${row.status === "Unread" ? "warn" : ""}">${escapeHtml(row.status || "Unread")}</strong>
        ${row.related_type === "inspection" && (currentUser?.permissions || []).includes("inspections") ? `<button type="button" class="outline" data-open-inspection-session="${Number(row.related_id)}">Open Inspection</button>` : ""}
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
      <td class="user-role">${escapeHtml(row.role || "No role")}</td>
      <td><span class="status-pill ${status === "Active" ? "status-complete" : "status-untouched"}">${status}</span>${row.reset_requested || row.reset_required || row.resetRequired ? `<small class="user-alert">Password action needed</small>` : ""}</td>
      <td class="user-login">${escapeHtml(login)}</td>
      <td class="row-actions">
        <button type="button" class="outline" data-edit-user='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="outline" data-login-activity="${row.id}">Login activity</button>
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
  return `
    <li>
      <div>
        <b>${escapeHtml(row.name)}</b>
        <span>${escapeHtml(row.outlet_code)} | ${escapeHtml((row.locations || []).join(", ") || "No locations")} | ${escapeHtml(row.description || "No description")}</span>
      </div>
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
  const name = row.name || row.asset_id || row.code || "Fixed Asset";
  const type = row.type || row.equipment_type || "Fixed Asset";
  const code = row.code || row.asset_id || "";
  const location = row.location || row.zone || "No location";
  return `
    <article>
      <div>
        <b>${escapeHtml(name)} - ${escapeHtml(type)}</b>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(location)} | Code: ${escapeHtml(code)} | QR: ${escapeHtml(row.qr_code || code || "Not assigned")}</span>
        <span>${escapeHtml(row.brand || "No brand")} ${escapeHtml(row.model || "")}</span>
      </div>
      <span class="row-actions">
        <strong class="${statusClass}">${escapeHtml(status)}<small>${escapeHtml(row.installation_date || row.last_checked || "No date")}</small></strong>
        <button type="button" class="outline" data-edit-equipment='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-equipment="${row.id}">Delete</button>
      </span>
    </article>
  `;
}

function workOrderRow(row) {
  const confirmation = row.outlet_confirmed ? "Outlet confirmed" : "Awaiting outlet confirmation";
  const reference = row.work_order_ref || `#${row.id}`;
  const pic = row.pic || row.assignee || "No PIC";
  const completion = row.completion_date ? `Completed ${row.completion_date}` : "No completion date";
  const verification = row.verified_at ? `Verified ${row.verified_at}` : "";
  const sla = row.sla_status || "No SLA";
  const due = row.due_date ? `Due ${row.due_date}` : "No due date";
  return `
    <article>
      <div>
        <b>${escapeHtml(reference)} ${escapeHtml(row.title)}</b>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(row.zone)} | ${escapeHtml(row.request_type)} | ${escapeHtml(row.category || "No category")} | ${escapeHtml(row.assignee)}</span>
        <span>${escapeHtml(pic)} | ${escapeHtml(due)} | ${escapeHtml(sla)} | ${escapeHtml(completion)}${verification ? ` | ${escapeHtml(verification)}` : ""}${row.action_taken ? ` | ${escapeHtml(row.action_taken)}` : ""}</span>
      </div>
      <span class="row-actions">
        <strong class="${row.priority === "High" || sla === "Overdue" ? "warn" : ""}">${escapeHtml(row.priority)}<small>${escapeHtml(row.status)} - ${confirmation}</small></strong>
        <button type="button" class="outline" data-edit-work-order='${escapeAttr(JSON.stringify(row))}'>Edit</button>
        <button type="button" class="danger" data-delete-work-order="${row.id}">Delete</button>
      </span>
    </article>
  `;
}

function findingRow(row) {
  const reference = row.finding_ref || `F-${row.id}`;
  const auditReference = row.audit_ref || `Audit ${row.audit_id}`;
  const department = row.assigned_department || "Unassigned";
  return `
    <article>
      <div>
        <b>${escapeHtml(reference)} ${escapeHtml(row.category || "No category")}</b>
        <span>${escapeHtml(auditReference)} | ${escapeHtml(row.outlet)} | ${escapeHtml(row.location)}</span>
        <span>${escapeHtml(department)} | ${escapeHtml(row.pic || "No PIC")} | ${escapeHtml(row.comment || "No comment")}</span>
        ${row.corrective_action || row.completion_date ? `<span>${escapeHtml(row.corrective_action || "No action taken")} | ${escapeHtml(row.completion_date || "No completion date")}</span>` : ""}
        ${row.verified_at || row.closed_at ? `<span>${escapeHtml(row.verified_by || "No verifier")} | ${escapeHtml(row.verified_at || "No verification date")} | ${escapeHtml(row.closed_at || "Not closed")}</span>` : ""}
      </div>
      <span class="row-actions">
        <strong class="${row.priority === "High" ? "warn" : ""}">${escapeHtml(row.priority)}<small>${escapeHtml(row.status)}</small></strong>
      </span>
    </article>
  `;
}
function scheduleRow(row) {
  const matchingSession = inspectionHistoryCache.find((session) =>
    session.schedule_id === row.id
  );
  const displayName = row.schedule_ref || `SCH-${String(row.id).padStart(5, "0")}`;
  const savedAt = row.created_at ? new Date(row.created_at).toLocaleString() : "No saved time";
  const status = row.inspection_id || matchingSession
    ? inspectionHistoryProgressStatus(matchingSession || { status: row.inspection_status, progress: row.progress })
    : { className: "status-untouched", label: "Not Started (0%)" };
  return `
    <article data-schedule-id="${row.id}">
      <div data-open-schedule='${escapeAttr(JSON.stringify(row))}'>
        <b>${escapeHtml(displayName)}${row.audit_ref || matchingSession?.audit_ref ? ` · ${escapeHtml(row.audit_ref || matchingSession.audit_ref)}` : ""}</b>
        <span>${escapeHtml(row.scheduled_date)} | ${escapeHtml(savedAt)}</span>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(row.zone || "No location")} | ${escapeHtml(row.auditor)}</span>
      </div>
      <span class="row-actions">
        <span class="status-pill ${status.className}">${escapeHtml(status.label)}</span>
        <button type="button" class="primary" data-open-schedule='${escapeAttr(JSON.stringify(row))}'>Open</button>
        <button type="button" class="outline" data-edit-schedule='${escapeAttr(JSON.stringify(row))}'>Edit</button>
      </span>
    </article>
  `;
}

function auditRow(row) {
  return `
    <article>
      <div><b>${escapeHtml(row.outlet)}</b><span>${escapeHtml(row.branch)} - ${escapeHtml(row.audit_date)}</span></div>
      <strong class="${row.score >= 90 ? "excellent" : ""}">${row.score}<small>${escapeHtml(row.rating)}</small></strong>
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
