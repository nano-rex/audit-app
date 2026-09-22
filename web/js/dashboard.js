async function loadDashboard() {
  const response = await authFetch(`/api/dashboard?unit=${encodeURIComponent(currentUnit)}`);
  const data = await response.json();
  renderMainDashboard(data);

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
}

async function loadSuperDashboard() {
  const targets = ["[data-super-rankings]", "[data-super-database]"];
  targets.forEach((selector) => setLoading(selector, "Loading super dashboard…"));
  const [dashboardResponse, usersResponse, rolesResponse, findingsResponse, equipmentResponse, databaseResponse] = await Promise.all([
    authFetch(`/api/dashboard?unit=${encodeURIComponent(currentUnit)}`),
    authFetch("/api/users"),
    authFetch("/api/roles"),
    authFetch("/api/findings"),
    authFetch("/api/equipment"),
    authFetch("/api/account/databases"),
  ]);
  const [dashboard, users, roles, findings, equipment, databases] = await Promise.all([
    dashboardResponse.json(), usersResponse.json(), rolesResponse.json(), findingsResponse.json(), equipmentResponse.json(), databaseResponse.json(),
  ]);
  const activeUsers = (users.items || []).filter((user) => user.active).length;
  const setStat = (name, value) => setText(`[data-super-stat="${name}"]`, value);
  setStat("users", (users.items || []).length);
  setStat("activeUsers", activeUsers);
  setStat("roles", (roles.items || []).length);
  setStat("departments", (setupOptions.departments || []).length);
  setStat("assets", (equipment.items || []).length);
  setStat("findings", (findings.items || []).filter((row) => !["Completed", "Closed"].includes(row.status)).length);
  setHtml("[data-super-rankings]", (dashboard.rankings || []).length
    ? dashboard.rankings.map((row, index) => rankingRow(row, index + 1)).join("")
    : `<p class="muted">No outlet performance data available.</p>`);
  const active = (databases.databases || []).find((database) => database.active);
  setText("[data-super-database]", active ? active.name : "No active database");
}

async function loadSuperSettings() {
  setLoading("[data-super-settings-summary]", "Loading system settings…");
  const superSettingsGrid = document.querySelector("#super-settings .settings-grid");
  document.querySelectorAll("#settings [data-super-only-setting]").forEach((panel) => {
    panel.hidden = false;
    superSettingsGrid?.appendChild(panel);
  });
  await loadSuperDatabases();
  const settingCount = Object.keys(setupOptions.settings || {}).length;
  const databaseCount = document.querySelector("[data-super-database-select]")?.options.length || 0;
  setHtml("[data-super-settings-summary]", `<div><dt>Stored settings</dt><dd>${settingCount}</dd></div><div><dt>Available databases</dt><dd>${databaseCount}</dd></div><div><dt>Role permissions</dt><dd>Managed separately from Super access</dd></div>`);
}

async function loadWorkOrders() {
  const response = await authFetch("/api/work-orders");
  const data = await response.json();
  workOrderCache = data.items;
  updateWorkOrderFilterSelects();
  renderWorkOrders();
  renderCorrectiveActions();
}

async function loadFindings() {
  const response = await authFetch("/api/findings");
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
      && (!findingFilters.auditId || String(row.audit_id) === findingFilters.auditId)
      && (!findingFilters.outlet || row.outlet === findingFilters.outlet)
      && (!findingFilters.location || row.location === findingFilters.location)
      && (!findingFilters.department || row.assigned_department === findingFilters.department)
      && (!findingFilters.category || row.category === findingFilters.category)
      && (!findingFilters.priority || row.priority === findingFilters.priority)
      && (!findingFilters.status || row.status === findingFilters.status);
  });
  const page = paginateList("findings", rows, findingFilters, renderFindings);
  setHtml("[data-findings]", (rows.length
    ? page.items.map(findingRow).join("")
    : `<article><div><b>No findings found</b><span>Completed inspections with failed criteria will appear here.</span></div></article>`) + page.controls);
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
  const response = await authFetch("/api/notifications");
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
  const response = await authFetch("/api/equipment");
  const data = await response.json();
  equipmentCache = data.items;
  updateEquipmentFilterSelects();
  updateEquipmentNameOptions();
  renderEquipment();
}

