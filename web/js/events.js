document.querySelectorAll("[data-open]").forEach((button) => {
  button.addEventListener("click", async () => {
    const dialog = document.getElementById(button.dataset.open);
    if (button.dataset.open === "schedule") {
      await resetScheduleForm();
    }
    dialog.showModal();
  });
});

document.addEventListener("click", async (event) => {
  try {
  if (!event.target.closest(".nav-row")) closeNavigationMenu();
  const findingLink = event.target.closest("[data-view-inspection-findings]");
  const allFindings = event.target.closest("[data-all-inspection-findings]");
  if (findingLink || allFindings) {
    findingFilters.auditId = findingLink?.dataset.viewInspectionFindings || "";
    setText("[data-finding-inspection-scope]", findingFilters.auditId ? `Audit #${findingFilters.auditId}` : "All inspections");
    renderFindings();
    showHistoryFindingsSection("findings");
    return;
  }
  if (event.target.closest("[data-back-to-schedules]")) {
    const readOnly = document.getElementById("inspection-form")?.dataset.completed === "true";
    if (!readOnly && !confirm("Return to scheduled work? Save your progress first to keep any changes.")) return;
    showGuidedContent(false);
    await loadGuidedSchedules();
    return;
  }
  const equipmentPageButton = event.target.closest("[data-equipment-page]");
  if (equipmentPageButton) {
    equipmentPage = Number(equipmentPageButton.dataset.equipmentPage);
    renderEquipment();
    return;
  }
  const menuButton = event.target.closest("[data-menu-toggle]");
  if (menuButton) {
    const menu = document.getElementById("tab-menu");
    const open = menu.hidden;
    menu.hidden = !open;
    menuButton.setAttribute("aria-expanded", String(open));
    return;
  }

  const moveTab = event.target.closest("[data-menu-move]");
  if (moveTab) {
    await moveNavigationTab(moveTab.dataset.menuMove, Number(moveTab.dataset.direction));
    return;
  }

  const menuOpenTab = event.target.closest("[data-menu-open-tab]");
  if (menuOpenTab) {
    openTab(menuOpenTab.dataset.menuOpenTab);
    document.getElementById("tab-menu").hidden = true;
    document.querySelector("[data-menu-toggle]")?.setAttribute("aria-expanded", "false");
    return;
  }

  const closeButton = event.target.closest("[data-close-dialog]");
  if (closeButton) {
    closeButton.closest("dialog")?.close();
    return;
  }

  const exportInspection = event.target.closest("[data-export-inspection-pdf]");
  if (exportInspection?.classList.contains("disabled")) {
    event.preventDefault();
    alert("Save this inspection before exporting a PDF.");
    return;
  }

  const editButton = event.target.closest("[data-edit-schedule]");
  if (editButton) {
    await openScheduleEditor(JSON.parse(editButton.dataset.editSchedule));
    return;
  }

  const openScheduleButton = event.target.closest("[data-open-schedule]");
  if (openScheduleButton) {
    openScheduledInspection(JSON.parse(openScheduleButton.dataset.openSchedule));
    return;
  }

  const openInspectionButton = event.target.closest("[data-open-inspection-session]");
  if (openInspectionButton) {
    await openInspectionSession(openInspectionButton.dataset.openInspectionSession);
    return;
  }

  const closeInspectionButton = event.target.closest("[data-close-inspection-session]");
  if (closeInspectionButton) {
    if (!confirm("Close this audit permanently? All findings must be closed and all three signatures recorded. The audit cannot be edited afterward.")) return;
    try {
      await requestJson("/api/inspection-sessions/close", "POST", { id: Number(closeInspectionButton.dataset.closeInspectionSession) });
      await loadInspectionHistory();
      if (signoffSession) await openSignoff(signoffSession.id);
    } catch (error) {
      alert(error.message);
    }
    return;
  }

  const deleteInspectionButton = event.target.closest("[data-delete-inspection-session]");
  if (deleteInspectionButton && confirm("Delete this inspection history item?")) {
    await requestJson(`/api/inspection-sessions/${deleteInspectionButton.dataset.deleteInspectionSession}`, "DELETE");
    loadInspectionHistory();
    return;
  }

  const activityButton = event.target.closest("[data-login-activity]");
  if (activityButton) {
    await showLoginActivity(activityButton.dataset.loginActivity, Number(activityButton.dataset.offset || 0));
    return;
  }

  const editUserButton = event.target.closest("[data-edit-user]");
  if (editUserButton) {
    openUserEditor(JSON.parse(editUserButton.dataset.editUser));
    return;
  }

  const editRoleButton = event.target.closest("[data-edit-role]");
  if (editRoleButton) {
    openRoleEditor(JSON.parse(editRoleButton.dataset.editRole));
    return;
  }

  const editPriorityButton = event.target.closest("[data-edit-priority]");
  if (editPriorityButton) {
    openPriorityEditor(JSON.parse(editPriorityButton.dataset.editPriority));
    return;
  }

  const editAuditTypeButton = event.target.closest("[data-edit-audit-type]");
  if (editAuditTypeButton) {
    openAuditTypeEditor(JSON.parse(editAuditTypeButton.dataset.editAuditType));
    return;
  }

  const editLocationButton = event.target.closest("[data-edit-location]");
  if (editLocationButton) {
    openLocationEditor(JSON.parse(editLocationButton.dataset.editLocation));
    return;
  }

  const editEquipmentButton = event.target.closest("[data-edit-equipment]");
  if (editEquipmentButton) {
    openEquipmentEditor(JSON.parse(editEquipmentButton.dataset.editEquipment));
    return;
  }

  const editDepartmentButton = event.target.closest("[data-edit-department]");
  if (editDepartmentButton) {
    openDepartmentEditor(JSON.parse(editDepartmentButton.dataset.editDepartment));
    return;
  }

  const editCategoryButton = event.target.closest("[data-edit-category]");
  if (editCategoryButton) {
    openCategoryEditor(JSON.parse(editCategoryButton.dataset.editCategory));
    return;
  }

  const editOutletButton = event.target.closest("[data-edit-outlet]");
  if (editOutletButton) {
    openOutletEditor(JSON.parse(editOutletButton.dataset.editOutlet));
    return;
  }

  const editZoneButton = event.target.closest("[data-edit-zone]");
  if (editZoneButton) {
    await openZoneEditor(JSON.parse(editZoneButton.dataset.editZone));
    return;
  }

  const attentionButton = event.target.closest("[data-attention-type]");
  if (attentionButton) {
    openAttentionItem(attentionButton.dataset.attentionType, Number(attentionButton.dataset.attentionId), attentionButton.dataset.attentionView).catch(showLoadError);
    return;
  }

  const editWorkOrderButton = event.target.closest("[data-edit-work-order]");
  if (editWorkOrderButton) {
    openWorkOrderEditor(JSON.parse(editWorkOrderButton.dataset.editWorkOrder));
    return;
  }

  const departmentButton = event.target.closest("[data-delete-department]");
  if (departmentButton && confirm("Delete this department?")) {
    await requestJson(`/api/setup/departments/${departmentButton.dataset.deleteDepartment}`, "DELETE");
    loadApp();
    return;
  }

  const categoryButton = event.target.closest("[data-delete-category]");
  if (categoryButton && confirm("Delete this category?")) {
    await requestJson(`/api/setup/categories/${categoryButton.dataset.deleteCategory}`, "DELETE");
    loadApp();
    return;
  }

  const outletButton = event.target.closest("[data-delete-outlet]");
  if (outletButton && confirm("Delete this outlet?")) {
    await requestJson(`/api/setup/outlets/${outletButton.dataset.deleteOutlet}`, "DELETE");
    loadApp();
    return;
  }

  const userButton = event.target.closest("[data-delete-user]");
  if (userButton && confirm("Delete this user?")) {
    await requestJson(`/api/users/${userButton.dataset.deleteUser}`, "DELETE");
    loadUsers();
    return;
  }

  const roleButton = event.target.closest("[data-delete-role]");
  if (roleButton && confirm("Delete this role? Users with this role will become unassigned.")) {
    await requestJson(`/api/roles/${roleButton.dataset.deleteRole}`, "DELETE");
    loadApp();
    return;
  }

  const priorityButton = event.target.closest("[data-delete-priority]");
  if (priorityButton && confirm("Delete this priority level?")) {
    await requestJson(`/api/setup/priorities/${priorityButton.dataset.deletePriority}`, "DELETE");
    loadApp();
    return;
  }

  const auditTypeButton = event.target.closest("[data-delete-audit-type]");
  if (auditTypeButton && confirm("Delete this audit type?")) {
    await requestJson(`/api/setup/audit-types/${auditTypeButton.dataset.deleteAuditType}`, "DELETE");
    loadApp();
    return;
  }

  const openNotificationButton = event.target.closest("[data-open-notification]");
  if (openNotificationButton) {
    const row = notificationCache.find((item) => String(item.id) === openNotificationButton.dataset.openNotification);
    const target = row && notificationTarget(row);
    if (!target) return;
    if (row.status === "Unread") await requestJson(`/api/notifications/${row.id}`, "PATCH", {});
    if (target.type === "user") showTab("users");
    else await openAttentionItem(target.type, target.id).catch(showLoadError);
    loadAttention().catch(() => {});
    return;
  }

  const readNotificationButton = event.target.closest("[data-read-notification]");
  if (readNotificationButton) {
    await requestJson(`/api/notifications/${readNotificationButton.dataset.readNotification}`, "PATCH", {});
    loadNotifications();
    return;
  }

  const deleteNotificationButton = event.target.closest("[data-delete-notification]");
  if (deleteNotificationButton && confirm("Delete this notification?")) {
    await requestJson(`/api/notifications/${deleteNotificationButton.dataset.deleteNotification}`, "DELETE");
    loadNotifications();
    return;
  }

  const locationButton = event.target.closest("[data-delete-location]");
  if (locationButton && confirm("Delete this location?")) {
    await requestJson(`/api/locations/${locationButton.dataset.deleteLocation}`, "DELETE");
    loadApp();
    return;
  }

  const zoneButton = event.target.closest("[data-delete-zone]");
  if (zoneButton && confirm("Delete this zone?")) {
    await requestJson(`/api/zones/${zoneButton.dataset.deleteZone}`, "DELETE");
    loadApp();
    return;
  }

  const equipmentButton = event.target.closest("[data-delete-equipment]");
  if (equipmentButton && confirm("Delete this fixed asset?")) {
    await requestJson(`/api/equipment/${equipmentButton.dataset.deleteEquipment}`, "DELETE");
    loadApp();
    return;
  }

  const workOrderButton = event.target.closest("[data-delete-work-order]");
  if (workOrderButton && confirm("Delete this work order?")) {
    await requestJson(`/api/work-orders/${workOrderButton.dataset.deleteWorkOrder}`, "DELETE");
    loadApp();
    return;
  }
  } catch (error) {
    showActionError(error);
  }
});


