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
  return allTabs.filter((tab) => allowedIds.has(tab.id));
}
