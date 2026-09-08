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
  if (equipmentButton && confirm("Delete this equipment item?")) {
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
  inspectionHistoryPage = 1;
  renderInspectionHistory();
});

document.querySelector("[data-history-prev]")?.addEventListener("click", () => {
  inspectionHistoryPage -= 1;
  renderInspectionHistory();
});

document.querySelector("[data-history-next]")?.addEventListener("click", () => {
  inspectionHistoryPage += 1;
  renderInspectionHistory();
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
    photoMarkState.marks.pop();
    renderPhotoMarker();
  }
  if (event.target.closest("[data-mark-clear]") && photoMarkState) {
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
  photoMarkState.drawing = false;
  renderPhotoMarker();
});

document.getElementById("photo-mark-form")?.addEventListener("submit", (event) => {
  event.preventDefault();
  saveMarkedPhoto();
  event.currentTarget.closest("dialog").close();
});

document.querySelectorAll("[data-open-signature]").forEach((button) => {
  button.addEventListener("click", () => openSignatureDialog(button.dataset.openSignature));
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

document.getElementById("signature-form")?.addEventListener("submit", (event) => {
  event.preventDefault();
  if (!signatureState?.kind) return;
  const canvas = document.querySelector("[data-signature-canvas]");
  const signatures = inspectionSignatures();
  signatures[signatureState.kind] = {
    name: formValue(event.currentTarget, "signatureName", ""),
    dataUrl: canvas.toDataURL("image/png"),
    signedAt: todayIsoDate(),
  };
  setInspectionSignatures(signatures);
  event.currentTarget.closest("dialog").close();
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
      `Equipment: ${detail.name}`,
      detail.code ? `Code: ${detail.code}` : "",
      `Type: ${detail.type}`,
      `Failed check: ${detail.criterion}`,
    ].filter(Boolean).join("\n"),
  });
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
