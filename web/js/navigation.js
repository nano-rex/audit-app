document.querySelectorAll("[data-tab]").forEach((button) => {
  button.addEventListener("click", () => {
    showTab(button.dataset.tab);
  });
});

document.querySelectorAll("[data-jump-tab]").forEach((button) => {
  button.addEventListener("click", () => showTab(button.dataset.jumpTab));
});

document.querySelectorAll("[data-inspection-subtab]").forEach((button) => {
  button.addEventListener("click", () => showInspectionSubtab(button.dataset.inspectionSubtab));
});

document.querySelectorAll("[data-outlet-subtab]").forEach((button) => {
  button.addEventListener("click", () => showOutletSubtab(button.dataset.outletSubtab));
});

function showTab(tabId) {
  const userSection = ["departments", "roles"].includes(tabId) ? tabId : null;
  if (userSection) tabId = "users";
  const allowedTabs = allowedAppTabs();
  if (!allowedTabs.some((tab) => tab.id === tabId)) {
    tabId = allowedTabs[0]?.id || defaultNavbarTabs[0];
  }
  document.querySelectorAll("[data-tab]").forEach((tab) => {
    tab.classList.toggle("active", tab.dataset.tab === tabId);
  });
  document.querySelectorAll(".tab-panel").forEach((panel) => {
    panel.classList.toggle("active", panel.id === tabId);
  });
  if (tabId === "users") showUserSubtab(userSection || activeUserSection);
  if (tabId === "inspections") showGuidedContent(false);
  if (tabId === "inspections" && pendingInspectionSchedule) {
    const row = pendingInspectionSchedule;
    pendingInspectionSchedule = null;
    applyInspectionSchedule(row).catch(showLoadError);
  }
  loadTabData(tabId);
}

function showOutletSubtab(tabId) {
  document.querySelectorAll("[data-outlet-subtab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.outletSubtab === tabId);
  });
  document.querySelectorAll("[data-outlet-panel]").forEach((panel) => {
    panel.classList.toggle("active", panel.dataset.outletPanel === tabId);
  });
}

function renderTabMenu() {
  const container = document.querySelector("[data-menu-tabs]");
  if (!container) return;
  container.innerHTML = allowedAppTabs().map((tab) => `
    <div class="menu-tab-row">
      <button type="button" class="outline" data-menu-open-tab="${tab.id}">${escapeHtml(tab.label)}</button>
      <label>
        <input type="checkbox" data-navbar-tab-toggle="${tab.id}" ${navbarTabs.includes(tab.id) ? "checked" : ""}>
        Navbar
      </label>
    </div>
  `).join("");
}

function applyNavbarTabs() {
  const allowedIds = new Set(allowedAppTabs().map((tab) => tab.id));
  document.querySelectorAll("[data-tab]").forEach((tab) => {
    tab.hidden = !allowedIds.has(tab.dataset.tab) || !navbarTabs.includes(tab.dataset.tab);
  });
  document.querySelectorAll(".tab-panel").forEach((panel) => {
    panel.hidden = !allowedIds.has(panel.id);
  });
  renderTabMenu();
}

function allowedAppTabs() {
  const permissions = currentUser?.permissions || allTabs.map((tab) => tab.id);
  const allowedIds = new Set(permissions);
  allowedIds.add("account");
  allowedIds.add("notifications");
  if (allowedIds.has("inspections")) allowedIds.add("findings");
  if (["users", "departments", "roles"].some((id) => allowedIds.has(id))) allowedIds.add("users");
  return allTabs.filter((tab) => !["departments", "roles"].includes(tab.id) && allowedIds.has(tab.id));
}

let activeUserSection = "users";

function showUserSubtab(sectionId) {
  const permissions = currentUser?.permissions || allTabs.map((tab) => tab.id);
  const allowed = ["users", "departments", "roles"].filter((id) => permissions.includes(id));
  activeUserSection = allowed.includes(sectionId) ? sectionId : allowed[0];
  document.querySelectorAll("[data-user-subtab]").forEach((button) => {
    button.hidden = !allowed.includes(button.dataset.userSubtab);
    const active = button.dataset.userSubtab === activeUserSection;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", String(active));
  });
  document.querySelectorAll("[data-user-panel]").forEach((panel) => {
    const active = panel.dataset.userPanel === activeUserSection;
    panel.hidden = !active;
    panel.classList.toggle("active", active);
  });
}

document.querySelectorAll("[data-user-subtab]").forEach((button) => {
  button.addEventListener("click", () => showUserSubtab(button.dataset.userSubtab));
});

function showHistoryFindingsSection(name) {
  document.querySelectorAll("[data-history-findings-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.historyFindingsTab === name);
    button.setAttribute("aria-pressed", String(button.dataset.historyFindingsTab === name));
  });
  document.querySelectorAll("[data-history-findings-panel]").forEach((panel) => { panel.hidden = panel.dataset.historyFindingsPanel !== name; });
}

document.querySelectorAll("[data-history-findings-tab]").forEach((button) => {
  button.addEventListener("click", () => showHistoryFindingsSection(button.dataset.historyFindingsTab));
});