document.getElementById("inspection-history-search")?.addEventListener("input", (event) => {
  inspectionHistorySearch = event.target.value;
  renderInspectionHistory();
});

[
  ["history-filter-from", "dateFrom"],
  ["history-filter-to", "dateTo"],
  ["history-filter-outlet", "outlet"],
  ["history-filter-location", "location"],
  ["history-filter-auditor", "auditor"],
  ["history-filter-department", "department"],
  ["history-filter-category", "category"],
  ["history-filter-priority", "priority"],
  ["history-filter-status", "status"],
  ["history-filter-pic", "pic"],
].forEach(([id, key]) => {
  document.getElementById(id)?.addEventListener("input", (event) => {
    historyFilters[key] = event.target.value;
    if (key === "outlet") updateHistoryFilterSelects();
    renderInspectionHistory();
  });
  document.getElementById(id)?.addEventListener("change", (event) => {
    historyFilters[key] = event.target.value;
    if (key === "outlet") updateHistoryFilterSelects();
    renderInspectionHistory();
  });
});

document.getElementById("location-outlet")?.addEventListener("change", (event) => {
  selectedLocationOutlet = event.target.value;
  loadLocations();
});

document.getElementById("zone-outlet")?.addEventListener("change", (event) => {
  selectedZoneOutlet = event.target.value;
  loadZones();
});

