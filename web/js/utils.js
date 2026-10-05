function todayIsoDate() {
  const date = new Date();
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function setText(selector, value) {
  const node = document.querySelector(selector);
  if (node) {
    node.textContent = value;
  }
}

function setHtml(selector, value) {
  const node = document.querySelector(selector);
  if (node) {
    node.innerHTML = value;
  }
}

function loadingMarkup(label = "Loading…") {
  return `<div class="loading-state" role="status" aria-live="polite"><span class="loading-spinner" aria-hidden="true"></span><span>${escapeHtml(label)}</span></div>`;
}

function setLoading(selector, label = "Loading…") {
  setHtml(selector, loadingMarkup(label));
}

function setCurrentInspectionName(value = "", mode = "New") {
  setText("[data-current-inspection-name]", value ? `${mode}: ${value}` : "New inspection");
}

async function loadBranding() {
  try {
    const response = await authFetch("/api/branding");
    if (response.ok) {
      branding = { ...brandingDefaults, ...await response.json() };
      currentUnit = branding.businessUnitLabel || branding.companyName || brandingDefaults.businessUnitLabel;
    }
  } catch (error) {
    branding = { ...brandingDefaults };
    currentUnit = branding.businessUnitLabel;
  }
  if (typeof setOrgTheme === "function" && branding.theme) setOrgTheme(branding.theme);
  applyBranding();
}

function showBrandLogo(url) {
  document.querySelectorAll("[data-brand-logo]").forEach((image) => {
    image.hidden = !url;
    if (url) image.src = url;
  });
}

function applyBranding() {
  document.title = branding.appTitle || brandingDefaults.appTitle;
  showBrandLogo(branding.logoUrl);
  setText("[data-brand-title]", branding.appTitle || brandingDefaults.appTitle);
  setText("[data-brand-subtitle]", branding.appSubtitle || brandingDefaults.appSubtitle);
  setText("[data-report-heading]", branding.reportHeading || brandingDefaults.reportHeading);
  unitTexts.forEach((node) => {
    node.textContent = currentUnit;
  });
}

function imageLabel(image) {
  if (typeof image === "string") return image;
  return image?.markedName || image?.name || "Image";
}

function storedImagesFromDataset(row) {
  if (!row?.dataset.savedImages) return [];
  return parseStoredImages(row.dataset.savedImages);
}

function parseStoredImages(value) {
  if (!value) return [];
  if (Array.isArray(value)) return value;
  try {
    const images = JSON.parse(value);
    return Array.isArray(images) ? images : [];
  } catch (error) {
    return [value];
  }
}

function parseStoredObject(value) {
  if (!value) return {};
  if (typeof value === "object" && !Array.isArray(value)) return value;
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch (error) {
    return {};
  }
}

function readFileAsDataUrl(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener("load", () => {
      resolve({
        name: file.name,
        type: file.type || "application/octet-stream",
        size: file.size,
        dataUrl: reader.result,
      });
    });
    reader.addEventListener("error", () => reject(reader.error));
    reader.readAsDataURL(file);
  });
}

async function readFilesAsStoredImages(files) {
  return Promise.all([...files].map(async (file) => {
    if (file.size > 10 * 1024 * 1024) throw new Error("Each image must be at most 10 MiB");
    const image = await uploadImage(await readFileAsDataUrl(file));
    return { ...image, uploadedAt: new Date().toISOString() };
  }));
}

async function uploadImage(image) {
  const data = await requestJson("/api/media", "POST", { image });
  return data.image;
}

function imageSource(image, marked = false) {
  return image?.[marked ? "markedUrl" : "url"] || image?.[marked ? "markedDataUrl" : "dataUrl"] || "";
}