function renderEquipment() {
  const filterKey = JSON.stringify(equipmentFilters);
  if (filterKey !== equipmentFilterKey) {
    equipmentPage = 1;
    equipmentFilterKey = filterKey;
  }
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
  const pageSize = getPaginationSize();
  const pages = Math.max(1, Math.ceil(rows.length / pageSize));
  equipmentPage = Math.max(1, Math.min(equipmentPage, pages));
  const visible = rows.slice((equipmentPage - 1) * pageSize, equipmentPage * pageSize);
  setHtml("[data-equipment]", rows.length
    ? visible.map(equipmentRow).join("") + `<nav aria-label="Fixed asset pages">
        <button type="button" data-equipment-page="${equipmentPage - 1}" ${equipmentPage === 1 ? "disabled" : ""}>Previous</button>
        <span>Page ${equipmentPage} of ${pages} · ${rows.length} assets</span>
        <button type="button" data-equipment-page="${equipmentPage + 1}" ${equipmentPage === pages ? "disabled" : ""}>Next</button>
      </nav>`
    : `<article><div><b>No fixed assets found</b><span>Adjust search or filters, or add a new fixed asset.</span></div></article>`);
}

window.addEventListener("pagination-size-changed", () => {
  equipmentPage = 1;
  if (typeof renderEquipment === "function" && equipmentCache.length) renderEquipment();
});

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
  const form = document.getElementById("report-filter-form");
  const query = new URLSearchParams({ unit: currentUnit });
  for (const key of ["from", "to", "outlet"]) {
    if (form?.elements[key].value) query.set(key, form.elements[key].value);
  }
  const response = await authFetch(`/api/reports?${query}`);
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || "Report could not be loaded");
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
  document.querySelector("[data-export-json]").href = `/api/reports/export.json?${query}`;
  document.querySelector("[data-export-csv]").href = `/api/reports/export.csv?${query}`;
  document.querySelector("[data-export-xls]").href = `/api/reports/export.xlsx?${query}`;
  setHtml("[data-rankings]", data.rankings.map((row, index) => rankingRow(row, index + 1)).join(""));
  for (const key of ["assigned", "completed", "pending", "responseRate"]) setText(`[data-kpi="${key}"]`, `${data.kpi[key]}${key === "responseRate" ? "%" : ""}`);
  setHtml("[data-bars]", data.rankings.map((row) => `<label>${escapeHtml(row.outlet)}<span style="--value:${row.latest}">${row.latest}</span></label>`).join(""));
  renderReportCharts(data.charts || {});
}

function renderReportCharts(charts) {
  renderPerformanceDistribution(charts.performanceDistribution || []);
  const chartMap = [
    ["priorityVsNonPriority", "Priority vs Non-Priority"],
    ["findingsByDepartment", "Issues by Department"],
    ["findingsByArea", "Issues by Area"],
    ["findingsByCategory", "Issues by Category"],
    ["monthlyAuditTrend", "Monthly Audit Count"],
    ["monthlyAuditScores", "Monthly Audit Scores", true],
    ["auditComparison", "Previous vs Current Audit", true],
    ["priorityTrend", "Priority Trend"],
    ["roomAuditTrend", "Room / Location Audit Trend", true],
    ["findingsTrend", "Findings Trend"],
    ["departmentPerformance", "Completion by Department", "percent"],
    ["locationPerformance", "Completion by Location", "percent"],
    ["categoryPerformance", "Completion by Category", "percent"],
  ];
  setHtml("[data-report-charts]", chartMap.map(([key, label, scale]) => auditChart(label, charts[key] || [], scale)).join(""));
}

