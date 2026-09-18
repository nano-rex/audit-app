async function loadLocations() {
  if (!selectedLocationOutlet) {
    setHtml("[data-location-records]", `<section class="admin-group"><ul><li><b>No outlet selected</b><span>Create an outlet first.</span></li></ul></section>`);
    return;
  }
  const response = await authFetch(`/api/locations?outlet=${encodeURIComponent(selectedLocationOutlet)}`);
  const data = await response.json();
  locationCache = data.items;
  renderLocations();
}

function renderLocations() {
  const page = paginateList("locations", locationCache, selectedLocationOutlet, renderLocations);
  setHtml("[data-location-records]", (page.items.length
    ? `<section class="admin-group"><ul>${page.items.map(locationRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No locations</b><span>Add a location for this outlet.</span></li></ul></section>`) + page.controls);
}

async function loadZones() {
  if (!selectedZoneOutlet) {
    const response = await authFetch("/api/zones");
    const data = await response.json();
    zoneCache = data.items || [];
    renderZones();
    return;
  }
  const response = await authFetch(`/api/zones?outlet=${encodeURIComponent(selectedZoneOutlet)}`);
  const data = await response.json();
  zoneCache = [
    ...zoneCache.filter((zone) => zone.outlet_code !== selectedZoneOutlet),
    ...data.items,
  ];
  renderZones();
}

function renderZones() {
  const rows = zoneCache.filter((row) => !selectedZoneOutlet || row.outlet_code === selectedZoneOutlet);
  const page = paginateList("zones", rows, selectedZoneOutlet, renderZones);
  const outlets = [...new Set(page.items.map((row) => row.outlet_code))];
  setHtml("[data-zone-records]", (page.items.length
    ? outlets.map((outlet) => `<section class="admin-group"><h3>${escapeHtml(outlet)}</h3><ul>${page.items.filter((row) => row.outlet_code === outlet).map(zoneRow).join("")}</ul></section>`).join("")
    : `<section class="admin-group"><ul><li><b>No zones</b><span>Add a zone for this outlet.</span></li></ul></section>`) + page.controls);
}

async function populateLocationEquipmentSelect(locationName = "") {
  const form = document.getElementById("location-form");
  const response = await authFetch(`/api/equipment?outlet=${encodeURIComponent(selectedLocationOutlet)}`);
  const data = await response.json();
  form.elements.equipmentIds.innerHTML = data.items.map((item) => {
    const selected = (item.location || item.zone || "") === locationName ? " selected" : "";
    const label = item.name || item.asset_id || item.code || `Fixed Asset ${item.id}`;
    return `<option value="${item.id}"${selected}>${escapeHtml(label)}</option>`;
  }).join("");
}

async function populateZoneLocationSelect(selectedLocations = []) {
  const form = document.getElementById("zone-form");
  const response = await authFetch(`/api/locations?outlet=${encodeURIComponent(selectedZoneOutlet)}`);
  const data = await response.json();
  const selected = new Set(selectedLocations);
  const currentZoneId = formValue(form, "zoneId", "");
  const zonesForOutlet = zoneCache.filter((zone) => zone.outlet_code === selectedZoneOutlet);
  const assignedByOtherZone = new Map();
  zonesForOutlet.forEach((zone) => {
    if (String(zone.id) === String(currentZoneId)) return;
    (zone.locations || []).forEach((location) => assignedByOtherZone.set(location, zone.name));
  });
  const container = document.querySelector("[data-zone-location-options]");
  if (!container) return;
  container.innerHTML = data.items.length
    ? data.items.map((location) => {
      const assignedZone = assignedByOtherZone.get(location.name);
      const disabled = Boolean(assignedZone);
      return `
        <label class="zone-location-option ${disabled ? "disabled" : ""}">
          <input type="checkbox" name="zoneLocations" value="${escapeAttr(location.name)}" ${selected.has(location.name) ? "checked" : ""} ${disabled ? "disabled" : ""}>
          <span>${escapeHtml(location.name)}</span>
          ${disabled ? `<small>Used in ${escapeHtml(assignedZone)}</small>` : ""}
        </label>
      `;
    }).join("")
    : `<p class="muted">No locations are set up for this outlet.</p>`;
}
async function loadSetup() {
  const response = await authFetch("/api/setup");
  const data = await response.json();
  setupOptions.departments = data.departments.map((row) => row.code);
  setupOptions.categories = (data.categories || []).filter((row) => row.active).map((row) => row.name);
  setupOptions.outlets = data.outlets.map((row) => row.code);
  setupOptions.zones = data.zones || [];
  setupOptions.roles = (data.roles || []).map((row) => row.name);
  setupOptions.priorities = (data.priorities || []).filter((row) => row.active).map((row) => row.name);
  setupOptions.auditTypes = (data.auditTypes || []).filter((row) => row.active).map((row) => row.name);
  setupOptions.settings = data.settings || {};
  setupOptions.tabs = data.tabs || allTabs;
  departmentCache = data.departments;
  categoryCache = data.categories || [];
  outletCache = data.outlets;
  zoneCache = data.zones || [];
  roleCache = data.roles || [];
  priorityCache = data.priorities || [];
  auditTypeCache = data.auditTypes || [];
  selectedLocationOutlet = selectedLocationOutlet || setupOptions.outlets[0] || "";
  updateUserFilterSelects();
  updateUserRoleSelects();
  renderDepartments();
  renderCategories();
  renderOutlets();
  renderRoles();
  renderPriorities();
  renderAuditTypes();
  populateSettingsForms();
  updateLocationOutletSelect();
  updateZoneOutletSelect();
}

