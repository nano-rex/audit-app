// What waits for the signed-in user: the counts beside each tab, and the unread notifications.
async function loadAttention() {
  const response = await authFetch("/api/todo");
  const data = await response.json();
  unreadNotifications = data.unreadNotifications || 0;
  attentionCounts = data.counts || {};
  const outdated = document.querySelector("[data-server-outdated]");
  if (outdated) outdated.hidden = !data.serverOutdated;
  renderUnreadBadge();
}

async function openAttentionItem(type, id, view = "") {
  if (type === "change") {
    showTab("approvals");
    return;
  }
  if (type === "schedule") {
    const response = await authFetch("/api/schedules");
    const row = ((await response.json()).items || []).find((item) => item.id === id);
    if (!row) throw new Error("That scheduled visit is no longer available");
    openScheduledInspection(row);
    return;
  }
  if (type === "work_request") {
    showTab("work-orders");
    showMaintenanceSubtab("requests");
    return;
  }
  if (type === "inspection") {
    await (view === "signoff" ? openSignoff(id) : openInspectionSession(id));
    return;
  }
  const response = await authFetch("/api/work-orders");
  const row = ((await response.json()).items || []).find((order) => order.id === id);
  if (!row) throw new Error("That work order is no longer available");
  await openWorkOrderEditor(row);
}

async function loadDashboard() {
  const [response] = await Promise.all([authFetch(`/api/dashboard?unit=${encodeURIComponent(currentUnit)}`), loadAttention()]);
  // Scheduled audits are opened from Inspections; do not offer them to accounts without that page.
  document.querySelectorAll("[data-scheduled-panel]").forEach((node) => { node.hidden = !(currentUser?.permissions || []).includes("inspections"); });
  const data = await response.json();
  renderMainDashboard(data);

  dashboardSchedules = data.today.scheduled || [];
  renderDashboardSchedules();
}

// Every scheduled visit not yet finished, a page at a time, with their number beside the heading.
let dashboardSchedules = [];

function renderDashboardSchedules() {
  const count = document.querySelector("[data-scheduled-count]");
  if (count) {
    count.textContent = String(dashboardSchedules.length);
    count.hidden = !dashboardSchedules.length;
  }
  const page = paginateList("dashboard-schedules", dashboardSchedules, "", renderDashboardSchedules);
  setHtml("[data-today-schedules]", (dashboardSchedules.length
    ? page.items.map(scheduleRow).join("")
    : `<article><div><b>No scheduled audits</b><span>Use Schedule Visit to plan one.</span></div></article>`) + (dashboardSchedules.length ? page.controls : ""));
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
  setStat("findings", (findings.items || []).filter((row) => row.status !== "Closed").length);
  setHtml("[data-super-rankings]", (dashboard.rankings || []).length
    ? dashboard.rankings.map((row, index) => rankingRow(row, index + 1)).join("")
    : `<p class="muted">No outlet performance data available.</p>`);
  const active = (databases.databases || []).find((database) => database.active);
  setText("[data-super-database]", active ? active.name : "No active database");
}

const themePresets = [
  { id: "default", label: "Forest", colour: "#47735f" },
  { id: "ottotree", label: "Ottotree", colour: "#1e99b4" },
  { id: "ocean", label: "Ocean", colour: "#2563eb" },
  { id: "plum", label: "Plum", colour: "#7c3aed" },
  { id: "ember", label: "Ember", colour: "#c2410c" },
  { id: "slate", label: "Slate", colour: "#475569" },
];

function themeFormValue() {
  const form = document.getElementById("theme-form");
  return {
    preset: form.querySelector('input[name="preset"]:checked')?.value || "default",
    accent: form.elements.useAccent.checked ? form.elements.accent.value : "",
    font: form.elements.font.value,
    corners: form.elements.corners.value,
    density: form.elements.density.value,
    mode: form.elements.mode.value,
    userChoice: form.elements.userChoice.checked,
  };
}

function fillThemeForm(theme) {
  const form = document.getElementById("theme-form");
  if (!form) return;
  setHtml("[data-theme-presets]", themePresets.map((preset) => `
    <label class="theme-preset"><input type="radio" name="preset" value="${preset.id}" ${preset.id === theme.preset ? "checked" : ""}>
      <span class="theme-swatch" style="--swatch:${preset.colour}"></span><span>${escapeHtml(preset.label)}</span></label>`).join(""));
  form.elements.useAccent.checked = Boolean(theme.accent);
  form.elements.accent.value = theme.accent || themePresets.find((preset) => preset.id === theme.preset)?.colour || "#47735f";
  form.elements.accent.disabled = !theme.accent;
  ["font", "corners", "density", "mode"].forEach((name) => { form.elements[name].value = theme[name]; });
  form.elements.userChoice.checked = theme.userChoice !== false;
  setText("[data-theme-message]", "");
}

