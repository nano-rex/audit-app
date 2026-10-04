// Maintenance: work requests (what should be fixed) and the work orders made from them.
let workRequestCache = [];
const workRequestFilters = { search: "", outlet: "", status: "Open" };

function showMaintenanceSubtab(name) {
  document.querySelectorAll("[data-maintenance-subtab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.maintenanceSubtab === name);
  });
  document.querySelectorAll("[data-maintenance-panel]").forEach((panel) => {
    panel.classList.toggle("active", panel.dataset.maintenancePanel === name);
  });
}

function canReviewRequests() {
  return currentUser?.role !== "Department/PIC" && (currentUser?.role === "Super" || (currentUser?.permissions || []).includes("work-orders"));
}

function canRequestWork() {
  return ["findings", "inspections", "work-orders"].some((page) => (currentUser?.permissions || []).includes(page)) || currentUser?.role === "Super";
}

async function loadWorkRequests() {
  const response = await authFetch("/api/work-requests");
  workRequestCache = (await response.json()).items || [];
  updateSelectOptions(document.getElementById("work-request-filter-outlet"), setupOptions.outlets, true, "All outlets");
  document.getElementById("work-request-filter-outlet").value = workRequestFilters.outlet;
  renderWorkRequests();
}

function renderWorkRequests() {
  const search = workRequestFilters.search.toLowerCase();
  const rows = workRequestCache.filter((row) => {
    const haystack = [row.request_ref, row.item_name, row.outlet, row.location, row.department, row.category, row.priority,
      row.description, row.requested_by, row.status, row.audit_ref, row.finding_refs, row.work_order_ref].join(" ").toLowerCase();
    return (!search || haystack.includes(search))
      && (!workRequestFilters.outlet || row.outlet === workRequestFilters.outlet)
      && (!workRequestFilters.status || row.status === workRequestFilters.status);
  });
  const page = paginateList("work-requests", rows, workRequestFilters, renderWorkRequests);
  setHtml("[data-work-requests]", (rows.length
    ? page.items.map(workRequestRow).join("")
    : `<article><div><b>No work requests</b><span>Raise one from a failed item on the Findings page, or use Add Work Request.</span></div></article>`) + page.controls);
}

function workRequestRow(row) {
  const created = row.created_at ? new Date(row.created_at).toLocaleDateString() : "";
  const source = row.audit_ref ? `${row.audit_ref}${row.finding_refs ? ` (${row.finding_refs})` : ""}` : "Reported directly";
  const statusClass = { Open: "status-untouched", Ordered: "status-progress", Closed: "status-complete", Declined: "status-complete" }[row.status] || "";
  const review = row.status === "Open" && canReviewRequests();
  return `
    <article>
      <div>
        <b>${escapeHtml(row.request_ref || `WR-${row.id}`)} · ${escapeHtml(row.item_name || "Item")}</b>
        <span>${escapeHtml(row.outlet)} | ${escapeHtml(row.location)} | ${escapeHtml(row.department || "No department")} | ${escapeHtml(row.category || "No category")} | ${escapeHtml(row.priority || "No priority")}</span>
        <span class="pre-line">${escapeHtml(row.description || "")}</span>
        <span>${escapeHtml(source)} | Requested by ${escapeHtml(row.requested_by || "someone")}${created ? ` on ${escapeHtml(created)}` : ""}${row.work_order_ref ? ` | ${escapeHtml(row.work_order_ref)} ${escapeHtml(row.work_order_status || "")}` : ""}${row.decline_remark ? ` | Declined: ${escapeHtml(row.decline_remark)}` : ""}</span>
      </div>
      <span class="row-actions">
        <span class="status-pill ${statusClass}">${escapeHtml(row.status)}</span>
        ${photoSetButton(parseStoredImages(row.images_json || "[]"), "Photos")}
        ${review ? `<button type="button" class="primary" data-order-from-request="${Number(row.id)}">Create work order</button>
        <button type="button" class="outline" data-decline-request="${Number(row.id)}">Decline</button>` : ""}
      </span>
    </article>`;
}

// The request dialog, either for a failed item (source) or for something reported directly.
async function openWorkRequestEditor(source = null) {
  const dialog = document.getElementById("work-request-dialog");
  const form = document.getElementById("work-request-form");
  form.reset();
  setText("[data-work-request-message]", "");
  form.dataset.findingIds = JSON.stringify(source?.findingIds || []);
  form.querySelectorAll("[data-ad-hoc-only]").forEach((node) => { node.hidden = Boolean(source); });
  form.elements.itemName.required = !source;
  updateSelectOptions(form.elements.outlet, setupOptions.outlets, true, "Select outlet");
  updateSelectOptions(form.elements.department, setupOptions.departments, false, "Select department");
  updateSelectOptions(form.elements.category, setupOptions.categories, false, "Select category");
  updateSelectOptions(form.elements.priority, setupOptions.priorities.length ? setupOptions.priorities : ["High", "Medium", "Low"], false, "Select priority");
  const sourceBox = form.querySelector("[data-work-request-source]");
  sourceBox.hidden = !source;
  if (source) {
    sourceBox.innerHTML = `<b>${escapeHtml(source.item_name)}</b><span>${escapeHtml(source.audit_ref || "")} | ${escapeHtml(source.outlet)} | ${escapeHtml(source.location)}</span>`
      + `<ul>${source.findings.map((finding) => `<li>${escapeHtml(finding.finding_ref || "")} ${escapeHtml(finding.criterion || "")}${finding.comment && finding.comment !== finding.criterion ? ` — ${escapeHtml(finding.comment)}` : ""}</li>`).join("")}</ul>`;
    form.elements.department.value = source.department || "";
    form.elements.category.value = source.category || "";
    form.elements.priority.value = source.priority || "";
    form.elements.description.value = source.findings.map((finding) => `${finding.criterion || "Check"}: ${finding.comment || "failed"}`).join("\n");
  } else {
    await updateWorkRequestLocations();
  }
  setWorkRequestPhotos(source?.images || []);
  dialog.showModal();
}