function renderSavedImageList(images, deleteAttribute = "data-delete-inspection-image", markAttribute = "") {
  if (!images.length) return "";
  return images.map((image, index) => `
    <span class="image-pill">
      ${photoThumbnailButton(image)}
      <span class="image-pill-name">${escapeHtml(imageLabel(image))}</span>
      ${image?.uploadedAt ? `<time datetime="${escapeAttr(image.uploadedAt)}">${escapeHtml(new Date(image.uploadedAt).toLocaleString())}</time>` : ""}
      ${markAttribute && imageSource(image) ? `<button type="button" ${markAttribute}="${index}" aria-label="Mark ${escapeAttr(imageLabel(image))}">Mark</button>` : ""}
      <button type="button" ${deleteAttribute}="${index}" aria-label="Remove ${escapeAttr(imageLabel(image))}">x</button>
    </span>
  `).join("");
}
// One image shown as a tile beside its Choose file tile, with a corner button to remove it.
function renderImageTile(container, image, removeAttribute, label) {
  if (!container) return;
  container.innerHTML = imageSource(image)
    ? `<span class="image-pill">${photoThumbnailButton(image, label)}<button type="button" ${removeAttribute} aria-label="Remove ${escapeAttr(label.toLowerCase())}">x</button></span>`
    : "";
}

function updateSelectOptions(select, values, includePlaceholder = false, placeholder = "Select option") {
  if (!select) return;
  const selected = select.value;
  const options = values.length
    ? values.map((value) => `<option>${escapeHtml(value)}</option>`).join("")
    : `<option value="">No setup records</option>`;
  select.innerHTML = [
    includePlaceholder ? `<option value="">${escapeHtml(placeholder)}</option>` : "",
    options,
  ].join("");
  if (values.includes(selected)) {
    select.value = selected;
  }
}

function updateSetupSelects() {
  document.querySelectorAll('select[name="outlet"]').forEach((select) => {
    const includePlaceholder = select.form?.id === "report-filter-form" || select.querySelector("option")?.textContent.startsWith("Select") || select.querySelector("option")?.textContent.startsWith("--");
    updateSelectOptions(select, setupOptions.outlets, includePlaceholder, select.form?.id === "report-filter-form" ? "All outlets" : "Select outlet");
  });
  document.querySelectorAll('select[name="requestType"], select[name="department"]').forEach((select) => {
    updateSelectOptions(select, setupOptions.departments, false, "Select department");
  });
  document.querySelectorAll('select[name="category"]').forEach((select) => {
    updateSelectOptions(select, setupOptions.categories, false, "Select category");
  });
  document.querySelectorAll('select[name="priority"]').forEach((select) => {
    updateSelectOptions(select, setupOptions.priorities.length ? setupOptions.priorities : ["High", "Medium", "Low"], false, "Select priority");
  });
  document.querySelectorAll('select[name="auditType"]').forEach((select) => {
    updateSelectOptions(select, setupOptions.auditTypes, false, "Select audit type");
  });
}

function updateLocationOutletSelect() {
  const select = document.getElementById("location-outlet");
  if (!select) return;
  updateSelectOptions(select, setupOptions.outlets, false, "Select outlet");
  if (setupOptions.outlets.includes(selectedLocationOutlet)) {
    select.value = selectedLocationOutlet;
  }
}

function updateZoneOutletSelect() {
  const select = document.getElementById("zone-outlet");
  if (!select) return;
  updateSelectOptions(select, setupOptions.outlets, true, "All outlets");
  if (setupOptions.outlets.includes(selectedZoneOutlet)) {
    select.value = selectedZoneOutlet;
  }
}

function updateEquipmentFilterSelects() {
  updateSelectOptions(document.getElementById("equipment-filter-outlet"), setupOptions.outlets, true, "All outlets");
  const outlet = equipmentFilters.outlet;
  if (outlet) document.getElementById("equipment-filter-outlet").value = outlet;
  const locations = [...new Set(equipmentCache
    .filter((row) => !outlet || row.outlet === outlet)
    .map((row) => row.location || row.zone || "")
    .filter(Boolean))].sort();
  const types = [...new Set(equipmentCache.map((row) => row.type || row.equipment_type || "").filter(Boolean))].sort();
  const brands = [...new Set(equipmentCache.map((row) => row.brand || "").filter(Boolean))].sort();
  updateSelectOptions(document.getElementById("equipment-filter-location"), locations, true, "All locations");
  updateSelectOptions(document.getElementById("equipment-filter-type"), types, true, "All types");
  updateSelectOptions(document.getElementById("equipment-filter-brand"), brands, true, "All brands");
  updateSelectOptions(document.getElementById("equipment-filter-category"), setupOptions.categories, true, "All categories");
  document.getElementById("equipment-filter-category").value = equipmentFilters.category;
  document.getElementById("equipment-filter-location").value = equipmentFilters.location;
  document.getElementById("equipment-filter-type").value = equipmentFilters.type;
  document.getElementById("equipment-filter-brand").value = equipmentFilters.brand;
}

