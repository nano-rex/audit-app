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
  const form = document.getElementById("dashboard-filter-form");
  const outletSelect = form?.elements.outlet;
  if (outletSelect && outletSelect.options.length <= 1) updateSelectOptions(outletSelect, setupOptions.outlets || [], true, "All outlets");
  const query = new URLSearchParams({ unit: currentUnit });
  for (const key of ["from", "to", "outlet"]) {
    if (form?.elements[key].value) query.set(key, form.elements[key].value);
  }
  const clear = form?.querySelector(".dash-clear");
  const filtered = [...query.keys()].length > 1;
  if (clear) clear.hidden = !filtered;
  let response;
  try {
    [response] = await Promise.all([authFetch(`/api/dashboard?${query}`), loadAttention()]);
  } catch (error) {
    // A bad filter (e.g. the start after the end) is shown beside the filters.
    if (!filtered) throw error;
    setText("[data-dashboard-filter-message]", error.message);
    return;
  }
  setText("[data-dashboard-filter-message]", "");
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
  const filtered = [...query.keys()].length > 1;
  const clear = form?.querySelector(".dash-clear");
  if (clear) clear.hidden = !filtered;
  let response;
  try {
    response = await authFetch(`/api/reports?${query}`);
  } catch (error) {
    // A bad filter (e.g. the start after the end) is shown beside the filters.
    if (!filtered) throw error;
    setText("[data-report-filter-message]", error.message);
    return;
  }
  setText("[data-report-filter-message]", "");
  const data = await response.json();
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

// ---- The Dashboard: headline figures, audits by outlet over time, and a card per outlet ----

const monthNames = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
// "2026-03" → "Mar 2026".
function monthLabel(month) {
  const [year, number] = String(month).split("-");
  return `${monthNames[Number(number) - 1] || number} ${year}`;
}

// One colour per outlet, spread round the colour wheel so neighbours differ.
function outletColour(index, count) {
  return `hsl(${Math.round((index * 360) / Math.max(1, count) + 350) % 360} 62% 52%)`;
}

// Green from 75%, orange from 50%, red below.
function rateClass(rate) {
  return rate >= 75 ? "rate-good" : rate >= 50 ? "rate-fair" : "rate-poor";
}

