async function loadDashboard() {
  const response = await fetch(`/api/dashboard?unit=${encodeURIComponent(currentUnit)}`);
  const data = await response.json();
  setText('[data-today="scheduled"]', data.today.scheduled.length);
  setText('[data-today="uploads"]', data.today.pendingUploads);
  setText('[data-today="followups"]', data.today.followUps);
  setText('[data-today="dueSoon"]', data.today.dueSoon || 0);
  setText('[data-today="overdue"]', data.today.overdue || 0);
  setText('[data-today="tasks"]', data.today.scheduled.length + data.today.pendingUploads + data.today.followUps + (data.today.dueSoon || 0) + (data.today.overdue || 0));

  setHtml("[data-outlets]", data.outlets.map((outlet) => `
    <article>
      <b>${escapeHtml(outlet.outlet)}</b>
      <strong>${outlet.average}</strong>
      <span>Average | ${outlet.audit_count} ${outlet.audit_count === 1 ? "audit" : "audits"}</span>
    </article>
  `).join(""));

  setHtml("[data-recent]", data.recent.map(auditRow).join(""));
  setHtml("[data-rankings]", data.rankings.map((row, index) => rankingRow(row, index + 1)).join(""));
  setHtml("[data-today-schedules]", data.today.scheduled.length
    ? data.today.scheduled.map(scheduleRow).join("")
    : `<article><div><b>No scheduled work</b><span>Create a schedule to assign outlet checks.</span></div></article>`);
  setHtml("[data-bars]", data.outlets.map((outlet) => `
    <label>${escapeHtml(outlet.outlet)}<span class="${outlet.latest >= 90 ? "excellent-bar" : ""}" style="--value:${outlet.latest}">${outlet.latest}</span></label>
  `).join(""));
  setText('[data-kpi="assigned"]', data.kpi.assigned);
  setText('[data-kpi="completed"]', data.kpi.completed);
  setText('[data-kpi="pending"]', data.kpi.pending);
  setText('[data-kpi="responseRate"]', `${data.kpi.responseRate}%`);
  loadReport();
}

async function loadWorkOrders() {
  const response = await fetch("/api/work-orders");
  const data = await response.json();
  workOrderCache = data.items;
  updateWorkOrderFilterSelects();
  renderWorkOrders();
  renderCorrectiveActions();
}

async function loadFindings() {
  const response = await fetch("/api/findings");
  const data = await response.json();
  findingCache = data.items || [];
  updateFindingFilterSelects();
  renderFindings();
}

function renderFindings() {
  const search = findingFilters.search.toLowerCase();
  const rows = findingCache.filter((row) => {
    const haystack = [
      row.finding_ref,
      row.audit_ref,
      row.outlet,
      row.location,
      row.category,
      row.priority,
      row.assigned_department,
      row.pic,
      row.comment,
      row.status,
    ].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!findingFilters.outlet || row.outlet === findingFilters.outlet)
      && (!findingFilters.location || row.location === findingFilters.location)
      && (!findingFilters.department || row.assigned_department === findingFilters.department)
      && (!findingFilters.category || row.category === findingFilters.category)
      && (!findingFilters.priority || row.priority === findingFilters.priority)
      && (!findingFilters.status || row.status === findingFilters.status);
  });
  setHtml("[data-findings]", rows.length
    ? rows.map(findingRow).join("")
    : `<article><div><b>No findings found</b><span>Completed inspections with failed criteria will appear here.</span></div></article>`);
}

