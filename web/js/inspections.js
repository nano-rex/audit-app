async function applyInspectionSchedule(row) {
  if (!row?.id) throw new Error("This schedule has no ID. Reload the scheduled work list.");
  const result = await requestJson("/api/schedules/start", "POST", { scheduleId: row.id });
  await openInspectionSession(result.id);
}

let guidedSchedules = [];
let inspectionLocationEquipment = new Map();
let inspectionLocationZones = new Map();
let inspectionPageDrafts = new Map();

function showGuidedContent(open) {
  document.querySelector("[data-guided-content]").hidden = !open;
  document.querySelector("[data-guided-schedules-panel]").hidden = open;
}

async function loadGuidedSchedules() {
  const response = await authFetch("/api/schedules");
  guidedSchedules = (await response.json()).items || [];
  renderGuidedSchedules();
}

// Drafts started without a schedule (older inspections) are work in progress too, so they are
// listed with the schedules instead of only in History.
function unscheduledDrafts() {
  return inspectionHistoryCache.filter((row) => !row.schedule_id && row.status !== "Completed" && !row.closed_at);
}

function renderGuidedSchedules() {
  // A completed visit is done: it is found in History, not here.
  const completed = (row) => row.status === "Completed" || row.inspection_status === "Completed";
  const cancelled = (row) => row.status === "Cancelled";
  // Only mine: visits assigned to me, and drafts I started.
  const mine = document.querySelector("[data-only-mine]")?.checked;
  const isMine = (entry) => entry.kind === "draft"
    ? entry.row.owner_user_id === currentUser?.id
    : (entry.row.assignees || []).includes(currentUser?.id) || inspectionHistoryCache.some((session) => session.schedule_id === entry.row.id && session.owner_user_id === currentUser?.id);
  const work = [
    ...guidedSchedules.filter((row) => !completed(row) && !cancelled(row)).map((row) => ({ kind: "schedule", row })),
    ...unscheduledDrafts().map((row) => ({ kind: "draft", row })),
    ...guidedSchedules.filter((row) => cancelled(row) && !completed(row)).map((row) => ({ kind: "schedule", row })),
  ];
  const shown = mine ? work.filter(isMine) : work;
  const page = paginateList("scheduled-work", shown, mine ? "mine" : "", renderGuidedSchedules);
  setHtml("[data-guided-schedules]", (page.items.length
    ? page.items.map((entry) => entry.kind === "draft" ? unscheduledDraftRow(entry.row) : scheduleRow(entry.row)).join("")
    : "<p>No scheduled work. Create a schedule to begin; completed audits are in History.</p>") + page.controls);
}

function unscheduledDraftRow(row) {
  const status = inspectionHistoryProgressStatus(row);
  const auditor = (currentUser?.inspectionPermissions || []).includes("auditor");
  return `
    <article>
      <div>
        <b>${escapeHtml(auditTitle(row, ""))}</b>
        <span>${escapeHtml(row.audit_date)} | ${escapeHtml(lastSaved(row))}</span>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(row.zone || "No location")} | ${escapeHtml(row.auditor || "No auditor")} | ${escapeHtml(scheduleLabel(null))}</span>
      </div>
      <span class="row-actions">
        <span class="status-pill ${status.className}">${escapeHtml(status.label)}</span>
        <button type="button" class="primary" data-open-inspection-session="${Number(row.id)}">Open</button>
        ${auditor ? `<button type="button" class="danger" data-delete-inspection-session="${Number(row.id)}">Delete</button>` : ""}
      </span>
    </article>
  `;
}

function openScheduledInspection(row) {
  pendingInspectionSchedule = row;
  showTab("inspections");
}

function showInspectionSubtab(tabId) {
  if (tabId === "guided") showGuidedContent(false);
  document.querySelectorAll("[data-inspection-subtab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.inspectionSubtab === tabId);
  });
  document.querySelectorAll("[data-inspection-panel]").forEach((panel) => {
    panel.classList.toggle("active", panel.dataset.inspectionPanel === tabId);
  });
}
async function loadInspectionItems() {
  const form = document.getElementById("inspection-form");
  if (!form || !checklistContainer) return;
  const outlet = form.elements.outlet.value;
  if (!outlet) {
    inspectionItems = [];
    checklistContainer.innerHTML = `<article class="check-item"><div><span>Outlet Required</span><strong>Select an outlet to load inspection items.</strong></div></article>`;
    return;
  }
  const [equipmentResponse, locationResponse, zoneResponse] = await Promise.all([
    authFetch(`/api/equipment?outlet=${encodeURIComponent(outlet)}&view=inspection`),
    authFetch(`/api/locations?outlet=${encodeURIComponent(outlet)}`),
    authFetch(`/api/zones?outlet=${encodeURIComponent(outlet)}`),
  ]);
  const equipmentData = await equipmentResponse.json();
  const locationData = await locationResponse.json();
  const zoneData = await zoneResponse.json();
  // A visit scheduled for particular locations shows only those; an empty list means all.
  const scope = new Set(visitLocations(form));
  const inScope = (name) => !scope.size || scope.has(name);
  // A visit for particular assets shows those assets only.
  const assets = new Set(visitAssets(form).map(String));
  inspectionItems = equipmentData.items.filter((item) => inScope(item.location || item.zone || "Unassigned")
    && (!assets.size || assets.has(String(item.id))));
  // A casual audit checks the same-named assets of a location once, as one group.
  if (form.dataset.auditStyle === "Casual") inspectionItems = groupForCasualAudit(inspectionItems);
  inspectionPageDrafts = new Map();
  renderInspectionFilter();
  const locationNames = new Set(locationData.items.map((location) => location.name).filter(inScope));
  inspectionItems.forEach((item) => {
    const location = item.location || item.zone || "Unassigned";
    locationNames.add(location);
  });
  inspectionLocationEquipment = new Map();
  inspectionItems.forEach((item) => {
    const location = item.location || item.zone || "Unassigned";
    if (!inspectionLocationEquipment.has(location)) inspectionLocationEquipment.set(location, []);
    inspectionLocationEquipment.get(location).push(item);
  });
  const mergedLocations = [...locationNames].sort().map((name) => ({ name }));
  checklistContainer.innerHTML = mergedLocations.length
    ? renderInspectionZones(mergedLocations, zoneData.items, inspectionItems)
    : `<article class="check-item"><div><span>No Items</span><strong>No locations or fixed assets are set up for this outlet yet.</strong></div></article>`;
  applyInspectionSessionItems();
  openFirstInspectionLocation();
  updateInspectionProgress();
}