document.querySelector("[data-open-location]")?.addEventListener("click", () => {
  if (!selectedLocationOutlet) {
    alert("Select an outlet before adding a location.");
    return;
  }
  openLocationEditor();
});

document.querySelector("[data-open-zone]")?.addEventListener("click", () => {
  if (!selectedZoneOutlet) {
    alert("Select an outlet before adding a zone.");
    return;
  }
  openZoneEditor();
});

document.querySelector("[data-open-department]")?.addEventListener("click", () => {
  openDepartmentEditor();
});

document.querySelector("[data-open-category]")?.addEventListener("click", () => {
  openCategoryEditor();
});

document.querySelector("[data-open-outlet]")?.addEventListener("click", () => {
  openOutletEditor();
});

document.querySelector("[data-open-user]")?.addEventListener("click", () => {
  openUserEditor();
});

document.querySelector("[data-open-role]")?.addEventListener("click", () => {
  openRoleEditor();
});

document.querySelectorAll("[data-open-equipment]").forEach((button) => button.addEventListener("click", async () => {
  resetEquipmentForm(button.dataset.openEquipment);
  await updateEquipmentLocationSelect();
  document.getElementById("equipment-dialog").showModal();
}));

document.querySelector("[data-add-equipment-criterion]")?.addEventListener("click", () => {
  addEquipmentCriterion();
});

