async function applyInspectionSchedule(row) {
  const form = document.getElementById("inspection-form");
  if (!form || !row) return;
  await loadInspectionHistory();
  const matchingSession = inspectionHistoryCache.find((session) =>
    session.outlet === row.outlet
    && session.audit_date === row.scheduled_date
    && session.auditor === row.auditor
    && session.status !== "Completed"
  );
  if (matchingSession) {
    await openInspectionSession(matchingSession.id);
    return;
  }
  form.elements.inspectionSessionId.value = "";
  setCurrentInspectionName();
  inspectionSessionItems = [];
  updateSetupSelects();
  form.elements.outlet.value = row.outlet || "";
  form.elements.auditDate.value = row.scheduled_date || "";
  form.elements.auditor.value = row.auditor || "";
  setInspectionSignatures({});
  await updateInspectionLocationSelect();
}

function openScheduledInspection(row) {
  pendingInspectionSchedule = row;
  showTab("inspections");
}

function showInspectionSubtab(tabId) {
  document.querySelectorAll("[data-inspection-subtab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.inspectionSubtab === tabId);
  });
  document.querySelectorAll("[data-inspection-panel]").forEach((panel) => {
    panel.classList.toggle("active", panel.dataset.inspectionPanel === tabId);
  });
}
async function loadChecklist() {
  await updateInspectionLocationSelect();
}

async function loadInspectionItems() {
  const form = document.getElementById("inspection-form");
  if (!form || !checklistContainer) return;
  const outlet = formValue(form, "outlet", "");
  if (!outlet) {
    inspectionItems = [];
    checklistContainer.innerHTML = `<article class="check-item"><div><span>Outlet Required</span><strong>Select an outlet to load inspection items.</strong></div></article>`;
    return;
  }
  const [equipmentResponse, locationResponse, zoneResponse] = await Promise.all([
    fetch(`/api/equipment?outlet=${encodeURIComponent(outlet)}`),
    fetch(`/api/locations?outlet=${encodeURIComponent(outlet)}`),
    fetch(`/api/zones?outlet=${encodeURIComponent(outlet)}`),
  ]);
  const equipmentData = await equipmentResponse.json();
  const locationData = await locationResponse.json();
  const zoneData = await zoneResponse.json();
  inspectionItems = equipmentData.items;
  const locationNames = new Set(locationData.items.map((location) => location.name));
  inspectionItems.forEach((item) => {
    const location = item.location || item.zone || "Unassigned";
    locationNames.add(location);
  });
  const mergedLocations = [...locationNames].sort().map((name) => ({ name }));
  checklistContainer.innerHTML = mergedLocations.length
    ? renderInspectionZones(mergedLocations, zoneData.items, inspectionItems)
    : `<article class="check-item"><div><span>No Items</span><strong>No locations or equipment are set up for this outlet yet.</strong></div></article>`;
  applyInspectionSessionItems();
  openFirstInspectionLocation();
  updateInspectionProgress();
}