// Categories were retired: an item is filed under its asset type.
function assetTypeOf(item) {
  return item?.type || item?.equipment_type || "";
}

// Assets with the same name in the same location become one item, keyed by the lowest asset id so
// a saved draft finds its group again. The group's checks apply to every asset in it.
function groupForCasualAudit(items) {
  const groups = new Map();
  [...items].sort((a, b) => Number(a.id) - Number(b.id)).forEach((item) => {
    const key = `${item.location || item.zone || "Unassigned"}\u0001${String(item.name || item.asset_id || "").trim().toLowerCase()}`;
    const group = groups.get(key);
    if (group) {
      group.groupIds.push(item.id);
      group.groupCount += 1;
    } else {
      groups.set(key, { ...item, groupIds: [item.id], groupCount: 1 });
    }
  });
  return [...groups.values()];
}

function visitAssets(form) {
  try {
    const value = JSON.parse(form.dataset.visitAssets || "[]");
    return Array.isArray(value) ? value : [];
  } catch (error) {
    return [];
  }
}

function visitLocations(form) {
  try {
    const value = JSON.parse(form.dataset.visitLocations || "[]");
    return Array.isArray(value) ? value : [];
  } catch (error) {
    return [];
  }
}

function renderInspectionZones(locations, zones, equipment) {
  const locationNames = locations.map((location) => location.name);
  const locationSet = new Set(locationNames);
  const assigned = new Set();
  // A location listed in two zones is inspected once, under the first; a second copy would
  // split its items from the tab that shows it.
  const normalizedZones = zones.map((zone) => {
    const own = (zone.locations || []).filter((name) => locationSet.has(name) && !assigned.has(name));
    own.forEach((name) => assigned.add(name));
    return { ...zone, locations: own };
  });
  const unassigned = locationNames.filter((name) => !assigned.has(name));
  let zoneOne = normalizedZones.find((zone) => zone.name.toLowerCase() === "zone-1");
  if (!zoneOne) {
    zoneOne = { name: "Zone-1", locations: [] };
    normalizedZones.unshift(zoneOne);
  }
  zoneOne.locations = [...new Set([...zoneOne.locations, ...unassigned])];
  const activeZones = normalizedZones.filter((zone) => zone.locations.length);
  inspectionLocationZones = new Map(activeZones.flatMap((zone) => zone.locations.map((location) => [location, zone.name])));
  return `<div class="inspection-zone-tabs" role="tablist" aria-label="Inspection zones">
    ${activeZones.map((zone, index) => `<button type="button" class="${index === 0 ? "active" : ""}" data-open-inspection-zone="${escapeAttr(zone.name)}" role="tab" aria-selected="${index === 0}">${escapeHtml(zone.name)} <small data-zone-tab-status="${escapeAttr(zone.name)}">(0%)</small></button>`).join("")}
  </div>${activeZones.map((zone) => inspectionZoneCard(zone.name, zone.locations, equipment)).join("")}`;
}

function inspectionZoneCard(zoneName, locations, equipment) {
  return `
    <section class="inspection-zone" data-inspection-zone="${escapeAttr(zoneName)}">
      <header>
        <h3>${escapeHtml(zoneName)}</h3>
        <span class="status-pill status-untouched" data-zone-status="${escapeAttr(zoneName)}">(0%)</span>
      </header>
      <div class="inspection-location-nav">
        ${locations.map((location, index) => `
          <button class="${index === 0 ? "active" : ""}" type="button" data-open-inspection-location="${escapeAttr(location)}">
            <span>${escapeHtml(location)}</span>
            <small class="status-pill status-untouched" data-location-nav-status="${escapeAttr(location)}">(0%)</small>
          </button>
        `).join("")}
      </div>
      ${locations.map((location) => inspectionLocationShell(location)).join("")}
    </section>
  `;
}

function inspectionLocationShell(location) {
  return `<section class="inspection-location" data-inspection-location="${escapeAttr(location)}" hidden>
    <header><h4>${escapeHtml(location)}</h4><span class="status-pill status-untouched" data-location-status="${escapeAttr(location)}">(0%)</span></header>
    <div data-location-items>${loadingMarkup("Loading items…")}</div>
  </section>`;
}

function captureInspectionPageDrafts(location) {
  const container = [...document.querySelectorAll("[data-inspection-location]")].find((node) => node.dataset.inspectionLocation === location)?.querySelector("[data-location-items]");
  if (!container?.querySelector("[data-equipment-id]")) return;
  const form = document.getElementById("inspection-form");
  const formData = new FormData(form);
  container.querySelectorAll("[data-equipment-id]").forEach((row) => {
    const equipmentId = row.dataset.equipmentId;
    const images = storedImagesFromDataset(row);
    row.querySelectorAll("[data-criterion]").forEach((criterionRow, index) => {
      const key = `${equipmentId}:${criterionRow.dataset.criterion}`;
      inspectionPageDrafts.set(key, {
        ...parseStoredObject(criterionRow.dataset.findingDetails), equipmentId,
        item: criterionRow.dataset.criterion,
        passed: formData.get(`equipment-${equipmentId}-criterion-${index}`) === "pass",
        notApplicable: false,
        notes: formData.get(`equipment-${equipmentId}-notes-${index}`) || "",
        images,
      });
    });
  });
}

