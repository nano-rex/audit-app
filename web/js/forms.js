async function resetScheduleForm() {
  const form = document.getElementById("schedule-form");
  form.reset();
  form.elements.scheduleId.value = "";
  form.querySelector("h2").textContent = "Schedule Audit Visit";
  form.querySelector('button[value="default"]').textContent = "Schedule";
  updateSelectOptions(form.elements.outlet, setupOptions.outlets, true, "Select outlet");
  await updateScheduleLocationSelect();
  form.elements.scheduledDate.value = todayIsoDate();
  form.elements.auditor.value = currentUser?.name || "";
  form.querySelector("[data-delete-current-schedule]").hidden = true;
  setText("[data-schedule-message]", "");
  // An audit that happens now is scheduled and opened in one step.
  form.querySelector("[data-schedule-start-now]").hidden = !(currentUser?.inspectionPermissions || []).includes("auditor");
}

async function openScheduleEditor(row) {
  const dialog = document.getElementById("schedule");
  const form = document.getElementById("schedule-form");
  await resetScheduleForm();
  form.elements.scheduleId.value = row.id;
  form.elements.outlet.value = row.outlet;
  await updateScheduleLocationSelect(row.visit_locations || []);
  form.elements.scheduledDate.value = row.scheduled_date;
  form.elements.auditor.value = row.auditor;
  form.elements.remarks.value = row.remarks || "";
  form.querySelector("h2").textContent = "Edit Scheduled Visit";
  form.querySelector('button[value="default"]').textContent = "Save Changes";
  // A visit whose audit is completed is kept with that audit record.
  form.querySelector("[data-delete-current-schedule]").hidden = row.inspection_status === "Completed";
  form.querySelector("[data-schedule-start-now]").hidden = true;
  // A visit that was opened has an audit; an unfinished one is deleted with the visit.
  form.dataset.draft = row.inspection_id && row.inspection_status !== "Completed" ? JSON.stringify({ ref: row.audit_ref || "", progress: row.progress || 0 }) : "";
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
  setText("[data-user-form-message]", "");
  updateSetupSelects();
  form.elements.inheritPermissions.checked = true;
  showEditorTab(form, "details");
}