document.querySelector("[data-equipment-criteria]")?.addEventListener("click", (event) => {
  const button = event.target.closest("[data-remove-equipment-criterion]");
  if (!button) return;
  button.closest(".criterion-edit-row")?.remove();
  if (!document.querySelector('[data-equipment-criteria] input[name="inspectionCriteria"]')) {
    addEquipmentCriterion();
  }
});

document.querySelector('#equipment-form input[name="name"]')?.addEventListener("change", (event) => {
  applyEquipmentTemplate(event.target.value.trim());
});

document.querySelector('#equipment-form input[name="code"]')?.addEventListener("input", (event) => {
  const form = event.target.form;
  if (form?.elements.qrCode) form.elements.qrCode.value = event.target.value.trim();
});

document.querySelector('#equipment-form select[name="outlet"]')?.addEventListener("change", () => {
  updateEquipmentLocationSelect();
});

document.querySelector('#work-order-form select[name="outlet"]')?.addEventListener("change", () => {
  updateWorkOrderLocationSelect();
});

document.querySelector(".mark-toolbar")?.addEventListener("click", (event) => {
  const toolButton = event.target.closest("[data-mark-tool]");
  if (toolButton && photoMarkState) {
    photoMarkState.tool = toolButton.dataset.markTool;
    document.querySelectorAll("[data-mark-tool]").forEach((button) => {
      button.classList.toggle("active", button === toolButton);
    });
  }
  if (event.target.closest("[data-mark-undo]") && photoMarkState) {
    const mark = photoMarkState.marks.pop();
    if (mark) photoMarkState.redoMarks.push(mark);
    renderPhotoMarker();
  }
  if (event.target.closest("[data-mark-redo]") && photoMarkState) {
    const mark = photoMarkState.redoMarks.pop();
    if (mark) photoMarkState.marks.push(mark);
    renderPhotoMarker();
  }
  if (event.target.closest("[data-mark-clear]") && photoMarkState) {
    photoMarkState.redoMarks = [...photoMarkState.marks.reverse(), ...(photoMarkState.redoMarks || [])];
    photoMarkState.marks = [];
    renderPhotoMarker();
  }
});

document.querySelector("[data-photo-mark-canvas]")?.addEventListener("pointerdown", (event) => {
  if (!photoMarkState) return;
  const point = photoMarkerPoint(event);
  photoMarkState.drawing = true;
  photoMarkState.startX = point.x;
  photoMarkState.startY = point.y;
  photoMarkState.points = [point];
});

document.querySelector("[data-photo-mark-canvas]")?.addEventListener("pointermove", (event) => {
  if (!photoMarkState?.drawing) return;
  const point = photoMarkerPoint(event);
  if (photoMarkState.tool === "freehand") {
    photoMarkState.points.push(point);
    renderPhotoMarker({ tool: "freehand", points: photoMarkState.points });
    return;
  }
  renderPhotoMarker({
    tool: photoMarkState.tool,
    x: photoMarkState.startX,
    y: photoMarkState.startY,
    w: point.x - photoMarkState.startX,
    h: point.y - photoMarkState.startY,
    text: document.querySelector('#photo-mark-form input[name="markText"]')?.value || "Issue",
  });
});

