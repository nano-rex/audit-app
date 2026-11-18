document.querySelectorAll("[data-tab]").forEach((button) => {
  button.addEventListener("click", () => {
    openTab(button.dataset.tab);
    closeNavigationMenu();
  });
});

// A line icon for each page in the sidebar, drawn with currentColor so it follows the theme.
const NAV_ICONS = {
  today: "M4 13h6V4H4zM14 20h6v-9h-6zM14 4v4h6V4zM4 20h6v-3H4z",
  inspections: "M9 4h6v3H9zM7 5H5v15h14V5h-2M8.5 13l2.5 2.5 4.5-5",
  "work-orders": "M14.5 5.5a4 4 0 0 0-5.2 5.2L4 16l4 4 5.3-5.3a4 4 0 0 0 5.2-5.2l-2.6 2.6-2.5-.6-.6-2.5z",
  notifications: "M6 16V11a6 6 0 0 1 12 0v5l2 2H4zM10 20a2 2 0 0 0 4 0",
  approvals: "M12 3l7 3v5c0 4.5-3 8-7 10-4-2-7-5.5-7-10V6zM8.5 12l2.5 2.5 4.5-5",
  categories: "M12 3l8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9",
  outlets: "M4 20V9l8-5 8 5v11M9 20v-6h6v6M3 20h18",
  users: "M9 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM3 20c0-3.3 2.7-6 6-6s6 2.7 6 6M16 4.5a3.5 3.5 0 0 1 0 6.5M18 14c2 .8 3 3 3 6",
  settings: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19 12l2-1.5-2-3.5-2.4.8a7 7 0 0 0-1.6-.9L14.5 4h-5L9 6.9a7 7 0 0 0-1.6.9L5 7 3 10.5 5 12l-2 1.5L5 17l2.4-.8a7 7 0 0 0 1.6.9l.5 2.9h5l.5-2.9a7 7 0 0 0 1.6-.9L19 17l2-3.5z",
  account: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21c0-4.4 3.6-8 8-8s8 3.6 8 8",
  "super-dashboard": "M4 20V10M10 20V4M16 20v-7M22 20H2",
  "super-settings": "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0M16 4v4M10 10v4M18 16v4",
};

document.querySelectorAll(".tabs [data-tab]").forEach((button) => {
  const path = NAV_ICONS[button.dataset.tab];
  if (!path || button.querySelector(".nav-icon")) return;
  const label = document.createElement("span");
  label.className = "nav-label";
  label.textContent = button.textContent.trim();
  button.textContent = "";
  button.insertAdjacentHTML("afterbegin", `<svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="${path}"/></svg>`);
  button.appendChild(label);
});

// On a narrow screen the page list is a drawer opened from the menu button.
function setNavigationDrawer(open) {
  document.body.classList.toggle("nav-open", open);
  const button = document.querySelector("[data-menu-toggle]");
  button?.setAttribute("aria-expanded", String(open));
  button?.setAttribute("aria-label", open ? "Close menu" : "Open menu");
  if (open) (document.querySelector(".tabs .active:not([hidden])") || document.querySelector(".tabs [data-tab]:not([hidden])"))?.focus();
}

// Entry from the bar or the page menu: some pages open on their most-used sub-page.
function openTab(tabId) {
  const child = defaultContextChild[tabId];
  if (child && allowedAppTabs().some((tab) => tab.id === child)) showContextTab(child);
  else showTab(tabId);
}

let unreadNotifications = 0;
// What waits in each section (from /api/todo), shown as a number beside its tab.
let attentionCounts = {};

function tabAttention(tabId) {
  const count = (...keys) => keys.reduce((total, key) => total + (Number(attentionCounts[key]) || 0), 0);
  return {
    inspections: count("guided", "signoff", "findings"),
    findings: count("findings"),
    "work-orders": count("requests", "orders"),
    notifications: unreadNotifications,
    users: count("resets"),
    approvals: count("approvals"),
  }[tabId] || 0;
}

// Subtabs that lead to the waiting work: [selector, count key].
const SUBTAB_ATTENTION = [
  ['[data-context-tab="inspections"]', "guided"],
  ['[data-context-tab="signoff"]', "signoff"],
  ['[data-context-tab="findings"]', "findings"],
  ['[data-maintenance-subtab="requests"]', "requests"],
  ['[data-maintenance-subtab="orders"]', "orders"],
  ['[data-user-subtab="users"]', "resets"],
];