function openUserEditor(row = null) {
  const dialog = document.getElementById("user-dialog");
  const form = document.getElementById("user-form");
  resetUserForm();
  if (row) {
    form.elements.userId.value = row.id;
    form.elements.name.value = row.name || "";
    form.elements.username.value = row.username || "";
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
  form.elements.inheritPermissions.checked = row?.permissionOverrides == null;
  renderUserPermissions(row?.permissionOverrides);
  renderUserOutlets(row?.outlets || []);
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
  updateSelectOptions(form.elements.department, setupOptions.departments, true, "No department");
  form.elements.department.value = row?.department || "";
  form.elements.outletScope.value = row?.outlet_scope || "all";
  form.dataset.roleName = row?.name || "";
  // A role may report to any role except itself and those below it.
  const below = new Set(row?.id ? roleDescendants(row.id) : []);
  const choices = roleCache.filter((role) => role.id !== row?.id && !below.has(role.id));
  form.elements.reportsTo.innerHTML = `<option value="">Nobody (top of a chain)</option>` + choices.map((role) => `<option value="${role.id}">${escapeHtml(role.name)}</option>`).join("");
  form.elements.reportsTo.value = row?.reports_to_id ? String(row.reports_to_id) : "";
  renderRoleOutlets();
  form.elements.name.disabled = Boolean(row?.protected);
  renderRolePermissions(row?.permissions || [], Boolean(row?.protected));
  form.querySelector("[data-role-inspection-permissions]").innerHTML = permissionCheckboxes(inspectionPermissionOptions, row?.inspectionPermissions || [], "inspectionPermissions", Boolean(row?.protected));
  showEditorTab(form, "details");
  form.querySelector("h2").textContent = row ? "Edit Role" : "Role Setup";
  form.querySelector('button[type="submit"], button[value="default"]').textContent = row ? "Save Changes" : "Save Role";
  form.querySelector('button[type="submit"], button[value="default"]').disabled = Boolean(row?.protected);
  dialog.showModal();
}

function openPriorityEditor(row = null) {
  const form = document.getElementById("priority-form");
  if (!form) return;
  form.reset();
  form.elements.priorityId.value = row?.id || "";
  form.elements.name.value = row?.name || "";
  form.elements.classification.value = row?.classification || "Priority";
  form.elements.dueDays.value = row?.due_days ?? row?.dueDays ?? 3;
  form.elements.active.checked = row ? Boolean(row.active) : true;
  form.querySelector('button[type="submit"]').textContent = row ? "Save Changes" : "Save Priority";
  form.scrollIntoView({ block: "nearest" });
}

function openAuditTypeEditor(row = null) {
  const form = document.getElementById("audit-type-form");
  if (!form) return;
  form.reset();
  form.elements.auditTypeId.value = row?.id || "";
  form.elements.name.value = row?.name || "";
  form.elements.description.value = row?.description || "";
  form.elements.active.checked = row ? Boolean(row.active) : true;
  form.querySelector('button[type="submit"]').textContent = row ? "Save Changes" : "Save Audit Type";
  form.scrollIntoView({ block: "nearest" });
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
  updateSelectOptions(form.elements.responsibleDepartment, setupOptions.departments, true, "No default department");
  form.elements.responsibleDepartment.value = row?.department || "";
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

async function loadWorkOrderComments(workOrderId = "") {
  const section = document.querySelector("[data-work-order-comments-section]");
  const list = document.querySelector("[data-work-order-comments]");
  if (!section || !list) return;
  section.hidden = !workOrderId;
  if (!workOrderId) {
    list.innerHTML = "";
    return;
  }
  const response = await fetch(`/api/comments?type=work_order&id=${encodeURIComponent(workOrderId)}`);
  const data = await response.json();
  list.innerHTML = (data.items || []).length
    ? data.items.map((row) => {
      const created = row.created_at ? new Date(row.created_at).toLocaleString() : "No date";
      return `<article><div><b>${escapeHtml(row.author || "User")}</b><span>${escapeHtml(created)}</span><span>${escapeHtml(row.comment || "")}</span></div></article>`;
    }).join("")
    : `<article><div><b>No comments</b><span>Add the first follow-up note.</span></div></article>`;
}

// Closing is final: say so when it is chosen.
function updateWorkOrderStage() {
  const form = document.getElementById("work-order-form");
  if (!form || form.dataset.mode === "finding") return;
  const closing = form.elements.status.value === "Closed" && form.dataset.currentStatus !== "Closed";
  setText("[data-work-order-stage-hint]", closing ? "Closing records today's date. A closed work order cannot be edited." : "");
}

document.querySelector('#work-order-form [name="status"]')?.addEventListener("change", updateWorkOrderStage);

// A new work order is made from a work request (requestId), prefilled from it as row.
async function openWorkOrderEditor(row = null, requestId = "") {
  activeFindingRow = null;
  const dialog = document.getElementById("work-order-dialog");
  const form = document.getElementById("work-order-form");
  const isEdit = Boolean(row?.id);
  form.reset();
  form.dataset.mode = "work-order";
  form.dataset.workRequestId = isEdit ? "" : String(requestId || "");
  setText("[data-work-order-message]", "");
  form.dataset.savedImages = JSON.stringify(parseStoredImages(row?.images_json || "[]"));
  form.querySelector("[data-work-order-evidence]").innerHTML = renderWorkOrderEvidence(storedImagesFromDataset(form));
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
    form.elements.dueDate.value = row.due_date || "";
    form.elements.vendor.value = row.vendor || "";
    form.elements.cost.value = row.cost || "";
    form.elements.pic.value = row.pic || "";
    form.elements.title.value = row.title || "";
    form.elements.description.value = row.description || "";
    form.elements.cause.value = row.cause || "";
    form.elements.recommendation.value = row.recommendation || "";
    form.elements.requiredAction.value = row.required_action || "";
    form.elements.closedAt.value = row.closed_at || "";
    form.querySelector("h2").textContent = isEdit ? "Edit Work Order" : "Create Work Order";
    form.querySelector('button[type="submit"]').textContent = isEdit ? "Save Changes" : "Save Work Order";
  } else {
    form.querySelector("h2").textContent = "Create Work Order";
    form.querySelector('button[type="submit"]').textContent = "Save Work Order";
  }
  const closed = row?.status === "Closed";
  form.querySelector("[data-work-order-closed]").hidden = !closed;
  form.querySelector('button[type="submit"]').hidden = closed;
  if (closed) form.querySelector("h2").textContent = "Closed Work Order";
  // Offer only the steps the workflow allows from here.
  const current = isEdit ? row.status || "Assigned" : "";
  form.dataset.currentStatus = current;
  updateSelectOptions(form.elements.status, isEdit ? [current, ...(workOrderTransitions[current] || [])] : ["Assigned", "Open"]);
  form.elements.status.value = current || (row?.status === "Open" ? "Open" : "Assigned");
  updateWorkOrderStage();
  await loadWorkOrderComments(row?.id || "");
  dialog.showModal();
}

async function openLocationEditor(row = null) {
  const dialog = document.getElementById("location-dialog");
  const form = document.getElementById("location-form");
  form.reset();
  form.elements.locationId.value = row?.id || "";
  form.elements.name.value = row?.name || "";
  form.elements.qrCode.value = row?.qr_code || row?.qrCode || "";
  form.elements.floor.value = row?.floor || "";
  form.elements.area.value = row?.area || "";
  form.elements.displayOrder.value = row?.display_order || row?.displayOrder || 0;
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

function resetEquipmentForm(kind = "asset") {
  const form = document.getElementById("equipment-form");
  form.reset();
  form.elements.equipmentId.value = "";
  form.dataset.savedImages = "[]";
  form.querySelector("[data-equipment-photos]").innerHTML = "";
  setEquipmentFormKind(kind);
  form.querySelector("h2").textContent = `Register ${itemKinds[kind].label}`;
  form.querySelector('button[type="submit"]').textContent = `Save ${itemKinds[kind].label}`;
  updateSetupSelects();
  updateSelectOptions(form.elements.itemCategory, setupOptions.categories, true, "No category");
  renderEquipmentCriteria(kind === "fixture" ? defaultFixtureCriteria : defaultInspectionCriteria);
  updateEquipmentNameOptions();
}

// A fixture or finish is a part of the building: it has no code, serial, or warranty to record.
function setEquipmentFormKind(kind) {
  const form = document.getElementById("equipment-form");
  form.elements.kind.value = kind;
  form.dataset.kind = kind;
  const fixture = kind === "fixture";
  form.elements.name.placeholder = fixture ? "Wall paint, floor tiles, water pipe, toilet, sink" : "Speaker, TV, amplifier, furniture";
  setText("[data-condition-label]", fixture ? "Condition" : "Operational Status");
}

async function openEquipmentEditor(row) {
  const dialog = document.getElementById("equipment-dialog");
  const form = document.getElementById("equipment-form");
  const kind = row.kind === "fixture" ? "fixture" : "asset";
  resetEquipmentForm(kind);
  form.elements.equipmentId.value = row.id;
  form.elements.itemCategory.value = row.category || "";
  form.elements.name.value = row.name || row.asset_id || "";
  form.elements.code.value = row.code || row.asset_id || "";
  form.elements.qrCode.value = row.code || row.asset_id || row.qr_code || row.qrCode || "";
  form.elements.outlet.value = row.outlet || "";
  await updateEquipmentLocationSelect(row.location || row.zone || "");
  form.elements.type.value = row.type || row.equipment_type || "";
  form.elements.operationalStatus.value = row.operational_status || row.health_status || "Operational";
  form.elements.brand.value = row.brand || "";
  form.elements.model.value = row.model || "";
  form.elements.serialNumber.value = row.serial_number || "";
  form.elements.installationDate.value = row.installation_date || "";
  form.elements.warrantyDate.value = row.warranty_date || "";
  form.elements.calibrationDate.value = row.calibration_date || "";
  form.elements.expiryDate.value = row.expiry_date || "";
  form.elements.temporaryRelocation.value = row.temporary_relocation || "";
  form.elements.inverterModel.value = row.inverter_model || "";
  form.elements.motorCapacity.value = row.motor_capacity || "";
  const photos = parseStoredImages(row.photos || "[]");
  form.dataset.savedImages = JSON.stringify(photos);
  form.querySelector("[data-equipment-photos]").innerHTML = renderSavedImageList(photos, "data-delete-equipment-photo");
  form.elements.description.value = row.description || row.notes || "";
  renderEquipmentCriteria(parseInspectionCriteria(row.inspection_criteria));
  form.querySelector("h2").textContent = `Edit ${itemKinds[kind].label}`;
  form.querySelector('button[type="submit"]').textContent = "Save Changes";
  dialog.showModal();
}

async function saveInspectionSession(complete = false) {
  const form = document.getElementById("inspection-form");
  if (form.dataset.saving === "true") return;
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
  form.dataset.saving = "true";
  try {
    const result = await requestJson(id ? `/api/inspection-sessions/${id}` : "/api/inspection-sessions", id ? "PATCH" : "POST", payload);
    form.elements.inspectionSessionId.value = result.id;
    if (complete) form.dataset.completed = "true";
    setCurrentInspectionName(result.inspectionName || `${payload.outlet}_${payload.auditDate}_${result.id}`, "Editing");
    updateInspectionProgress();
    loadInspectionHistory();
    if (complete) {
      setCurrentInspectionName(result.inspectionName, "Completed");
      document.querySelector("[data-save-inspection-progress]").disabled = true;
      loadDashboard();
      await openSignoff(result.id);
    }
  } catch (error) {
    alert(`Unable to save inspection: ${error.message}`);
    console.error(error);
  } finally {
    form.dataset.saving = "false";
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
    locations: chosenVisitLocations(form),
    scheduledDate: formValue(form, "scheduledDate", "Today"),
    auditor: formValue(form, "auditor", "Unassigned"),
    remarks: formValue(form, "remarks", ""),
  };
  const id = formValue(form, "scheduleId", "");
  if (!form.reportValidity()) return;
  const saved = await requestJson(id ? `/api/schedules/${id}` : "/api/schedules", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  resetScheduleForm();
  if (event.submitter?.value === "start" && saved.id) {
    await applyInspectionSchedule({ id: saved.id });
    loadGuidedSchedules().catch(showLoadError);
    return;
  }
  loadApp();
});

document.querySelector("[data-delete-current-schedule]")?.addEventListener("click", async (event) => {
  const form = event.currentTarget.closest("form");
  const id = formValue(form, "scheduleId", "");
  if (!id) return;
  // A visit that was opened already has a draft audit; say so before it goes too.
  const draft = form.dataset.draft ? JSON.parse(form.dataset.draft) : null;
  const warning = draft
    ? `Delete this scheduled visit? Its draft audit ${draft.ref} (${draft.progress}% done) will be deleted too.`
    : "Delete this scheduled visit?";
  if (!confirm(warning.replace("  ", " "))) return;
  try {
    await requestJson(`/api/schedules/${id}`, "DELETE");
  } catch (error) {
    setText("[data-schedule-message]", error.message);
    return;
  }
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
    cause: formValue(form, "cause", ""),
    recommendation: formValue(form, "recommendation", ""),
    requiredAction: formValue(form, "requiredAction", ""),
    images: storedImagesFromDataset(form),
    assignee: formValue(form, "assignee", "Technical Support"),
    dueDate: formValue(form, "dueDate", ""),
    vendor: formValue(form, "vendor", ""),
    cost: Number(formValue(form, "cost", "0")) || 0,
    pic: formValue(form, "pic", ""),
    workRequestId: Number(form.dataset.workRequestId) || undefined,
  };
  if (activeFindingRow) {
    if (!payload.description.trim()) {
      setText("[data-work-order-message]", "Describe what is wrong before saving the finding.");
      return;
    }
    activeFindingRow.dataset.findingDetails = JSON.stringify({
      category: payload.category, priority: payload.priority, assignedDepartment: payload.requestType, pic: payload.pic,
      cause: payload.cause, recommendation: payload.recommendation, requiredAction: payload.requiredAction,
    });
    // The remark is a single-line field; keep the description readable there.
    activeFindingRow.querySelector('input[name*="-notes-"]').value = payload.description.replace(/\s*\n+\s*/g, "; ").trim();
    renderFindingSummary(activeFindingRow);
    const asset = activeFindingRow.closest("[data-equipment-id]");
    asset.dataset.savedImages = JSON.stringify(payload.images);
    asset.querySelector("[data-saved-images]").innerHTML = renderInspectionImages(payload.images);
    activeFindingRow = null;
    form.closest("dialog").close();
    updateInspectionProgress();
    return;
  }
  try {
    await requestJson(id ? `/api/work-orders/${id}` : "/api/work-orders", id ? "PATCH" : "POST", payload);
  } catch (error) {
    // Workflow rules (missing completion evidence, verifier permission) are explained in the dialog.
    setText("[data-work-order-message]", error.message);
    return;
  }
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("equipment-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "equipmentId", "");
  // An empty code is generated by the server (AST-00012, FXT-00012).
  const code = formValue(form, "code", "").trim();
  const payload = {
    businessUnit: currentUnit,
    kind: form.elements.kind.value,
    category: form.elements.itemCategory.value,
    name: formValue(form, "name", code),
    code,
    outlet: formValue(form, "outlet", ""),
    location: formValue(form, "location", ""),
    type: formValue(form, "type", ""),
    operationalStatus: formValue(form, "operationalStatus", "Operational"),
    brand: formValue(form, "brand", ""),
    model: formValue(form, "model", ""),
    serialNumber: formValue(form, "serialNumber", ""),
    installationDate: formValue(form, "installationDate", ""),
    warrantyDate: formValue(form, "warrantyDate", ""),
    calibrationDate: formValue(form, "calibrationDate", ""),
    expiryDate: formValue(form, "expiryDate", ""),
    temporaryRelocation: formValue(form, "temporaryRelocation", ""),
    inverterModel: formValue(form, "inverterModel", ""),
    motorCapacity: formValue(form, "motorCapacity", ""),
    photos: storedImagesFromDataset(form),
    description: formValue(form, "description", ""),
    replacementFlag: formValue(form, "operationalStatus", "Operational") === "Replace",
    inspectionCriteria: collectEquipmentCriteria(form),
  };
  if (!form.reportValidity()) return;
  await requestJson(id ? `/api/equipment/${id}` : "/api/equipment", id ? "PATCH" : "POST", payload);
  form.closest("dialog").close();
  resetEquipmentForm();
  loadApp();
});

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
    department: form.elements.responsibleDepartment.value,
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
  try {
    await requestJson(id ? `/api/setup/outlets/${id}` : "/api/setup/outlets", id ? "PATCH" : "POST", payload);
  } catch (error) {
    alert(error.message);
    return;
  }
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("user-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "userId", "");
  const payload = {
    name: formValue(form, "name", "New User"),
    username: formValue(form, "username", ""),
    email: formValue(form, "email", "user@example.com"),
    role: formValue(form, "role", ""),
    department: formValue(form, "department", ""),
    title: formValue(form, "title", ""),
    responsibilities: formValue(form, "responsibilities", ""),
    password: formValue(form, "password", ""),
    active: Boolean(form.elements.active.checked),
    resetRequired: Boolean(form.elements.resetRequired.checked),
    resetPassword: Boolean(form.elements.resetPassword.checked),
    permissionOverrides: userPermissionOverrides(form),
    outlets: userOutletChoices(form),
  };
  const button = form.querySelector('button[type="submit"]');
  button.disabled = true;
  setText("[data-user-form-message]", "");
  try {
    await requestJson(id ? `/api/users/${id}` : "/api/users", id ? "PATCH" : "POST", payload);
    form.closest("dialog").close();
    resetUserForm();
    loadApp();
  } catch (error) {
    setText("[data-user-form-message]", error.message);
  } finally {
    button.disabled = false;
  }
});

document.getElementById("role-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "roleId", "");
  const payload = {
    name: formValue(form, "name", "New Role"),
    description: formValue(form, "description", ""),
    department: form.elements.department.value,
    outletScope: form.elements.outletScope.value,
    reportsTo: Number(form.elements.reportsTo.value) || null,
    permissions: [...form.querySelectorAll('input[name="permissions"]:checked')].map((input) => input.value),
    inspectionPermissions: [...form.querySelectorAll('input[name="inspectionPermissions"]:checked')].map((input) => input.value),
  };
  const message = form.querySelector("[data-role-outlets-hint]");
  try {
    await requestJson(id ? `/api/roles/${id}` : "/api/roles", id ? "PATCH" : "POST", payload);
    // Then each person's outlets, as chosen in the list under Outlet access.
    for (const [userId, outlets] of roleOutletChoices(form)) {
      await requestJson(`/api/users/${userId}`, "PATCH", { outlets });
    }
  } catch (error) {
    if (message) message.textContent = error.message;
    form.querySelector("[data-role-outlets]").hidden = false;
    return;
  }
  form.closest("dialog").close();
  loadApp();
});

