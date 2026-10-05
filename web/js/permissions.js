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
  // A role that belongs to a department brings it along, and its outlet access decides the picker.
  const role = roleCache.find((row) => row.name === form.elements.role.value);
  if (role?.department) form.elements.department.value = role.department;
  const ticked = [...form.querySelectorAll('[name="userOutlet"]:checked')].map((input) => input.value);
  renderUserOutlets(role?.outlet_scope === "one" ? ticked.slice(0, 1) : ticked);
});

// A role limited to one outlet takes a single choice; one limited to selected outlets takes several.
function renderUserOutlets(selected = []) {
  const form = document.getElementById("user-form");
  const role = roleCache.find((row) => row.name === form.elements.role.value);
  const scope = role?.outlet_scope || "all";
  const box = form.querySelector("[data-user-outlets]");
  box.hidden = scope === "all";
  if (scope === "all") return;
  const chosen = new Set(selected);
  const type = scope === "one" ? "radio" : "checkbox";
  setText("[data-user-outlets-hint]", scope === "one"
    ? `A ${role.name} works at one outlet and sees only that outlet.`
    : `A ${role.name} sees only the outlets ticked here.`);
  form.querySelector("[data-user-outlet-options]").innerHTML = (setupOptions.outlets || []).map((code) =>
    `<label class="zone-location-option"><input type="${type}" name="userOutlet" value="${escapeAttr(code)}"${chosen.has(code) ? " checked" : ""}><span>${escapeHtml(code)}</span></label>`).join("");
}

function userOutletChoices(form) {
  return [...form.querySelectorAll('[name="userOutlet"]:checked')].map((input) => input.value);
}

// The role editor lists the people with the role, so their outlets can be chosen there too.
function renderRoleOutlets() {
  const form = document.getElementById("role-form");
  const scope = form.elements.outletScope.value;
  const box = form.querySelector("[data-role-outlets]");
  box.hidden = scope === "all";
  if (scope === "all") return;
  const people = (userCache || []).filter((user) => user.role === form.dataset.roleName);
  setText("[data-role-outlets-hint]", people.length
    ? (scope === "one" ? "Choose the one outlet each person works at; they see only that outlet." : "Tick the outlets each person covers; they see only those.")
    : "Nobody has this role yet. After giving it to someone in Users, choose their outlets here or in their user editor.");
  const outlets = setupOptions.outlets || [];
  form.querySelector("[data-role-outlet-people]").innerHTML = people.map((user) => {
    const chosen = new Set(user.outlets || []);
    const options = scope === "one"
      ? `<select data-role-outlet-user="${user.id}"><option value="">Choose an outlet</option>${outlets.map((code) => `<option${chosen.has(code) ? " selected" : ""}>${escapeHtml(code)}</option>`).join("")}</select>`
      : `<div class="role-outlet-ticks">${outlets.map((code) => `<label class="zone-location-option"><input type="checkbox" data-role-outlet-user="${user.id}" value="${escapeAttr(code)}"${chosen.has(code) ? " checked" : ""}><span>${escapeHtml(code)}</span></label>`).join("")}</div>`;
    return `<div class="role-outlet-person" data-role-outlet-person="${user.id}"><b>${escapeHtml(user.name)}</b><small class="muted">${escapeHtml(user.email || "")}</small>${options}</div>`;
  }).join("");
}

// [userId, outlets] for each person whose choice differs from what is saved.
function roleOutletChoices(form) {
  if (form.elements.outletScope.value === "all") return [];
  const changes = [];
  form.querySelectorAll("[data-role-outlet-person]").forEach((row) => {
    const id = Number(row.dataset.roleOutletPerson);
    const select = row.querySelector("select[data-role-outlet-user]");
    const outlets = select ? (select.value ? [select.value] : []) : [...row.querySelectorAll("input[data-role-outlet-user]:checked")].map((input) => input.value);
    const saved = (userCache.find((user) => user.id === id)?.outlets || []).slice().sort();
    if (JSON.stringify(outlets.slice().sort()) !== JSON.stringify(saved)) changes.push([id, outlets]);
  });
  return changes;
}

document.querySelector('#role-form [name="outletScope"]')?.addEventListener("change", () => renderRoleOutlets());