function renderPriorities() {
  setHtml("[data-priority-records]", priorityCache.length
    ? priorityCache.map(priorityRow).join("")
    : `<article><div><b>No priorities</b><span>Add priority wording, classification, and due days.</span></div></article>`);
}

function renderAuditTypes() {
  setHtml("[data-audit-type-records]", auditTypeCache.length
    ? auditTypeCache.map(auditTypeRow).join("")
    : `<article><div><b>No audit types</b><span>Add audit types for new audit records.</span></div></article>`);
}

function populateSettingsForms() {
  const getSetting = (key, fallback = "") => setupOptions.settings[key] ?? fallback;
  const scoring = {
    passMark: getSetting("scoring.passMark", 70),
    weightingMode: getSetting("scoring.weighting", "Equal"),
    ratingBands: {
      excellent: getSetting("scoring.excellentBand", 90),
      good: getSetting("scoring.goodBand", 70),
      needsImprovement: getSetting("scoring.belowBand", 60),
    },
  };
  const report = {
    appTitle: getSetting("report.appTitle", "Ottotree Audit"),
    appSubtitle: getSetting("report.appSubtitle", "Loudspeaker & Mini Studio operations"),
    businessUnitLabel: getSetting("report.businessUnitLabel", "Ottotree"),
    todayHeading: getSetting("report.todayHeading", "inspections for today"),
    reportHeading: getSetting("report.reportHeading", "audit report"),
    loginTitle: getSetting("report.loginTitle", "Ottotree Audit"),
    companyName: getSetting("report.companyName", "Ottotree"),
    departmentHeader: getSetting("report.departmentHeader", "Facilities Department"),
    logoUrl: getSetting("report.logoUrl", ""),
  };
  const system = {
    notificationChannels: ["In-App", getSetting("system.emailEnabled") ? "Email" : "", getSetting("system.whatsappEnabled") ? "WhatsApp" : "", getSetting("system.pushEnabled") ? "Push" : ""].filter(Boolean),
    futureIntegrations: [
      getSetting("system.preventiveMaintenanceEnabled") ? "Preventive Maintenance" : "",
      getSetting("system.cmmsEnabled") ? "CMMS" : "",
      getSetting("system.aiPhotoDetectionEnabled") ? "AI Photo Defect Detection" : "",
      getSetting("system.aiSummaryEnabled") ? "AI Audit Summary" : "",
      getSetting("system.aiRecommendationEnabled") ? "AI Corrective Recommendation" : "",
    ].filter(Boolean),
  };
  const scoringForm = document.getElementById("scoring-settings-form");
  if (scoringForm) {
    scoringForm.elements.passMark.value = scoring.passMark ?? 70;
    const weights = getSetting("scoring.weights", {});
    setHtml("[data-category-weights]", setupOptions.categories.map((category) => `<label>${escapeHtml(category)}<input type="number" min="0.1" max="100" step="0.1" required data-category-weight="${escapeAttr(category)}" value="${Number(weights[category] ?? 1)}"></label>`).join(""));
    scoringForm.elements.weightingMode.value = scoring.weightingMode || "Equal";
    scoringForm.elements.excellentFrom.value = scoring.ratingBands?.excellent ?? 90;
    scoringForm.elements.goodFrom.value = scoring.ratingBands?.good ?? 75;
    scoringForm.elements.needsImprovementFrom.value = scoring.ratingBands?.needsImprovement ?? 60;
  }
  const systemForm = document.getElementById("system-settings-form");
  if (systemForm) {
    systemForm.elements.appTitle.value = report.appTitle;
    systemForm.elements.appSubtitle.value = report.appSubtitle;
    systemForm.elements.businessUnitLabel.value = report.businessUnitLabel;
    systemForm.elements.todayHeading.value = report.todayHeading;
    systemForm.elements.reportHeading.value = report.reportHeading;
    systemForm.elements.loginTitle.value = report.loginTitle;
    systemForm.elements.companyName.value = report.companyName || "Ottotree";
    systemForm.elements.departmentHeader.value = report.departmentHeader || "Facilities Department";
    systemForm.elements.logoUrl.value = report.logoUrl || "";
    systemForm.elements.channels.value = (system.notificationChannels || ["In-App"]).join(", ");
    systemForm.elements.integrations.value = (system.futureIntegrations || []).join(", ");
  }
}

