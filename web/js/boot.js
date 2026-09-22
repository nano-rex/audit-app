let appReady = false;
let appLoading = null;
let inspectionsInitialized = false;
const tabLoads = new Map();

function showTabLoading(tabId) {
  const targetTabId = superTabTargets[tabId] || tabId;
  const targets = {
    today: [["[data-outlets]", "Loading dashboard…"], ["[data-recent]", "Loading recent audits…"], ["[data-rankings]", "Loading rankings…"], ["[data-today-schedules]", "Loading scheduled work…"], ["[data-bars]", "Loading scores…"], ["[data-dashboard-charts]", "Loading charts…"]],
    reports: [["[data-report-charts]", "Loading report…"], ["[data-rankings]", "Loading report…"], ["[data-bars]", "Loading report…"]],
    findings: [["[data-inspection-history]", "Loading history…"], ["[data-findings]", "Loading findings…"]],
    "work-orders": [["[data-work-orders]", "Loading work orders…"]],
    "corrective-actions": [["[data-corrective-actions]", "Loading corrective actions…"]],
    equipment: [["[data-equipment]", "Loading fixed assets…"]],
    users: [["[data-users]", "Loading users…"], ["[data-department-records]", "Loading departments…"], ["[data-role-records]", "Loading roles…"]],
    notifications: [["[data-notifications]", "Loading notifications…"]],
    outlets: [["[data-location-records]", "Loading locations…"], ["[data-zone-records]", "Loading zones…"], ["[data-outlet-records]", "Loading outlets…"]],
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

async function loadTabData(tabId) {
  if (!appReady) return;
  if (tabLoads.has(tabId)) return tabLoads.get(tabId);
  showTabLoading(tabId);
  const loaders = {
    today: loadDashboard,
    reports: async () => { await loadDashboard(); await loadReport(); },
    findings: () => Promise.all([loadInspectionHistory(), loadFindings()]),
    "work-orders": loadWorkOrders,
    "corrective-actions": loadWorkOrders,
    equipment: loadEquipment,
    users: () => (currentUser?.permissions || ["users"]).includes("users") ? loadUsers() : Promise.resolve(),
    account: loadAccount,
    notifications: loadNotifications,
    outlets: () => Promise.all([loadLocations(), loadZones()]),
    inspections: async () => {
      await Promise.all([loadGuidedSchedules(), loadInspectionHistory()]);
    },
  };
  if (tabId.startsWith("super-")) {
    const target = superTabTargets[tabId];
    if (target === "dashboard") loaders[tabId] = loadSuperDashboard;
    else if (target === "settings") loaders[tabId] = loadSuperSettings;
    else loaders[tabId] = loaders[target];
  }
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
  const activeTab = document.querySelector(".tab-panel.active")?.id;
  await Promise.all([loadBranding(), loadSetup()]);
  applyNavbarTabs();
  updateSetupSelects();
  setInspectionSignatures(inspectionSignatures());
  appReady = true;
  const tab = allowedAppTabs().some((item) => item.id === activeTab) ? activeTab : allowedAppTabs()[0]?.id || "today";
  showTab(tab);
  await loadTabData(tab);
}

wireAuth();
loadApp();