document.getElementById("theme-form")?.addEventListener("input", (event) => {
  const form = event.currentTarget;
  if (event.target.name === "preset" && !form.elements.useAccent.checked) {
    form.elements.accent.value = themePresets.find((preset) => preset.id === event.target.value)?.colour || form.elements.accent.value;
  }
  form.elements.accent.disabled = !form.elements.useAccent.checked;
  previewOrgTheme(themeFormValue());
  setText("[data-theme-message]", "Previewing. Save to apply for everyone.");
});

document.querySelector("[data-theme-undo]")?.addEventListener("click", () => {
  restoreOrgTheme();
  fillThemeForm(orgTheme);
});

document.getElementById("theme-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const theme = themeFormValue();
  try {
    await requestJson("/api/settings", "POST", { settings: Object.fromEntries(Object.entries(theme).map(([key, value]) => [`theme.${key}`, value])) });
    setOrgTheme(theme);
    setText("[data-theme-message]", "Theme saved for this organization.");
  } catch (error) {
    setText("[data-theme-message]", error.message);
  }
});

async function loadSuperSettings() {
  fillThemeForm(savedOrgTheme);
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
}

async function loadFindings() {
  const response = await authFetch("/api/findings");
  const data = await response.json();
  findingCache = data.items || [];
  updateFindingFilterSelects();
  renderFindings();
}

// Findings that pass the filters, gathered into one entry per inspected item.
function findingGroups() {
  const search = findingFilters.search.toLowerCase();
  const rows = findingCache.filter((row) => {
    const haystack = [row.finding_ref, row.audit_ref, row.item_name, row.criterion, row.outlet, row.location, row.category,
      row.priority, row.assigned_department, row.pic, row.comment, row.status, row.request_ref].join(" ").toLowerCase();
    const status = findingFilters.status === "active" ? row.status !== "Closed" : !findingFilters.status || row.status === findingFilters.status;
    return (!search || haystack.includes(search)) && status
      && (!findingFilters.auditId || String(row.audit_id) === findingFilters.auditId)
      && (!findingFilters.outlet || row.outlet === findingFilters.outlet)
      && (!findingFilters.location || row.location === findingFilters.location)
      && (!findingFilters.department || row.assigned_department === findingFilters.department)
      && (!findingFilters.category || row.category === findingFilters.category)
      && (!findingFilters.priority || row.priority === findingFilters.priority);
  });
  const groups = new Map();
  rows.forEach((row) => {
    const key = [row.audit_id, row.equipment_id || row.item_name, row.location].join("|");
    if (!groups.has(key)) {
      groups.set(key, { key, findings: [], item_name: row.item_name, item_kind: row.item_kind, audit_id: row.audit_id, audit_ref: row.audit_ref,
        outlet: row.outlet, location: row.location, department: row.assigned_department, category: row.category, priority: row.priority, images: [] });
    }
    const group = groups.get(key);
    group.findings.push(row);
    // The same photo is often attached to several checks of one item; show it once.
    parseStoredImages(row.images_json || "[]").forEach((image) => {
      if (!group.images.some((seen) => imageSource(seen) === imageSource(image))) group.images.push(image);
    });
  });
  return [...groups.values()].map((group) => ({ ...group, findingIds: group.findings.map((row) => row.id) }));
}

function renderFindings() {
  const groups = findingGroups();
  const page = paginateList("findings", groups, findingFilters, renderFindings);
  setHtml("[data-findings]", (groups.length
    ? page.items.map(findingItemRow).join("")
    : `<article><div><b>No findings</b><span>Items that failed a check in a completed inspection appear here.</span></div></article>`) + page.controls);
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
    : `<article><div><b>No work orders found</b><span>Create one from a work request, or adjust the filters.</span></div></article>`);
}