document.querySelector("[data-photo-mark-canvas]")?.addEventListener("pointerup", (event) => {
  if (!photoMarkState?.drawing) return;
  const point = photoMarkerPoint(event);
  const mark = photoMarkState.tool === "freehand"
    ? { tool: "freehand", points: photoMarkState.points }
    : {
      tool: photoMarkState.tool,
      x: photoMarkState.startX,
      y: photoMarkState.startY,
      w: point.x - photoMarkState.startX,
      h: point.y - photoMarkState.startY,
      text: document.querySelector('#photo-mark-form input[name="markText"]')?.value || "Issue",
    };
  photoMarkState.marks.push(mark);
  photoMarkState.redoMarks = [];
  photoMarkState.drawing = false;
  renderPhotoMarker();
});

document.getElementById("photo-mark-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  await saveMarkedPhoto();
  form.closest("dialog").close();
});

document.querySelector('#schedule-form select[name="outlet"]')?.addEventListener("change", () => {
  updateScheduleLocationSelect();
});

document.querySelector("#schedule-form [data-visit-location-options]")?.addEventListener("change", (event) => {
  const container = event.currentTarget;
  const all = container.querySelector("[data-visit-all]");
  const picks = [...container.querySelectorAll('input[name="visitLocation"]')];
  if (event.target === all) picks.forEach((input) => { input.checked = false; });
  all.checked = !picks.some((input) => input.checked);
});


checklistContainer?.addEventListener("change", async (event) => {
  const input = event.target.closest("[data-inspection-check]");
  if (!input) {
    if (event.target.matches("[data-equipment-images]")) {
      const itemRow = event.target.closest("[data-equipment-id]");
      const existingImages = storedImagesFromDataset(itemRow);
      const newImages = await readFilesAsStoredImages(event.target.files);
      const images = [...existingImages, ...newImages];
      if (itemRow) itemRow.dataset.savedImages = JSON.stringify(images);
      const savedImages = itemRow?.querySelector("[data-saved-images]");
      if (savedImages) savedImages.innerHTML = renderInspectionImages(images);
      event.target.value = "";
      updateInspectionProgress();
    }
    return;
  }
  const row = input.closest(".criteria-row");
  const notes = row.querySelector('input[name*="-notes-"]');
  row.classList.toggle("passed", input.checked);
  if (notes) {
    notes.disabled = input.checked;
    if (input.checked) notes.value = "";
  }
  updateInspectionProgress();
  if (input.checked) return;
  await openFindingEditor(row);
});

// The department that normally handles a category, when one is set on it.
function categoryDepartment(name) {
  return categoryCache.find((row) => row.name === name)?.department || "";
}

document.querySelector("[data-inspection-filter-kind]")?.addEventListener("change", (event) => {
  inspectionFilter.kind = event.target.value;
  applyInspectionFilter();
});

document.querySelector("[data-inspection-filter-category]")?.addEventListener("change", (event) => {
  inspectionFilter.category = event.target.value;
  applyInspectionFilter();
});

// Choosing a category on a finding or work order brings its responsible department with it.
document.querySelector('#work-order-form [name="category"]')?.addEventListener("change", (event) => {
  const department = categoryDepartment(event.target.value);
  if (department) event.target.form.elements.requestType.value = department;
});