function auditChart(title, source, score = false) {
  const entries = (Array.isArray(source) ? source : Object.entries(source).map(([label, count]) => ({ label, count })))
    .map((row) => ({ label: row.label || row.month || row.outlet || "Unassigned",
      value: Math.max(0, Number((score ? row.score : row.count ?? row.audits ?? row.average ?? row.score) ?? 0) || 0) }));
  const maximum = score ? 100 : Math.max(1, ...entries.map((row) => row.value));
  const hasData = entries.length && (score || entries.some((row) => row.value > 0));
  return `<article class="panel mini-chart"><h2>${escapeHtml(title)}</h2>
    <p class="muted">${score === "percent" ? "Completed corrective actions · 0–100%" : score ? "Average completed audit score · 0–100" : "Number of records"}</p>
    ${hasData ? `<ol class="audit-chart">${entries.map((row) => `<li>
      <div class="audit-chart-label"><span>${escapeHtml(row.label)}</span><strong>${row.value}${score === "percent" ? "%" : score ? "/100" : ""}</strong></div>
      <div class="audit-chart-track" aria-hidden="true"><span style="width:${Math.min(100, row.value * 100 / maximum)}%"></span></div>
    </li>`).join("")}</ol>` : '<p class="muted">No data yet</p>'}</article>`;
}

function renderMainDashboard(data) {
  const stats = data.stats || {};
  for (const key of ["total", "auditsCompleted", "auditsPending", "priorityIssues", "nonPriorityIssues", "outstandingFindings", "completedCorrectiveActions"]) {
    setText(`[data-dashboard="${key}"]`, stats[key] ?? 0);
  }
  setText('[data-dashboard="overallAuditScore"]', stats.overallAuditScore == null ? "No completed audits" : `${stats.overallAuditScore}/100`);
  const charts = data.charts || {};
  const definitions = [
    ["auditScores", "Audit Scores by Outlet", true],
    ["priorityVsNonPriority", "Priority vs Non-Priority"],
    ["findingsByDepartment", "Issues by Department"],
    ["findingsByArea", "Issues by Area"],
    ["findingsByCategory", "Issues by Description Category"],
    ["monthlyAuditTrend", "Monthly Completed Audit Trend"],
  ];
  setHtml("[data-dashboard-charts]", definitions.map(([key, label, score]) => auditChart(label, charts[key] || [], score)).join(""));
}

function renderPerformanceDistribution(rows) {
  const bands = [
    ["Excellent", "#24c362"], ["Good", "var(--blue)"],
    ["Below Expectation", "var(--orange)"], ["Critical", "var(--red)"],
  ].map(([label, color]) => ({ label, color, count: Math.max(0, Number(rows.find((row) => row.label === label)?.count) || 0) }));
  const total = bands.reduce((sum, band) => sum + band.count, 0);
  const chart = document.querySelector("[data-performance-chart]");
  if (!chart) return;
  chart.hidden = total === 0;
  setText("[data-performance-summary]", total ? `${total} completed ${total === 1 ? "audit" : "audits"}` : "No completed audits");
  setHtml("[data-performance-legend]", total ? bands.map((band) => `<span><i class="legend-swatch" style="background:${band.color}" aria-hidden="true"></i>${band.label}: ${band.count} (${Math.round(band.count * 100 / total)}%)</span>`).join("") : "");
  let end = 0;
  const stops = bands.map((band) => {
    const start = end;
    end += total ? band.count * 100 / total : 0;
    return `${band.color} ${start}% ${end}%`;
  });
  chart.style.background = total ? `conic-gradient(${stops.join(", ")})` : "var(--border)";
  chart.setAttribute("aria-label", total ? bands.map((band) => `${band.label}: ${band.count}`).join(", ") : "No completed audits");
}