async function loadNotifications() {
  const response = await authFetch("/api/notifications");
  const data = await response.json();
  notificationCache = data.items || [];
  unreadNotifications = notificationCache.filter((row) => row.status === "Unread").length;
  renderUnreadBadge();
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

// Fixed assets and variable assets are separate tabs over one register.
function setEquipmentKind(kind) {
  equipmentFilters.kind = kind;
  setText("[data-equipment-title]", kind === "fixture" ? "Variable Assets" : "Fixed Assets");
  const search = document.getElementById("equipment-search");
  if (search) search.placeholder = kind === "fixture" ? "Search variable assets" : "Search fixed assets";
  document.querySelectorAll("#equipment [data-show-kind]").forEach((node) => { node.hidden = node.dataset.showKind !== kind; });
  // Both kinds have the same details; the choices in each filter follow the tab shown.
  equipmentFilters.status = equipmentFilters.type = equipmentFilters.brand = "";
  if (typeof updateEquipmentFilterSelects === "function" && document.getElementById("equipment-filter-status")) updateEquipmentFilterSelects();
  if (equipmentCache.length) renderEquipment();
}

async function loadEquipment() {
  const response = await authFetch("/api/equipment");
  const data = await response.json();
  equipmentCache = data.items;
  updateEquipmentFilterSelects();
  updateEquipmentNameOptions();
  renderEquipment();
}

// Item dates are typed as text: 2026-10-09, 09.10.2026, 9/10/2026, or 09-10-2026 (day first).
function parseItemDate(text) {
  const value = String(text || "").trim();
  let match = value.match(/^(\d{4})[-./](\d{1,2})[-./](\d{1,2})$/);
  let [year, month, day] = match ? [match[1], match[2], match[3]] : [];
  if (!match) {
    match = value.match(/^(\d{1,2})[-./](\d{1,2})[-./](\d{2}|\d{4})$/);
    if (!match) return null;
    [day, month, year] = [match[1], match[2], match[3].length === 2 ? `20${match[3]}` : match[3]];
  }
  const date = new Date(Number(year), Number(month) - 1, Number(day));
  return date.getMonth() === Number(month) - 1 && date.getDate() === Number(day) ? date : null;
}

// Expired, ends within 30 or 90 days, still valid, or not recorded.
function matchesDateFilter(text, filter) {
  if (!filter) return true;
  const date = parseItemDate(text);
  if (filter === "none") return !date;
  if (!date) return false;
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const days = Math.round((date - today) / 86400000);
  if (filter === "expired") return days < 0;
  if (filter === "valid") return days >= 0;
  return days >= 0 && days <= Number(filter);
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
      row.category,
    ].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!equipmentFilters.kind || (row.kind || "asset") === equipmentFilters.kind)
      && (!equipmentFilters.outlet || row.outlet === equipmentFilters.outlet)
      && (!equipmentFilters.location || location === equipmentFilters.location)
      && (!equipmentFilters.type || type === equipmentFilters.type)
      && (!equipmentFilters.brand || brand === equipmentFilters.brand)
      && (!equipmentFilters.status || (row.operational_status || row.health_status || "") === equipmentFilters.status)
      && matchesDateFilter(row.warranty_date, equipmentFilters.warranty)
      && matchesDateFilter(row.expiry_date, equipmentFilters.expiry);
  });
  const pageSize = getPaginationSize();
  const pages = Math.max(1, Math.ceil(rows.length / pageSize));
  equipmentPage = Math.max(1, Math.min(equipmentPage, pages));
  const visible = rows.slice((equipmentPage - 1) * pageSize, equipmentPage * pageSize);
  setHtml("[data-equipment]", rows.length
    ? visible.map(equipmentRow).join("") + `<nav aria-label="Fixed asset pages">
        <button type="button" data-equipment-page="${equipmentPage - 1}" ${equipmentPage === 1 ? "disabled" : ""}>Previous</button>
        <span>Page ${equipmentPage} of ${pages} · ${rows.length} items</span>
        <button type="button" data-equipment-page="${equipmentPage + 1}" ${equipmentPage === pages ? "disabled" : ""}>Next</button>
      </nav>`
    : `<article><div><b>Nothing found</b><span>Adjust search or filters, or add a fixed asset or a variable asset.</span></div></article>`);
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
  document.querySelector("[data-report-critical]").innerHTML = data.criticalIssues.length
    ? data.criticalIssues.map(workOrderRow).join("")
    : `<article><div><b>No critical issues</b><span>High priority work orders will appear here.</span></div></article>`;
  document.querySelector("[data-export-pdf]").href = `/api/reports/export.pdf?${query}`;
  document.querySelector("[data-export-xls]").href = `/api/reports/export.xlsx?${query}`;
  setHtml("[data-rankings]", data.rankings.length ? data.rankings.map((row, index) => rankingRow(row, index + 1)).join("")
    : `<article><div><b>No completed audits</b><span>Outlets are ranked once their audits are completed.</span></div></article>`);
  // The figures as charts (report-charts.js); the performance donut is shared with the Dashboard.
  renderPerformanceDistribution((data.charts || {}).performanceDistribution || []);
  renderReportPage(data);
  renderPerformance(data);
}

