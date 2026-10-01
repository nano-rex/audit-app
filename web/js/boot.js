let appReady = false;
let appLoading = null;
let inspectionsInitialized = false;
const tabLoads = new Map();

function showTabLoading(tabId) {
  const targetTabId = tabId;
  const targets = {
    today: [["[data-today-schedules]", "Loading scheduled work…"], ["[data-bars]", "Loading scores…"], ["[data-dashboard-charts]", "Loading charts…"]],
    reports: [["[data-report-charts]", "Loading report…"], ["[data-rankings]", "Loading report…"], ["[data-bars]", "Loading report…"]],
    findings: [["[data-inspection-history]", "Loading history…"], ["[data-findings]", "Loading findings…"]],
    "work-orders": [["[data-work-orders]", "Loading work orders…"]],
    equipment: [["[data-equipment]", "Loading fixed assets…"]],
    categories: [["[data-category-records]", "Loading categories…"]],
    // Departments and roles are loaded with the initial setup catalog. Keep
    // those cached lists visible while the user list refreshes.
    users: [["[data-users]", "Loading users…"]],
    notifications: [["[data-notifications]", "Loading notifications…"]],
    outlets: [["[data-location-records]", "Loading locations…"], ["[data-zone-records]", "Loading zones…"]],
    inspections: [["[data-guided-schedules]", "Loading scheduled inspections…"], ["[data-inspection-history]", "Loading inspection history…"]],
  };
  (targets[targetTabId] || []).forEach(([selector, label]) => setLoading(selector, label));
}

function showLoadError(error) {
  let notice = document.getElementById("load-error");
  if (!notice) {
    notice = document.createElement("p");
    notice.id = "load-error";
    notice.setAttribute("role", "alert");
    document.body.prepend(notice);
  }
  notice.textContent = `Could not load data: ${error.message}. Select the tab again to retry, or reload the page.`;
}

// An action that fails must say so. Forms without their own message area report here:
// inside the open dialog when there is one, otherwise in the banner at the top of the page.
function showActionError(error) {
  const text = error?.message || String(error || "") || "The action could not be completed.";
  const dialog = document.querySelector("dialog[open]");
  if (!dialog) {
    let notice = document.getElementById("load-error");
    if (!notice) {
      notice = document.createElement("p");
      notice.id = "load-error";
      notice.setAttribute("role", "alert");
      document.body.prepend(notice);
    }
    notice.textContent = `That could not be completed: ${text}`;
    return;
  }
  let note = dialog.querySelector("[data-action-error]");
  if (!note) {
    note = document.createElement("p");
    note.className = "form-message";
    note.dataset.actionError = "";
    note.setAttribute("role", "alert");
    (dialog.querySelector("form") || dialog).append(note);
  }
  note.textContent = text;
  note.scrollIntoView?.({ block: "nearest" });
}

if (typeof window !== "undefined") {
  window.addEventListener("unhandledrejection", (event) => {
    event.preventDefault?.();
    showActionError(event.reason);
  });
  // A message belongs to one attempt; drop it when the dialog closes.
  document.addEventListener("close", (event) => event.target.querySelector?.("[data-action-error]")?.remove(), true);
}

async function loadTabData(tabId) {
  if (!appReady) return;
  if (tabLoads.has(tabId)) return tabLoads.get(tabId);
  showTabLoading(tabId);
  const loaders = {
    today: loadDashboard,
    reports: async () => { await loadDashboard(); await loadReport(); },
    findings: () => Promise.all([loadInspectionHistory(), loadFindings()]),
    "work-orders": loadWorkOrders,
    equipment: loadEquipment,
    categories: async () => {
      if (!categoryCache.length) await loadSetup();
      else renderCategories();
    },
    users: () => (currentUser?.permissions || ["users"]).includes("users") ? loadUsers() : Promise.resolve(),
    account: loadAccount,
    notifications: loadNotifications,
    outlets: async () => {
      // Outlet rows are already part of the initial setup response. Keep them
      // visible while the dependent location and zone lists refresh.
      renderOutlets();
      await Promise.all([loadLocations(), loadZones()]);
    },
    inspections: async () => {
      await Promise.all([loadGuidedSchedules(), loadInspectionHistory()]);
    },
  };
  loaders["super-dashboard"] = loadSuperDashboard;
  loaders["super-settings"] = loadSuperSettings;
  if (!loaders[tabId]) return;
  const pending = Promise.resolve().then(loaders[tabId]).then(() => {
    document.getElementById("load-error")?.remove();
  }).catch(showLoadError).finally(() => tabLoads.delete(tabId));
  tabLoads.set(tabId, pending);
  return pending;
}

function loadApp() {
  if (appLoading) return appLoading;
  appLoading = initializeApp().catch(showLoadError).finally(() => { appLoading = null; });
  return appLoading;
}

async function initializeApp() {
  appReady = false;
  if (!await requireLogin()) return;
  const activeTab = activeTabId || document.querySelector(".tab-panel.active")?.id;
  const contextTab = activeContextTab;
  await Promise.all([loadBranding(), loadSetup()]);
  applyNavbarTabs();
  updateSetupSelects();
  setInspectionSignatures(inspectionSignatures());
  appReady = true;
  const tab = allowedAppTabs().some((item) => item.id === activeTab) ? activeTab : allowedAppTabs()[0]?.id || "today";
  loadAttention().catch(() => {});
  if (contextTab && tab === activeTab) showContextTab(contextTab);
  else showTab(tab);
  await loadTabData(activeTabId || tab);
}

wireAuth();
loadApp();
