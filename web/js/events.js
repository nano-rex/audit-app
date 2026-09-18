document.querySelectorAll("[data-open]").forEach((button) => {
  button.addEventListener("click", async () => {
    const dialog = document.getElementById(button.dataset.open);
    if (button.dataset.open === "schedule") {
      await resetScheduleForm();
    }
    if (button.dataset.open === "new-audit") await resetNewAuditForm();
    dialog.showModal();
  });
});

document.addEventListener("click", async (event) => {
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
    if (!confirm("Return to scheduled work? Save your progress first to keep any changes.")) return;
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

  const menuOpenTab = event.target.closest("[data-menu-open-tab]");
  if (menuOpenTab) {
    showTab(menuOpenTab.dataset.menuOpenTab);
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
    } catch (error) {
      alert(error.message);
    }
    return;
  }

  const deleteInspectionButton = event.target.closest("[data-delete-inspection-session]");
  if (deleteInspectionButton && confirm("Delete this inspection history item?")) {
    await requestJson(`/api/inspection-sessions/${deleteInspectionButton.dataset.deleteInspectionSession}`, "DELETE");
    if (localStorage.getItem(lastInspectionSessionKey) === deleteInspectionButton.dataset.deleteInspectionSession) {
      localStorage.removeItem(lastInspectionSessionKey);
    }
    loadInspectionHistory();
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
});

