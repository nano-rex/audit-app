async function resetScheduleForm() {
  const form = document.getElementById("schedule-form");
  form.reset();
  form.elements.scheduleId.value = "";
  form.querySelector("h2").textContent = "Schedule Audit Visit";
  form.querySelector('button[value="default"]').textContent = "Schedule";
  updateSelectOptions(form.elements.outlet, setupOptions.outlets, true, "Select outlet");
  await updateScheduleLocationSelect();
  form.elements.status.value = "Pending";
  form.querySelector("[data-delete-current-schedule]").hidden = true;
}

async function openScheduleEditor(row) {
  const dialog = document.getElementById("schedule");
  const form = document.getElementById("schedule-form");
  await resetScheduleForm();
  form.elements.scheduleId.value = row.id;
  form.elements.outlet.value = row.outlet;
  await updateScheduleLocationSelect(row.zone || "");
  form.elements.scheduledDate.value = row.scheduled_date;
  form.elements.auditor.value = row.auditor;
  form.elements.status.value = row.status || "Pending";
  form.elements.remarks.value = row.remarks || "";
  form.querySelector("h2").textContent = "Edit Scheduled Visit";
  form.querySelector('button[value="default"]').textContent = "Save Changes";
  form.querySelector("[data-delete-current-schedule]").hidden = false;
  dialog.showModal();
}

function resetUserForm() {
  const form = document.getElementById("user-form");
  form.reset();
  form.elements.userId.value = "";
  form.elements.active.checked = true;
  form.elements.resetRequired.checked = false;
  form.elements.resetPassword.checked = false;
  form.querySelector("h2").textContent = "User Setup";
  form.querySelector('button[type="submit"]').textContent = "Save User";
  updateSetupSelects();
}

function openUserEditor(row = null) {
  const dialog = document.getElementById("user-dialog");
  const form = document.getElementById("user-form");
  resetUserForm();
  if (row) {
    form.elements.userId.value = row.id;
    form.elements.name.value = row.name || "";
    form.elements.email.value = row.email || "";
    form.elements.role.value = row.role || "";
    form.elements.department.value = row.department || "";
    form.elements.title.value = row.title || "";
    form.elements.responsibilities.value = row.responsibilities || "";
    form.elements.active.checked = row.active !== 0 && row.active !== false;
    form.elements.resetRequired.checked = Boolean(row.reset_required || row.resetRequired);
    form.querySelector("h2").textContent = "Edit User";
    form.querySelector('button[type="submit"]').textContent = "Save Changes";
  }
  dialog.showModal();
}

function renderRolePermissions(selected = [], disabled = false) {
  const container = document.querySelector("[data-role-permissions]");
  if (!container) return;
  const selectedSet = new Set(selected);
  container.innerHTML = (setupOptions.tabs || allTabs).map((tab) => `
    <label class="permission-option">
      <input type="checkbox" name="permissions" value="${escapeAttr(tab.id)}" ${selectedSet.has(tab.id) ? "checked" : ""} ${disabled ? "disabled" : ""}>
      <span>${escapeHtml(tab.label)}</span>
    </label>
  `).join("");
}

function openRoleEditor(row = null) {
  const dialog = document.getElementById("role-dialog");
  const form = document.getElementById("role-form");
  form.reset();
  form.elements.roleId.value = row?.id || "";
  form.elements.name.value = row?.name || "";
  form.elements.description.value = row?.description || "";
  form.elements.name.disabled = Boolean(row?.protected);
  renderRolePermissions(row?.permissions || [], Boolean(row?.protected));
  form.querySelector("h2").textContent = row ? "Edit Role" : "Role Setup";
  form.querySelector('button[type="submit"], button[value="default"]').textContent = row ? "Save Changes" : "Save Role";
  form.querySelector('button[type="submit"], button[value="default"]').disabled = Boolean(row?.protected);
  dialog.showModal();
}

