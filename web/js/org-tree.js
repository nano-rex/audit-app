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