document.getElementById("priority-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "priorityId", "");
  const payload = {
    name: formValue(form, "name", "Priority"),
    classification: formValue(form, "classification", "Priority"),
    dueDays: Number(formValue(form, "dueDays", "0")) || 0,
    active: Boolean(form.elements.active.checked),
  };
  await requestJson(id ? `/api/setup/priorities/${id}` : "/api/setup/priorities", id ? "PATCH" : "POST", payload);
  form.reset();
  form.elements.active.checked = true;
  loadApp();
});

document.getElementById("audit-type-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "auditTypeId", "");
  const payload = {
    name: formValue(form, "name", "Routine Audit"),
    description: formValue(form, "description", ""),
    active: Boolean(form.elements.active.checked),
  };
  await requestJson(id ? `/api/setup/audit-types/${id}` : "/api/setup/audit-types", id ? "PATCH" : "POST", payload);
  form.reset();
  form.elements.active.checked = true;
  loadApp();
});

document.getElementById("scoring-settings-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  try {
    await requestJson("/api/settings", "POST", { settings: {
      "scoring.passMark": Number(form.elements.passMark.value),
      "scoring.weighting": form.elements.weightingMode.value,
      "scoring.excellentBand": Number(form.elements.excellentFrom.value),
      "scoring.goodBand": Number(form.elements.goodFrom.value),
      "scoring.belowBand": Number(form.elements.needsImprovementFrom.value),
      "scoring.weights": Object.fromEntries([...form.querySelectorAll("[data-category-weight]")].map((input) => [input.dataset.categoryWeight, Number(input.value)])),
    } });
    setText("[data-scoring-message]", "Scoring settings saved.");
    loadApp();
  } catch (error) { setText("[data-scoring-message]", error.message); }
});

