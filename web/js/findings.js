// The Findings page: each finding can be edited (the details once set during the inspection) or
// removed, until it is closed or has a work request or work order.
function canEditFindings() {
  return Boolean(currentUser) && currentUser.role !== "Department/PIC";
}

function findingIsSettled(row) {
  return row.status === "Closed" || Boolean(row.request_ref || row.order_ref || row.work_request_id);
}

function findingPhotos() {
  return parseStoredImages(document.getElementById("finding-form").dataset.photos || "[]");
}

function setFindingPhotos(images) {
  const form = document.getElementById("finding-form");
  form.dataset.photos = JSON.stringify(images || []);
  form.querySelector("[data-finding-photos]").innerHTML = renderSavedImageList(images || [], "data-delete-finding-photo");
}

function chooseOption(select, value) {
  if (value && ![...select.options].some((option) => option.value === value)) select.add(new Option(value, value));
  select.value = value || "";
}

function openFindingDialog(id) {
  const row = findingCache.find((item) => item.id === Number(id));
  if (!row) return;
  const form = document.getElementById("finding-form");
  form.reset();
  setText("[data-finding-message]", "");
  form.elements.findingId.value = row.id;
  updateSelectOptions(form.elements.category, setupOptions.assetTypes || [], true, "No asset type");
  updateSelectOptions(form.elements.priority, setupOptions.priorities || [], false, "Select priority");
  updateSelectOptions(form.elements.department, setupOptions.departments || [], false, "Select department");
  chooseOption(form.elements.category, row.category);
  chooseOption(form.elements.priority, row.priority);
  chooseOption(form.elements.department, row.assigned_department);
  form.elements.pic.value = row.pic || "";
  form.elements.dueDate.value = row.due_date || "";
  form.elements.comment.value = row.comment || "";
  form.elements.cause.value = row.cause || "";
  form.elements.recommendation.value = row.recommendation || "";
  form.elements.requiredAction.value = row.required_action || "";
  setFindingPhotos(parseStoredImages(row.images_json || "[]"));
  setText("[data-finding-context]", `${row.finding_ref || "Finding"} · ${row.item_name || "Item"} · ${row.criterion || ""} · ${row.outlet} | ${row.location}`);
  form.querySelector("[data-delete-current-finding]").hidden = findingIsSettled(row);
  document.getElementById("finding-dialog").showModal();
}

async function deleteFinding(id) {
  const row = findingCache.find((item) => item.id === Number(id));
  if (!confirm(`Remove ${row?.finding_ref || "this finding"}? It is taken off the audit's findings; the audit's score stays as recorded.`)) return false;
  await requestJson(`/api/findings/${Number(id)}`, "DELETE");
  await loadFindings();
  return true;
}

document.getElementById("finding-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  if (!form.reportValidity()) return;
  try {
    await requestJson(`/api/findings/${form.elements.findingId.value}`, "PATCH", {
      category: form.elements.category.value, priority: form.elements.priority.value, department: form.elements.department.value,
      pic: form.elements.pic.value, dueDate: form.elements.dueDate.value, comment: form.elements.comment.value,
      cause: form.elements.cause.value, recommendation: form.elements.recommendation.value,
      requiredAction: form.elements.requiredAction.value, images: findingPhotos(),
    });
  } catch (error) {
    setText("[data-finding-message]", error.message);
    return;
  }
  form.closest("dialog").close();
  await loadFindings();
});

document.querySelector("[data-delete-current-finding]")?.addEventListener("click", async (event) => {
  const form = event.currentTarget.closest("form");
  try {
    if (await deleteFinding(form.elements.findingId.value)) form.closest("dialog").close();
  } catch (error) {
    setText("[data-finding-message]", error.message);
  }
});

document.querySelector("[data-finding-photos-upload]")?.addEventListener("change", async (event) => {
  try {
    setFindingPhotos([...findingPhotos(), ...await readFilesAsStoredImages(event.target.files)]);
  } catch (error) {
    setText("[data-finding-message]", error.message);
  } finally {
    event.target.value = "";
  }
});

document.querySelector("[data-finding-photos]")?.addEventListener("click", (event) => {
  const button = event.target.closest("[data-delete-finding-photo]");
  if (!button) return;
  const images = findingPhotos();
  images.splice(Number(button.dataset.deleteFindingPhoto), 1);
  setFindingPhotos(images);
});

document.addEventListener("click", async (event) => {
  const edit = event.target.closest("[data-edit-finding]");
  if (edit) {
    openFindingDialog(edit.dataset.editFinding);
    return;
  }
  const remove = event.target.closest("[data-delete-finding]");
  if (remove) {
    try {
      await deleteFinding(remove.dataset.deleteFinding);
    } catch (error) {
      showActionError(error);
    }
  }
});
