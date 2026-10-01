const unitTexts = document.querySelectorAll("[data-unit-text]");
const checklistContainer = document.querySelector("[data-checklist]");
const brandingDefaults = {
  appTitle: "Audit App",
  appSubtitle: "Facilities audit workspace",
  businessUnitLabel: "Facilities",
  todayHeading: "inspections for today",
  reportHeading: "audit report",
  loginTitle: "Audit App",
};
let branding = { ...brandingDefaults };
let currentUnit = branding.businessUnitLabel;
const allTabs = [
  { id: "today", label: "Dashboard" },
  { id: "inspections", label: "Inspections" },
  { id: "findings", label: "History & Findings" },
  { id: "work-orders", label: "Work Orders" },
  { id: "equipment", label: "Fixed Assets" },
  { id: "reports", label: "Reports" },
  { id: "corrective-actions", label: "Corrective Actions" },
  { id: "notifications", label: "Notifications" },
  { id: "categories", label: "Categories" },
  { id: "departments", label: "Departments" },
  { id: "outlets", label: "Outlets" },
  { id: "users", label: "Users" },
  { id: "roles", label: "Roles" },
  { id: "settings", label: "Settings" },
  { id: "account", label: "Account" },
];
// The Super account sees every regular page plus these two restricted ones; the server validates the same list.
const superTabs = [
  { id: "super-dashboard", label: "Super Dashboard" },
  { id: "super-settings", label: "Super Settings" },
];
const contextParents = { reports: "today", findings: "inspections", "corrective-actions": "inspections", equipment: "categories" };
const defaultNavbarTabs = ["today", "inspections", "findings", "equipment", "reports"];
const setupOptions = {
  departments: [],
  categories: [],
  outlets: [],
  zones: [],
  roles: [],
  priorities: [],
  auditTypes: [],
  settings: {},
  tabs: allTabs,
};
let selectedLocationOutlet = "";
let selectedZoneOutlet = "";
let equipmentCache = [];
let equipmentPage = 1;
let equipmentFilterKey = "";
let departmentCache = [];
let categoryCache = [];
let outletCache = [];
let zoneCache = [];
let locationCache = [];
let userCache = [];
let roleCache = [];
let priorityCache = [];
let auditTypeCache = [];
let notificationCache = [];
let workOrderCache = [];
let findingCache = [];
let inspectionItems = [];
let inspectionSessionItems = [];
let pendingInspectionSchedule = null;
let activeFindingRow = null;
let inspectionHistoryCache = [];
let inspectionHistorySearch = "";
let photoMarkState = null;
let signatureState = null;
const lastInspectionSessionKey = "ottotree:lastInspectionSessionId";
const departmentFilters = { search: "" };
const categoryFilters = { search: "" };
const outletFilters = { search: "" };
const userFilters = { search: "", role: "", department: "" };
const roleFilters = { search: "" };
const notificationFilters = { search: "", status: "" };
const historyFilters = { dateFrom: "", dateTo: "", outlet: "", location: "", auditor: "", department: "", category: "", priority: "", status: "", pic: "" };
const findingFilters = { search: "", outlet: "", location: "", department: "", category: "", priority: "", status: "", auditId: "" };
const workOrderFilters = { search: "", outlet: "", location: "", department: "", category: "", priority: "", status: "" };
const equipmentFilters = {
  search: "",
  outlet: "",
  location: "",
  type: "",
  brand: "",
};
const defaultInspectionCriteria = [
  "Present and correctly placed",
  "Clean and free from visible damage",
  "Operational during inspection",
  "Label, cable, or accessory is complete",
];