function openDepartmentEditor(row = null) {
  const dialog = document.getElementById("department-dialog");
  const form = document.getElementById("department-form");
  form.reset();
  form.elements.departmentId.value = row?.id || "";
  form.elements.code.value = row?.code || "";
  form.elements.description.value = row?.description || "";
  form.elements.responsibilities.value = row?.responsibilities || "";
  form.querySelector("h2").textContent = row ? "Edit Department" : "Department Setup";
  form.querySelector('button[type="submit"]').textContent = row ? "Save Changes" : "Save Department";
  dialog.showModal();
}

function openCategoryEditor(row = null) {
  const dialog = document.getElementById("category-dialog");
  const form = document.getElementById("category-form");
  form.reset();
  form.elements.categoryId.value = row?.id || "";
  form.elements.name.value = row?.name || "";
  form.elements.description.value = row?.description || "";
  form.elements.sequence.value = row?.sequence || "";
  form.elements.active.checked = row ? Boolean(row.active) : true;
  form.querySelector("h2").textContent = row ? "Edit Category" : "Category Setup";
  form.querySelector('button[type="submit"], button[value="default"]').textContent = row ? "Save Changes" : "Save Category";
  dialog.showModal();
}

function openOutletEditor(row = null) {
  const dialog = document.getElementById("outlet-dialog");
  const form = document.getElementById("outlet-form");
  form.reset();
  form.elements.outletId.value = row?.id || "";
  form.elements.code.value = row?.code || "";
  form.elements.location.value = row?.location || "";
  form.elements.description.value = row?.description || "";
  form.querySelector("h2").textContent = row ? "Edit Outlet" : "Outlet Setup";
  form.querySelector('button[type="submit"]').textContent = row ? "Save Changes" : "Save Outlet";
  dialog.showModal();
}

async function updateWorkOrderLocationSelect(selected = "") {
  const form = document.getElementById("work-order-form");
  if (!form) return;
  const outlet = formValue(form, "outlet", selectedLocationOutlet || setupOptions.outlets[0] || "");
  const response = await fetch(`/api/locations?outlet=${encodeURIComponent(outlet)}`);
  const data = await response.json();
  const values = data.items.map((row) => row.name);
  updateSelectOptions(form.elements.zone, values, false, "Select location");
  if (values.includes(selected)) {
    form.elements.zone.value = selected;
  }
}

function workOrderCompletionPhotos() {
  const form = document.getElementById("work-order-form");
  return parseStoredImages(form?.dataset.completionPhotos || "[]");
}

function setWorkOrderCompletionPhotos(images) {
  const form = document.getElementById("work-order-form");
  if (!form) return;
  form.dataset.completionPhotos = JSON.stringify(images || []);
  const container = form.querySelector("[data-work-order-completion-photos]");
  if (container) {
    container.innerHTML = renderSavedImageList(images || [], "data-delete-work-order-completion-photo");
  }
}

async function openWorkOrderEditor(row = null) {
  const dialog = document.getElementById("work-order-dialog");
  const form = document.getElementById("work-order-form");
  const isEdit = Boolean(row?.id);
  form.reset();
  updateSetupSelects();
  form.elements.workOrderId.value = row?.id || "";
  if (row) {
    form.elements.outlet.value = row.outlet || "";
  }
  await updateWorkOrderLocationSelect(row?.zone || "");
  if (row) {
    form.elements.requestType.value = row.request_type || "";
    form.elements.category.value = row.category || "";
    form.elements.priority.value = row.priority || "Medium";
    form.elements.status.value = row.status || "Assigned";
    form.elements.assignee.value = row.assignee || "";
    form.elements.pic.value = row.pic || "";
    form.elements.title.value = row.title || "";
    form.elements.description.value = row.description || "";
    form.elements.actionTaken.value = row.action_taken || "";
    form.elements.completionDate.value = row.completion_date || "";
    form.elements.completionRemark.value = row.completion_remark || "";
    form.elements.verifiedBy.value = row.verified_by || "";
    form.elements.verifiedAt.value = row.verified_at || "";
    form.elements.closedAt.value = row.closed_at || "";
    form.elements.verificationRemark.value = row.verification_remark || "";
    form.querySelector("h2").textContent = isEdit ? "Edit Work Order" : "Create Work Order";
    form.querySelector('button[type="submit"]').textContent = isEdit ? "Save Changes" : "Save Work Order";
  } else {
    form.querySelector("h2").textContent = "Create Work Order";
    form.querySelector('button[type="submit"]').textContent = "Save Work Order";
  }
  setWorkOrderCompletionPhotos(parseStoredImages(row?.completion_photo || "[]"));
  dialog.showModal();
}