function renderInspectionLocationItems(location) {
  captureInspectionPageDrafts(location);
  const section = [...document.querySelectorAll("[data-inspection-location]")].find((node) => node.dataset.inspectionLocation === location);
  const container = section?.querySelector("[data-location-items]");
  if (!container) return;
  const all = inspectionLocationEquipment.get(location) || [];
  const items = all.filter(matchesInspectionFilter);
  const page = paginateList(`inspection-location-${location}`, items, { location, ...inspectionFilter }, () => renderInspectionLocationItems(location));
  const empty = all.length
    ? `<article class="check-item"><div><span>Filtered</span><strong>Nothing in this location matches the filter.</strong></div></article>`
    : `<article class="check-item"><div><span>Nothing to inspect</span><strong>No fixed or variable assets are assigned to this location.</strong></div></article>`;
  container.innerHTML = (page.items.length ? page.items.map(inspectionItemCard).join("") : empty) + page.controls;
  applyInspectionSessionItems();
  page.items.forEach((equipment) => {
    const rows = container.querySelectorAll(`[data-equipment-id="${equipment.id}"] [data-criterion]`);
    rows.forEach((criterionRow) => {
      const draft = inspectionPageDrafts.get(`${equipment.id}:${criterionRow.dataset.criterion}`);
      if (!draft) return;
      criterionRow.dataset.findingDetails = JSON.stringify(draft);
      renderFindingSummary(criterionRow);
      const id = equipment.id;
      const index = [...criterionRow.parentElement.querySelectorAll("[data-criterion]")].indexOf(criterionRow);
      const check = criterionRow.querySelector("[data-inspection-check]");
      const note = criterionRow.querySelector('input[name*="-notes-"]');
      if (check) check.checked = draft.passed;
      if (note) { note.value = draft.notes || ""; note.disabled = draft.passed; }
      const row = criterionRow.closest("[data-equipment-id]");
      if (row && draft.images) { row.dataset.savedImages = JSON.stringify(draft.images); row.querySelector("[data-saved-images]").innerHTML = renderInspectionImages(draft.images); }
    });
  });
}

function matchesInspectionFilter(item) {
  return (!inspectionFilter.kind || (item.kind || "asset") === inspectionFilter.kind)
    && (!inspectionFilter.category || assetTypeOf(item) === inspectionFilter.category);
}

// Offer the asset types the outlet's items actually use, and say how much of the checklist is shown.
function renderInspectionFilter() {
  const select = document.querySelector("[data-inspection-filter-category]");
  if (!select) return;
  const categories = [...new Set(inspectionItems.map(assetTypeOf).filter(Boolean))].sort();
  if (inspectionFilter.category && !categories.includes(inspectionFilter.category)) inspectionFilter.category = "";
  updateSelectOptions(select, categories, true, "All asset types");
  select.value = inspectionFilter.category;
  const kind = document.querySelector("[data-inspection-filter-kind]");
  if (kind) kind.value = inspectionFilter.kind;
  const shown = inspectionItems.filter(matchesInspectionFilter).length;
  const filtered = Boolean(inspectionFilter.kind || inspectionFilter.category);
  setText("[data-inspection-filter-summary]", filtered
    ? `Showing ${shown} of ${inspectionItems.length} items. Progress below counts the shown items; the whole checklist must be finished to complete the inspection.`
    : `${inspectionItems.length} items to inspect.`);
}

function applyInspectionFilter() {
  renderInspectionFilter();
  // Re-render the locations already opened; the others are filtered when they are opened.
  document.querySelectorAll("[data-inspection-location]").forEach((section) => {
    if (section.dataset.loaded) renderInspectionLocationItems(section.dataset.inspectionLocation);
  });
  updateInspectionProgress();
}

function inspectionItemCard(item) {
  const criteria = parseInspectionCriteria(item.inspection_criteria);
  return `
    <article class="check-item inspection-item" data-equipment-id="${item.id}">
      <div>
        <span>${escapeHtml(item.kind === "fixture" ? "Variable asset" : "Fixed asset")} | ${escapeHtml(assetTypeOf(item) || "No asset type")} | ${escapeHtml(item.groupCount > 1 ? `${item.groupCount} assets` : item.code || item.asset_id || "")}</span>
        <strong>${escapeHtml(item.name || item.asset_id || "Fixed asset")}${item.groupCount > 1 ? ` <span class="group-count">×${item.groupCount}</span>` : ""}</strong>
        ${item.groupCount > 1 ? `<small class="muted">${item.groupCount} assets with this name here are graded together</small>` : ""}
      </div>
      <button class="outline pass-all" type="button" data-pass-all>Pass all</button>
      <div class="image-field"><span class="image-field-label">Images</span><div class="image-tiles"><label class="image-pick"><input type="file" name="equipment-${item.id}-images" accept="image/*" capture="environment" multiple data-equipment-images aria-label="Add images"><span>Choose file</span></label><div class="saved-images" data-saved-images></div></div></div>
      ${criteria.map((criterion, index) => `
        <div class="criteria-row" data-criterion="${escapeAttr(criterion)}">
          <label><input type="checkbox" name="equipment-${item.id}-criterion-${index}" value="pass" data-inspection-check='${escapeAttr(JSON.stringify({
            equipmentId: item.id,
            name: item.name || item.asset_id || "Fixed asset",
            code: item.code || item.asset_id || "",
            type: item.type || item.equipment_type || "Fixed Asset",
            outlet: item.outlet,
            location: item.location || item.zone || "",
            category: assetTypeOf(item),
            criterion,
          }))}'> ${escapeHtml(criterion)}</label>
          <button class="outline" type="button" data-record-finding>Finding details</button>
          <input name="equipment-${item.id}-notes-${index}" placeholder="Remark if not passed">
          <small class="finding-summary" data-finding-summary hidden></small>
        </div>
      `).join("")}
    </article>
  `;
}