function updateWorkOrderFilterSelects() {
  updateSelectOptions(document.getElementById("work-order-filter-outlet"), setupOptions.outlets, true, "All outlets");
  updateSelectOptions(document.getElementById("work-order-filter-department"), setupOptions.departments, true, "All departments");
  updateSelectOptions(document.getElementById("work-order-filter-category"), setupOptions.categories, true, "All categories");
  updateSelectOptions(document.getElementById("work-order-filter-priority"), setupOptions.priorities.length ? setupOptions.priorities : ["High", "Medium", "Low"], true, "All priorities");
  if (workOrderFilters.outlet) document.getElementById("work-order-filter-outlet").value = workOrderFilters.outlet;
  if (workOrderFilters.department) document.getElementById("work-order-filter-department").value = workOrderFilters.department;
  if (workOrderFilters.category) document.getElementById("work-order-filter-category").value = workOrderFilters.category;
  if (workOrderFilters.priority) document.getElementById("work-order-filter-priority").value = workOrderFilters.priority;
  const locations = [...new Set(workOrderCache
    .filter((row) => !workOrderFilters.outlet || row.outlet === workOrderFilters.outlet)
    .map((row) => row.zone || "")
    .filter(Boolean))].sort();
  updateSelectOptions(document.getElementById("work-order-filter-location"), locations, true, "All locations");
  document.getElementById("work-order-filter-location").value = workOrderFilters.location;
}

function updateFindingFilterSelects() {
  updateSelectOptions(document.getElementById("finding-filter-outlet"), setupOptions.outlets, true, "All outlets");
  updateSelectOptions(document.getElementById("finding-filter-department"), setupOptions.departments, true, "All departments");
  updateSelectOptions(document.getElementById("finding-filter-category"), setupOptions.categories, true, "All categories");
  updateSelectOptions(document.getElementById("finding-filter-priority"), setupOptions.priorities.length ? setupOptions.priorities : ["High", "Medium", "Low"], true, "All priorities");
  if (findingFilters.outlet) document.getElementById("finding-filter-outlet").value = findingFilters.outlet;
  if (findingFilters.department) document.getElementById("finding-filter-department").value = findingFilters.department;
  if (findingFilters.category) document.getElementById("finding-filter-category").value = findingFilters.category;
  if (findingFilters.priority) document.getElementById("finding-filter-priority").value = findingFilters.priority;
  const locations = [...new Set(findingCache
    .filter((row) => !findingFilters.outlet || row.outlet === findingFilters.outlet)
    .map((row) => row.location || "")
    .filter(Boolean))].sort();
  updateSelectOptions(document.getElementById("finding-filter-location"), locations, true, "All locations");
  document.getElementById("finding-filter-location").value = findingFilters.location;
}

function updateHistoryFilterSelects() {
  updateSelectOptions(document.getElementById("history-filter-outlet"), setupOptions.outlets, true, "All outlets");
  updateSelectOptions(document.getElementById("history-filter-department"), setupOptions.departments, true, "All departments");
  updateSelectOptions(document.getElementById("history-filter-category"), setupOptions.categories, true, "All categories");
  updateSelectOptions(document.getElementById("history-filter-priority"), setupOptions.priorities.length ? setupOptions.priorities : ["High", "Medium", "Low"], true, "All priorities");
  const locations = [...new Set(inspectionHistoryCache.flatMap((session) =>
    (session.locations || []).filter(Boolean)
  ))].sort();
  updateSelectOptions(document.getElementById("history-filter-location"), locations, true, "All locations");
  const fields = {
    "history-filter-outlet": historyFilters.outlet,
    "history-filter-department": historyFilters.department,
    "history-filter-category": historyFilters.category,
    "history-filter-priority": historyFilters.priority,
    "history-filter-location": historyFilters.location,
    "history-filter-status": historyFilters.status,
  };
  Object.entries(fields).forEach(([id, value]) => {
    const node = document.getElementById(id);
    if (node) node.value = value || "";
  });
}

