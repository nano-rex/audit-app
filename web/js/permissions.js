const inspectionPermissionOptions = [
  { id: "auditor", label: "Auditor — create, edit, complete, and sign inspections" },
  { id: "verifier", label: "Verifier — verify and sign inspections" },
  { id: "acknowledger", label: "Acknowledger — acknowledge and sign inspections" },
];

function permissionCheckboxes(options, selected, name, disabled) {
  return options.map((option) => `<label class="permission-option"><input type="checkbox" name="${name}" value="${escapeAttr(option.id)}" ${selected.includes(option.id) ? "checked" : ""} ${disabled ? "disabled" : ""}><span>${escapeHtml(option.label)}</span></label>`).join("");
}

// Add / edit / delete and approve, per kind of record, as a table of checkboxes.
function actionPermissionTable(selected = [], name = "actions", disabled = false) {
  const chosen = new Set(selected);
  const records = setupOptions.changeRecords || [];
  const box = (value) => `<input type="checkbox" name="${name}" value="${escapeAttr(value)}"${chosen.has(value) ? " checked" : ""}${disabled ? " disabled" : ""}>`;
  return `<table class="action-permissions"><thead><tr><th>Records</th><th>Add / edit / delete</th><th>Approve changes</th></tr></thead>
    <tbody>${records.map((record) => `<tr><th scope="row">${escapeHtml(record.label)}</th>
      <td><label>${box(`${record.id}.manage`)}<span class="visually-hidden">Add, edit, delete ${escapeHtml(record.label)}</span></label></td>
      <td><label>${box(`${record.id}.approve`)}<span class="visually-hidden">Approve ${escapeHtml(record.label)} changes</span></label></td></tr>`).join("")}</tbody></table>`;
}

function checkedValues(form, name) {
  return [...form.querySelectorAll(`input[name="${name}"]:checked`)].map((input) => input.value);
}

function showEditorTab(form, name) {
  form.querySelectorAll("[data-editor-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.editorTab === name);
    button.setAttribute("aria-pressed", String(button.dataset.editorTab === name));
  });
  form.querySelectorAll("[data-editor-panel]").forEach((panel) => { panel.hidden = panel.dataset.editorPanel !== name; });
}

function renderUserPermissions(overrides = null) {
  const form = document.getElementById("user-form");
  const role = roleCache.find((row) => row.name === form.elements.role.value);
  const inherit = form.elements.inheritPermissions.checked;
  const disabled = inherit || role?.protected;
  const value = inherit ? role || {} : overrides || role || {};
  const permissionTabs = (setupOptions.tabs || allTabs).filter((tab) => tab.id !== "settings");
  form.querySelector("[data-user-permissions]").innerHTML = permissionCheckboxes(permissionTabs, value.permissions || [], "userPermissions", disabled);
  form.querySelector("[data-user-inspection-permissions]").innerHTML = permissionCheckboxes(inspectionPermissionOptions, value.inspectionPermissions || [], "userInspectionPermissions", disabled);
  const actions = inherit || !overrides?.actions ? role?.actions || [] : overrides.actions;
  form.querySelector("[data-user-action-permissions]").innerHTML = actionPermissionTable(actions, "userActions", disabled);
  form.elements.inheritPermissions.disabled = Boolean(role?.protected);
}

function userPermissionOverrides(form) {
  if (form.elements.inheritPermissions.checked || form.elements.role.value === "Super") return null;
  return {
    permissions: [...form.querySelectorAll('input[name="userPermissions"]:checked')].map((input) => input.value),
    inspectionPermissions: [...form.querySelectorAll('input[name="userInspectionPermissions"]:checked')].map((input) => input.value),
    actions: checkedValues(form, "userActions"),
  };
}

document.addEventListener("click", (event) => {
  const button = event.target.closest("[data-editor-tab]");
  if (button) showEditorTab(button.closest("form"), button.dataset.editorTab);
});
document.querySelector('#user-form [name="inheritPermissions"]').addEventListener("change", () => renderUserPermissions());
document.querySelector('#user-form [name="role"]').addEventListener("change", () => {
  const form = document.getElementById("user-form");
  if (form.elements.role.value === "Super") form.elements.inheritPermissions.checked = true;
  if (form.elements.inheritPermissions.checked) renderUserPermissions();
  // A role that belongs to a department brings it along.
  const role = roleCache.find((row) => row.name === form.elements.role.value);
  if (role?.department) form.elements.department.value = role.department;
});

// A person's outlets: "All outlets" (the default) or the ones ticked; ticking one unticks All,
// and clearing them all ticks All again.
function renderUserOutletOptions(outlets = null) {
  const container = document.querySelector("[data-user-outlet-options]");
  if (!container) return;
  const chosen = new Set(outlets || []);
  container.innerHTML = `<label class="zone-location-option"><input type="checkbox" data-user-outlet-all${outlets == null ? " checked" : ""}><span>All outlets</span></label>`
    + (setupOptions.outlets || []).map((code) => `<label class="zone-location-option"><input type="checkbox" name="userOutlet" value="${escapeAttr(code)}"${chosen.has(code) ? " checked" : ""}><span>${escapeHtml(code)}</span></label>`).join("");
}

function chosenUserOutlets(form) {
  if (!form.querySelector("[data-user-outlet-options]") || form.querySelector("[data-user-outlet-all]")?.checked) return null;
  return [...form.querySelectorAll('input[name="userOutlet"]:checked')].map((input) => input.value);
}

document.querySelector("[data-user-outlet-options]")?.addEventListener("change", (event) => {
  const container = event.currentTarget;
  const all = container.querySelector("[data-user-outlet-all]");
  const picks = [...container.querySelectorAll('input[name="userOutlet"]')];
  if (event.target === all) picks.forEach((input) => { input.checked = false; });
  all.checked = !picks.some((input) => input.checked);
});