document.getElementById("system-settings-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  await requestJson("/api/settings", "POST", {
    settings: {
      "report.companyName": formValue(form, "companyName", "Ottotree"),
      "report.departmentHeader": formValue(form, "departmentHeader", "Facilities Department"),
      "report.logoUrl": formValue(form, "logoUrl", ""),
      "report.appTitle": formValue(form, "appTitle", "Ottotree Audit"),
      "report.appSubtitle": formValue(form, "appSubtitle", "Loudspeaker & Mini Studio operations"),
      "report.businessUnitLabel": formValue(form, "businessUnitLabel", "Ottotree"),
      "report.reportHeading": formValue(form, "reportHeading", "audit report"),
      "report.loginTitle": formValue(form, "loginTitle", "Ottotree Audit"),
    },
  });
  loadApp();
});

document.getElementById("feature-visibility-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  try {
    await requestJson("/api/settings", "POST", { settings: {
      "system.findingsEnabled": Boolean(form.elements.findingsEnabled.checked),
      "system.requirePhotoEveryAsset": Boolean(form.elements.requirePhotoEveryAsset.checked),
    } });
    setText("[data-feature-visibility-message]", "Workflow options saved.");
    await loadApp();
  } catch (error) {
    setText("[data-feature-visibility-message]", error.message);
  }
});