function setBadge(host, count, label) {
  if (!host) return;
  let badge = host.querySelector(":scope > .tab-badge");
  if (!badge) {
    badge = document.createElement("span");
    badge.className = "tab-badge";
    host.appendChild(badge);
  }
  badge.hidden = !count;
  badge.textContent = count > 99 ? "99+" : String(count);
  badge.setAttribute("aria-label", count ? `${count} ${label}` : "");
}

function renderUnreadBadge() {
  attentionCounts.notifications = unreadNotifications;
  let total = 0;
  document.querySelectorAll(".tabs [data-tab]").forEach((button) => {
    const count = tabAttention(button.dataset.tab);
    setBadge(button, count, "waiting");
    if (!button.hidden && !button.hasAttribute("data-nested-only")) total += count;
  });
  // The menu button shows only while the page list is a closed drawer; it carries the sum.
  setBadge(document.querySelector("[data-menu-toggle]"), total, "waiting");
  document.querySelectorAll("[data-menu-open-tab]").forEach((button) => setBadge(button, tabAttention(button.dataset.menuOpenTab), "waiting"));
  SUBTAB_ATTENTION.forEach(([selector, key]) => {
    document.querySelectorAll(selector).forEach((button) => setBadge(button, Number(attentionCounts[key]) || 0, "waiting"));
  });
}

document.querySelectorAll("[data-jump-tab]").forEach((button) => {
  button.addEventListener("click", () => showTab(button.dataset.jumpTab));
});

document.querySelectorAll("[data-inspection-subtab]").forEach((button) => {
  button.addEventListener("click", () => showInspectionSubtab(button.dataset.inspectionSubtab));
});

document.querySelectorAll("[data-outlet-subtab]").forEach((button) => {
  button.addEventListener("click", () => showOutletSubtab(button.dataset.outletSubtab));
});

let activeTabId = null;
let activeContextTab = null;

function showTab(tabId) {
  if (typeof restoreOrgTheme === "function" && activeTabId === "super-settings" && tabId !== "super-settings") restoreOrgTheme();
  const userSection = ["departments", "roles"].includes(tabId) ? tabId : null;
  if (userSection) tabId = "users";
  const allowedTabs = allowedAppTabs();
  if (!allowedTabs.some((tab) => tab.id === tabId)) {
    tabId = allowedTabs[0]?.id || defaultNavbarTabs[0];
  }
  const panelId = tabId;
  activeTabId = tabId;
  activeContextTab = null;
  // A contextual child page keeps its parent selected on the bar.
  const barTabId = contextParents[tabId] || tabId;
  document.querySelectorAll("[data-tab]").forEach((tab) => {
    tab.classList.toggle("active", tab.dataset.tab === barTabId);
    if (tab.dataset.tab === barTabId) tab.setAttribute("aria-current", "page");
    else tab.removeAttribute("aria-current");
  });
  const barTab = [...allTabs, ...superTabs].find((tab) => tab.id === barTabId);
  setText("[data-page-title]", barTab?.label || "");
  document.querySelectorAll("[data-context-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.contextTab === panelId);
  });
  document.querySelectorAll(".tab-panel").forEach((panel) => {
    const active = panel.id === panelId || contextParents[panelId] === panel.id;
    panel.classList.toggle("active", active);
    panel.classList.toggle("context-child-active", contextParents[panelId] === panel.id);
    panel.hidden = !active;
  });
  if (panelId === "users") showUserSubtab(userSection || activeUserSection);
  if (panelId === "inspections") showGuidedContent(false);
  // The register opens on fixed assets; the Variable Assets tab switches it afterwards.
  if (panelId === "equipment") setEquipmentKind("asset");
  if (panelId === "inspections" && pendingInspectionSchedule) {
    const row = pendingInspectionSchedule;
    pendingInspectionSchedule = null;
    applyInspectionSchedule(row).catch(showLoadError);
  }
  layoutNavbar();
  loadTabData(tabId);
}