// A failed criterion is recorded with the work-order form, limited to the fields a finding keeps.
async function openFindingEditor(row) {
  const detail = JSON.parse(row.querySelector("[data-inspection-check]").dataset.inspectionCheck);
  const saved = parseStoredObject(row.dataset.findingDetails);
  const notes = row.querySelector('input[name*="-notes-"]');
  await openWorkOrderEditor({
    outlet: detail.outlet,
    zone: detail.location,
    request_type: saved.assignedDepartment || categoryDepartment(saved.category || detail.category) || setupOptions.departments[0] || "",
    category: saved.category || detail.category || setupOptions.categories[0] || "",
    priority: saved.priority || "High",
    status: "Assigned",
    assignee: "Technical Support",
    pic: saved.pic || "",
    title: `${detail.name} - ${detail.criterion}`,
    description: notes?.value.trim() || `Failed check: ${detail.criterion}`,
    cause: saved.cause || "",
    recommendation: saved.recommendation || "",
    required_action: saved.requiredAction || "",
    images_json: row.closest("[data-equipment-id]").dataset.savedImages || "[]",
  });
  activeFindingRow = row;
  const findingForm = document.getElementById("work-order-form");
  findingForm.dataset.mode = "finding";
  findingForm.querySelector("h2").textContent = "Record Audit Finding";
  findingForm.querySelector('button[type="submit"]').textContent = "Save Finding to Draft";
  findingForm.querySelector("[data-work-order-closed]").hidden = true;
}

document.querySelector("[data-work-order-evidence-upload]").addEventListener("change", async (event) => {
  const input = event.target;
  const form = input.form;
  const images = [...storedImagesFromDataset(form), ...await readFilesAsStoredImages(input.files)];
  form.dataset.savedImages = JSON.stringify(images);
  form.querySelector("[data-work-order-evidence]").innerHTML = renderWorkOrderEvidence(images);
  input.value = "";
});

document.querySelector("[data-work-order-evidence]").addEventListener("click", (event) => {
  const form = event.currentTarget.closest("form");
  const remove = event.target.closest("[data-delete-work-evidence]");
  if (remove) {
    const images = storedImagesFromDataset(form);
    images.splice(Number(remove.dataset.deleteWorkEvidence), 1);
    form.dataset.savedImages = JSON.stringify(images);
    event.currentTarget.innerHTML = renderWorkOrderEvidence(images);
  }
  const mark = event.target.closest("[data-mark-work-evidence]");
  if (mark) openPhotoMarker(form, Number(mark.dataset.markWorkEvidence));
});

document.querySelector("[data-equipment-photos-upload]")?.addEventListener("change", async (event) => {
  const input = event.target;
  const form = input.form;
  try {
    const images = [...storedImagesFromDataset(form), ...await readFilesAsStoredImages(input.files)];
    form.dataset.savedImages = JSON.stringify(images);
    form.querySelector("[data-equipment-photos]").innerHTML = renderSavedImageList(images, "data-delete-equipment-photo");
  } catch (error) {
    alert(error.message);
  } finally {
    input.value = "";
  }
});

document.querySelector("[data-equipment-photos]")?.addEventListener("click", (event) => {
  const remove = event.target.closest("[data-delete-equipment-photo]");
  if (!remove) return;
  const form = event.currentTarget.closest("form");
  const images = storedImagesFromDataset(form);
  images.splice(Number(remove.dataset.deleteEquipmentPhoto), 1);
  form.dataset.savedImages = JSON.stringify(images);
  event.currentTarget.innerHTML = images.length ? renderSavedImageList(images, "data-delete-equipment-photo") : '<span class="muted">No photos attached.</span>';
});

checklistContainer?.addEventListener("click", (event) => {
  const zoneButton = event.target.closest("[data-open-inspection-zone]");
  if (zoneButton) {
    openInspectionZone(zoneButton.dataset.openInspectionZone);
    return;
  }
  const deleteImageButton = event.target.closest("[data-delete-inspection-image]");
  if (deleteImageButton) {
    const itemRow = deleteImageButton.closest("[data-equipment-id]");
    const images = storedImagesFromDataset(itemRow);
    images.splice(Number(deleteImageButton.dataset.deleteInspectionImage), 1);
    if (itemRow) itemRow.dataset.savedImages = JSON.stringify(images);
    const savedImages = itemRow?.querySelector("[data-saved-images]");
    if (savedImages) savedImages.innerHTML = renderInspectionImages(images);
    updateInspectionProgress();
    return;
  }

  const passAllButton = event.target.closest("[data-pass-all]");
  if (passAllButton) {
    // Leaves alone any criterion that already has a remark: that is a recorded failure.
    passAllButton.closest("[data-equipment-id]").querySelectorAll(".criteria-row").forEach((row) => {
      const checkbox = row.querySelector("[data-inspection-check]");
      const notes = row.querySelector('input[name*="-notes-"]');
      if (checkbox.checked || checkbox.disabled || notes?.value.trim()) return;
      checkbox.checked = true;
      row.classList.add("passed");
      if (notes) notes.disabled = true;
    });
    updateInspectionProgress();
    return;
  }

  const findingButton = event.target.closest("[data-record-finding]");
  if (findingButton) {
    openFindingEditor(findingButton.closest(".criteria-row")).catch(showLoadError);
    return;
  }

  const markImageButton = event.target.closest("[data-mark-inspection-image]");
  if (markImageButton) {
    const itemRow = markImageButton.closest("[data-equipment-id]");
    openPhotoMarker(itemRow, Number(markImageButton.dataset.markInspectionImage));
    return;
  }

  const button = event.target.closest("[data-open-inspection-location]");
  if (!button) return;
  openInspectionLocation(button.dataset.openInspectionLocation);
});