async function openLocationEditor(row = null) {
  const dialog = document.getElementById("location-dialog");
  const form = document.getElementById("location-form");
  form.reset();
  form.elements.locationId.value = row?.id || "";
  form.elements.name.value = row?.name || "";
  form.elements.size.value = row?.size || "";
  form.querySelector("h2").textContent = row ? "Edit Location" : "Add Location";
  await populateLocationEquipmentSelect(row?.name || "");
  dialog.showModal();
}

async function openZoneEditor(row = null) {
  const dialog = document.getElementById("zone-dialog");
  const form = document.getElementById("zone-form");
  form.reset();
  if (row?.outlet_code) {
    selectedZoneOutlet = row.outlet_code;
    updateZoneOutletSelect();
  }
  form.elements.zoneId.value = row?.id || "";
  form.elements.name.value = row?.name || "";
  form.elements.description.value = row?.description || "";
  form.querySelector("h2").textContent = row ? "Edit Zone" : "Zone Setup";
  form.querySelector('button[type="submit"], button[value="default"]').textContent = row ? "Save Changes" : "Save Zone";
  await populateZoneLocationSelect(row?.locations || []);
  dialog.showModal();
}

function resetEquipmentForm() {
  const form = document.getElementById("equipment-form");
  form.reset();
  form.elements.equipmentId.value = "";
  form.querySelector("h2").textContent = "Register Equipment";
  form.querySelector('button[type="submit"]').textContent = "Save Equipment";
  updateSetupSelects();
  renderEquipmentCriteria();
  updateEquipmentNameOptions();
}

async function openEquipmentEditor(row) {
  const dialog = document.getElementById("equipment-dialog");
  const form = document.getElementById("equipment-form");
  resetEquipmentForm();
  form.elements.equipmentId.value = row.id;
  form.elements.name.value = row.name || row.asset_id || "";
  form.elements.code.value = row.code || row.asset_id || "";
  form.elements.outlet.value = row.outlet || "";
  await updateEquipmentLocationSelect(row.location || row.zone || "");
  form.elements.type.value = row.type || row.equipment_type || "";
  form.elements.operationalStatus.value = row.operational_status || row.health_status || "Operational";
  form.elements.brand.value = row.brand || "";
  form.elements.model.value = row.model || "";
  form.elements.serialNumber.value = row.serial_number || "";
  form.elements.installationDate.value = row.installation_date || "";
  form.elements.description.value = row.description || row.notes || "";
  renderEquipmentCriteria(parseInspectionCriteria(row.inspection_criteria));
  form.querySelector("h2").textContent = "Edit Equipment";
  form.querySelector('button[type="submit"]').textContent = "Save Changes";
  dialog.showModal();
}

wireForm("new-audit-form", "/api/audits", (form) => ({
  businessUnit: currentUnit,
  outlet: formValue(form, "outlet", ""),
  auditDate: formValue(form, "auditDate", todayIsoDate()),
  auditor: formValue(form, "auditor", "Unnamed Auditor"),
  auditType: formValue(form, "auditType", "Standard"),
  score: Math.max(0, Math.min(100, Number(formValue(form, "score", "0")) || 0)),
}));