function renderFindingSummary(criterionRow) {
  const summary = criterionRow?.querySelector("[data-finding-summary]");
  if (!summary) return;
  const details = parseStoredObject(criterionRow.dataset.findingDetails);
  // Every item carries its asset type; a priority means finding details were recorded for this criterion.
  const parts = details.priority ? [details.priority, details.category, details.assignedDepartment, details.pic && `PIC ${details.pic}`].filter(Boolean) : [];
  summary.textContent = parts.length ? `Finding: ${parts.join(" · ")}` : "";
  summary.hidden = !parts.length;
}

function renderInspectionImages(images) {
  return renderSavedImageList(images, "data-delete-inspection-image", "data-mark-inspection-image");
}

function renderWorkOrderEvidence(images) {
  return renderSavedImageList(images, "data-delete-work-evidence", "data-mark-work-evidence");
}

function openPhotoMarker(itemRow, imageIndex) {
  const images = storedImagesFromDataset(itemRow);
  const image = images[imageIndex];
  if (!itemRow || !imageSource(image)) return;
  const dialog = document.getElementById("photo-mark-dialog");
  const canvas = document.querySelector("[data-photo-mark-canvas]");
  const ctx = canvas.getContext("2d");
  const source = new Image();
  source.addEventListener("load", () => {
    const ratio = Math.min(960 / source.width, 640 / source.height, 1);
    canvas.width = Math.max(1, Math.round(source.width * ratio));
    canvas.height = Math.max(1, Math.round(source.height * ratio));
    photoMarkState = {
      itemRow,
      imageIndex,
      image,
      source,
      scale: ratio,
      tool: "rect",
      drawing: false,
      startX: 0,
      startY: 0,
      points: [],
      marks: structuredClone(image.marks || []),
      redoMarks: [],
    };
    renderPhotoMarker();
    dialog.showModal();
  });
  source.src = imageSource(image);
}

function renderPhotoMarker(preview = null) {
  if (!photoMarkState) return;
  const canvas = document.querySelector("[data-photo-mark-canvas]");
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(photoMarkState.source, 0, 0, canvas.width, canvas.height);
  [...photoMarkState.marks, preview].filter(Boolean).forEach((mark) => drawPhotoMark(ctx, mark));
}

function drawPhotoMark(ctx, mark) {
  ctx.save();
  ctx.strokeStyle = "#f34047";
  ctx.fillStyle = "#f34047";
  ctx.lineWidth = 4;
  ctx.font = "22px Arial";
  if (mark.tool === "rect") {
    ctx.strokeRect(mark.x, mark.y, mark.w, mark.h);
  } else if (mark.tool === "circle") {
    ctx.beginPath();
    ctx.ellipse(mark.x + mark.w / 2, mark.y + mark.h / 2, Math.abs(mark.w / 2), Math.abs(mark.h / 2), 0, 0, Math.PI * 2);
    ctx.stroke();
  } else if (mark.tool === "arrow") {
    const endX = mark.x + mark.w;
    const endY = mark.y + mark.h;
    const angle = Math.atan2(mark.h, mark.w);
    ctx.beginPath();
    ctx.moveTo(mark.x, mark.y);
    ctx.lineTo(endX, endY);
    ctx.lineTo(endX - 18 * Math.cos(angle - Math.PI / 6), endY - 18 * Math.sin(angle - Math.PI / 6));
    ctx.moveTo(endX, endY);
    ctx.lineTo(endX - 18 * Math.cos(angle + Math.PI / 6), endY - 18 * Math.sin(angle + Math.PI / 6));
    ctx.stroke();
  } else if (mark.tool === "text") {
    ctx.fillText(mark.text || "Issue", mark.x, mark.y);
  } else if (mark.tool === "freehand") {
    ctx.beginPath();
    mark.points.forEach((point, index) => {
      if (index === 0) ctx.moveTo(point.x, point.y);
      else ctx.lineTo(point.x, point.y);
    });
    ctx.stroke();
  }
  ctx.restore();
}

function photoMarkerPoint(event) {
  const rect = event.currentTarget.getBoundingClientRect();
  return {
    x: (event.clientX - rect.left) * (event.currentTarget.width / rect.width),
    y: (event.clientY - rect.top) * (event.currentTarget.height / rect.height),
  };
}

async function saveMarkedPhoto() {
  if (!photoMarkState) return;
  const canvas = document.querySelector("[data-photo-mark-canvas]");
  const images = storedImagesFromDataset(photoMarkState.itemRow);
  const current = images[photoMarkState.imageIndex];
  if (!current) return;
  const marked = await uploadImage({ name: "marked-photo.png", dataUrl: canvas.toDataURL("image/png") });
  current.markedUrl = marked.url;
  current.markedId = marked.id;
  delete current.markedDataUrl;
  current.markedName = current.name ? `marked-${current.name}` : "marked-photo.png";
  current.marks = photoMarkState.marks;
  photoMarkState.itemRow.dataset.savedImages = JSON.stringify(images);
  const savedImages = photoMarkState.itemRow.querySelector("[data-saved-images]");
  if (savedImages) savedImages.innerHTML = photoMarkState.itemRow.id === "work-order-form" ? renderWorkOrderEvidence(images) : renderInspectionImages(images);
  updateInspectionProgress();
  photoMarkState = null;
}

function parseInspectionCriteria(value) {
  if (Array.isArray(value)) return value.filter(Boolean);
  if (!value) return defaultInspectionCriteria;
  try {
    const parsed = JSON.parse(value);
    if (Array.isArray(parsed) && parsed.length) return parsed.filter(Boolean);
  } catch (error) {
    const lines = String(value).split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
    if (lines.length) return lines;
  }
  return defaultInspectionCriteria;
}

function renderEquipmentCriteria(criteria = defaultInspectionCriteria) {
  const container = document.querySelector("[data-equipment-criteria]");
  if (!container) return;
  container.innerHTML = criteria.map((criterion) => `
    <label class="criterion-edit-row">
      <input name="inspectionCriteria" value="${escapeAttr(criterion)}" placeholder="Inspection criterion">
      <button class="outline" type="button" data-remove-equipment-criterion>Remove</button>
    </label>
  `).join("");
}

