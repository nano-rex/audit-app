let appReady = false;
let appLoading = null;
let inspectionsInitialized = false;
const tabLoads = new Map();

function showTabLoading(tabId) {
  const targetTabId = tabId;
  const targets = {
    today: [["[data-outlets]", "Loading dashboard…"], ["[data-recent]", "Loading recent audits…"], ["[data-rankings]", "Loading rankings…"], ["[data-today-schedules]", "Loading scheduled work…"], ["[data-bars]", "Loading scores…"], ["[data-dashboard-charts]", "Loading charts…"]],
    reports: [["[data-report-charts]", "Loading report…"], ["[data-rankings]", "Loading report…"], ["[data-bars]", "Loading report…"]],
    findings: [["[data-inspection-history]", "Loading history…"], ["[data-findings]", "Loading findings…"]],
    "work-orders": [["[data-work-orders]", "Loading work orders…"]],
    "corrective-actions": [["[data-corrective-actions]", "Loading corrective actions…"]],
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