function showContextTab(tabId) {
  if (tabId === "fixtures") {
    showTab("equipment");
    if (activeTabId !== "equipment") return;
    setEquipmentKind("fixture");
  } else if (tabId === "signoff") {
    showTab("inspections");
    if (activeTabId !== "inspections") return;
    showInspectionSubtab("signoff");
    loadSignoff().catch(showLoadError);
  } else {
    showTab(tabId);
    if (activeTabId !== tabId) return;
    if (tabId === "inspections") showInspectionSubtab("guided");
  }
  activeContextTab = tabId;
  document.querySelectorAll(`[data-context-tab]`).forEach((button) => {
    button.classList.toggle("active", button.dataset.contextTab === tabId);
  });
}

Object.entries(contextParents).forEach(([child, parent]) => {
  const panel = document.getElementById(child);
  const host = document.getElementById(parent);
  if (panel && host) host.appendChild(panel);
});

document.querySelectorAll("[data-context-tab]").forEach((button) => {
  button.addEventListener("click", () => showContextTab(button.dataset.contextTab));
});

function showOutletSubtab(tabId) {
  document.querySelectorAll("[data-outlet-subtab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.outletSubtab === tabId);
  });
  document.querySelectorAll("[data-outlet-panel]").forEach((panel) => {
    panel.classList.toggle("active", panel.dataset.outletPanel === tabId);
  });
}

let navigationSaving = false;

function orderedAppTabs() {
  const available = allowedAppTabs().filter((tab) => !contextParents[tab.id]);
  const order = currentUser?.navigationOrder?.length ? currentUser.navigationOrder : defaultNavbarTabs;
  const ids = [...new Set([...order, ...available.map((tab) => tab.id)])];
  return ids.map((id) => available.find((tab) => tab.id === id)).filter(Boolean);
}

// The sidebar (or drawer) lists every page the account may open, in the account's order.
function layoutNavbar() {
  const nav = document.querySelector(".tabs");
  if (!nav) return;
  const buttons = orderedAppTabs().map((tab) => nav.querySelector(`[data-tab="${tab.id}"]`)).filter(Boolean);
  nav.querySelectorAll("[data-tab]").forEach((button) => { button.hidden = true; });
  for (const button of buttons) {
    nav.appendChild(button);
    button.hidden = false;
  }
  renderUnreadBadge();
}

function renderTabMenu() {
  const container = document.querySelector("[data-menu-tabs]");
  if (!container) return;
  const tabs = orderedAppTabs();
  container.innerHTML = tabs.map((tab, index) => `
    <div class="menu-tab-row">
      <button type="button" class="outline" data-menu-open-tab="${tab.id}">${escapeHtml(tab.label)}${tabAttention(tab.id) ? `<span class="tab-badge">${tabAttention(tab.id) > 99 ? "99+" : tabAttention(tab.id)}</span>` : ""}</button>
      <div class="menu-tab-moves">
        <button type="button" class="outline" data-menu-move="${tab.id}" data-direction="-1" aria-label="Move ${escapeAttr(tab.label)} up" ${navigationSaving || index === 0 ? "disabled" : ""}>↑</button>
        <button type="button" class="outline" data-menu-move="${tab.id}" data-direction="1" aria-label="Move ${escapeAttr(tab.label)} down" ${navigationSaving || index === tabs.length - 1 ? "disabled" : ""}>↓</button>
      </div>
    </div>
  `).join("");
}

async function moveNavigationTab(id, direction) {
  if (navigationSaving || !currentUser || ![-1, 1].includes(direction)) return;
  const order = orderedAppTabs().map((tab) => tab.id);
  const index = order.indexOf(id), target = index + direction;
  if (index < 0 || target < 0 || target >= order.length) return;
  [order[index], order[target]] = [order[target], order[index]];
  const owner = currentUser.id;
  const previous = currentUser.navigationOrder;
  const hiddenPages = (previous || []).filter((page) => !order.includes(page));
  currentUser.navigationOrder = [...order, ...hiddenPages];
  navigationSaving = true;
  applyNavbarTabs();
  setText("[data-navigation-status]", "Saving order…");
  try {
    const data = await requestJson("/api/account/navigation", "PATCH", { order: currentUser.navigationOrder });
    if (currentUser?.id !== owner) return;
    currentUser.navigationOrder = data.navigationOrder;
    setText("[data-navigation-status]", "Order saved to your account.");
  } catch (error) {
    if (currentUser?.id !== owner) return;
    currentUser.navigationOrder = previous;
    setText("[data-navigation-status]", `Order not saved: ${error.message}`);
  } finally {
    navigationSaving = false;
    applyNavbarTabs();
    if (currentUser?.id === owner) {
      const same = document.querySelector(`[data-menu-move="${id}"][data-direction="${direction}"]`);
      const focus = same && !same.disabled ? same : document.querySelector(`[data-menu-open-tab="${id}"]`);
      focus?.focus();
    }
  }
}