function addEquipmentCriterion(value = "") {
  const container = document.querySelector("[data-equipment-criteria]");
  if (!container) return;
  container.insertAdjacentHTML("beforeend", `
    <label class="criterion-edit-row">
      <input name="inspectionCriteria" value="${escapeAttr(value)}" placeholder="Inspection criterion">
      <button class="outline" type="button" data-remove-equipment-criterion>Remove</button>
    </label>
  `);
}

function collectEquipmentCriteria(form) {
  const values = [...form.querySelectorAll('input[name="inspectionCriteria"]')]
    .map((input) => input.value.trim())
    .filter(Boolean);
  return values.length ? values : defaultInspectionCriteria;
}

async function loadInspectionHistory() {
  const response = await authFetch("/api/inspection-sessions");
  const data = await response.json();
  inspectionHistoryCache = data.items || [];
  updateHistoryFilterSelects();
  renderInspectionHistory();
  // Scheduled Work also lists drafts without a schedule, which come from this list.
  if (document.querySelector("[data-guided-schedules]")) renderGuidedSchedules();
}

// The location report uses the History filters: one outlet, one location, and the dates chosen.
function updateLocationExport() {
  const ready = Boolean(historyFilters.outlet && historyFilters.location);
  const query = new URLSearchParams({ outlet: historyFilters.outlet, location: historyFilters.location, from: historyFilters.dateFrom || "", to: historyFilters.dateTo || "" });
  document.querySelectorAll("[data-location-export]").forEach((link) => {
    link.classList.toggle("disabled", !ready);
    link.href = ready ? `/api/location-report.${link.dataset.locationExport}?${query}` : "#";
  });
  setText("[data-location-export-hint]", ready
    ? `Every completed audit of ${historyFilters.location} at ${historyFilters.outlet}${historyFilters.dateFrom || historyFilters.dateTo ? " in the chosen dates" : ""}.`
    : "Choose an outlet and a location to export every audit of that location.");
}

function renderInspectionHistory() {
  updateLocationExport();
  const search = inspectionHistorySearch.toLowerCase();
  const rows = inspectionHistoryCache.filter((row) => {
    const savedAt = row.created_at ? new Date(row.created_at).toLocaleString() : "";
    const haystack = [row.audit_ref, row.inspection_name, scheduleLabel(row.schedule_id), row.id, row.audit_date, savedAt, row.outlet, row.zone, row.auditor, row.status, row.progress, ...(row.locations || []), ...(row.categories || []), ...(row.departments || []), ...(row.priorities || []), ...(row.pics || [])].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!historyFilters.dateFrom || row.audit_date >= historyFilters.dateFrom)
      && (!historyFilters.dateTo || row.audit_date <= historyFilters.dateTo)
      && (!historyFilters.outlet || row.outlet === historyFilters.outlet)
      && (!historyFilters.location || (row.locations || []).includes(historyFilters.location))
      && (!historyFilters.auditor || String(row.auditor || "").toLowerCase().includes(historyFilters.auditor.toLowerCase()))
      && (!historyFilters.department || (row.departments || []).includes(historyFilters.department))
      && (!historyFilters.category || (row.categories || []).includes(historyFilters.category))
      && (!historyFilters.priority || (row.priorities || []).includes(historyFilters.priority))
      && (!historyFilters.status || (row.closed_at ? "Closed" : row.status) === historyFilters.status)
      && (!historyFilters.pic || (row.pics || []).join(" ").toLowerCase().includes(historyFilters.pic.toLowerCase()));
  });
  const page = paginateList("inspections", rows, { ...historyFilters, search }, renderInspectionHistory);
  setHtml("[data-inspection-history]", (page.items.length
    ? page.items.map(inspectionHistoryRow).join("")
    : `<article><div><b>No inspection history</b><span>Saved progress and completed inspections appear here.</span></div></article>`) + page.controls);
  setText("[data-inspection-history-count]", `${rows.length} ${rows.length === 1 ? "record" : "records"}`);
}

function inspectionHistoryRow(row) {
  const status = inspectionHistoryProgressStatus(row);
  return `
    <article>
      <div>
        <b>${escapeHtml(auditTitle(row, ""))}</b>
        <span>${escapeHtml(row.audit_date)} | ${escapeHtml(lastSaved(row))}</span>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(row.zone || "No location")} | ${escapeHtml(row.auditor || "No auditor")} | ${escapeHtml(scheduleLabel(row.schedule_id))} | Findings: ${escapeHtml(row.findings_count || 0)}</span>
      </div>
      <span class="row-actions">
        <span class="status-pill ${status.className}">${escapeHtml(status.label)}</span>
        ${(currentUser?.permissions || []).includes("inspections") ? `<button type="button" class="outline" data-open-inspection-session="${row.id}">Open</button>` : ""}
        ${row.audit_id ? `<button type="button" class="outline" data-view-inspection-findings="${row.audit_id}">Findings (${row.findings_count || 0})</button>` : ""}
        ${row.status === "Completed" ? `<a class="button-link outline" data-download href="/api/inspection-sessions/${row.id}/export.pdf">PDF</a>
        <a class="button-link outline" data-download href="/api/inspection-sessions/${row.id}/export.xlsx">Excel</a>` : ""}
        ${row.status === "Completed" && !row.closed_at && (currentUser?.inspectionPermissions || []).includes("verifier") ? `<button type="button" class="outline" data-close-inspection-session="${row.id}">Close audit</button>` : ""}
        ${row.status !== "Completed" && (currentUser?.inspectionPermissions || []).includes("auditor") ? `<button type="button" class="danger" data-delete-inspection-session="${row.id}">Delete</button>` : ""}
      </span>
    </article>
  `;
}

