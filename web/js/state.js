const unitTexts = document.querySelectorAll("[data-unit-text]");
const questionSection = document.querySelector("[data-question-section]");
const questionTitle = document.querySelector("[data-question-title]");
const questionCopy = document.querySelector("[data-question-copy]");
const checklistContainer = document.querySelector("[data-checklist]");
const currentUnit = "Ottotree";
const allTabs = [
  { id: "today", label: "To-do" },
  { id: "inspections", label: "Inspections" },
  { id: "findings", label: "Findings" },
  { id: "work-orders", label: "Work Orders" },
  { id: "equipment", label: "Equipment" },
  { id: "reports", label: "Reports" },
  { id: "categories", label: "Categories" },
  { id: "departments", label: "Departments" },
  { id: "outlets", label: "Outlets" },
  { id: "users", label: "Users" },
  { id: "roles", label: "Roles" },
];
const defaultNavbarTabs = ["today", "inspections", "findings", "equipment", "reports"];
const setupOptions = {
  departments: [],
  categories: [],
  outlets: [],
  zones: [],
  roles: [],
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