async function updateWorkRequestLocations() {
  const form = document.getElementById("work-request-form");
  const outlet = form.elements.outlet.value;
  const names = outlet ? ((await (await authFetch(`/api/locations?outlet=${encodeURIComponent(outlet)}`)).json()).items || []).map((row) => row.name) : [];
  updateSelectOptions(form.elements.location, names, true, outlet ? "Select location" : "Select an outlet first");
}

function workRequestPhotos() {
  return parseStoredImages(document.getElementById("work-request-form").dataset.photos || "[]");
}

function setWorkRequestPhotos(images) {
  const form = document.getElementById("work-request-form");
  form.dataset.photos = JSON.stringify(images || []);
  form.querySelector("[data-work-request-photos]").innerHTML = renderSavedImageList(images || [], "data-delete-work-request-photo");
}

document.querySelector('#work-request-form select[name="outlet"]')?.addEventListener("change", () => updateWorkRequestLocations().catch(showLoadError));

document.querySelector("[data-work-request-photos-upload]")?.addEventListener("change", async (event) => {
  try {
    setWorkRequestPhotos([...workRequestPhotos(), ...await readFilesAsStoredImages(event.target.files)]);
  } catch (error) {
    setText("[data-work-request-message]", error.message);
  } finally {
    event.target.value = "";
  }
});

document.querySelector("[data-work-request-photos]")?.addEventListener("click", (event) => {
  const button = event.target.closest("[data-delete-work-request-photo]");
  if (!button) return;
  const images = workRequestPhotos();
  images.splice(Number(button.dataset.deleteWorkRequestPhoto), 1);
  setWorkRequestPhotos(images);
});

document.getElementById("work-request-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  if (!form.reportValidity()) return;
  const findingIds = JSON.parse(form.dataset.findingIds || "[]");
  const button = event.submitter;
  if (button) button.disabled = true;
  try {
    const result = await requestJson("/api/work-requests", "POST", {
      businessUnit: currentUnit, findingIds,
      outlet: form.elements.outlet.value, location: form.elements.location.value, itemName: form.elements.itemName.value,
      department: form.elements.department.value, category: form.elements.category.value, priority: form.elements.priority.value,
      description: form.elements.description.value, images: workRequestPhotos(),
    });
    form.closest("dialog").close();
    setText("[data-findings-message]", `${result.requestRef} raised. It is on Maintenance > Work Requests.`);
    await Promise.all([loadFindings().catch(() => {}), loadWorkRequests().catch(() => {})]);
    loadAttention().catch(() => {});
  } catch (error) {
    setText("[data-work-request-message]", error.message);
  } finally {
    if (button) button.disabled = false;
  }
});

document.querySelectorAll("[data-maintenance-subtab]").forEach((button) => {
  button.addEventListener("click", () => showMaintenanceSubtab(button.dataset.maintenanceSubtab));
});

document.querySelector("[data-open-work-request]")?.addEventListener("click", () => openWorkRequestEditor().catch(showLoadError));

document.getElementById("work-request-search")?.addEventListener("input", (event) => {
  workRequestFilters.search = event.target.value;
  renderWorkRequests();
});
document.getElementById("work-request-filter-outlet")?.addEventListener("change", (event) => {
  workRequestFilters.outlet = event.target.value;
  renderWorkRequests();
});
document.getElementById("work-request-filter-status")?.addEventListener("change", (event) => {
  workRequestFilters.status = event.target.value;
  renderWorkRequests();
});

document.addEventListener("click", async (event) => {
  const order = event.target.closest("[data-order-from-request]");
  if (order) {
    const request = workRequestCache.find((row) => row.id === Number(order.dataset.orderFromRequest));
    if (!request) return;
    await openWorkOrderEditor({
      outlet: request.outlet, zone: request.location, request_type: request.department, category: request.category,
      priority: request.priority, status: "Assigned", assignee: request.department, pic: "",
      title: `${request.request_ref} - ${request.item_name || "Work"}`, description: request.description,
      images_json: request.images_json || "[]",
    }, request.id);
    return;
  }
  const decline = event.target.closest("[data-decline-request]");
  if (decline) {
    const remark = prompt("Why is no work needed? The findings on this request will be closed with this reason.");
    if (remark === null) return;
    try {
      await requestJson(`/api/work-requests/${Number(decline.dataset.declineRequest)}`, "PATCH", { action: "decline", remark });
      await loadWorkRequests();
      loadAttention().catch(() => {});
    } catch (error) {
      alert(error.message);
    }
    return;
  }
  const raise = event.target.closest("[data-request-work]");
  if (raise) {
    const group = findingGroups().find((item) => item.key === raise.dataset.requestWork);
    if (group) await openWorkRequestEditor(group);
  }
});