function inspectionHistoryProgressStatus(row) {
  const progress = Number(row.progress) || 0;
  if (row.closed_at) return { className: "status-complete", label: "Closed (100%)" };
  if (row.status === "Completed") return { className: "status-complete", label: `Completed (${progress}%)` };
  if (progress >= 100) return { className: "status-progress", label: "Not completed (100%)" };
  if (progress > 0) return { className: "status-progress", label: `In Progress (${progress}%)` };
  return { className: "status-untouched", label: "Not Started (0%)" };
}

function collectInspectionPayload(complete = false) {
  const form = document.getElementById("inspection-form");
  const formData = new FormData(form);
  const items = [];
  [...form.querySelectorAll("[data-equipment-id]")].forEach((row) => {
    const equipment = inspectionItems.find((item) => String(item.id) === row.dataset.equipmentId);
    const images = storedImagesFromDataset(row);
    row.querySelectorAll("[data-criterion]").forEach((criterionRow, index) => {
      const criterion = criterionRow.dataset.criterion;
      const passed = formData.get(`equipment-${row.dataset.equipmentId}-criterion-${index}`) === "pass";
      const details = parseStoredObject(criterionRow.dataset.findingDetails);
      items.push({
        ...details,
        equipmentId: row.dataset.equipmentId,
        ...(equipment?.groupCount > 1 ? { groupIds: equipment.groupIds, groupCount: equipment.groupCount } : {}),
        location: equipment?.location || equipment?.zone || "",
        section: equipment?.name || equipment?.asset_id || "Fixed Asset",
        item: criterion,
        // A finding is filed under the item's asset type unless another one was chosen for it.
        category: details.category || assetTypeOf(equipment || {}),
        passed,
        notApplicable: false,
        score: passed ? 100 : 0,
        evidenceStatus: images.length ? images.map(imageLabel).join(", ") : "Missing image",
        notes: formData.get(`equipment-${row.dataset.equipmentId}-notes-${index}`) || "",
        images,
      });
    });
  });
  const loadedIds = new Set([...form.querySelectorAll("[data-equipment-id]")].map((row) => row.dataset.equipmentId));
  // Locations are rendered lazily to keep large outlets responsive. Preserve
  // untouched locations from the session cache when saving the active one.
  inspectionItems.filter((equipment) => !loadedIds.has(String(equipment.id))).forEach((equipment) => {
    const drafted = parseInspectionCriteria(equipment.inspection_criteria).map((criterion) => inspectionPageDrafts.get(`${equipment.id}:${criterion}`)).filter(Boolean);
    const saved = drafted.length ? drafted : inspectionSessionItems.filter((item) => String(item.equipmentId) === String(equipment.id));
    if (saved.length) {
      // Checks kept from a location page left earlier carry the asset's name, location, and asset type.
      items.push(...saved.map((entry) => ({
        ...(equipment.groupCount > 1 ? { groupIds: equipment.groupIds, groupCount: equipment.groupCount } : {}),
        location: equipment.location || equipment.zone || "",
        section: equipment.name || equipment.asset_id || "Fixed Asset",
        category: assetTypeOf(equipment),
        notApplicable: false,
        score: entry.passed ? 100 : 0,
        evidenceStatus: (entry.images || []).length ? entry.images.map(imageLabel).join(", ") : "Missing image",
        ...entry,
        category: entry.category || assetTypeOf(equipment),
      })));
      return;
    }
    parseInspectionCriteria(equipment.inspection_criteria).forEach((criterion) => items.push({
      equipmentId: equipment.id,
      ...(equipment.groupCount > 1 ? { groupIds: equipment.groupIds, groupCount: equipment.groupCount } : {}),
      location: equipment.location || equipment.zone || "",
      section: equipment.name || equipment.asset_id || "Fixed Asset",
      item: criterion,
      category: assetTypeOf(equipment),
      passed: false,
      notApplicable: false,
      score: 0,
      evidenceStatus: "Missing image",
      notes: "",
      images: [],
    }));
  });
  return {
    businessUnit: currentUnit,
    outlet: form.elements.outlet.value,
    zone: form.dataset.zoneLabel || "All Locations",
    auditDate: formValue(form, "auditDate", todayIsoDate()),
    auditor: formValue(form, "auditor", "Unnamed Inspector"),
    auditTime: form.elements.auditTime.value || null,
    auditType: form.elements.auditType.value || null,
    remarks: form.elements.remarks.value || (form.dataset.remarksNull === "true" ? null : ""),
    complete,
    items,
    signatures: inspectionSignatures(),
  };
}

function inspectionSignatures() {
  const form = document.getElementById("inspection-form");
  return parseStoredObject(form?.dataset.signatures || "{}");
}

// Signatures are given on the Sign-off page; the checklist only carries them through a save.
function setInspectionSignatures(signatures = {}) {
  const form = document.getElementById("inspection-form");
  if (form) form.dataset.signatures = JSON.stringify(signatures || {});
}

function inspectionProgress(payload = collectInspectionPayload(false)) {
  if (!payload.items.length) return 0;
  const complete = payload.items.filter(isInspectionItemComplete).length;
  return Math.floor(complete * 100 / payload.items.length);
}

function isInspectionItemComplete(item) {
  return Boolean(item.passed || item.notApplicable || (item.notes || "").trim());
}

function statusForProgress(done, total) {
  if (!total || done === 0) return { className: "status-untouched" };
  if (done >= total) return { className: "status-complete" };
  return { className: "status-progress" };
}

function updateInspectionProgress() {
  const payload = collectInspectionPayload(false);
  const progress = inspectionProgress(payload);
  setText("[data-inspection-progress]", `${progress}% complete`);
  const bar = document.querySelector("[data-inspection-progress-bar]");
  if (bar) bar.value = progress;
  updateInspectionStatusPills(payload);
  updateInspectionActions(progress, payload);
}

