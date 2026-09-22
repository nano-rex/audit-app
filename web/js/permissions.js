const inspectionPermissionOptions = [
  { id: "auditor", label: "Auditor — create, edit, complete, and sign inspections" },
  { id: "verifier", label: "Verifier — verify and sign inspections" },
  { id: "acknowledger", label: "Acknowledger — acknowledge and sign inspections" },
];

function permissionCheckboxes(options, selected, name, disabled) {
  return options.map((option) => `<label class="permission-option"><input type="checkbox" name="${name}" value="${escapeAttr(option.id)}" ${selected.includes(option.id) ? "checked" : ""} ${disabled ? "disabled" : ""}><span>${escapeHtml(option.label)}</span></label>`).join("");
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
  form.elements.inheritPermissions.disabled = Boolean(role?.protected);
}

function userPermissionOverrides(form) {
  if (form.elements.inheritPermissions.checked || form.elements.role.value === "Super") return null;
  return {
    permissions: [...form.querySelectorAll('input[name="userPermissions"]:checked')].map((input) => input.value),
    inspectionPermissions: [...form.querySelectorAll('input[name="userInspectionPermissions"]:checked')].map((input) => input.value),
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
});
