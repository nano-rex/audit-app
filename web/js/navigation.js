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
  const resolvedTabId = superTabTargets[tabId] || tabId;
  const userSection = ["departments", "roles"].includes(resolvedTabId) ? resolvedTabId : null;
  if (userSection) tabId = "users";
  const allowedTabs = allowedAppTabs();
  if (!allowedTabs.some((tab) => tab.id === tabId)) {
    tabId = allowedTabs[0]?.id || defaultNavbarTabs[0];
  }
  const panelId = superTabTargets[tabId] || tabId;
  document.querySelectorAll("[data-tab]").forEach((tab) => {
    tab.classList.toggle("active", tab.dataset.tab === tabId);
  });
  document.querySelectorAll(".tab-panel").forEach((panel) => {
    panel.classList.toggle("active", panel.id === panelId);
  });
  if (panelId === "users") showUserSubtab(userSection || activeUserSection);
  if (panelId === "inspections") showGuidedContent(false);
  if (panelId === "inspections" && pendingInspectionSchedule) {
    const row = pendingInspectionSchedule;
    pendingInspectionSchedule = null;
    applyInspectionSchedule(row).catch(showLoadError);
  }
  layoutNavbar();
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

let navigationSaving = false;

function orderedAppTabs() {
  const available = allowedAppTabs();
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
}

function renderTabMenu() {
  const container = document.querySelector("[data-menu-tabs]");
  if (!container) return;
  const tabs = orderedAppTabs();
  container.innerHTML = tabs.map((tab, index) => `
    <div class="menu-tab-row">
      <button type="button" class="outline" data-menu-open-tab="${tab.id}">${escapeHtml(tab.label)}</button>
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
    tab.hidden = !allowedIds.has(tab.dataset.tab);
  });
  document.querySelectorAll(".tab-panel").forEach((panel) => {
    panel.hidden = !allowedIds.has(panel.id) && ![...allowedIds].some((id) => superTabTargets[id] === panel.id);
  });
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
  if (currentUser?.role === "Super") return superTabs;
  const permissions = currentUser?.permissions || allTabs.map((tab) => tab.id);
  const allowedIds = new Set(permissions);
  allowedIds.add("account");
  allowedIds.add("notifications");
  if (allowedIds.has("inspections")) allowedIds.add("findings");
  if (["users", "departments", "roles"].some((id) => allowedIds.has(id))) allowedIds.add("users");
  const regular = allTabs.filter((tab) => !["departments", "roles"].includes(tab.id) && allowedIds.has(tab.id));
  return currentUser?.role === "Super" ? [...superTabs, ...regular] : regular;
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