checklistContainer?.addEventListener("input", (event) => {
  if (event.target.matches('input[name*="-notes-"]')) updateInspectionProgress();
});

document.getElementById("equipment-search")?.addEventListener("input", (event) => {
  equipmentFilters.search = event.target.value;
  renderEquipment();
});

document.getElementById("equipment-filter-outlet")?.addEventListener("change", (event) => {
  equipmentFilters.outlet = event.target.value;
  equipmentFilters.location = "";
  updateEquipmentFilterSelects();
  renderEquipment();
});

document.getElementById("equipment-filter-location")?.addEventListener("change", (event) => {
  equipmentFilters.location = event.target.value;
  renderEquipment();
});

document.getElementById("equipment-filter-type")?.addEventListener("change", (event) => {
  equipmentFilters.type = event.target.value;
  renderEquipment();
});

document.getElementById("equipment-filter-brand")?.addEventListener("change", (event) => {
  equipmentFilters.brand = event.target.value;
  renderEquipment();
});

document.getElementById("equipment-filter-category")?.addEventListener("change", (event) => {
  equipmentFilters.category = event.target.value;
  renderEquipment();
});

document.getElementById("department-search")?.addEventListener("input", (event) => {
  departmentFilters.search = event.target.value;
  renderDepartments();
});

document.getElementById("category-search")?.addEventListener("input", (event) => {
  categoryFilters.search = event.target.value;
  renderCategories();
});

document.getElementById("outlet-search")?.addEventListener("input", (event) => {
  outletFilters.search = event.target.value;
  renderOutlets();
});

document.getElementById("user-search")?.addEventListener("input", (event) => {
  userFilters.search = event.target.value;
  renderUsers();
});

document.getElementById("user-filter-role")?.addEventListener("change", (event) => {
  userFilters.role = event.target.value;
  renderUsers();
});

document.getElementById("user-filter-department")?.addEventListener("change", (event) => {
  userFilters.department = event.target.value;
  renderUsers();
});

document.getElementById("role-search")?.addEventListener("input", (event) => {
  roleFilters.search = event.target.value;
  renderRoles();
});

document.getElementById("finding-search")?.addEventListener("input", (event) => {
  findingFilters.search = event.target.value;
  renderFindings();
});

document.getElementById("finding-filter-outlet")?.addEventListener("change", (event) => {
  findingFilters.outlet = event.target.value;
  findingFilters.location = "";
  updateFindingFilterSelects();
  renderFindings();
});

document.getElementById("finding-filter-location")?.addEventListener("change", (event) => {
  findingFilters.location = event.target.value;
  renderFindings();
});

document.getElementById("finding-filter-department")?.addEventListener("change", (event) => {
  findingFilters.department = event.target.value;
  renderFindings();
});

document.getElementById("finding-filter-category")?.addEventListener("change", (event) => {
  findingFilters.category = event.target.value;
  renderFindings();
});

document.getElementById("finding-filter-priority")?.addEventListener("change", (event) => {
  findingFilters.priority = event.target.value;
  renderFindings();
});

document.getElementById("finding-filter-status")?.addEventListener("change", (event) => {
  findingFilters.status = event.target.value;
  renderFindings();
});

