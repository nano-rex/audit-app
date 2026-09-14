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
  for (const name of ["loadBranding", "loadSetup", "loadDashboard", "loadReport", "loadFindings", "loadWorkOrders", "loadEquipment", "loadUsers", "loadNotifications", "loadLocations", "loadZones", "loadChecklist", "restoreLastInspectionSession", "loadInspectionHistory"]) {
    context[name] = async () => { calls.push(name); };
  }
  Object.assign(context, {
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