// "3 d 4 h", "2 h 15 m", "12 m": how long a step took.
function durationText(seconds) {
  if (seconds == null) return "—";
  const minutes = Math.floor(seconds / 60);
  const days = Math.floor(minutes / 1440), hours = Math.floor((minutes % 1440) / 60), rest = minutes % 60;
  if (days) return `${days} d ${hours} h`;
  if (hours) return `${hours} h ${rest} m`;
  return minutes ? `${minutes} m` : "under a minute";
}


function renderPerformance(data) {
  setHtml("[data-time-to-act]", (data.timeToAct || []).map((row) => `
    <article><span>${escapeHtml(row.label)}</span><strong>${escapeHtml(durationText(row.averageSeconds))}</strong>
    <small>${row.count ? `${row.count} time${row.count === 1 ? "" : "s"}; longest ${escapeHtml(durationText(row.longestSeconds))}` : "Nothing yet in this period"}</small></article>`).join(""));
  const people = data.people || [];
  const cell = (value) => `<td>${escapeHtml(value)}</td>`;
  setHtml("[data-kpi-people]", people.length ? `
    <thead><tr><th>Person</th><th>Audits started</th><th>Audits completed</th><th>Avg. audit time</th><th>Signed</th><th>Audits closed</th>
    <th>Requests raised</th><th>Requests acted on</th><th>Avg. time to act</th><th>Orders created</th><th>Orders closed</th><th>Avg. time to close</th></tr></thead>
    <tbody>${people.map((row) => `<tr><th scope="row">${escapeHtml(row.name)}</th>${cell(row.audit_started)}${cell(row.audit_completed)}${cell(durationText(row.auditSeconds))}
      ${cell(row.audit_signed)}${cell(row.audit_closed)}${cell(row.request_raised)}${cell(row.order_created + row.request_declined)}${cell(durationText(row.requestSeconds))}
      ${cell(row.order_created)}${cell(row.order_closed)}${cell(durationText(row.orderSeconds))}</tr>`).join("")}</tbody>`
    : "<tbody><tr><td>No activity in this period.</td></tr></tbody>");
}

function auditChart(title, source, score = false) {
  const entries = (Array.isArray(source) ? source : Object.entries(source).map(([label, count]) => ({ label, count })))
    .map((row) => ({ label: row.label || row.month || row.outlet || "Unassigned",
      value: Math.max(0, Number((score ? row.score : row.count ?? row.audits ?? row.average ?? row.score) ?? 0) || 0) }));
  const maximum = score ? 100 : Math.max(1, ...entries.map((row) => row.value));
  const hasData = entries.length && (score || entries.some((row) => row.value > 0));
  return `<article class="panel mini-chart"><h2>${escapeHtml(title)}</h2>
    <p class="muted">${score === "percent" ? "Closed work orders · 0–100%" : score ? "Average completed audit score · 0–100" : "Number of records"}</p>
    ${hasData ? `<ol class="audit-chart">${entries.map((row) => `<li>
      <div class="audit-chart-label"><span>${escapeHtml(row.label)}</span><strong>${row.value}${score === "percent" ? "%" : score ? "/100" : ""}</strong></div>
      <div class="audit-chart-track" aria-hidden="true"><span style="width:${Math.min(100, row.value * 100 / maximum)}%"></span></div>
    </li>`).join("")}</ol>` : '<p class="muted">No data yet</p>'}</article>`;
}

function renderMainDashboard(data) {
  const stats = data.stats || {};
  for (const key of ["total", "auditsCompleted", "auditsPending", "priorityIssues", "nonPriorityIssues", "outstandingFindings", "closedWorkOrders"]) {
    setText(`[data-dashboard="${key}"]`, stats[key] ?? 0);
  }
  setText('[data-dashboard="overallAuditScore"]', stats.overallAuditScore == null ? "No completed audits" : `${stats.overallAuditScore}/100`);
  const charts = data.charts || {};
  const definitions = [
    ["auditScores", "Audit Scores by Outlet", true],
    ["priorityVsNonPriority", "Priority vs Non-Priority"],
    ["findingsByDepartment", "Issues by Department"],
    ["findingsByArea", "Issues by Area"],
    ["findingsByCategory", "Issues by Asset Type"],
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