document.querySelector("[data-menu-tabs]")?.addEventListener("change", (event) => {
  const toggle = event.target.closest("[data-navbar-tab-toggle]");
  if (!toggle) return;
  const tabId = toggle.dataset.navbarTabToggle;
  if (toggle.checked) {
    if (!navbarTabs.includes(tabId)) navbarTabs.push(tabId);
  } else {
    navbarTabs = navbarTabs.filter((id) => id !== tabId);
  }
  if (!navbarTabs.length) {
    navbarTabs = [...defaultNavbarTabs];
  }
  applyNavbarTabs();
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

document.querySelector("[data-open-work-order]")?.addEventListener("click", async () => {
  await openWorkOrderEditor();
});

document.querySelector("[data-open-equipment]")?.addEventListener("click", async () => {
  resetEquipmentForm();
  await updateEquipmentLocationSelect();
  document.getElementById("equipment-dialog").showModal();
});

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

document.querySelector("[data-work-order-completion-photo]")?.addEventListener("change", async (event) => {
  const existingImages = workOrderCompletionPhotos();
  const newImages = await readFilesAsStoredImages(event.target.files);
  setWorkOrderCompletionPhotos([...existingImages, ...newImages]);
  event.target.value = "";
});

document.querySelector("[data-work-order-completion-photos]")?.addEventListener("click", (event) => {
  const button = event.target.closest("[data-delete-work-order-completion-photo]");
  if (!button) return;
  const images = workOrderCompletionPhotos();
  images.splice(Number(button.dataset.deleteWorkOrderCompletionPhoto), 1);
  setWorkOrderCompletionPhotos(images);
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

document.querySelectorAll("[data-open-signature]").forEach((button) => {
  button.addEventListener("click", () => openSignatureDialog(button.dataset.openSignature));
});

document.querySelector("[data-save-inspection-signatures]")?.addEventListener("click", async () => {
  const id = document.getElementById("inspection-form").elements.inspectionSessionId.value;
  if (!id) { alert("Save the inspection before saving signatures."); return; }
  try {
    await requestJson(`/api/inspection-sessions/${id}`, "PATCH", { signatures: inspectionSignatures() });
    await openInspectionSession(id);
    alert("Signatures saved.");
  } catch (error) { alert(error.message); }
});

document.querySelector("[data-signature-canvas]")?.addEventListener("pointerdown", (event) => {
  if (!signatureState) return;
  const point = photoMarkerPoint(event);
  signatureState.drawing = true;
  signatureState.lastX = point.x;
  signatureState.lastY = point.y;
});

document.querySelector("[data-signature-canvas]")?.addEventListener("pointermove", (event) => {
  if (!signatureState?.drawing) return;
  const canvas = event.currentTarget;
  const ctx = canvas.getContext("2d");
  const point = photoMarkerPoint(event);
  ctx.strokeStyle = "#111d27";
  ctx.lineWidth = 3;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.moveTo(signatureState.lastX, signatureState.lastY);
  ctx.lineTo(point.x, point.y);
  ctx.stroke();
  signatureState.lastX = point.x;
  signatureState.lastY = point.y;
});

document.querySelector("[data-signature-canvas]")?.addEventListener("pointerup", () => {
  if (signatureState) signatureState.drawing = false;
});

document.querySelector("[data-signature-clear]")?.addEventListener("click", () => {
  const canvas = document.querySelector("[data-signature-canvas]");
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
});

document.querySelector("[data-signature-upload]")?.addEventListener("change", async (event) => {
  const [image] = await readFilesAsStoredImages(event.target.files);
  if (!imageSource(image)) return;
  const canvas = document.querySelector("[data-signature-canvas]");
  const ctx = canvas.getContext("2d");
  const source = new Image();
  source.addEventListener("load", () => {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    const ratio = Math.min(canvas.width / source.width, canvas.height / source.height);
    const width = source.width * ratio;
    const height = source.height * ratio;
    ctx.drawImage(source, (canvas.width - width) / 2, (canvas.height - height) / 2, width, height);
  });
  source.src = imageSource(image);
  event.target.value = "";
});

document.getElementById("signature-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!signatureState?.kind) return;
  const form = event.currentTarget;
  const canvas = document.querySelector("[data-signature-canvas]");
  const signatures = inspectionSignatures();
  signatures[signatureState.kind] = await uploadImage({
    name: formValue(form, "signatureName", ""),
    dataUrl: canvas.toDataURL("image/png"),
    signedAt: todayIsoDate(),
  });
  setInspectionSignatures(signatures);
  form.closest("dialog").close();
});

document.querySelector('#schedule-form select[name="outlet"]')?.addEventListener("change", () => {
  updateScheduleLocationSelect();
});

document.querySelector('#inspection-form select[name="outlet"]')?.addEventListener("change", () => {
  updateInspectionLocationSelect();
});

checklistContainer?.addEventListener("change", async (event) => {
  const input = event.target.closest("[data-inspection-check]");
  if (!input) {
    const naInput = event.target.closest("[data-inspection-na]");
    if (naInput) {
      const row = naInput.closest(".criteria-row");
      const pass = row.querySelector("[data-inspection-check]");
      const notes = row.querySelector('input[name*="-notes-"]');
      if (naInput.checked && pass) pass.checked = false;
      row.classList.toggle("passed", naInput.checked || Boolean(pass?.checked));
      if (notes) {
        notes.disabled = naInput.checked || Boolean(pass?.checked);
        if (notes.disabled) notes.value = "";
      }
      updateInspectionProgress();
      return;
    }
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
  const naInput = row.querySelector("[data-inspection-na]");
  if (input.checked && naInput) naInput.checked = false;
  row.classList.toggle("passed", input.checked);
  if (notes) {
    notes.disabled = input.checked;
    if (input.checked) notes.value = "";
  }
  updateInspectionProgress();
  if (input.checked) return;
  const detail = JSON.parse(input.dataset.inspectionCheck);
  const department = setupOptions.departments[0] || "";
  const category = setupOptions.categories[0] || "";
  await openWorkOrderEditor({
    outlet: detail.outlet,
    zone: detail.location,
    request_type: department,
    category,
    priority: "High",
    status: "Assigned",
    assignee: "Technical Support",
    title: `${detail.name} - ${detail.criterion}`,
    description: [
      `Fixed asset: ${detail.name}`,
      detail.code ? `Code: ${detail.code}` : "",
      `Type: ${detail.type}`,
      `Failed check: ${detail.criterion}`,
    ].filter(Boolean).join("\n"),
    images_json: row.closest("[data-equipment-id]").dataset.savedImages || "[]",
  });
  activeFindingRow = row;
  const findingForm = document.getElementById("work-order-form");
  findingForm.querySelector("h2").textContent = "Record Audit Finding";
  findingForm.querySelector('button[type="submit"]').textContent = "Save Finding to Draft";
  findingForm.querySelector("[data-corrective-fields]").hidden = true;
  findingForm.querySelector("[data-verification-fields]").hidden = true;
});

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

checklistContainer?.addEventListener("click", (event) => {
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

["corrective-search", "corrective-filter-outlet", "corrective-filter-department", "corrective-filter-status"].forEach((id) => {
  document.getElementById(id)?.addEventListener("input", renderCorrectiveActions);
  document.getElementById(id)?.addEventListener("change", renderCorrectiveActions);
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