async function saveInspectionSession(complete = false) {
  const form = document.getElementById("inspection-form");
  const payload = collectInspectionPayload(complete);
  if (complete) {
    const error = validateInspectionComplete(payload);
    if (error) {
      alert(error);
      updateInspectionProgress();
      return;
    }
  }
  const id = formValue(form, "inspectionSessionId", "");
  try {
    const result = await requestJson(id ? `/api/inspection-sessions/${id}` : "/api/inspection-sessions", id ? "PATCH" : "POST", payload);
    form.elements.inspectionSessionId.value = result.id;
    setCurrentInspectionName(result.inspectionName || `${payload.outlet}_${payload.auditDate}_${result.id}`, "Editing");
    localStorage.setItem(lastInspectionSessionKey, result.id);
    updateInspectionProgress();
    loadInspectionHistory();
    if (complete) {
      loadDashboard();
    }
  } catch (error) {
    alert("Unable to save inspection progress. Please try again.");
    console.error(error);
  }
}

document.querySelector("[data-save-inspection-progress]")?.addEventListener("click", (event) => {
  event.preventDefault();
  const payload = collectInspectionPayload(false);
  saveInspectionSession(isInspectionReadyToComplete(payload));
});

document.getElementById("inspection-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const payload = collectInspectionPayload(false);
  await saveInspectionSession(isInspectionReadyToComplete(payload));
});

document.getElementById("schedule-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const payload = {
    businessUnit: currentUnit,
    outlet: formValue(form, "outlet", ""),
    zone: formValue(form, "zone", "Unassigned"),
    scheduledDate: formValue(form, "scheduledDate", "Today"),
    auditor: formValue(form, "auditor", "Unassigned"),
    remarks: formValue(form, "remarks", ""),
    status: formValue(form, "status", "Pending"),
  };
  const id = formValue(form, "scheduleId", "");
  await requestJson(id ? `/api/schedules/${id}` : "/api/schedules", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  resetScheduleForm();
  loadApp();
});