function renderWorkOrders() {
  const search = workOrderFilters.search.toLowerCase();
  const rows = workOrderCache.filter((row) => {
    const haystack = [
      row.outlet,
      row.zone,
      row.request_type,
      row.category,
      row.priority,
      row.title,
      row.description,
      row.assignee,
      row.pic,
      row.status,
      row.action_taken,
      row.completion_date,
      row.completion_remark,
      row.verified_by,
      row.verified_at,
      row.verification_remark,
      row.closed_at,
    ].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!workOrderFilters.outlet || row.outlet === workOrderFilters.outlet)
      && (!workOrderFilters.location || row.zone === workOrderFilters.location)
      && (!workOrderFilters.department || row.request_type === workOrderFilters.department)
      && (!workOrderFilters.category || row.category === workOrderFilters.category)
      && (!workOrderFilters.priority || row.priority === workOrderFilters.priority)
      && (!workOrderFilters.status || row.status === workOrderFilters.status);
  });
  setHtml("[data-work-orders]", rows.length
    ? rows.map(workOrderRow).join("")
    : `<article><div><b>No work orders found</b><span>Adjust search or filters, or add a new work order.</span></div></article>`);
}

function renderCorrectiveActions() {
  const search = (document.getElementById("corrective-search")?.value || "").toLowerCase();
  const outlet = document.getElementById("corrective-filter-outlet")?.value || "";
  const department = document.getElementById("corrective-filter-department")?.value || "";
  const status = document.getElementById("corrective-filter-status")?.value || "";
  updateSelectOptions(document.getElementById("corrective-filter-outlet"), setupOptions.outlets, true, "All outlets");
  updateSelectOptions(document.getElementById("corrective-filter-department"), setupOptions.departments, true, "All departments");
  if (outlet) document.getElementById("corrective-filter-outlet").value = outlet;
  if (department) document.getElementById("corrective-filter-department").value = department;
  const rows = workOrderCache.filter((row) => {
    const haystack = [row.work_order_ref, row.outlet, row.zone, row.request_type, row.title, row.action_taken, row.pic, row.status, row.sla_status].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!outlet || row.outlet === outlet)
      && (!department || row.request_type === department)
      && (!status || row.status === status);
  });
  setHtml("[data-corrective-actions]", rows.length
    ? rows.map(workOrderRow).join("")
    : `<article><div><b>No corrective actions found</b><span>Work orders and finding follow-ups appear here.</span></div></article>`);
}

async function loadNotifications() {
  const response = await fetch("/api/notifications");
  const data = await response.json();
  notificationCache = data.items || [];
  renderNotifications();
}

function renderNotifications() {
  const search = notificationFilters.search.toLowerCase();
  const rows = notificationCache.filter((row) => {
    const haystack = [row.title, row.message, row.channel, row.status, row.related_type].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!notificationFilters.status || row.status === notificationFilters.status);
  });
  setHtml("[data-notifications]", rows.length
    ? rows.map(notificationRow).join("")
    : `<article><div><b>No notifications found</b><span>Assigned, due soon, overdue, and completed notices appear here.</span></div></article>`);
}

async function loadEquipment() {
  const response = await fetch("/api/equipment");
  const data = await response.json();
  equipmentCache = data.items;
  updateEquipmentFilterSelects();
  renderEquipment();
}

function renderEquipment() {
  const search = equipmentFilters.search.toLowerCase();
  const rows = equipmentCache.filter((row) => {
    const location = row.location || row.zone || "";
    const type = row.type || row.equipment_type || "";
    const brand = row.brand || "";
    const haystack = [
      row.name,
      row.description,
      type,
      row.operational_status || row.health_status,
      row.code || row.asset_id,
      row.model,
      row.serial_number,
      brand,
      row.outlet,
      location,
    ].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!equipmentFilters.outlet || row.outlet === equipmentFilters.outlet)
      && (!equipmentFilters.location || location === equipmentFilters.location)
      && (!equipmentFilters.type || type === equipmentFilters.type)
      && (!equipmentFilters.brand || brand === equipmentFilters.brand);
  });
  setHtml("[data-equipment]", rows.length
    ? rows.map(equipmentRow).join("")
    : `<article><div><b>No fixed assets found</b><span>Adjust search or filters, or add a new fixed asset.</span></div></article>`);
  updateEquipmentNameOptions();
}