function updateInspectionStatusPills(payload) {
  const summary = [];
  if (inspectionFilter.kind || inspectionFilter.category) {
    const shown = new Set(inspectionItems.filter(matchesInspectionFilter).map((item) => String(item.id)));
    payload = { ...payload, items: payload.items.filter((item) => shown.has(String(item.equipmentId))) };
  }
  document.querySelectorAll("[data-inspection-location]").forEach((section) => {
    const location = section.dataset.inspectionLocation;
    const items = payload.items.filter((item) => item.location === location);
    const done = items.filter(isInspectionItemComplete).length;
    const status = statusForProgress(done, items.length);
    const progress = inspectionProgress({ items });
    const pill = section.querySelector("[data-location-status]");
    // A location without fixed assets has nothing to inspect; do not show it as outstanding.
    const className = items.length ? status.className : "status-none";
    const label = items.length ? `(${progress}%)` : "None";
    if (pill) {
      pill.className = `status-pill ${className}`;
      pill.textContent = label;
    }
    document.querySelectorAll("[data-location-nav-status]").forEach((navPill) => {
      if (navPill.dataset.locationNavStatus !== location) return;
      navPill.className = `status-pill ${className}`;
      navPill.textContent = label;
    });
  });
  document.querySelectorAll("[data-inspection-zone]").forEach((section) => {
    const locationNames = [...section.querySelectorAll("[data-inspection-location]")].map((node) => node.dataset.inspectionLocation);
    const items = payload.items.filter((item) => locationNames.includes(item.location));
    const status = statusForProgress(items.filter(isInspectionItemComplete).length, items.length);
    const progress = inspectionProgress({ items });
    const pill = section.querySelector("[data-zone-status]");
    if (pill) {
      pill.className = `status-pill ${status.className}`;
      pill.textContent = `(${progress}%)`;
    }
    summary.push(`<span class="status-pill ${status.className}">${escapeHtml(section.dataset.inspectionZone)} (${progress}%)</span>`);
    const tabStatus = [...document.querySelectorAll("[data-zone-tab-status]")].find((node) => node.dataset.zoneTabStatus === section.dataset.inspectionZone);
    if (tabStatus) tabStatus.textContent = `(${progress}%)`;
  });
  setHtml("[data-inspection-zone-progress]", summary.join(""));
}

function openInspectionLocation(location) {
  const section = [...document.querySelectorAll("[data-inspection-location]")].find((node) => node.dataset.inspectionLocation === location);
  const zoneName = inspectionLocationZones.get(location);
  if (zoneName) activateInspectionZone(zoneName);
  if (section && !section.dataset.loaded) {
    const items = inspectionLocationEquipment.get(location) || [];
    const container = section.querySelector("[data-location-items]");
    container.innerHTML = items.length ? "" : `<article class="check-item"><div><span>Nothing to inspect</span><strong>No fixed or variable assets are assigned to this location.</strong></div></article>`;
    section.dataset.loaded = "true";
    if (items.length) renderInspectionLocationItems(location);
  }
  document.querySelectorAll("[data-inspection-location]").forEach((section) => {
    section.hidden = section.dataset.inspectionLocation !== location;
  });
  document.querySelectorAll("[data-open-inspection-location]").forEach((button) => {
    button.classList.toggle("active", button.dataset.openInspectionLocation === location);
  });
}

function activateInspectionZone(zoneName) {
  document.querySelectorAll("[data-inspection-zone]").forEach((zone) => {
    zone.hidden = zone.dataset.inspectionZone !== zoneName;
  });
  document.querySelectorAll("[data-open-inspection-zone]").forEach((button) => {
    const active = button.dataset.openInspectionZone === zoneName;
    button.classList.toggle("active", active);
    button.setAttribute("aria-selected", String(active));
  });
}

function openInspectionZone(zoneName) {
  activateInspectionZone(zoneName);
  const zone = [...document.querySelectorAll("[data-inspection-zone]")].find((node) => node.dataset.inspectionZone === zoneName);
  const first = zone?.querySelector("[data-open-inspection-location]");
  if (first) openInspectionLocation(first.dataset.openInspectionLocation);
}

function openFirstInspectionLocation() {
  const first = document.querySelector("[data-open-inspection-location]");
  if (first) openInspectionLocation(first.dataset.openInspectionLocation);
}

// Failed checks always need photo evidence; passed ones only when the workflow option asks for every asset.
function inspectionItemNeedsPhoto(item) {
  if (item.notApplicable) return false;
  return !item.passed || setupOptions.settings["system.requirePhotoEveryAsset"] !== false;
}

// What still stands between a checklist and completion, in the words shown beside the button.
function inspectionBlockers(payload) {
  if (!payload.items.length) return [];
  const unchecked = payload.items.filter((item) => !isInspectionItemComplete(item));
  const needPhoto = [...new Set(payload.items.filter((item) => inspectionItemNeedsPhoto(item) && !(item.images || []).length).map((item) => item.section || "an item"))];
  const blockers = [];
  if (unchecked.length) blockers.push(`${unchecked.length} check${unchecked.length === 1 ? "" : "s"} still to tick, or to give a remark if failed`);
  if (needPhoto.length) {
    const names = needPhoto.slice(0, 5).join(", ") + (needPhoto.length > 5 ? ` and ${needPhoto.length - 5} more` : "");
    blockers.push(`a photo for ${names}${setupOptions.settings["system.requirePhotoEveryAsset"] !== false ? " (every inspected item needs one)" : ""}`);
  }
  return blockers;
}

function isInspectionReadyToComplete(payload) {
  return Boolean(payload.items.length && payload.items.every((item) => (!inspectionItemNeedsPhoto(item) || (item.images || []).length) && isInspectionItemComplete(item)));
}

