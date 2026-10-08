// Several fixed or variable assets at once: adding one at every chosen outlet and location, and
// editing a group that shares a name and asset type. Code (also the QR label) and serial number
// belong to each item, so a bulk edit sets them per item.

const bulkEquipmentFields = [
  ["name", "name", (row) => row.name || row.asset_id || ""],
  ["type", "type", (row) => row.type || row.equipment_type || ""],
  ["operationalStatus", "operationalStatus", (row) => row.operational_status || row.health_status || "Operational"],
  ["brand", "brand", (row) => row.brand || ""],
  ["model", "model", (row) => row.model || ""],
  ["installationDate", "installationDate", (row) => row.installation_date || ""],
  ["warrantyDate", "warrantyDate", (row) => row.warranty_date || ""],
  ["calibrationDate", "calibrationDate", (row) => row.calibration_date || ""],
  ["expiryDate", "expiryDate", (row) => row.expiry_date || ""],
  ["temporaryRelocation", "temporaryRelocation", (row) => row.temporary_relocation || ""],
  ["inverterModel", "inverterModel", (row) => row.inverter_model || ""],
  ["motorCapacity", "motorCapacity", (row) => row.motor_capacity || ""],
  ["description", "description", (row) => row.description || row.notes || ""],
];
let equipmentBulkState = null;

function equipmentGroupKey(row) {
  return [row.kind === "fixture" ? "fixture" : "asset", row.name || row.asset_id || "", row.type || row.equipment_type || ""].join("\u0001");
}

function equipmentGroup(row) {
  const key = equipmentGroupKey(row);
  return equipmentCache.filter((item) => equipmentGroupKey(item) === key);
}

// ---- Adding at several outlets and locations. ----

function equipmentAddCount(form) {
  const outlet = form.elements.outlet.value;
  const location = form.elements.location.value;
  const outlets = outlet === allChoice ? setupOptions.outlets : [outlet];
  const rows = equipmentFormLocations.filter((row) => outlets.includes(row.outlet_code));
  if (location === allChoice) return rows.length;
  if (!location) return outlets.length;
  return outlets.filter((code) => rows.some((row) => row.outlet_code === code && row.name === location)).length;
}

function updateBulkAddNote(form) {
  const note = form.querySelector("[data-bulk-add-note]");
  const many = form.dataset.mode === "add" && (form.elements.outlet.value === allChoice || form.elements.location.value === allChoice);
  form.dataset.many = String(many);
  note.hidden = !many;
  if (!many) return;
  const count = equipmentAddCount(form);
  const plural = itemKinds[form.elements.kind.value].plural;
  note.textContent = count
    ? `This adds ${count} ${count === 1 ? itemKinds[form.elements.kind.value].label.toLowerCase() : plural}, one at each chosen outlet and location. Each gets its own code; set codes and serial numbers afterwards with Edit all.`
    : "None of these outlets has that location.";
}

async function saveEquipmentBulkAdd(payload) {
  const { code, serialNumber, outlet, location, ...shared } = payload;
  const result = await requestJson("/api/equipment/bulk", "POST", {
    ...shared,
    outlets: outlet === allChoice ? "all" : [outlet],
    locations: location === allChoice ? "all" : location ? [location] : [],
  });
  if (result && !result.pending) showNotice(`Added ${result.count} ${itemKinds[payload.kind].plural}.`);
}

// ---- Editing a group. ----

function markMixed(field, mixed) {
  if (!mixed) return;
  if (field.tagName === "SELECT") {
    field.add(Object.assign(document.createElement("option"), { value: "", textContent: "Mixed — keep each" }), 0);
    field.options[0].dataset.mixedOption = "";
    field.value = "";
    return;
  }
  field.dataset.placeholder = field.placeholder;
  field.dataset.mixed = "";
  field.placeholder = "Mixed — leave blank to keep each";
  field.value = "";
}

function bulkGroupOptions(kind) {
  const groups = new Map();
  equipmentCache.filter((row) => (row.kind === "fixture" ? "fixture" : "asset") === kind).forEach((row) => {
    const key = equipmentGroupKey(row);
    groups.set(key, [...(groups.get(key) || []), row]);
  });
  return [...groups.entries()].filter(([, rows]) => rows.length > 1)
    .sort(([, a], [, b]) => (a[0].name || "").localeCompare(b[0].name || "") || (a[0].type || "").localeCompare(b[0].type || ""));
}

async function openEquipmentBulkEditor(kind, key = "") {
  const groups = bulkGroupOptions(kind);
  if (!groups.length) {
    showNotice(`No ${itemKinds[kind].plural} share a name and asset type yet.`);
    return;
  }
  const chosen = groups.find(([groupKey]) => groupKey === key) || groups[0];
  const form = document.getElementById("equipment-form");
  resetEquipmentForm(kind, "bulk");
  form.querySelector("[data-bulk-group]").innerHTML = groups.map(([groupKey, rows]) =>
    `<option value="${escapeAttr(groupKey)}">${escapeHtml(rows[0].name || rows[0].asset_id)} · ${escapeHtml(rows[0].type || rows[0].equipment_type || "No asset type")} (${rows.length})</option>`).join("");
  form.querySelector("[data-bulk-group]").value = chosen[0];
  fillEquipmentBulkEditor(form, chosen[1]);
  const dialog = document.getElementById("equipment-dialog");
  if (!dialog.open) dialog.showModal();
}