document.getElementById("location-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const id = formValue(form, "locationId", "");
  const payload = {
    outlet: selectedLocationOutlet,
    name: formValue(form, "name", "New Location"),
    qrCode: formValue(form, "qrCode", ""),
    floor: formValue(form, "floor", ""),
    area: formValue(form, "area", ""),
    displayOrder: Number(formValue(form, "displayOrder", "0")) || 0,
    size: formValue(form, "size", ""),
    equipmentIds: [...form.querySelectorAll('[data-location-asset-options] input:checked')].map((input) => Number(input.value)),
  };
  try {
    await requestJson(id ? `/api/locations/${id}` : "/api/locations", id ? "PATCH" : "POST", payload);
  } catch (error) {
    alert(error.message);
    return;
  }
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
  try {
    await requestJson(id ? `/api/zones/${id}` : "/api/zones", id ? "PATCH" : "POST", payload);
  } catch (error) {
    alert(error.message);
    return;
  }
  form.closest("dialog").close();
  loadApp();
});


document.querySelector("[data-report-logo-upload]")?.addEventListener("change", async (event) => {
  const form = document.getElementById("system-settings-form");
  const save = form.querySelector('button[type="submit"]');
  save.disabled = true;
  try {
    const [image] = await readFilesAsStoredImages(event.target.files);
    if (image) form.elements.logoUrl.value = image.url;
    renderReportLogo();
    setText("[data-report-logo-message]", "Logo uploaded. Save Settings to apply it.");
  } catch (error) {
    setText("[data-report-logo-message]", error.message);
  } finally { save.disabled = false; }
});
function renderReportLogo() {
  const url = document.getElementById("system-settings-form")?.elements.logoUrl.value;
  renderImageTile(document.querySelector("[data-report-logo-tile]"), url ? { url, name: "Report logo" } : {}, "data-remove-report-logo", "Report logo");
}

document.querySelector("[data-report-logo-tile]")?.addEventListener("click", (event) => {
  if (!event.target.closest("[data-remove-report-logo]")) return;
  document.getElementById("system-settings-form").elements.logoUrl.value = "";
  renderReportLogo();
  setText("[data-report-logo-message]", "Logo removed. Save Settings to apply it.");
});