document.getElementById("work-order-search")?.addEventListener("input", (event) => {
  workOrderFilters.search = event.target.value;
  renderWorkOrders();
});

document.getElementById("work-order-filter-outlet")?.addEventListener("change", (event) => {
  workOrderFilters.outlet = event.target.value;
  workOrderFilters.location = "";
  updateWorkOrderFilterSelects();
  renderWorkOrders();
});

document.getElementById("work-order-filter-location")?.addEventListener("change", (event) => {
  workOrderFilters.location = event.target.value;
  renderWorkOrders();
});

document.getElementById("work-order-filter-department")?.addEventListener("change", (event) => {
  workOrderFilters.department = event.target.value;
  renderWorkOrders();
});

document.getElementById("work-order-filter-category")?.addEventListener("change", (event) => {
  workOrderFilters.category = event.target.value;
  renderWorkOrders();
});

document.getElementById("work-order-filter-priority")?.addEventListener("change", (event) => {
  workOrderFilters.priority = event.target.value;
  renderWorkOrders();
});

document.getElementById("work-order-filter-status")?.addEventListener("change", (event) => {
  workOrderFilters.status = event.target.value;
  renderWorkOrders();
});

document.getElementById("notification-search")?.addEventListener("input", (event) => {
  notificationFilters.search = event.target.value;
  renderNotifications();
});

document.getElementById("notification-filter-status")?.addEventListener("change", (event) => {
  notificationFilters.status = event.target.value;
  renderNotifications();
});

document.querySelector("[data-refresh-notifications]")?.addEventListener("click", loadNotifications);
document.querySelector("[data-read-all-notifications]")?.addEventListener("click", async () => {
  await requestJson("/api/notifications/all", "PATCH", {});
  loadNotifications();
});

document.querySelector("[data-add-work-order-comment]")?.addEventListener("click", async () => {
  const form = document.getElementById("work-order-form");
  const id = formValue(form, "workOrderId", "");
  const comment = formValue(form, "timelineComment", "");
  if (!id || !comment.trim()) return;
  await requestJson("/api/comments", "POST", {
    recordType: "work_order",
    recordId: Number(id),
    comment,
  });
  form.elements.timelineComment.value = "";
  loadWorkOrderComments(id);
});

document.getElementById("report-filter-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  try { await loadReport(); setText("[data-report-filter-message]", "Filters applied."); }
  catch (error) { setText("[data-report-filter-message]", error.message); }
});
document.getElementById("report-filter-form")?.addEventListener("reset", () => {
  setTimeout(() => loadReport().catch(showLoadError), 0);
});

async function showLoginActivity(id, offset = 0) {
  const response = await authFetch(`/api/users/${encodeURIComponent(id)}/activity?offset=${offset}`);
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || "Login activity could not be loaded");
  let dialog = document.getElementById("login-activity");
  if (!dialog) {
    dialog = document.createElement("dialog");
    dialog.id = "login-activity";
    dialog.setAttribute("aria-labelledby", "login-activity-title");
    document.body.appendChild(dialog);
  }
  dialog.innerHTML = `<h2 id="login-activity-title">Login activity: ${escapeHtml(data.name)}</h2>
    <p>Successful logins · newest first</p>
    <div class="list">${data.items.length ? data.items.map((row) => `<article><div>
      <b>${escapeHtml(row.logged_at)}</b><span>${escapeHtml(row.email)}</span>
      <span>${row.remember_me ? "Remember me" : "Standard session"} · ${escapeHtml(row.user_agent || "Unknown device")}</span>
    </div></article>`).join("") : "<p>No recorded logins.</p>"}</div>
    <nav aria-label="Login activity pages">
      <button type="button" data-login-activity="${escapeAttr(id)}" data-offset="${Math.max(0, offset - 50)}" ${offset === 0 ? "disabled" : ""}>Previous</button>
      <span>Page ${Math.floor(offset / 50) + 1}</span>
      <button type="button" data-login-activity="${escapeAttr(id)}" data-offset="${offset + 50}" ${data.hasMore ? "" : "disabled"}>Next</button>
    </nav><form method="dialog"><button>Close</button></form>`;
  if (!dialog.open) dialog.showModal();
}