function applyNavbarTabs() {
  const allowedIds = new Set(allowedAppTabs().map((tab) => tab.id));
  document.querySelectorAll("[data-tab]").forEach((tab) => {
    tab.hidden = tab.hasAttribute("data-nested-only") || !allowedIds.has(tab.dataset.tab);
  });
  document.querySelectorAll(".tab-panel").forEach((panel) => {
    panel.hidden = !allowedIds.has(panel.id);
  });
  const findingsEnabled = currentUser?.role === "Super" || setupOptions.settings["system.findingsEnabled"] !== false;
  document.querySelectorAll('[data-feature-section="findings"]').forEach((node) => { node.hidden = !findingsEnabled; });
  // Reports, and the location reports in History, need the Reports permission.
  document.querySelectorAll('[data-context-tab="reports"], [data-context-tab="activity"], [data-requires="reports"]').forEach((node) => { node.hidden = !allowedIds.has("reports"); });
  if (typeof applyActionPermissions === "function") applyActionPermissions();
  renderTabMenu();
  layoutNavbar();
}

function closeNavigationMenu() {
  const menu = document.getElementById("tab-menu");
  if (menu) menu.hidden = true;
  document.querySelector("[data-reorder-toggle]")?.setAttribute("aria-expanded", "false");
  if (document.body?.classList.contains("nav-open")) setNavigationDrawer(false);
}

if (typeof window !== "undefined") {
  window.addEventListener("keydown", (event) => {
    const drawer = document.body.classList.contains("nav-open");
    if (event.key === "Escape" && (drawer || !document.getElementById("tab-menu")?.hidden)) {
      closeNavigationMenu();
      if (drawer) document.querySelector("[data-menu-toggle]")?.focus();
    }
  });
  document.querySelector("[data-nav-scrim]")?.addEventListener("click", closeNavigationMenu);
}

function allowedAppTabs() {
  const isSuper = currentUser?.role === "Super";
  const findingsEnabled = isSuper || setupOptions.settings["system.findingsEnabled"] !== false;
  const permissions = currentUser?.permissions || allTabs.map((tab) => tab.id);
  const allowedIds = new Set(permissions);
  allowedIds.add("account");
  allowedIds.add("notifications");
  // Approvals is for anyone who may change or approve managed records.
  if (isSuper || (currentUser?.actions || []).length) allowedIds.add("approvals");
  allowedIds.add("settings");
  // Inspection users see their audits' History, and Findings while that feature is on.
  if (allowedIds.has("inspections")) allowedIds.add("history");
  if (allowedIds.has("inspections") && findingsEnabled) allowedIds.add("findings");
  if (["users", "departments", "roles"].some((id) => allowedIds.has(id))) allowedIds.add("users");
  if (allowedIds.has("reports")) {
    allowedIds.add("today");
    // The Activity Log sits beside Reports and needs the same permission.
    allowedIds.add("activity");
  }
  if (allowedIds.has("equipment")) allowedIds.add("categories");
  // History and Findings are sub-pages of Inspections, so their users need that page to reach them.
  if (allowedIds.has("history") || (allowedIds.has("findings") && findingsEnabled)) allowedIds.add("inspections");
  if (!findingsEnabled) allowedIds.delete("findings");
  const regular = allTabs.filter((tab) => !["departments", "roles"].includes(tab.id) && allowedIds.has(tab.id));
  return isSuper ? [...regular, ...superTabs] : regular;
}

let activeUserSection = "users";

function showUserSubtab(sectionId) {
  const permissions = currentUser?.permissions || allTabs.map((tab) => tab.id);
  const allowed = ["users", "departments", "roles"].filter((id) => permissions.includes(id));
  // The organization tree is for those who manage people or roles.
  if (allowed.includes("users") || allowed.includes("roles")) allowed.push("organization");
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
  if (activeUserSection === "organization") renderOrganization();
}

document.querySelectorAll("[data-user-subtab]").forEach((button) => {
  button.addEventListener("click", () => showUserSubtab(button.dataset.userSubtab));
});