document.querySelector("[data-delete-current-schedule]")?.addEventListener("click", async (event) => {
  const form = event.currentTarget.closest("form");
  const id = formValue(form, "scheduleId", "");
  if (!id) return;
  if (!confirm("Delete this scheduled visit?")) return;
  await requestJson(`/api/schedules/${id}`, "DELETE");
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("work-order-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "workOrderId", "");
  const payload = {
    businessUnit: currentUnit,
    outlet: formValue(form, "outlet", ""),
    zone: formValue(form, "zone", "Unassigned"),
    requestType: formValue(form, "requestType", ""),
    category: formValue(form, "category", ""),
    priority: formValue(form, "priority", "Medium"),
    status: formValue(form, "status", "Assigned"),
    title: formValue(form, "title", "Work order"),
    description: formValue(form, "description", ""),
    assignee: formValue(form, "assignee", "Technical Support"),
    pic: formValue(form, "pic", ""),
    actionTaken: formValue(form, "actionTaken", ""),
    completionDate: formValue(form, "completionDate", ""),
    completionRemark: formValue(form, "completionRemark", ""),
    completionPhoto: workOrderCompletionPhotos(),
    verifiedBy: formValue(form, "verifiedBy", ""),
    verifiedAt: formValue(form, "verifiedAt", ""),
    closedAt: formValue(form, "closedAt", ""),
    verificationRemark: formValue(form, "verificationRemark", ""),
  };
  await requestJson(id ? `/api/work-orders/${id}` : "/api/work-orders", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("equipment-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "equipmentId", "");
  const code = formValue(form, "code", `EQ-${Date.now()}`);
  const payload = {
    businessUnit: currentUnit,
    name: formValue(form, "name", code),
    code,
    assetId: code,
    qrCode: code,
    outlet: formValue(form, "outlet", ""),
    location: formValue(form, "location", ""),
    type: formValue(form, "type", "Equipment"),
    operationalStatus: formValue(form, "operationalStatus", "Operational"),
    brand: formValue(form, "brand", ""),
    model: formValue(form, "model", ""),
    serialNumber: formValue(form, "serialNumber", ""),
    installationDate: formValue(form, "installationDate", ""),
    description: formValue(form, "description", ""),
    replacementFlag: formValue(form, "operationalStatus", "Operational") === "Replace",
    inspectionCriteria: collectEquipmentCriteria(form),
  };
  await requestJson(id ? `/api/equipment/${id}` : "/api/equipment", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  resetEquipmentForm();
  loadApp();
});

wireForm("admin-form", "/api/admin", (form) => ({
  recordType: formValue(form, "recordType", "Outlet"),
  name: formValue(form, "name", "New record"),
  parent: formValue(form, "parent", ""),
  detail: formValue(form, "detail", ""),
}));

document.getElementById("department-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "departmentId", "");
  const payload = {
    code: formValue(form, "code", "Department").toUpperCase(),
    description: formValue(form, "description", ""),
    responsibilities: formValue(form, "responsibilities", ""),
  };
  await requestJson(id ? `/api/setup/departments/${id}` : "/api/setup/departments", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("category-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "categoryId", "");
  const payload = {
    name: formValue(form, "name", "New Category"),
    description: formValue(form, "description", ""),
    sequence: Number(formValue(form, "sequence", "0")) || 0,
    active: form.elements.active.checked,
  };
  await requestJson(id ? `/api/setup/categories/${id}` : "/api/setup/categories", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("outlet-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "outletId", "");
  const payload = {
    code: formValue(form, "code", "Outlet").toUpperCase(),
    location: formValue(form, "location", ""),
    description: formValue(form, "description", ""),
  };
  await requestJson(id ? `/api/setup/outlets/${id}` : "/api/setup/outlets", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("user-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "userId", "");
  const payload = {
    name: formValue(form, "name", "New User"),
    email: formValue(form, "email", "user@example.com"),
    role: formValue(form, "role", ""),
    department: formValue(form, "department", ""),
    title: formValue(form, "title", ""),
    responsibilities: formValue(form, "responsibilities", ""),
    password: formValue(form, "password", ""),
    active: Boolean(form.elements.active.checked),
    resetRequired: Boolean(form.elements.resetRequired.checked),
    resetPassword: Boolean(form.elements.resetPassword.checked),
  };
  await requestJson(id ? `/api/users/${id}` : "/api/users", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  resetUserForm();
  loadApp();
});

document.getElementById("role-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "roleId", "");
  const payload = {
    name: formValue(form, "name", "New Role"),
    description: formValue(form, "description", ""),
    permissions: [...form.querySelectorAll('input[name="permissions"]:checked')].map((input) => input.value),
  };
  await requestJson(id ? `/api/roles/${id}` : "/api/roles", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("location-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "locationId", "");
  const payload = {
    outlet: selectedLocationOutlet,
    name: formValue(form, "name", "New Location"),
    size: formValue(form, "size", ""),
    equipmentIds: [...form.elements.equipmentIds.selectedOptions].map((option) => Number(option.value)),
  };
  await requestJson(id ? `/api/locations/${id}` : "/api/locations", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("zone-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "zoneId", "");
  const payload = {
    outlet: selectedZoneOutlet,
    name: formValue(form, "name", "Zone-1"),
    description: formValue(form, "description", ""),
    locations: [...form.querySelectorAll('input[name="zoneLocations"]:checked')].map((input) => input.value),
  };
  await requestJson(id ? `/api/zones/${id}` : "/api/zones", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  loadApp();
});

wireForm("captain-form", "/api/captain-logins", (form) => ({
  outlet: formValue(form, "outlet", ""),
  captainName: formValue(form, "captainName", "Unnamed Captain"),
}));