async function loadUsers() {
  const response = await authFetch("/api/users");
  const data = await response.json();
  userCache = data.items;
  renderUsers();
}

function renderDepartments() {
  const search = departmentFilters.search.toLowerCase();
  const rows = departmentCache.filter((row) => [row.code, row.description, row.responsibilities].join(" ").toLowerCase().includes(search));
  setHtml("[data-department-records]", rows.length
    ? `<section class="admin-group"><ul>${rows.map(departmentRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No departments found</b><span>Adjust search or add a department.</span></li></ul></section>`);
}

function renderCategories() {
  const search = categoryFilters.search.toLowerCase();
  const rows = categoryCache.filter((row) => [row.name, row.description, row.sequence, row.active ? "active" : "inactive"].join(" ").toLowerCase().includes(search));
  const page = paginateList("categories", rows, categoryFilters, renderCategories);
  setHtml("[data-category-records]", (rows.length
    ? `<section class="admin-group"><ul>${page.items.map(categoryRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No categories found</b><span>Adjust search or add a category.</span></li></ul></section>`) + page.controls);
}

function renderOutlets() {
  const search = outletFilters.search.toLowerCase();
  const rows = outletCache.filter((row) => [row.code, row.location, row.description].join(" ").toLowerCase().includes(search));
  const page = paginateList("outlets", rows, outletFilters, renderOutlets);
  setHtml("[data-outlet-records]", (rows.length
    ? `<section class="admin-group"><ul>${page.items.map(outletRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No outlets found</b><span>Adjust search or add an outlet.</span></li></ul></section>`) + page.controls);
}

function updateUserFilterSelects() {
  updateSelectOptions(document.getElementById("user-filter-department"), setupOptions.departments, true, "All departments");
  document.getElementById("user-filter-department").value = userFilters.department;
  updateSelectOptions(document.getElementById("user-filter-role"), setupOptions.roles || [], true, "All roles");
  document.getElementById("user-filter-role").value = userFilters.role;
}

function updateUserRoleSelects() {
  document.querySelectorAll('select[name="role"]').forEach((select) => {
    updateSelectOptions(select, setupOptions.roles || ["Super"], true, "Select role");
  });
}

function renderRoles() {
  const search = roleFilters.search.toLowerCase();
  const rows = roleCache.filter((row) => [row.name, row.description, (row.permissions || []).join(" ")].join(" ").toLowerCase().includes(search));
  setHtml("[data-role-records]", rows.length
    ? `<section class="admin-group"><ul>${rows.map(roleRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No roles found</b><span>Add a role to assign non-admin access.</span></li></ul></section>`);
}

function renderUsers() {
  const search = userFilters.search.toLowerCase();
  const rows = userCache.filter((row) => {
    const haystack = [row.name, row.email, row.role, row.department, row.title, row.responsibilities].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!userFilters.role || row.role === userFilters.role)
      && (!userFilters.department || row.department === userFilters.department);
  });
  setHtml("[data-users]", rows.length
    ? rows.map(userRow).join("")
    : `<article><div><b>No users found</b><span>Adjust filters or add a user.</span></div></article>`);
}
