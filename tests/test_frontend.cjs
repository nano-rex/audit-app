const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { test } = require("node:test");
const scripts = path.join(__dirname, "../web/js");
const source = (name) => fs.readFileSync(path.join(scripts, name), "utf8");

test("all browser scripts parse", () => {
  for (const name of fs.readdirSync(scripts).filter((name) => name.endsWith(".js"))) {
    new vm.Script(source(name), { filename: name });
  }
});

test("startup loads only the active tab and coalesces duplicate requests", async () => {
  const calls = [];
  const document = {
    querySelector: () => ({ id: "today" }),
    getElementById: () => null,
  };
  const context = vm.createContext({ document, currentUser: {}, Promise, Map });
  for (const name of ["loadBranding", "loadSetup", "loadDashboard", "loadReport", "loadFindings", "loadWorkOrders", "loadEquipment", "loadUsers", "loadAccount", "loadNotifications", "loadLocations", "loadZones", "loadChecklist", "restoreLastInspectionSession", "loadInspectionHistory", "loadGuidedSchedules"]) {
    context[name] = async () => { calls.push(name); };
  }
  Object.assign(context, {
    requireLogin: async () => true,
    wireAuth() {}, applyNavbarTabs() {}, updateSetupSelects() {}, setInspectionSignatures() {},
    inspectionSignatures: () => ({}), allowedAppTabs: () => [{ id: "today" }, { id: "equipment" }],
    showTab: (tab) => context.loadTabData(tab),
  });
  vm.runInContext(source("boot.js"), context);
  await context.loadApp();
  assert.deepEqual(calls.sort(), ["loadBranding", "loadDashboard", "loadSetup"].sort());
  let release;
  let requests = 0;
  context.loadEquipment = () => { requests++; return new Promise((resolve) => { release = resolve; }); };
  const first = context.loadTabData("equipment");
  const second = context.loadTabData("equipment");
  await Promise.resolve();
  assert.equal(requests, 1);
  release();
  await Promise.all([first, second]);
  calls.length = 0;
  await context.loadTabData("inspections");
  assert.deepEqual(calls.sort(), ["loadGuidedSchedules", "loadInspectionHistory"].sort());
});

test("equipment renders at most 100 rows and resets pagination on filtering", () => {
  let rendered = "";
  const context = vm.createContext({
    equipmentCache: Array.from({ length: 2330 }, (_, id) => ({ id, name: `Asset ${id}`, outlet: id < 3 ? "Small" : "Large" })),
    equipmentFilters: { search: "", outlet: "", location: "", type: "", brand: "" },
    equipmentPage: 1, equipmentPageSize: 100, equipmentFilterKey: "",
    setHtml: (_, html) => { rendered = html; },
    equipmentRow: () => "<article></article>",
    document: { getElementById: () => null },
  });
  vm.runInContext(source("dashboard.js"), context);
  context.renderEquipment();
  assert.equal((rendered.match(/<article>/g) || []).length, 100);
  context.equipmentPage = 24;
  context.renderEquipment();
  assert.equal((rendered.match(/<article>/g) || []).length, 30);
  context.equipmentFilters.outlet = "Small";
  context.renderEquipment();
  assert.equal(context.equipmentPage, 1);
  assert.equal((rendered.match(/<article>/g) || []).length, 3);
});

test("inspection dates use the device's local day", () => {
  const priorTimezone = process.env.TZ;
  process.env.TZ = "Asia/Kuala_Lumpur";
  try {
    class FixedDate extends Date {
      constructor() { super("2026-09-13T17:00:00Z"); }
    }
    const context = vm.createContext({ Date: FixedDate });
    vm.runInContext(source("utils.js"), context);
    assert.equal(context.todayIsoDate(), "2026-09-14");
  } finally {
    if (priorTimezone === undefined) delete process.env.TZ;
    else process.env.TZ = priorTimezone;
  }
});

test("user management groups departments and roles without widening permissions", () => {
  const makeNode = (dataset) => ({ dataset, hidden: false, attributes: {},
    classList: { toggle() {} }, addEventListener() {},
    setAttribute(key, value) { this.attributes[key] = value; },
  });
  const sections = ["users", "departments", "roles"];
  const buttons = sections.map((id) => makeNode({ userSubtab: id }));
  const panels = sections.map((id) => makeNode({ userPanel: id }));
  const context = vm.createContext({
    currentUser: { permissions: ["departments"] },
    allTabs: ["today", ...sections].map((id) => ({ id, label: id })),
    document: { querySelectorAll(selector) {
      return selector === "[data-user-subtab]" ? buttons : selector === "[data-user-panel]" ? panels : [];
    } },
  });
  vm.runInContext(source("navigation.js"), context);
  assert.equal(JSON.stringify(context.allowedAppTabs().map((tab) => tab.id)), '["users"]');
  context.showUserSubtab("users");
  assert.deepEqual(buttons.map((button) => button.hidden), [true, false, true]);
  assert.deepEqual(panels.map((panel) => panel.hidden), [true, false, true]);
  context.currentUser.permissions = [...sections];
  context.showUserSubtab("roles");
  assert.deepEqual(buttons.map((button) => button.hidden), [false, false, false]);
  assert.deepEqual(panels.map((panel) => panel.hidden), [true, true, false]);
  assert.equal(buttons[2].attributes["aria-pressed"], "true");
  assert.equal(JSON.stringify(context.allowedAppTabs().map((tab) => tab.id)), '["users"]');
});