function updateInspectionActions(progress, payload) {
  const button = document.querySelector("[data-save-inspection-progress]");
  const form = document.getElementById("inspection-form");
  const completed = form?.dataset.completed === "true";
  if (button) button.textContent = completed ? "Inspection Completed" : isInspectionReadyToComplete(payload) ? "Complete Inspection" : "Save Progress";
  // Say what is missing, so a checklist at 100% is not mistaken for a completed one.
  const blockers = completed ? [] : inspectionBlockers(payload);
  const note = document.querySelector("[data-inspection-blockers]");
  if (note) {
    note.hidden = !blockers.length || progress === 0;
    note.textContent = blockers.length ? `To complete this inspection, add ${blockers.join("; and ")}.` : "";
  }
  const id = form ? formValue(form, "inspectionSessionId", "") : "";
  const editable = (currentUser?.inspectionPermissions || []).includes("auditor") && !completed;
  if (button) button.disabled = !editable;
  // A completed or view-only checklist is shown as recorded.
  checklistContainer?.querySelectorAll("[data-inspection-check], [data-equipment-images], [data-record-finding], [data-pass-all], [data-delete-inspection-image], [data-mark-inspection-image]").forEach((control) => { control.disabled = !editable; });
  if (!editable) checklistContainer?.querySelectorAll('input[name*="-notes-"]').forEach((control) => { control.disabled = true; });
  document.querySelectorAll("[data-export-inspection]").forEach((link) => {
    link.href = id ? `/api/inspection-sessions/${id}/export.${link.dataset.exportInspection}` : "#";
    link.classList.toggle("disabled", !id);
  });
}

function validateInspectionComplete(payload) {
  if (!payload.items.length) return "No inspection items are loaded for this location.";
  const missingImage = payload.items.find((item) => inspectionItemNeedsPhoto(item) && !item.images.length);
  if (missingImage) return `Upload image(s) for ${missingImage.section}.`;
  const missingRemark = payload.items.find((item) => !item.passed && !item.notApplicable && !item.notes.trim());
  if (missingRemark) return `Enter a remark for unchecked criterion: ${missingRemark.section} - ${missingRemark.item}.`;
  return "";
}

function applyInspectionSessionItems() {
  if (!inspectionSessionItems.length) return;
  document.querySelectorAll("[data-equipment-id]").forEach((row) => {
    const saved = inspectionSessionItems.filter((item) => String(item.equipmentId) === row.dataset.equipmentId);
    if (!saved.length) return;
    const images = saved[0].images || [];
    row.dataset.savedImages = JSON.stringify(images);
    const savedImages = row.querySelector("[data-saved-images]");
    if (savedImages) {
      savedImages.innerHTML = renderInspectionImages(images);
    }
    row.querySelectorAll("[data-criterion]").forEach((criterionRow, index) => {
      const item = saved.find((entry) => entry.item === criterionRow.dataset.criterion) || saved[index];
      if (!item) return;
      criterionRow.dataset.findingDetails = JSON.stringify({
        category: item.category, priority: item.priority, assignedDepartment: item.assignedDepartment, pic: item.pic,
        cause: item.cause, recommendation: item.recommendation, requiredAction: item.requiredAction,
      });
      renderFindingSummary(criterionRow);
      const checkbox = criterionRow.querySelector("[data-inspection-check]");
      const notes = criterionRow.querySelector('input[name*="-notes-"]');
      checkbox.checked = Boolean(item.passed);
      criterionRow.classList.toggle("passed", checkbox.checked);
      if (notes) {
        notes.value = item.notes || "";
        notes.disabled = checkbox.checked;
      }
    });
  });
}

// Where the inspection's back button leads: Scheduled Work, or Sign-off when the audit was
// opened there for review ({ signoffId } to return to that audit's sign-off page).
let inspectionReturn = null;

async function openInspectionSession(id, returnTo = null) {
  inspectionsInitialized = true;
  inspectionReturn = returnTo;
  setText("[data-back-to-schedules]", returnTo ? "Back to Sign-off" : "Back to Scheduled Work");
  const response = await authFetch(`/api/inspection-sessions/${id}`);
  if (!response.ok) throw new Error(`Inspection could not be loaded (${response.status})`);
  const session = await response.json();
  const form = document.getElementById("inspection-form");
  form.elements.inspectionSessionId.value = session.id;
  form.dataset.completed = String(session.status === "Completed");
  form.dataset.closed = String(Boolean(session.closed_at));
  form.dataset.ownerId = session.owner_user_id ? String(session.owner_user_id) : "";
  setText("[data-current-schedule]", session.schedule_id ? `Schedule SCH-${String(session.schedule_id).padStart(5, "0")}` : "Saved inspection");
  setCurrentInspectionName(session.audit_ref || session.inspection_name || "", session.closed_at ? "Closed" : session.status === "Completed" ? "Completed" : "Editing");
  document.querySelector("[data-save-inspection-progress]").disabled = session.status === "Completed";
  form.elements.outlet.value = session.outlet || "";
  form.dataset.visitLocations = JSON.stringify(session.visit_locations || []);
  form.dataset.visitAssets = JSON.stringify(session.visit_scope?.by === "assets" ? session.visit_scope.assets || [] : []);
  form.dataset.auditStyle = session.audit_style || "Detailed";
  form.dataset.zoneLabel = session.zone || "All Locations";
  inspectionSessionItems = session.items || [];
  form.elements.auditDate.value = session.audit_date || "";
  form.elements.auditor.value = session.auditor || "";
  form.elements.auditTime.value = session.audit_time || "";
  const typeSelect = form.elements.auditType;
  updateSelectOptions(typeSelect, [...new Set([...setupOptions.auditTypes, session.audit_type || ""])], false);
  typeSelect.value = session.audit_type || "";
  form.elements.remarks.value = session.remarks || "";
  form.dataset.remarksNull = String(session.remarks == null);
  const readOnly = session.status === "Completed" || !(currentUser?.inspectionPermissions || []).includes("auditor");
  ["auditDate", "auditTime", "auditType", "remarks"].forEach((name) => { form.elements[name].disabled = readOnly; });
  // The audit type is chosen with the visit and sets how assets are grouped, so it stays as set.
  form.elements.auditType.disabled = true;
  form.elements.outlet.disabled = true;
  setInspectionSignatures(session.signatures || {});
  await updateInspectionLocationSelect();
  showTab("inspections");
  showInspectionSubtab("guided");
  showGuidedContent(true);
}