function updateEquipmentNameOptions() {
  const datalist = document.getElementById("equipment-name-options");
  if (!datalist) return;
  const names = [...new Set(equipmentCache.map((row) => row.name || row.asset_id || "").filter(Boolean))].sort();
  datalist.innerHTML = names.map((name) => `<option value="${escapeAttr(name)}"></option>`).join("");
}

function applyEquipmentTemplate(name) {
  const form = document.getElementById("equipment-form");
  const template = equipmentCache.find((row) => (row.name || row.asset_id || "") === name);
  if (!form || !template) return;
  form.elements.type.value = template.type || template.equipment_type || "";
  form.elements.brand.value = template.brand || "";
  form.elements.description.value = template.description || template.notes || "";
  renderEquipmentCriteria(parseInspectionCriteria(template.inspection_criteria));
}

async function loadReport() {
  const response = await fetch(`/api/reports?unit=${encodeURIComponent(currentUnit)}`);
  const data = await response.json();
  document.querySelector('[data-report="audits"]').textContent = data.monthlySummary.audits;
  document.querySelector('[data-report="averageScore"]').textContent = `${data.monthlySummary.averageScore}/100`;
  document.querySelector('[data-report="openWorkOrders"]').textContent = data.monthlySummary.openWorkOrders;
  document.querySelector('[data-report="auditsPending"]').textContent = data.monthlySummary.auditsPending || 0;
  document.querySelector('[data-report="totalFindings"]').textContent = data.monthlySummary.totalFindings || 0;
  document.querySelector('[data-report="priorityFindings"]').textContent = data.monthlySummary.priorityFindings || 0;
  document.querySelector('[data-report="nonPriorityFindings"]').textContent = data.monthlySummary.nonPriorityFindings || 0;
  document.querySelector('[data-report="overdueFindings"]').textContent = data.monthlySummary.overdueFindings || 0;
  document.querySelector('[data-report="completionRate"]').textContent = `${data.monthlySummary.completionRate || 0}%`;
  document.querySelector("[data-report-critical]").innerHTML = data.criticalIssues.length
    ? data.criticalIssues.map(workOrderRow).join("")
    : `<article><div><b>No critical issues</b><span>High priority work orders will appear here.</span></div></article>`;
  document.querySelector("[data-export-json]").href = `/api/reports/export.json?unit=${encodeURIComponent(currentUnit)}`;
  document.querySelector("[data-export-csv]").href = `/api/reports/export.csv?unit=${encodeURIComponent(currentUnit)}`;
  document.querySelector("[data-export-xls]").href = `/api/reports/export.xls?unit=${encodeURIComponent(currentUnit)}`;
  renderReportCharts(data.charts || {});
}

function renderReportCharts(charts) {
  const chartMap = [
    ["priorityVsNonPriority", "Priority vs Non-Priority"],
    ["findingsByDepartment", "Issues by Department"],
    ["findingsByArea", "Issues by Area"],
    ["findingsByCategory", "Issues by Category"],
    ["monthlyAuditTrend", "Monthly Audit Trend"],
    ["findingsTrend", "Findings Trend"],
    ["departmentPerformance", "Department Performance"],
    ["locationPerformance", "Location Performance"],
    ["categoryPerformance", "Category Performance"],
  ];
  setHtml("[data-report-charts]", chartMap.map(([key, label]) => {
    const source = charts[key] || [];
    const entries = Array.isArray(source)
      ? source.map((row) => [row.label || row.month || row.outlet || "Unassigned", row.count ?? row.average ?? row.score ?? 0])
      : Object.entries(source);
    return `
      <article class="panel mini-chart">
        <h2>${escapeHtml(label)}</h2>
        <div class="bars">${entries.length ? entries.map(([name, value]) => `
          <label>${escapeHtml(name)}<span style="--value:${Math.min(100, Number(value) || 0)}">${escapeHtml(value)}</span></label>
        `).join("") : `<p class="muted">No data</p>`}</div>
      </article>
    `;
  }).join(""));
}