test("Edit User opens from the real dialog markup and saves existing users", async () => {
  const html = fs.readFileSync(path.join(__dirname, "../web/html/dialogs/user.html"), "utf8");
  const submitMarkup = html.match(/<button[^>]*type="submit"[^>]*>/);
  const elements = Object.fromEntries([...html.matchAll(/name="([^"]+)"/g)].map((match) => [match[1], { value: "", checked: false }]));
  const heading = {}, button = {};
  let opened = false, closed = false, submit;
  const dialog = { showModal() { opened = true; }, close() { closed = true; } };
  const form = { elements, reset() {}, closest: () => dialog,
    querySelector(selector) { return selector === "h2" ? heading : submitMarkup ? button : null; },
    addEventListener(event, handler) { if (event === "submit") submit = handler; },
  };
  const dummy = { addEventListener() {} };
  let request;
  const context = vm.createContext({
    document: { getElementById: (id) => id === "user-form" ? form : id === "user-dialog" ? dialog : dummy, querySelector: () => dummy },
    wireForm() {}, updateSetupSelects() {}, setText() {}, loadApp() {}, showEditorTab() {}, renderUserPermissions() {}, userPermissionOverrides() { return null; },
    formValue: (target, key, fallback) => target.elements[key]?.value || fallback,
    requestJson: async (...args) => { request = args; },
  });
  vm.runInContext(source("forms.js"), context);
  context.openUserEditor({ id: 9, name: "User", email: "user@example.test", role: "Auditor", department: "Technical", active: 1 });
  assert.equal(opened, true);
  assert.equal(heading.textContent, "Edit User");
  assert.equal(elements.name.value, "User");
  elements.name.value = "Updated user";
  await submit({ preventDefault() {}, currentTarget: form });
  assert.equal(request[0], "/api/users/9");
  assert.equal(request[1], "PATCH");
  assert.equal(request[2].name, "Updated user");
  assert.equal(closed, true);
});

test("performance distribution reflects counts and hides empty charts", () => {
  const chart = { style: {}, setAttribute(key, value) { this[key] = value; } };
  const content = {};
  const context = vm.createContext({ document: { querySelector: () => chart },
    setText: (key, value) => { content[key] = value; }, setHtml: (key, value) => { content[key] = value; } });
  vm.runInContext(source("dashboard.js"), context);
  context.renderPerformanceDistribution([]);
  assert.equal(chart.hidden, true);
  assert.equal(content["[data-performance-summary]"], "No completed audits");
  context.renderPerformanceDistribution([{ label: "Good", count: 3 }, { label: "Critical", count: 1 }]);
  assert.equal(chart.hidden, false);
  assert.match(chart.style.background, /var\(--blue\) 0% 75%/);
  assert.match(content["[data-performance-legend]"], /Good: 3 \(75%\)/);
  assert.equal(content["[data-performance-summary]"], "4 completed audits");
  context.renderPerformanceDistribution([]);
  assert.equal(chart.hidden, true);
  assert.equal(chart.style.background, "var(--border)");
});

test("pagination handles navigation, filtering, page sizes, and record deletion", () => {
  const handlers = {};
  const context = vm.createContext({ document: { addEventListener: (type, handler) => { handlers[type] = handler; } } });
  vm.runInContext(source("pagination.js"), context);
  for (const key of ["outlets", "zones", "locations", "findings", "inspections", "categories"]) {
    let rows = Array.from({ length: 61 }, (_, id) => id), filter = "", result;
    const render = () => { result = context.paginateList(key, rows, filter, render); };
    render();
    assert.equal(result.items.length, 25);
    handlers.click({ target: { closest: () => ({ dataset: { listPage: key, page: "3" } }) } });
    assert.equal(result.items[0], 50);
    assert.equal(result.items.length, 11);
    rows = rows.slice(0, 30);
    render();
    assert.equal(result.items[0], 25);
    filter = "new search";
    render();
    assert.equal(result.items[0], 0);
    handlers.change({ target: { dataset: { listSize: key }, value: "10" } });
    assert.equal(result.items.length, 10);
    handlers.change({ target: { dataset: { listJump: key }, value: "999" } });
    assert.equal(result.items[0], 20);
    rows = [];
    render();
    assert.match(result.controls, /Showing 0–0 of 0/);
  }
});

test("dashboard renders actual counts, scaled charts, monthly audits, and empty states", () => {
  const rendered = {};
  const context = vm.createContext({
    setText: (selector, value) => { rendered[selector] = value; },
    setHtml: (selector, value) => { rendered[selector] = value; },
    escapeHtml: (value) => String(value).replaceAll("<", "&lt;").replaceAll(">", "&gt;"),
  });
  vm.runInContext(source("dashboard.js"), context);
  context.renderMainDashboard({ stats: { total: 8, auditsCompleted: 5, auditsPending: 3, overallAuditScore: 72 }, charts: {
    monthlyAuditTrend: [{ month: "2026-01", audits: 120 }, { month: "2026-02", audits: 60 }],
    auditScores: [{ label: "<Outlet>", score: 0 }],
  } });
  assert.equal(rendered['[data-dashboard="total"]'], 8);
  assert.equal(rendered['[data-dashboard="overallAuditScore"]'], "72/100");
  const charts = rendered["[data-dashboard-charts]"];
  assert.equal((charts.match(/<article /g) || []).length, 6);
  assert.match(charts, /2026-01<\/span><strong>120<\/strong>/);
  assert.match(charts, /width:50%/);
  assert.match(charts, /&lt;Outlet&gt;/);
  assert.match(charts, /0\/100/);
  assert.match(charts, /width:0%/);
  context.renderMainDashboard({ stats: { overallAuditScore: null }, charts: {} });
  assert.equal(rendered['[data-dashboard="overallAuditScore"]'], "No completed audits");
  assert.equal((rendered["[data-dashboard-charts]"].match(/No data yet/g) || []).length, 6);
  const html = fs.readFileSync(path.join(__dirname, "../web/html/tabs/today.html"), "utf8");
  assert.equal((html.match(/data-dashboard="/g) || []).length, 8);
  assert.match(html, /data-dashboard-charts/);
});


test("navigation follows account order and fits the available navbar width", () => {
  const context = vm.createContext({
    currentUser: { id: 1, permissions: ["today", "inspections", "reports"], navigationOrder: ["reports", "today"] },
    allTabs: ["today", "inspections", "reports", "account", "notifications"].map((id) => ({ id, label: id })),
    defaultNavbarTabs: ["today", "inspections"],
    document: { querySelectorAll: () => [] },
  });
  vm.runInContext(source("navigation.js"), context);
  assert.equal(JSON.stringify(context.orderedAppTabs().map((tab) => tab.id)), '["reports","today","inspections","account","notifications"]');
  assert.equal(context.navbarVisibleCount([90, 100, 110, 120], 400, 390), 2);
  assert.equal(context.navbarVisibleCount([90, 100, 110, 120], 320, 900), 3);
  assert.equal(context.navbarVisibleCount([90, 100, 110, 120], 600, 1400), 4);
  assert.equal(context.navbarVisibleCount([90, 100], 70, 280), 1);
  context.currentUser = { id: 2, permissions: ["today"], navigationOrder: ["inspections", "account"] };
  assert.equal(JSON.stringify(context.orderedAppTabs().map((tab) => tab.id)), '["account","today","notifications"]');
});

test("navigation reordering saves the account and rolls back failed saves", async () => {
  let saved, message;
  const context = vm.createContext({
    currentUser: { id: 7, permissions: ["today", "reports"], navigationOrder: ["today", "reports"] },
    allTabs: ["today", "reports"].map((id) => ({ id, label: id })),
    defaultNavbarTabs: ["today"],
    document: { querySelectorAll: () => [], querySelector: () => null },
    setText: (_, value) => { message = value; },
    requestJson: async (path, method, payload) => { saved = { path, method, payload }; return { navigationOrder: payload.order }; },
  });
  vm.runInContext(source("navigation.js"), context);
  await context.moveNavigationTab("reports", -1);
  assert.equal(saved.path, "/api/account/navigation");
  assert.equal(JSON.stringify(saved.payload.order), '["reports","today"]');
  assert.match(message, /saved/);
  context.requestJson = async () => { throw new Error("Offline"); };
  await context.moveNavigationTab("reports", 1);
  assert.equal(JSON.stringify(context.currentUser.navigationOrder), '["reports","today"]');
  assert.match(message, /not saved: Offline/);
});