async function updateEquipmentLocationSelect(selected = "") {
  const form = document.getElementById("equipment-form");
  if (!form) return;
  const outlet = formValue(form, "outlet", selectedLocationOutlet || setupOptions.outlets[0] || "");
  const response = await authFetch(`/api/locations?outlet=${encodeURIComponent(outlet)}`);
  const data = await response.json();
  const values = data.items.map((row) => row.name);
  updateSelectOptions(form.elements.location, values, false, "Select location");
  if (values.includes(selected)) {
    form.elements.location.value = selected;
  }
}

async function updateInspectionLocationSelect() {
  await loadInspectionItems();
}

// A visit covers every location unless particular ones are ticked; "All locations" and a
// selection are mutually exclusive, and clearing the selection returns to All.
async function updateScheduleLocationSelect(selected = []) {
  const form = document.getElementById("schedule-form");
  if (!form) return;
  const outlet = formValue(form, "outlet", "");
  const container = form.querySelector("[data-visit-location-options]");
  if (!outlet) {
    container.innerHTML = `<p class="muted">Select an outlet to choose locations.</p>`;
    return;
  }
  const response = await authFetch(`/api/locations?outlet=${encodeURIComponent(outlet)}`);
  const names = (await response.json()).items.map((row) => row.name);
  const chosen = new Set(selected.filter((name) => names.includes(name)));
  container.innerHTML = `<label class="zone-location-option"><input type="checkbox" data-visit-all${chosen.size ? "" : " checked"}><span>All locations</span></label>`
    + names.map((name) => `<label class="zone-location-option"><input type="checkbox" name="visitLocation" value="${escapeAttr(name)}"${chosen.has(name) ? " checked" : ""}><span>${escapeHtml(name)}</span></label>`).join("");
}

// People who can audit at the chosen outlet, ticked when already assigned.
async function loadScheduleAssignees(selected = null) {
  const form = document.getElementById("schedule-form");
  if (!form) return;
  const container = form.querySelector("[data-assignee-options]");
  const chosen = new Set((selected ?? [...form.querySelectorAll('input[name="assignee"]:checked')].map((input) => Number(input.value))).map(Number));
  const outlet = formValue(form, "outlet", "");
  const search = form.querySelector("[data-assignee-search]");
  if (!outlet) {
    container.innerHTML = "";
    search.hidden = true;
    setText("[data-assignee-hint]", "Choose an outlet to see who can audit there.");
    return;
  }
  const people = ((await (await authFetch(`/api/schedules/assignees?outlet=${encodeURIComponent(outlet)}`)).json()).items) || [];
  search.hidden = people.length < 8;
  search.value = "";
  setText("[data-assignee-hint]", people.length
    ? `People who can audit at ${outlet}. Leave everyone unticked to leave the visit unassigned.`
    : `Nobody with audit permission covers ${outlet} yet.`);
  container.innerHTML = people.map((person) => `
    <label class="assignee-option" data-assignee-name="${escapeAttr(person.name.toLowerCase())}">
      <input type="checkbox" name="assignee" value="${person.id}"${chosen.has(person.id) ? " checked" : ""}>
      ${typeof personAvatar === "function" ? personAvatar(person, "small") : ""}
      <span><b>${escapeHtml(person.name)}</b><small>${escapeHtml(person.title || person.role || "")}</small></span>
    </label>`).join("");
}

function chosenVisitLocations(form) {
  return [...form.querySelectorAll('input[name="visitLocation"]:checked')].map((input) => input.value);
}

async function requestJson(url, method, payload) {
  const response = await authFetch(url, {
    method,
    headers: { "Content-Type": "application/json" },
    body: payload === undefined ? undefined : JSON.stringify(payload),
  });
  if (!response.ok) {
    const data = await response.json().catch(() => ({}));
    throw new Error(data.error || `Request failed: ${response.status}`);
  }
  const data = await response.json();
  // A change that waits for approval is not an error, but the person should know it is not applied yet.
  if (data?.pending && typeof showNotice === "function") showNotice(data.message || "Sent for approval.");
  return data;
}

function formValue(form, name, fallback = "") {
  const value = new FormData(form).get(name);
  return value ? String(value) : fallback;
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#039;",
  }[char]));
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, "&#096;");
}