function renderInspectionZones(locations, zones, equipment) {
  const locationNames = locations.map((location) => location.name);
  const locationSet = new Set(locationNames);
  const assigned = new Set();
  const normalizedZones = zones.map((zone) => ({
    ...zone,
    locations: (zone.locations || []).filter((name) => locationSet.has(name)),
  }));
  normalizedZones.forEach((zone) => zone.locations.forEach((name) => assigned.add(name)));
  const unassigned = locationNames.filter((name) => !assigned.has(name));
  let zoneOne = normalizedZones.find((zone) => zone.name.toLowerCase() === "zone-1");
  if (!zoneOne) {
    zoneOne = { name: "Zone-1", locations: [] };
    normalizedZones.unshift(zoneOne);
  }
  zoneOne.locations = [...new Set([...zoneOne.locations, ...unassigned])];
  return normalizedZones
    .filter((zone) => zone.locations.length)
    .map((zone) => inspectionZoneCard(zone.name, zone.locations, equipment))
    .join("");
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
      ${locations.map((location) => inspectionLocationCard(location, equipment.filter((item) => (item.location || item.zone || "") === location))).join("")}
    </section>
  `;
}

function inspectionLocationCard(location, items) {
  return `
    <section class="inspection-location" data-inspection-location="${escapeAttr(location)}" hidden>
      <header>
        <h4>${escapeHtml(location)}</h4>
        <span class="status-pill status-untouched" data-location-status="${escapeAttr(location)}">(0%)</span>
      </header>
      ${items.length
        ? items.map(inspectionItemCard).join("")
        : `<article class="check-item"><div><span>No Equipment</span><strong>No equipment is assigned to this location.</strong></div></article>`}
    </section>
  `;
}

function inspectionItemCard(item) {
  const criteria = parseInspectionCriteria(item.inspection_criteria);
  const categoryOptions = setupOptions.categories.length
    ? setupOptions.categories.map((category) => `<option>${escapeHtml(category)}</option>`).join("")
    : `<option>Others</option>`;
  return `
    <article class="check-item inspection-item" data-equipment-id="${item.id}">
      <div>
        <span>${escapeHtml(item.type || item.equipment_type || "Equipment")} | ${escapeHtml(item.code || item.asset_id || "")}</span>
        <strong>${escapeHtml(item.name || item.asset_id || "Equipment item")}</strong>
      </div>
      <label>Images<input type="file" name="equipment-${item.id}-images" accept="image/*" capture="environment" multiple data-equipment-images><small data-saved-images></small></label>
      ${criteria.map((criterion, index) => `
        <div class="criteria-row" data-criterion="${escapeAttr(criterion)}">
          <label><input type="checkbox" name="equipment-${item.id}-criterion-${index}" value="pass" data-inspection-check='${escapeAttr(JSON.stringify({
            equipmentId: item.id,
            name: item.name || item.asset_id || "Equipment item",
            code: item.code || item.asset_id || "",
            type: item.type || item.equipment_type || "Equipment",
            outlet: item.outlet,
            location: item.location || item.zone || "",
            criterion,
          }))}'> ${escapeHtml(criterion)}</label>
          <label class="checkbox-line"><input type="checkbox" name="equipment-${item.id}-na-${index}" value="na" data-inspection-na> N/A</label>
          <select name="equipment-${item.id}-category-${index}" aria-label="Category">${categoryOptions}</select>
          <input name="equipment-${item.id}-notes-${index}" placeholder="Required when unchecked">
        </div>
      `).join("")}
    </article>
  `;
}

function renderInspectionImages(images) {
  return renderSavedImageList(images, "data-delete-inspection-image", "data-mark-inspection-image");
}

function openPhotoMarker(itemRow, imageIndex) {
  const images = storedImagesFromDataset(itemRow);
  const image = images[imageIndex];
  if (!itemRow || !image?.dataUrl) return;
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
      marks: [],
      redoMarks: [],
    };
    renderPhotoMarker();
    dialog.showModal();
  });
  source.src = image.markedDataUrl || image.dataUrl;
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

function saveMarkedPhoto() {
  if (!photoMarkState) return;
  const canvas = document.querySelector("[data-photo-mark-canvas]");
  const images = storedImagesFromDataset(photoMarkState.itemRow);
  const current = images[photoMarkState.imageIndex];
  if (!current) return;
  current.markedDataUrl = canvas.toDataURL("image/png");
  current.markedName = current.name ? `marked-${current.name}` : "marked-photo.png";
  current.marks = photoMarkState.marks;
  photoMarkState.itemRow.dataset.savedImages = JSON.stringify(images);
  const savedImages = photoMarkState.itemRow.querySelector("[data-saved-images]");
  if (savedImages) savedImages.innerHTML = renderInspectionImages(images);
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
  const response = await fetch("/api/inspection-sessions");
  const data = await response.json();
  inspectionHistoryCache = data.items || [];
  updateHistoryFilterSelects();
  renderInspectionHistory();
}

function renderInspectionHistory() {
  const search = inspectionHistorySearch.toLowerCase();
  const rows = inspectionHistoryCache.filter((row) => {
    const savedAt = row.created_at ? new Date(row.created_at).toLocaleString() : "";
    const haystack = [row.inspection_name, row.id, row.audit_date, savedAt, row.outlet, row.zone, row.auditor, row.status, row.progress, ...(row.locations || []), ...(row.categories || []), ...(row.departments || []), ...(row.priorities || []), ...(row.pics || [])].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!historyFilters.dateFrom || row.audit_date >= historyFilters.dateFrom)
      && (!historyFilters.dateTo || row.audit_date <= historyFilters.dateTo)
      && (!historyFilters.outlet || row.outlet === historyFilters.outlet)
      && (!historyFilters.location || (row.locations || []).includes(historyFilters.location))
      && (!historyFilters.auditor || String(row.auditor || "").toLowerCase().includes(historyFilters.auditor.toLowerCase()))
      && (!historyFilters.department || (row.departments || []).includes(historyFilters.department))
      && (!historyFilters.category || (row.categories || []).includes(historyFilters.category))
      && (!historyFilters.priority || (row.priorities || []).includes(historyFilters.priority))
      && (!historyFilters.status || row.status === historyFilters.status)
      && (!historyFilters.pic || (row.pics || []).join(" ").toLowerCase().includes(historyFilters.pic.toLowerCase()));
  });
  const pages = Math.max(1, Math.ceil(rows.length / inspectionHistoryPageSize));
  inspectionHistoryPage = Math.min(Math.max(1, inspectionHistoryPage), pages);
  const start = (inspectionHistoryPage - 1) * inspectionHistoryPageSize;
  const pageRows = rows.slice(start, start + inspectionHistoryPageSize);
  setHtml("[data-inspection-history]", pageRows.length
    ? pageRows.map(inspectionHistoryRow).join("")
    : `<article><div><b>No inspection history</b><span>Saved progress and completed inspections appear here.</span></div></article>`);
  setText("[data-inspection-history-count]", `${rows.length} ${rows.length === 1 ? "record" : "records"}`);
  setText("[data-history-page]", `Page ${inspectionHistoryPage} of ${pages}`);
  const prev = document.querySelector("[data-history-prev]");
  const next = document.querySelector("[data-history-next]");
  if (prev) prev.disabled = inspectionHistoryPage <= 1;
  if (next) next.disabled = inspectionHistoryPage >= pages;
}

function inspectionHistoryRow(row) {
  const savedAt = row.created_at ? new Date(row.created_at).toLocaleString() : "No saved time";
  const status = inspectionHistoryProgressStatus(row);
  return `
    <article>
      <div data-open-inspection-session="${row.id}">
        <b>${escapeHtml(row.inspection_name || `${row.outlet}_${row.audit_date}_${row.id}`)}</b>
        <span>${escapeHtml(row.audit_date)} | ${escapeHtml(savedAt)}</span>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(row.zone)} | ${escapeHtml(row.auditor)} | Findings: ${escapeHtml(row.findings_count || 0)}</span>
      </div>
      <span class="row-actions">
        <span class="status-pill ${status.className}">${escapeHtml(status.label)}</span>
        <button type="button" class="outline" data-open-inspection-session="${row.id}">Open</button>
        ${row.status === "Completed" ? `<a class="button-link outline" href="/api/inspection-sessions/${row.id}/export.pdf">PDF</a>` : ""}
        <button type="button" class="danger" data-delete-inspection-session="${row.id}">Delete</button>
      </span>
    </article>
  `;
}

function inspectionHistoryProgressStatus(row) {
  const progress = Number(row.progress) || 0;
  if (row.status === "Completed") return { className: "status-complete", label: `Completed (${progress}%)` };
  if (progress >= 100) return { className: "status-complete", label: "Ready to Complete (100%)" };
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
      const notApplicable = formData.get(`equipment-${row.dataset.equipmentId}-na-${index}`) === "na";
      items.push({
        equipmentId: row.dataset.equipmentId,
        location: equipment?.location || equipment?.zone || "",
        section: equipment?.name || equipment?.asset_id || "Equipment",
        item: criterion,
        category: formData.get(`equipment-${row.dataset.equipmentId}-category-${index}`) || "",
        passed,
        notApplicable,
        score: passed || notApplicable ? 100 : 0,
        evidenceStatus: images.length ? images.map(imageLabel).join(", ") : "Missing image",
        notes: formData.get(`equipment-${row.dataset.equipmentId}-notes-${index}`) || "",
        images,
        workOrderRequested: !passed && !notApplicable,
      });
    });
  });
  return {
    businessUnit: currentUnit,
    outlet: formValue(form, "outlet", ""),
    zone: "All Locations",
    auditDate: formValue(form, "auditDate", todayIsoDate()),
    auditor: formValue(form, "auditor", "Unnamed Inspector"),
    complete,
    items,
    signatures: inspectionSignatures(),
  };
}

function inspectionSignatures() {
  const form = document.getElementById("inspection-form");
  return parseStoredObject(form?.dataset.signatures || "{}");
}

function setInspectionSignatures(signatures = {}) {
  const form = document.getElementById("inspection-form");
  if (!form) return;
  form.dataset.signatures = JSON.stringify(signatures || {});
  const labels = {
    auditedBy: "Audited",
    verifiedBy: "Verified",
    acknowledgedBy: "Acknowledged",
  };
  const html = Object.entries(labels).map(([key, label]) => {
    const signed = Boolean(signatures?.[key]?.dataUrl);
    return `<span class="status-pill ${signed ? "status-complete" : "status-untouched"}">${label}: ${signed ? "Signed" : "Unsigned"}</span>`;
  }).join("");
  setHtml("[data-signature-status]", html);
}

function openSignatureDialog(kind) {
  const dialog = document.getElementById("signature-dialog");
  const form = document.getElementById("signature-form");
  const canvas = document.querySelector("[data-signature-canvas]");
  const ctx = canvas.getContext("2d");
  const signatures = inspectionSignatures();
  const labels = {
    auditedBy: "Audited By",
    verifiedBy: "Verified By",
    acknowledgedBy: "Acknowledged By",
  };
  form.reset();
  form.elements.signatureName.value = signatures[kind]?.name || "";
  setText("[data-signature-title]", labels[kind] || "Signature");
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  if (signatures[kind]?.dataUrl) {
    const image = new Image();
    image.addEventListener("load", () => ctx.drawImage(image, 0, 0, canvas.width, canvas.height));
    image.src = signatures[kind].dataUrl;
  }
  signatureState = { kind, drawing: false, lastX: 0, lastY: 0 };
  dialog.showModal();
}

function inspectionProgress(payload = collectInspectionPayload(false)) {
  if (!payload.items.length) return 0;
  const complete = payload.items.filter(isInspectionItemComplete).length;
  return Math.round(complete * 100 / payload.items.length);
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
  document.querySelectorAll("[data-inspection-location]").forEach((section) => {
    const location = section.dataset.inspectionLocation;
    const items = payload.items.filter((item) => item.location === location);
    const done = items.filter(isInspectionItemComplete).length;
    const status = statusForProgress(done, items.length);
    const progress = inspectionProgress({ items });
    const pill = section.querySelector("[data-location-status]");
    if (pill) {
      pill.className = `status-pill ${status.className}`;
      pill.textContent = `(${progress}%)`;
    }
    document.querySelectorAll("[data-location-nav-status]").forEach((navPill) => {
      if (navPill.dataset.locationNavStatus !== location) return;
      navPill.className = `status-pill ${status.className}`;
      navPill.textContent = `(${progress}%)`;
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
  });
  setHtml("[data-inspection-zone-progress]", summary.join(""));
}

function openInspectionLocation(location) {
  document.querySelectorAll("[data-inspection-location]").forEach((section) => {
    section.hidden = section.dataset.inspectionLocation !== location;
  });
  document.querySelectorAll("[data-open-inspection-location]").forEach((button) => {
    button.classList.toggle("active", button.dataset.openInspectionLocation === location);
  });
}

function openFirstInspectionLocation() {
  const first = document.querySelector("[data-open-inspection-location]");
  if (first) openInspectionLocation(first.dataset.openInspectionLocation);
}

function isInspectionReadyToComplete(payload) {
  return Boolean(payload.items.length && payload.items.every((item) => (item.notApplicable || (item.images || []).length) && isInspectionItemComplete(item)));
}

function updateInspectionActions(progress, payload) {
  const button = document.querySelector("[data-save-inspection-progress]");
  if (button) button.textContent = isInspectionReadyToComplete(payload) ? "Complete Inspection" : "Save Progress";
  const form = document.getElementById("inspection-form");
  const id = form ? formValue(form, "inspectionSessionId", "") : "";
  const link = document.querySelector("[data-export-inspection-pdf]");
  if (!link) return;
  if (id) {
    link.href = `/api/inspection-sessions/${id}/export.pdf`;
    link.classList.remove("disabled");
  } else {
    link.href = "#";
    link.classList.add("disabled");
  }
}

function validateInspectionComplete(payload) {
  if (!payload.items.length) return "No inspection items are loaded for this location.";
  const missingImage = payload.items.find((item) => !item.notApplicable && !item.images.length);
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
      const checkbox = criterionRow.querySelector("[data-inspection-check]");
      const na = criterionRow.querySelector("[data-inspection-na]");
      const notes = criterionRow.querySelector('input[name*="-notes-"]');
      checkbox.checked = Boolean(item.passed);
      if (na) na.checked = Boolean(item.notApplicable);
      criterionRow.classList.toggle("passed", checkbox.checked || Boolean(item.notApplicable));
      if (notes) {
        notes.value = item.notes || "";
        notes.disabled = checkbox.checked || Boolean(item.notApplicable);
      }
      const category = criterionRow.querySelector('select[name*="-category-"]');
      if (category && item.category) category.value = item.category;
    });
  });
}

async function openInspectionSession(id) {
  const response = await fetch(`/api/inspection-sessions/${id}`);
  const session = await response.json();
  const form = document.getElementById("inspection-form");
  form.elements.inspectionSessionId.value = session.id;
  setCurrentInspectionName(session.inspection_name || `${session.outlet}_${session.audit_date}_${session.id}`, "Editing");
  form.elements.outlet.value = session.outlet || "";
  inspectionSessionItems = session.items || [];
  form.elements.auditDate.value = session.audit_date || "";
  form.elements.auditor.value = session.auditor || "";
  setInspectionSignatures(session.signatures || {});
  await updateInspectionLocationSelect();
  showTab("inspections");
  showInspectionSubtab("guided");
}

async function restoreLastInspectionSession() {
  const id = localStorage.getItem(lastInspectionSessionKey);
  if (!id) return;
  try {
    await openInspectionSession(id);
  } catch (error) {
    localStorage.removeItem(lastInspectionSessionKey);
  }
}
