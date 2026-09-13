async function loadLocations() {
  if (!selectedLocationOutlet) {
    setHtml("[data-location-records]", `<section class="admin-group"><ul><li><b>No outlet selected</b><span>Create an outlet first.</span></li></ul></section>`);
    return;
  }
  const response = await fetch(`/api/locations?outlet=${encodeURIComponent(selectedLocationOutlet)}`);
  const data = await response.json();
  setHtml("[data-location-records]", data.items.length
    ? `<section class="admin-group"><ul>${data.items.map(locationRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No locations</b><span>Add a location for this outlet.</span></li></ul></section>`);
}

async function loadZones() {
  if (!selectedZoneOutlet) {
    const response = await fetch("/api/zones");
    const data = await response.json();
    zoneCache = data.items || [];
    setHtml("[data-zone-records]", setupOptions.outlets.length
      ? setupOptions.outlets.map((outlet) => {
        const rows = zoneCache.filter((zone) => zone.outlet_code === outlet);
        return `<section class="admin-group"><h3>${escapeHtml(outlet)}</h3><ul>${rows.length
          ? rows.map(zoneRow).join("")
          : `<li><b>No zones</b><span>Add a zone for this outlet.</span></li>`}</ul></section>`;
      }).join("")
      : `<section class="admin-group"><ul><li><b>No outlets</b><span>Create an outlet first.</span></li></ul></section>`);
    return;
  }
  const response = await fetch(`/api/zones?outlet=${encodeURIComponent(selectedZoneOutlet)}`);
  const data = await response.json();
  zoneCache = [
    ...zoneCache.filter((zone) => zone.outlet_code !== selectedZoneOutlet),
    ...data.items,
  ];
  setHtml("[data-zone-records]", data.items.length
    ? `<section class="admin-group"><h3>${escapeHtml(selectedZoneOutlet)}</h3><ul>${data.items.map(zoneRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No zones</b><span>Add a zone for this outlet.</span></li></ul></section>`);
}

async function populateLocationEquipmentSelect(locationName = "") {
  const form = document.getElementById("location-form");
  const response = await fetch(`/api/equipment?outlet=${encodeURIComponent(selectedLocationOutlet)}`);
  const data = await response.json();
  form.elements.equipmentIds.innerHTML = data.items.map((item) => {
    const selected = (item.location || item.zone || "") === locationName ? " selected" : "";
    const label = item.name || item.asset_id || item.code || `Equipment ${item.id}`;
    return `<option value="${item.id}"${selected}>${escapeHtml(label)}</option>`;
  }).join("");
}

async function populateZoneLocationSelect(selectedLocations = []) {
  const form = document.getElementById("zone-form");
  const response = await fetch(`/api/locations?outlet=${encodeURIComponent(selectedZoneOutlet)}`);
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
async function loadAdmin() {
  const response = await fetch("/api/admin");
  const data = await response.json();
  setHtml("[data-admin-records]", Object.entries(data).map(([type, records]) => `
    <section class="admin-group">
      <h3>${escapeHtml(type)}</h3>
      <ul>
        ${records.map((record) => `
          <li>
            <b>${escapeHtml(record.name)}</b>
            <span>${escapeHtml(record.parent || "No parent")} | ${escapeHtml(record.detail || "No detail")}</span>
          </li>
        `).join("")}
      </ul>
    </section>
  `).join(""));
}

async function loadSetup() {
  const response = await fetch("/api/setup");
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
  loadLocations();
  loadZones();
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
    passMark: getSetting("scoring.passMark", 80),
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
    reportHeading: getSetting("report.reportHeading", "monthly audit report"),
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
    scoringForm.elements.passMark.value = scoring.passMark ?? 80;
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

async function loadRoles() {
  const response = await fetch("/api/roles");
  const data = await response.json();
  roleCache = data.items || [];
  setupOptions.roles = roleCache.map((row) => row.name);
  setupOptions.tabs = data.tabs || allTabs;
  updateUserRoleSelects();
  renderRoles();
}

async function loadUsers() {
  const response = await fetch("/api/users");
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
  setHtml("[data-category-records]", rows.length
    ? `<section class="admin-group"><ul>${rows.map(categoryRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No categories found</b><span>Adjust search or add a category.</span></li></ul></section>`);
}

function renderOutlets() {
  const search = outletFilters.search.toLowerCase();
  const rows = outletCache.filter((row) => [row.code, row.location, row.description].join(" ").toLowerCase().includes(search));
  setHtml("[data-outlet-records]", rows.length
    ? `<section class="admin-group"><ul>${rows.map(outletRow).join("")}</ul></section>`
    : `<section class="admin-group"><ul><li><b>No outlets found</b><span>Adjust search or add an outlet.</span></li></ul></section>`);
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
