let appReady = false;
let appLoading = null;
let inspectionsInitialized = false;
const tabLoads = new Map();

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
