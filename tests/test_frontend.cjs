const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { test } = require("node:test");
const scripts = path.join(__dirname, "../web/js");
const source = (name) => fs.readFileSync(path.join(scripts, name), "utf8");
// Globals that state.js and the browser provide to every script.
const emptyDocument = () => ({
  querySelector: () => null, querySelectorAll: () => [], getElementById: () => null, addEventListener() {},
  createElement: () => ({ setAttribute() {}, textContent: "" }), body: { prepend() {} },
});
const shared = () => ({
  document: emptyDocument(),
  window: { addEventListener() {}, dispatchEvent() {}, innerWidth: 1280 },
  Event: class {},
  contextParents: { reports: "today", findings: "inspections", equipment: "categories" },
  superTabs: [{ id: "super-dashboard", label: "Super Dashboard" }, { id: "super-settings", label: "Super Settings" }],
  setupOptions: { settings: {}, departments: [] },
  localStorage: { getItem: () => null, setItem() {} },
  setLoading() {},
});

test("all browser scripts parse", () => {
  for (const name of fs.readdirSync(scripts).filter((name) => name.endsWith(".js"))) {
    new vm.Script(source(name), { filename: name });
  }
});

test("startup loads only the active tab and coalesces duplicate requests", async () => {
  const calls = [];
  const document = { ...emptyDocument(), querySelector: () => ({ id: "today" }) };
  const context = vm.createContext({ ...shared(), document, currentUser: {}, Promise, Map, activeTabId: null, activeContextTab: null });
  for (const name of ["loadBranding", "loadSetup", "loadDashboard", "loadReport", "loadFindings", "loadWorkOrders", "loadEquipment", "loadUsers", "loadAccount", "loadNotifications", "loadLocations", "loadZones", "loadInspectionHistory", "loadGuidedSchedules", "loadSuperDashboard", "loadSuperSettings", "loadAttention"]) {
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
  assert.deepEqual(calls.sort(), ["loadAttention", "loadBranding", "loadDashboard", "loadSetup"].sort());
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

test("equipment renders one page of rows and resets pagination on filtering", () => {
  let rendered = "";
  const context = vm.createContext({
    equipmentCache: Array.from({ length: 2330 }, (_, id) => ({ id, name: `Asset ${id}`, outlet: id < 3 ? "Small" : "Large" })),
    equipmentFilters: { search: "", outlet: "", location: "", type: "", brand: "" },
    ...shared(),
    equipmentPage: 1, equipmentFilterKey: "", getPaginationSize: () => 30,
    setHtml: (_, html) => { rendered = html; },
    equipmentRow: () => "<article></article>",
  });
  vm.runInContext(source("dashboard.js"), context);
  context.renderEquipment();
  assert.equal((rendered.match(/<article>/g) || []).length, 30);
  context.equipmentPage = 78;
  context.renderEquipment();
  assert.equal((rendered.match(/<article>/g) || []).length, 20);
  context.equipmentFilters.outlet = "Small";
  context.renderEquipment();
  assert.equal(context.equipmentPage, 1);
  assert.equal((rendered.match(/<article>/g) || []).length, 3);
});

test("asset date filters read the date formats people type", () => {
  const context = vm.createContext({ ...shared() });
  vm.runInContext(source("dashboard.js"), context);
  const day = (offset) => { const date = new Date(); date.setDate(date.getDate() + offset); return date; };
  const dotted = (date) => `${String(date.getDate()).padStart(2, "0")}.${String(date.getMonth() + 1).padStart(2, "0")}.${date.getFullYear()}`;
  const iso = (date) => `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  assert.equal(context.parseItemDate("20/04/2027").getMonth(), 3);
  assert.equal(context.parseItemDate("12.3.2016").getDate(), 12);
  assert.equal(context.parseItemDate("31.02.2027"), null);
  assert.equal(context.parseItemDate("soon"), null);
  assert.ok(context.matchesDateFilter(dotted(day(-1)), "expired"));
  assert.ok(context.matchesDateFilter(iso(day(10)), "30"));
  assert.ok(!context.matchesDateFilter(iso(day(60)), "30"));
  assert.ok(context.matchesDateFilter(iso(day(60)), "90"));
  assert.ok(context.matchesDateFilter(dotted(day(0)), "valid"));
  assert.ok(context.matchesDateFilter("", "none") && !context.matchesDateFilter("", "valid"));
  assert.ok(context.matchesDateFilter("anything", ""));
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
    ...shared(),
    currentUser: { permissions: ["departments"] },
    allTabs: ["today", ...sections].map((id) => ({ id, label: id })),
    document: { ...emptyDocument(), querySelectorAll(selector) {
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
    wireForm() {}, updateSetupSelects() {}, setText() {}, loadApp() {}, showEditorTab() {}, renderUserPermissions() {}, renderUserOutletOptions() {}, chosenUserOutlets: () => null, userPermissionOverrides() { return null; },
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
  const chart = { style: {}, setAttribute(key, value) { this[key] = value; }, addEventListener() {} };
  const content = {};
  const context = vm.createContext({ ...shared(), document: { ...emptyDocument(), querySelector: () => chart },
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
  const context = vm.createContext({ ...shared(),
    // A saved page size of 25; the default is 10.
    localStorage: { value: "25", getItem() { return this.value; }, setItem(_, value) { this.value = value; } },
    document: { ...emptyDocument(), addEventListener: (type, handler) => { handlers[type] = handler; } } });
  vm.runInContext(source("pagination.js"), context);
  for (const key of ["outlets", "zones", "locations", "findings", "inspections", "categories"]) {
    context.localStorage.value = "25";  // The size chosen for the previous list is shared by all lists.
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

test("the dashboard shows the signed-in person's tasks, figures, activity, and times; outlets stay on Reports", () => {
  const rendered = {};
  const context = vm.createContext({
    ...shared(),
    currentUnit: "Ottotree",
    setText: (selector, value) => { rendered[selector] = value; },
    setHtml: (selector, value) => { rendered[selector] = value; },
    escapeHtml: (value) => String(value).replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;"),
  });
  vm.runInContext(source("dashboard.js"), context);
  assert.equal(context.greeting("Rex Tan", 9), "Good morning, Rex");
  assert.equal(context.greeting("", 20), "Good evening");
  context.renderMyDashboard({
    name: "Rex", role: "Auditor",
    kpi: { audit_started: 4, audit_completed: 3, auditSeconds: 7200, audit_signed: 2, audit_closed: 1, request_raised: 5, order_created: 2, request_declined: 1, requestSeconds: 90000, order_closed: 0 },
    months: ["2026-10", "2026-11"],
    series: [{ label: "Audits", values: [2, 5] }, { label: "Work requests", values: [0, 3] }, { label: "Work orders", values: [0, 0] }, { label: "Findings", values: [0, 1] }],
    timeToAct: [{ key: "audit", label: "Start to completion of an audit", averageSeconds: 7200 }],
    teamTimeToAct: [{ key: "audit", label: "Start to completion of an audit", averageSeconds: 14400 }, { key: "order", label: "Work order opened to closed", averageSeconds: null }],
    recent: [{ label: "Completed audit", record_ref: "AUDIT-<1>", created_at: 0, outlet: "TST", duration_ms: 60000 }],
  }, {
    items: [{ type: "work_order", id: 7, action: "Resolve work order", title: "WO-7 Fix", detail: "TST | Due 2026-11-01", overdue: true }],
    total: 1, counts: { orders: 1, signoff: 0, notifications: 3 },
  }, { from: "2026-10-01", to: "" });
  assert.equal(rendered["[data-my-greeting]"].split(",")[1].trim(), "Rex");
  assert.equal(rendered["[data-my-role]"], "Auditor · Ottotree");
  assert.equal(rendered["[data-my-period]"], "Your figures, 1 Oct 2026 – today");
  assert.equal(rendered['[data-my="audits"]'], 3);
  assert.equal(rendered['[data-my-note="audits"]'], "avg 2 h 0 m start to finish · 4 started");
  assert.equal(rendered['[data-my="requests"]'], 3);
  assert.equal(rendered['[data-my-note="orders"]'], "None closed yet");
  const chips = rendered["[data-my-task-chips]"];
  assert.match(chips, /data-my-chip="orders"><b>1<\/b> work orders to resolve/);
  assert.match(chips, /<b>3<\/b> unread notifications/);
  assert.doesNotMatch(chips, /sign or close/);
  assert.match(rendered["[data-my-tasks]"], /dash-task-overdue/);
  assert.match(rendered["[data-my-tasks]"], /data-open-task="work_order" data-task-id="7"/);
  assert.match(rendered["[data-my-activity-chart]"], /Nov 2026: 9 steps/);
  assert.match(rendered["[data-my-activity-chart]"], /Audits: 7 steps/);
  const times = rendered["[data-my-times]"];
  assert.match(times, /faster than average/);
  assert.match(times, /width:50%/);
  assert.equal((times.match(/<li>/g) || []).length, 1);
  assert.match(rendered["[data-my-recent]"], /AUDIT-&lt;1&gt;/);
  assert.match(rendered["[data-my-recent]"], /Took 1 m/);
  context.renderMyDashboard({}, { items: [], counts: {} });
  assert.equal(rendered["[data-my-period]"], "Your figures, all time");
  assert.match(rendered["[data-my-tasks]"], /Nothing waiting for you/);
  assert.match(rendered["[data-my-activity-chart]"], /no recorded steps/);
  assert.match(rendered["[data-my-times]"], /No timed steps/);
  const html = fs.readFileSync(path.join(__dirname, "../web/html/tabs/today.html"), "utf8");
  for (const hook of ["data-my-tasks", "data-my-activity-chart", "data-my-times", "data-my-recent", 'id="dashboard-filter-form"', "data-today-schedules"]) assert.match(html, new RegExp(hook));
  assert.doesNotMatch(html, /today-hero|data-dashboard-area|data-dashboard-outlets/);
  const reports = fs.readFileSync(path.join(__dirname, "../web/html/tabs/reports.html"), "utf8");
  assert.match(reports, /data-report-area/);
  assert.match(reports, /data-report-outlets/);
  context.renderOutletActivity({ months: ["2026-03"], outlets: [{ outlet: "C", total: 1, completed: 1, completionRate: 100, monthly: [1] }] }, "[data-report-area]", "[data-report-outlets]");
  assert.match(rendered["[data-report-area]"], /Mar 2026: 1 audit</);
  assert.match(rendered["[data-report-outlets]"], /rate-good">100% completed/);
});


test("navigation follows account order and fits the available navbar width", () => {
  const context = vm.createContext({
    ...shared(),
    currentUser: { id: 1, permissions: ["today", "inspections", "work-orders"], navigationOrder: ["work-orders", "today"] },
    allTabs: ["today", "inspections", "work-orders", "account", "notifications"].map((id) => ({ id, label: id })),
    defaultNavbarTabs: ["today", "inspections"],
  });
  vm.runInContext(source("navigation.js"), context);
  assert.equal(JSON.stringify(context.orderedAppTabs().map((tab) => tab.id)), '["work-orders","today","inspections","account","notifications"]');
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
    ...shared(),
    currentUser: { id: 7, permissions: ["today", "work-orders"], navigationOrder: ["today", "work-orders"] },
    allTabs: ["today", "work-orders"].map((id) => ({ id, label: id })),
    defaultNavbarTabs: ["today"],
    setText: (_, value) => { message = value; },
    requestJson: async (path, method, payload) => { saved = { path, method, payload }; return { navigationOrder: payload.order }; },
  });
  vm.runInContext(source("navigation.js"), context);
  await context.moveNavigationTab("work-orders", -1);
  assert.equal(saved.path, "/api/account/navigation");
  assert.equal(JSON.stringify(saved.payload.order), '["work-orders","today"]');
  assert.match(message, /saved/);
  context.requestJson = async () => { throw new Error("Offline"); };
  await context.moveNavigationTab("work-orders", 1);
  assert.equal(JSON.stringify(context.currentUser.navigationOrder), '["work-orders","today"]');
  assert.match(message, /not saved: Offline/);
});

test("the Super account keeps every regular page and adds the two restricted ones", () => {
  const context = vm.createContext({
    ...shared(),
    currentUser: { id: 1, role: "Super", permissions: ["today", "inspections", "equipment"] },
    allTabs: ["today", "inspections", "findings", "equipment", "categories", "notifications", "settings", "account"].map((id) => ({ id, label: id })),
    defaultNavbarTabs: ["today", "inspections"],
  });
  context.setupOptions.settings["system.findingsEnabled"] = false;
  vm.runInContext(source("navigation.js"), context);
  const allowed = context.allowedAppTabs().map((tab) => tab.id);
  assert.equal(allowed.slice(-2).join(), "super-dashboard,super-settings");
  assert.ok(allowed.includes("findings"), "Super keeps sections that are switched off for other users");
  assert.equal(context.orderedAppTabs().map((tab) => tab.id).join(), "today,inspections,categories,notifications,settings,account,super-dashboard,super-settings");
});

test("a finding can be removed until it is closed or has work", () => {
  const context = vm.createContext({ ...shared(), currentUser: { role: "Auditor" } });
  vm.runInContext(source("findings.js"), context);
  assert.equal(context.canEditFindings(), true);
  assert.equal(context.findingIsSettled({ status: "Open" }), false);
  assert.equal(context.findingIsSettled({ status: "Requested", request_ref: "WR-2026-00001" }), true);
  assert.equal(context.findingIsSettled({ status: "Open", order_ref: "WO-2026-00002" }), true);
  assert.equal(context.findingIsSettled({ status: "Closed" }), true);
  context.currentUser = { role: "Department/PIC" };
  assert.equal(context.canEditFindings(), false);
});

test("photo evidence is always needed for failed checks and optional for passed ones", () => {
  const context = vm.createContext({ ...shared(), checklistContainer: null });
  vm.runInContext(source("inspections.js"), context);
  const photo = [{ url: "/api/media/x.png" }];
  const passed = { passed: true, notes: "", images: [] };
  const failed = { passed: false, notes: "Cracked", images: [] };
  assert.equal(context.isInspectionReadyToComplete({ items: [passed] }), false, "every asset needs a photo by default");
  assert.equal(context.isInspectionReadyToComplete({ items: [{ ...passed, images: photo }] }), true);
  context.setupOptions.settings["system.requirePhotoEveryAsset"] = false;
  assert.equal(context.isInspectionReadyToComplete({ items: [passed] }), true);
  assert.equal(context.isInspectionReadyToComplete({ items: [passed, failed] }), false);
  assert.match(context.validateInspectionComplete({ items: [{ ...failed, section: "Speaker" }] }), /Upload image/);
  assert.equal(context.isInspectionReadyToComplete({ items: [passed, { ...failed, images: photo }] }), true);
  assert.equal(context.isInspectionReadyToComplete({ items: [{ passed: false, notes: "", images: photo }] }), false, "a failed check still needs its remark");
});

test("a notification opens its record only for users who can reach that page", () => {
  const context = vm.createContext({ ...shared(), currentUser: { permissions: ["work-orders"] }, escapeHtml: String, escapeAttr: String });
  vm.runInContext(source("rows.js"), context);
  const target = (row) => JSON.stringify(context.notificationTarget(row));
  assert.equal(target({ related_type: "work_order", related_id: 7 }), '{"type":"work_order","id":7}');
  assert.equal(target({ related_type: "inspection", related_id: 3 }), "null");
  assert.equal(target({ related_type: "work_order", related_id: null }), "null");
  assert.match(context.notificationRow({ id: 1, title: "Due soon", status: "Unread", related_type: "work_order", related_id: 7 }), /data-open-notification="1"/);
  assert.doesNotMatch(context.notificationRow({ id: 2, title: "Audit verified", status: "Read", related_type: "inspection", related_id: 3 }), /data-open-notification/);
});

test("the checklist filter narrows items by kind and asset type", () => {
  const context = vm.createContext({ ...shared(), checklistContainer: null, inspectionFilter: { kind: "", category: "" } });
  vm.runInContext(source("inspections.js"), context);
  // The filter's "category" is the asset type now that categories are retired.
  const items = [{ id: 1, type: "Plumbing", kind: "fixture" }, { id: 2, type: "AV Equipment" }, { id: 3, kind: "fixture" }];
  const shown = () => items.filter(context.matchesInspectionFilter).map((item) => item.id).join();
  assert.equal(shown(), "1,2,3");
  context.inspectionFilter.kind = "fixture";
  assert.equal(shown(), "1,3");
  context.inspectionFilter.category = "Plumbing";
  assert.equal(shown(), "1");
  context.inspectionFilter.kind = "asset";
  context.inspectionFilter.category = "";
  assert.equal(shown(), "2", "an item without a kind is a fixed asset");
});

test("the photo viewer zooms about a point and keeps the photo in view", () => {
  const image = { naturalWidth: 2000, naturalHeight: 1000, style: {} };
  const stage = { clientWidth: 1000, clientHeight: 600, classList: { toggle() {} } };
  const context = vm.createContext({ ...shared(), setText() {},
    imageSource: (picture, marked = false) => picture?.[marked ? "markedUrl" : "url"] || "",
    document: { ...emptyDocument(), querySelector: (selector) => selector.includes("stage") ? stage : image } });
  vm.runInContext(source("photo-viewer.js"), context);
  const view = () => vm.runInContext("photoViewer", context);
  context.fitPhoto();
  assert.equal(view().scale, 0.5, "the whole photo fits the stage");
  assert.equal(view().y, 50, "and is centred where it is smaller than the stage");
  context.zoomPhoto(2, 250, 300);
  assert.equal(view().scale, 1);
  assert.equal(view().x, -250, "the point under the cursor stays put");
  view().x += 5000;
  context.applyPhotoTransform();
  assert.equal(view().x, 0, "it cannot be dragged past its left edge");
  view().x -= 50000;
  context.applyPhotoTransform();
  assert.equal(view().x, -1000, "or past its right edge");
  for (let step = 0; step < 20; step++) context.zoomPhoto(2);
  assert.equal(view().scale, 4, "zoom stops at eight times the fitted size");
  for (let step = 0; step < 20; step++) context.zoomPhoto(0.5);
  assert.equal(view().scale, 0.5, "and never goes below the fitted size");
  assert.equal(context.photoThumbnail({ url: "/api/media/a.jpg", markedUrl: "/api/media/b.png" }), "/api/media/b.png?thumb=1");
});

test("the organization theme sets the page attributes and a custom accent derives readable shades", () => {
  const stored = {};
  const style = { values: {}, setProperty(name, value) { this.values[name] = value; }, removeProperty(name) { delete this.values[name]; } };
  const root = { dataset: {}, style };
  let prefersDark = false;
  const context = vm.createContext({
    ...shared(),
    localStorage: { getItem: (key) => stored[key] ?? null, setItem: (key, value) => { stored[key] = value; }, removeItem: (key) => { delete stored[key]; } },
    matchMedia: () => ({ matches: prefersDark, addEventListener() {} }),
    document: { ...emptyDocument(), documentElement: root },
  });
  vm.runInContext(source("theme.js"), context);
  assert.equal(root.dataset.palette, "default");
  context.setOrgTheme({ preset: "ottotree", font: "noto-sans-sc", corners: "square", density: "compact", mode: "dark", userChoice: false });
  assert.equal([root.dataset.palette, root.dataset.font, root.dataset.corners, root.dataset.density, root.dataset.theme].join(), "ottotree,noto-sans-sc,square,compact,dark");
  assert.match(stored["audit-app-org-theme"], /ottotree/, "kept for the next page load");
  context.setTheme("light");
  assert.equal(root.dataset.theme, "dark", "users cannot override when the organization fixes the appearance");
  context.setOrgTheme({ preset: "ottotree", mode: "system", userChoice: true });
  assert.equal(root.dataset.theme, "light", "the user's choice applies when allowed");
  context.setTheme("");
  prefersDark = true;
  context.applyTheme();
  assert.equal(root.dataset.theme, "dark", "the organization default follows the device");
  context.setTheme("light");
  context.setOrgTheme({ accent: "#ffd400" });
  const fill = style.values["--accent-fill"];
  assert.ok(fill && context.relativeLuminance(fill) <= 0.2, "a pale accent is darkened so white text on it reads");
  assert.equal(style.values["--on-accent"], "#ffffff");
  context.previewOrgTheme({ preset: "plum" });
  assert.equal(root.dataset.palette, "plum");
  context.restoreOrgTheme();
  assert.equal(root.dataset.palette, "default", "an unsaved preview is undone");
  assert.equal(style.values["--accent"], "#ffd400");
});
