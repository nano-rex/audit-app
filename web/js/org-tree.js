// The organization tree: roles by whom they report to, with the people in each role.
function roleDescendants(roleId) {
  const below = [];
  const walk = (id) => roleCache.filter((role) => role.reports_to_id === id).forEach((role) => {
    if (below.includes(role.id)) return;
    below.push(role.id);
    walk(role.id);
  });
  walk(roleId);
  return below;
}

function orgPeople(role) {
  return (userCache || []).filter((user) => user.role === role.name && user.active !== 0 && user.active !== false);
}

function orgNode(role, seen) {
  seen.add(role.id);
  const people = orgPeople(role);
  const shown = people.slice(0, 8);
  const scope = { one: "One outlet each", several: "Selected outlets" }[role.outlet_scope];
  const children = roleCache.filter((child) => child.reports_to_id === role.id && !seen.has(child.id));
  return `<li>
    <div class="org-node">
      <b>${escapeHtml(role.name)}</b>
      <small>${escapeHtml([role.department, scope].filter(Boolean).join(" · ") || "All outlets")}</small>
      <ul class="org-people">${shown.map((user) => `<li>${escapeHtml(user.name)}${(user.outlets || []).length ? ` <small>${escapeHtml(user.outlets.join(", "))}</small>` : ""}</li>`).join("")}
        ${people.length > shown.length ? `<li><small>and ${people.length - shown.length} more</small></li>` : ""}
        ${people.length ? "" : `<li><small>Nobody yet</small></li>`}</ul>
    </div>
    ${children.length ? `<ul>${children.map((child) => orgNode(child, seen)).join("")}</ul>` : ""}
  </li>`;
}

function renderOrgTree() {
  const container = document.querySelector("[data-org-tree]");
  if (!container) return;
  const known = new Set(roleCache.map((role) => role.id));
  const tops = roleCache.filter((role) => !role.reports_to_id || !known.has(role.reports_to_id));
  const chains = tops.filter((role) => roleCache.some((child) => child.reports_to_id === role.id));
  const alone = tops.filter((role) => !chains.includes(role));
  const seen = new Set();
  container.innerHTML = chains.length
    ? chains.map((role) => `<div class="org-chart"><ul>${orgNode(role, seen)}</ul></div>`).join("")
    : `<p class="muted">No chain of command yet. Edit a role in Roles and choose who it reports to.</p>`;
  setHtml("[data-org-unlinked]", alone.length
    ? `<h3>Not in a chain</h3><div class="org-unlinked">${alone.map((role) => `<span class="org-chip"><b>${escapeHtml(role.name)}</b> <small>${orgPeople(role).length} ${orgPeople(role).length === 1 ? "person" : "people"}</small></span>`).join("")}</div>`
    : "");
}

// ---- People: each person under the one they report to. ----
let orgView = "people";

function personInitials(name) {
  return String(name || "?").trim().split(/\s+/).slice(0, 2).map((part) => part[0] || "").join("").toUpperCase() || "?";
}

function personAvatar(user, size = "") {
  const source = imageSource(user.profilePhoto || {});
  if (source) return `<img class="org-avatar ${size}" src="${escapeAttr(photoThumbnail(user.profilePhoto))}" alt="">`;
  // A steady colour per person, from their name, for the initials.
  const hue = [...String(user.name || "")].reduce((total, char) => total + char.charCodeAt(0), 0) % 360;
  return `<span class="org-avatar org-initials ${size}" style="--hue:${hue}" aria-hidden="true">${escapeHtml(personInitials(user.name))}</span>`;
}

function activeUsers() {
  return (userCache || []).filter((user) => user.active !== 0 && user.active !== false);
}

// The person someone reports to: in the nearest role above theirs that has people, the one
// covering the most of the same outlets (an all-outlet role covers every outlet).
function managerFor(user, roleByName, roleById, peopleByRole) {
  const role = roleByName.get(user.role);
  const covers = (person) => {
    const scope = roleByName.get(person.role)?.outlet_scope || "all";
    if (scope === "all" || !(user.outlets || []).length) return 0.5;
    return (user.outlets || []).filter((code) => (person.outlets || []).includes(code)).length;
  };
  const seen = new Set();
  let parentId = role?.reports_to_id;
  while (parentId && !seen.has(parentId)) {
    seen.add(parentId);
    const candidates = peopleByRole.get(parentId) || [];
    const ranked = candidates.map((person) => [covers(person), person]).filter(([score]) => score > 0)
      .sort((a, b) => b[0] - a[0] || a[1].name.localeCompare(b[1].name));
    if (ranked.length) return ranked[0][1];
    parentId = roleById.get(parentId)?.reports_to_id;
  }
  return null;
}