function fillEquipmentBulkEditor(form, rows) {
  rows = [...rows].sort((a, b) => a.outlet.localeCompare(b.outlet) || (a.location || "").localeCompare(b.location || "") || (a.code || "").localeCompare(b.code || ""));
  const initial = {};
  bulkEquipmentFields.forEach(([field, key, read]) => {
    const values = new Set(rows.map(read));
    const element = form.elements[field];
    if (values.size === 1) {
      const [value] = values;
      if (field === "operationalStatus") keepStatusOption(element, value);
      else element.value = value;
      initial[key] = value;
    } else {
      markMixed(element, true);
      initial[key] = null;
    }
  });
  const photos = rows.map((row) => JSON.stringify(parseStoredImages(row.photos || "[]")));
  const samePhotos = new Set(photos).size === 1;
  form.dataset.savedImages = samePhotos ? photos[0] : "[]";
  form.querySelector("[data-equipment-photos]").innerHTML = samePhotos ? renderSavedImageList(JSON.parse(photos[0]), "data-delete-equipment-photo") : "";
  renderEquipmentCriteria(parseInspectionCriteria(rows[0].inspection_criteria));
  equipmentBulkState = {
    ids: rows.map((row) => row.id),
    initial,
    photos: samePhotos ? JSON.stringify(storedImagesFromDataset(form)) : null,
    criteria: JSON.stringify(collectEquipmentCriteria(form)),
  };
  const kind = form.elements.kind.value;
  form.querySelector("h2").textContent = `Edit ${rows.length} ${itemKinds[kind].plural}`;
  form.querySelector('button[type="submit"]').textContent = "Save Changes";
  form.querySelector("[data-bulk-all]").checked = true;
  form.querySelector("[data-bulk-items]").innerHTML = rows.map((row) => `<tr>
      <td><input type="checkbox" name="bulkItem" value="${row.id}" checked aria-label="Edit ${escapeAttr(row.code || row.asset_id || "")}"></td>
      <td>${escapeHtml(row.outlet)}</td>
      <td>${escapeHtml(row.location || row.zone || "No location")}</td>
      <td><input data-bulk-code="${row.id}" value="${escapeAttr(row.code || row.asset_id || "")}" aria-label="Code" required></td>
      <td data-asset-only><input data-bulk-serial="${row.id}" value="${escapeAttr(row.serial_number || "")}" placeholder="Serial number" aria-label="Serial number"></td>
    </tr>`).join("");
}

// The changed shared fields go to every ticked item, with each item's own code and serial number.
async function saveEquipmentBulkEdit(form) {
  const ids = [...form.querySelectorAll('input[name="bulkItem"]:checked')].map((input) => Number(input.value));
  if (!ids.length) {
    showActionError(new Error("Tick at least one item to edit."));
    return false;
  }
  const { initial } = equipmentBulkState;
  const fields = {};
  bulkEquipmentFields.forEach(([field, key]) => {
    const value = form.elements[field].value.trim();
    if (initial[key] === null ? value === "" : value === initial[key]) return;
    fields[key] = value;
  });
  if ("operationalStatus" in fields) fields.replacementFlag = fields.operationalStatus === "Replace";
  const photos = JSON.stringify(storedImagesFromDataset(form));
  if (photos !== (equipmentBulkState.photos ?? "[]")) fields.photos = JSON.parse(photos);
  const criteria = JSON.stringify(collectEquipmentCriteria(form));
  if (criteria !== equipmentBulkState.criteria) fields.inspectionCriteria = JSON.parse(criteria);
  const items = ids.map((id) => ({
    id,
    code: form.querySelector(`[data-bulk-code="${id}"]`).value.trim(),
    serialNumber: form.querySelector(`[data-bulk-serial="${id}"]`).value.trim(),
  }));
  const result = await requestJson("/api/equipment/bulk", "PATCH", { ids, fields, items });
  if (result && !result.pending) showNotice(`Saved ${ids.length} ${itemKinds[form.elements.kind.value].plural}.`);
  return true;
}

document.addEventListener("click", (event) => {
  const button = event.target.closest("[data-bulk-edit-equipment]");
  if (!button) return;
  const row = equipmentCache.find((item) => String(item.id) === button.dataset.bulkEditEquipment);
  openEquipmentBulkEditor(row ? (row.kind === "fixture" ? "fixture" : "asset") : equipmentFilters.kind || "asset", row ? equipmentGroupKey(row) : "");
});

document.querySelector("[data-bulk-group]")?.addEventListener("change", (event) => {
  openEquipmentBulkEditor(document.getElementById("equipment-form").elements.kind.value, event.target.value);
});

document.querySelector("[data-bulk-all]")?.addEventListener("change", (event) => {
  event.target.closest("table").querySelectorAll('input[name="bulkItem"]').forEach((input) => { input.checked = event.target.checked; });
});

document.querySelector("[data-bulk-items]")?.addEventListener("change", (event) => {
  if (event.target.name !== "bulkItem") return;
  const boxes = [...event.currentTarget.querySelectorAll('input[name="bulkItem"]')];
  document.querySelector("[data-bulk-all]").checked = boxes.every((input) => input.checked);
});
