document.querySelectorAll("[data-tab]").forEach((button) => {
  button.addEventListener("click", () => {
    openTab(button.dataset.tab);
  });
});

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
  let offBar = 0;
  document.querySelectorAll(".tabs [data-tab]").forEach((button) => {
    const count = tabAttention(button.dataset.tab);
    setBadge(button, count, "waiting");
    // A tab that does not fit on the bar passes its count to the menu button.
    if (button.hidden && !button.hasAttribute("data-nested-only") && allowedAppTabs().some((tab) => tab.id === button.dataset.tab)) offBar += count;
  });
  setBadge(document.querySelector("[data-menu-toggle]"), offBar, "waiting in the menu");
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
  });
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
  // The register opens on fixed assets; the Fixtures & Finishes tab switches it afterwards.
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
  if (["history", "findings"].includes(tabId)) {
    showTab("findings");
    if (activeTabId !== "findings") return;
    showHistoryFindingsSection(tabId);
  } else if (tabId === "fixtures") {
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

function navbarVisibleCount(widths, availableWidth, viewportWidth, gap = 8) {
  const limit = viewportWidth <= 480 ? 2 : widths.length;
  let used = 0, count = 0;
  for (const width of widths.slice(0, limit)) {
    const next = used + (count ? gap : 0) + width;
    if (next > availableWidth && count) break;
    used = next;
    count++;
  }
  return count;
}

function layoutNavbar() {
  const nav = document.querySelector(".tabs");
  if (!nav || !nav.clientWidth) return;
  const buttons = orderedAppTabs().map((tab) => nav.querySelector(`[data-tab="${tab.id}"]`)).filter(Boolean);
  nav.querySelectorAll("[data-tab]").forEach((button) => { button.hidden = true; });
  for (const button of buttons) {
    nav.appendChild(button);
    button.hidden = false;
  }
  const gap = Number.parseFloat(getComputedStyle(nav).columnGap) || 0;
  const count = navbarVisibleCount(buttons.map((button) => button.getBoundingClientRect().width), nav.clientWidth, window.innerWidth, gap);
  buttons.forEach((button, index) => { button.hidden = index >= count; });
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
  document.querySelectorAll('[data-context-tab="reports"], [data-requires="reports"]').forEach((node) => { node.hidden = !allowedIds.has("reports"); });
  renderTabMenu();
  layoutNavbar();
}

function closeNavigationMenu() {
  const menu = document.getElementById("tab-menu");
  if (menu) menu.hidden = true;
  document.querySelector("[data-menu-toggle]")?.setAttribute("aria-expanded", "false");
}

if (typeof window !== "undefined") {
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !document.getElementById("tab-menu")?.hidden) {
      closeNavigationMenu();
      document.querySelector("[data-menu-toggle]")?.focus();
    }
  });
  window.addEventListener("resize", layoutNavbar);
  if (typeof ResizeObserver !== "undefined") {
    const nav = document.querySelector(".tabs");
    if (nav) new ResizeObserver(layoutNavbar).observe(nav);
  }
  document.fonts?.ready.then(layoutNavbar);
}

function allowedAppTabs() {
  const isSuper = currentUser?.role === "Super";
  const findingsEnabled = isSuper || setupOptions.settings["system.findingsEnabled"] !== false;
  const permissions = currentUser?.permissions || allTabs.map((tab) => tab.id);
  const allowedIds = new Set(permissions);
  allowedIds.add("account");
  allowedIds.add("notifications");
  allowedIds.add("settings");
  if (allowedIds.has("inspections") && findingsEnabled) allowedIds.add("findings");
  if (["users", "departments", "roles"].some((id) => allowedIds.has(id))) allowedIds.add("users");
  if (allowedIds.has("reports")) allowedIds.add("today");
  if (allowedIds.has("equipment")) allowedIds.add("categories");
  // Findings is a sub-page of Inspections, so its users need that page to reach it.
  if (allowedIds.has("findings") && findingsEnabled) allowedIds.add("inspections");
  if (!findingsEnabled) allowedIds.delete("findings");
  const regular = allTabs.filter((tab) => !["departments", "roles"].includes(tab.id) && allowedIds.has(tab.id));
  return isSuper ? [...regular, ...superTabs] : regular;
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