function personNode(user, children, roleByName) {
  const role = roleByName.get(user.role);
  const below = (children.get(user.id) || []).sort((a, b) => a.role.localeCompare(b.role) || a.name.localeCompare(b.name));
  return `<li>
    <div class="org-person">
      ${personAvatar(user)}
      <b>${escapeHtml(user.name)}</b>
      <span>${escapeHtml(user.title || user.role || "")}</span>
      ${user.title && user.role ? `<small>${escapeHtml(user.role)}</small>` : ""}
      <div class="org-outlets">${(user.outlets || []).map((code) => `<span>${escapeHtml(code)}</span>`).join("") || (role?.outlet_scope && role.outlet_scope !== "all" ? `<span class="none">No outlet yet</span>` : `<span class="all">All outlets</span>`)}</div>
    </div>
    ${below.length ? `<ul>${below.map((child) => personNode(child, children, roleByName)).join("")}</ul>` : ""}
  </li>`;
}

function renderPeopleTree() {
  const container = document.querySelector("[data-org-people]");
  if (!container) return;
  const roleByName = new Map(roleCache.map((role) => [role.name, role]));
  const roleById = new Map(roleCache.map((role) => [role.id, role]));
  const chained = new Set(roleCache.filter((role) => role.reports_to_id && roleById.has(role.reports_to_id)).flatMap((role) => [role.id, role.reports_to_id]));
  const people = activeUsers();
  const inChain = people.filter((user) => chained.has(roleByName.get(user.role)?.id));
  const peopleByRole = new Map();
  inChain.forEach((user) => {
    const id = roleByName.get(user.role).id;
    peopleByRole.set(id, [...(peopleByRole.get(id) || []), user]);
  });
  const children = new Map();
  const roots = [];
  inChain.forEach((user) => {
    const manager = managerFor(user, roleByName, roleById, peopleByRole);
    if (manager) children.set(manager.id, [...(children.get(manager.id) || []), user]);
    else roots.push(user);
  });
  roots.sort((a, b) => a.name.localeCompare(b.name));
  container.innerHTML = roots.length
    ? roots.map((user) => `<div class="org-chart org-people-chart"><ul>${personNode(user, children, roleByName)}</ul></div>`).join("")
    : `<p class="muted">Nobody is in a chain of command yet. Set who each role reports to under Roles, and give people those roles.</p>`;
  const others = people.filter((user) => !inChain.includes(user)).sort((a, b) => a.name.localeCompare(b.name));
  setHtml("[data-org-people-others]", others.length
    ? `<h3>Not in a chain</h3><div class="org-unlinked">${others.map((user) => `<span class="org-chip org-person-chip">${personAvatar(user, "small")}<b>${escapeHtml(user.name)}</b> <small>${escapeHtml(user.role || "No role")}</small></span>`).join("")}</div>`
    : "");
}

function renderOrganization() {
  document.querySelectorAll("[data-org-view]").forEach((button) => {
    button.classList.toggle("active", button.dataset.orgView === orgView);
    button.setAttribute("aria-pressed", String(button.dataset.orgView === orgView));
  });
  document.querySelectorAll("[data-org-panel]").forEach((panel) => { panel.hidden = panel.dataset.orgPanel !== orgView; });
  if (orgView === "people") renderPeopleTree();
  else renderOrgTree();
  // A chart wider than the screen starts centred on its top, not cut off at the left.
  requestAnimationFrame(() => document.querySelectorAll(".org-tree").forEach((tree) => {
    tree.scrollLeft = Math.max(0, (tree.scrollWidth - tree.clientWidth) / 2);
  }));
}

document.querySelectorAll("[data-org-view]").forEach((button) => button.addEventListener("click", () => {
  orgView = button.dataset.orgView;
  renderOrganization();
}));
