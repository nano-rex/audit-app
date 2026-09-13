const unitTexts = document.querySelectorAll("[data-unit-text]");
const checklistContainer = document.querySelector("[data-checklist]");
const brandingDefaults = {
  appTitle: "Audit App",
  appSubtitle: "Facilities audit workspace",
  businessUnitLabel: "Facilities",
  todayHeading: "inspections for today",
  reportHeading: "monthly audit report",
  loginTitle: "Audit App",
};
let branding = { ...brandingDefaults };
let currentUnit = branding.businessUnitLabel;
const allTabs = [
  { id: "today", label: "To-do" },
  { id: "inspections", label: "Inspections" },
  { id: "findings", label: "Findings" },
  { id: "work-orders", label: "Work Orders" },
  { id: "equipment", label: "Equipment" },
  { id: "reports", label: "Reports" },
  { id: "corrective-actions", label: "Corrective Actions" },
  { id: "notifications", label: "Notifications" },
  { id: "categories", label: "Categories" },
  { id: "departments", label: "Departments" },
  { id: "outlets", label: "Outlets" },
  { id: "users", label: "Users" },
  { id: "roles", label: "Roles" },
  { id: "settings", label: "Settings" },
];
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
let navbarTabs = [...defaultNavbarTabs];
let selectedLocationOutlet = "";
let selectedZoneOutlet = "";
let equipmentCache = [];
let departmentCache = [];
let categoryCache = [];
let outletCache = [];
let zoneCache = [];
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
let inspectionHistoryCache = [];
let inspectionHistorySearch = "";
let inspectionHistoryPage = 1;
let photoMarkState = null;
let signatureState = null;
const inspectionHistoryPageSize = 8;
const lastInspectionSessionKey = "ottotree:lastInspectionSessionId";
const departmentFilters = { search: "" };
const categoryFilters = { search: "" };
const outletFilters = { search: "" };
const userFilters = { search: "", role: "", department: "" };
const roleFilters = { search: "" };
const notificationFilters = { search: "", status: "" };
const historyFilters = { dateFrom: "", dateTo: "", outlet: "", location: "", auditor: "", department: "", category: "", priority: "", status: "", pic: "" };
const findingFilters = { search: "", outlet: "", location: "", department: "", category: "", priority: "", status: "" };
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