// A smooth line through points that never overshoots them, so stacked layers never cross.
function smoothPath(points) {
  return points.map(([x, y], index) => {
    if (!index) return `M${x.toFixed(1)},${y.toFixed(1)}`;
    const [px, py] = points[index - 1], mid = ((px + x) / 2).toFixed(1);
    return `C${mid},${py.toFixed(1)} ${mid},${y.toFixed(1)} ${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(" ");
}

// A round top for the scale: 7 → 8, 23 → 25, 112 → 120.
function niceMax(value) {
  if (value <= 4) return 4;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  const step = [1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10].find((factor) => factor * magnitude >= value);
  return step * magnitude;
}

// Areas stacked over months: series = [{ label, colour, values: [per month] }], the first at the bottom.
function stackedAreaChart(months, series) {
  if (!months.length || !series.some((item) => item.values.some(Boolean))) return '<p class="muted chart-empty">No audits in this period</p>';
  const width = 1000, height = 240, left = 34, right = 12, top = 10, bottom = 26;
  const plotWidth = width - left - right, plotHeight = height - top - bottom;
  const totals = months.map((_, index) => series.reduce((sum, item) => sum + (item.values[index] || 0), 0));
  const max = niceMax(Math.max(...totals));
  const x = (index) => left + (months.length === 1 ? plotWidth / 2 : index * plotWidth / (months.length - 1));
  const y = (value) => top + plotHeight - value * plotHeight / max;
  const grid = [0, 1, 2, 3, 4].map((step) => max * step / 4).map((tick) => `<line class="chart-grid-line" x1="${left}" x2="${width - right}" y1="${y(tick).toFixed(1)}" y2="${y(tick).toFixed(1)}"></line>
    <text class="chart-axis" x="${left - 6}" y="${(y(tick) + 4).toFixed(1)}" text-anchor="end">${Math.round(tick * 10) / 10}</text>`).join("");
  const every = Math.max(1, Math.ceil(months.length / 10));
  const axis = months.map((month, index) => index % every ? "" : `<text class="chart-axis" x="${x(index).toFixed(1)}" y="${height - 6}" text-anchor="${index === 0 && months.length > 1 ? "start" : index === months.length - 1 && months.length > 1 ? "end" : "middle"}">${escapeHtml(monthLabel(month))}</text>`).join("");
  let base = months.map(() => 0);
  const layers = series.map((item) => {
    const lower = base.map((value, index) => [x(index), y(value)]);
    base = base.map((value, index) => value + (item.values[index] || 0));
    const upper = base.map((value, index) => [x(index), y(value)]);
    const line = smoothPath(upper);
    const back = smoothPath([...lower].reverse()).replace(/^M/, "L");
    const total = item.values.reduce((sum, value) => sum + value, 0);
    return `<g class="area-layer"><title>${escapeHtml(item.label)}: ${total} audit${total === 1 ? "" : "s"}</title>
      <path d="${line} ${back} Z" style="fill:${item.colour}"></path><path class="area-line" d="${line}" style="stroke:${item.colour}"></path></g>`;
  }).join("");
  // An invisible column per month, so hovering anywhere shows that month's total.
  const columns = months.map((month, index) => {
    const half = months.length === 1 ? plotWidth / 2 : plotWidth / (months.length - 1) / 2;
    return `<rect class="area-hover" x="${Math.max(left, x(index) - half).toFixed(1)}" y="${top}" width="${(Math.min(width - right, x(index) + half) - Math.max(left, x(index) - half)).toFixed(1)}" height="${plotHeight}"><title>${escapeHtml(monthLabel(month))}: ${totals[index]} audit${totals[index] === 1 ? "" : "s"}</title></rect>`;
  }).join("");
  return `<div class="chart-scroll"><svg class="area-chart" viewBox="0 0 ${width} ${height}" role="img" aria-label="Audits per month by outlet">${grid}${layers}${columns}${axis}</svg></div>
    <ul class="dash-legend">${series.map((item) => `<li><i style="background:${item.colour}"></i>${escapeHtml(item.label)}</li>`).join("")}</ul>`;
}

// One outlet's audits per month as a small filled curve.
function sparkline(values) {
  const width = 300, height = 56, pad = 3;
  if (!values.length || !values.some(Boolean)) return `<svg class="dash-spark" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none" aria-hidden="true"><line x1="0" x2="${width}" y1="${height - pad}" y2="${height - pad}"></line></svg>`;
  const max = Math.max(...values);
  const x = (index) => values.length === 1 ? width / 2 : index * width / (values.length - 1);
  const y = (value) => height - pad - value * (height - pad * 2) / max;
  const line = smoothPath(values.map((value, index) => [x(index), y(value)]));
  return `<svg class="dash-spark" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none" aria-hidden="true">
    <path class="dash-spark-fill" d="${line} L${x(values.length - 1).toFixed(1)},${height} L${x(0).toFixed(1)},${height} Z"></path><path class="dash-spark-line" d="${line}"></path></svg>`;
}

function outletCard(row) {
  const pending = (row.inProgress || 0) + (row.notStarted || 0);
  const share = (value) => row.total ? value * 100 / row.total : 0;
  const parts = [["completed", row.completed || 0, "Completed"], ["progress", row.inProgress || 0, "In progress"], ["unstarted", row.notStarted || 0, "Not started"]];
  return `<article class="panel dash-outlet">
    <h3>${escapeHtml(row.outlet)}</h3>
    <p class="dash-outlet-meta"><span>${row.total} audit${row.total === 1 ? "" : "s"}</span><b class="${rateClass(row.completionRate || 0)}">${row.completionRate || 0}% completed</b></p>
    ${sparkline(row.monthly || [])}
    <div class="dash-status" role="img" aria-label="${parts.map(([, value, label]) => `${label}: ${value}`).join(", ")}">${parts.map(([key, value, label]) => value
      ? `<span class="dash-status-${key}" style="width:${share(value)}%" title="${label}: ${value}"></span>` : "").join("")}</div>
    <dl>
      <div><dt>Pending</dt><dd>${pending}</dd></div>
      <div><dt>Completed</dt><dd>${row.completed || 0}</dd></div>
      <div><dt>Avg score</dt><dd>${row.average == null ? "—" : `${row.average}/100`}</dd></div>
      <div><dt>Open issues</dt><dd>${row.openFindings || 0}</dd></div>
    </dl>
  </article>`;
}

function renderMainDashboard(data) {
  const stats = data.stats || {};
  for (const key of ["total", "auditsCompleted", "priorityIssues", "nonPriorityIssues", "outstandingFindings", "closedWorkOrders"]) {
    setText(`[data-dashboard="${key}"]`, stats[key] ?? 0);
  }
  setText('[data-dashboard="overallAuditScore"]', stats.overallAuditScore == null ? "No completed audits" : `${stats.overallAuditScore}/100`);
  const activity = data.outletActivity || { months: [], outlets: [] };
  const outlets = activity.outlets || [];
  const total = stats.total || 0, completed = stats.auditsCompleted || 0;
  const rate = total ? Math.round(completed * 100 / total) : 0;
  setText('[data-dashboard-note="total"]', `across ${outlets.length} outlet${outlets.length === 1 ? "" : "s"}`);
  setText('[data-dashboard-note="auditsCompleted"]', `${stats.auditsPending || 0} still pending`);
  setHtml('[data-dashboard="completionRate"]', total ? `<span class="${rateClass(rate)}">${rate}%</span>` : "—");
  renderOutletActivity(activity, "[data-dashboard-area]", "[data-dashboard-outlets]");
}

// Audits by outlet over the months, and a card per outlet; shared by the Dashboard and Reports.
function renderOutletActivity(activity, areaSelector, cardsSelector) {
  const outlets = (activity || {}).outlets || [];
  setHtml(areaSelector, stackedAreaChart((activity || {}).months || [], outlets.map((row, index) => ({
    label: row.outlet, colour: outletColour(index, outlets.length), values: row.monthly || [] }))));
  setHtml(cardsSelector, outlets.length ? outlets.map(outletCard).join("")
    : '<article class="panel dash-outlet"><h3>No audits yet</h3><p class="muted">Each outlet appears here once it has an audit in this period.</p></article>');
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
