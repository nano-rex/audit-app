// The organization chart: each person, under the person they report to.
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

// ---- Each person under the one they report to. ----

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
    if (!person.outlets || !user.outlets) return 0.5;
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

function personNode(user, children) {
  const below = (children.get(user.id) || []).sort((a, b) => a.role.localeCompare(b.role) || a.name.localeCompare(b.name));
  return `<li>
    <div class="org-person">
      ${personAvatar(user)}
      <b>${escapeHtml(user.name)}</b>
      <span>${escapeHtml(user.role || "No designation")}</span>
    </div>
    ${below.length ? `<ul>${below.map((child) => personNode(child, children)).join("")}</ul>` : ""}
  </li>`;
}

function renderPeopleTree() {
  const container = document.querySelector("[data-org-people]");
  if (!container) return;
  const roleByName = new Map(roleCache.map((role) => [role.name, role]));
  const roleById = new Map(roleCache.map((role) => [role.id, role]));
  const people = activeUsers();
  const peopleByRole = new Map();
  people.forEach((user) => {
    const id = roleByName.get(user.role)?.id;
    if (id) peopleByRole.set(id, [...(peopleByRole.get(id) || []), user]);
  });
  const children = new Map();
  const tops = [];
  people.forEach((user) => {
    const manager = managerFor(user, roleByName, roleById, peopleByRole);
    if (manager) children.set(manager.id, [...(children.get(manager.id) || []), user]);
    else tops.push(user);
  });
  // Everyone nobody is above is at the top: people on their own in a row that wraps to the
  // screen, then each chain drawn as its own tree, the largest first.
  const size = (user) => 1 + (children.get(user.id) || []).reduce((total, child) => total + size(child), 0);
  const order = (a, b) => a.role.localeCompare(b.role) || a.name.localeCompare(b.name);
  const alone = tops.filter((user) => !children.has(user.id)).sort(order);
  const heads = tops.filter((user) => children.has(user.id)).sort((a, b) => size(b) - size(a) || order(a, b));
  container.innerHTML = (alone.length ? `<ul class="org-row">${alone.map((user) => personNode(user, children)).join("")}</ul>` : "")
    + heads.map((user) => `<div class="org-chart org-people-chart"><ul>${personNode(user, children)}</ul></div>`).join("")
    || `<p class="muted">No people yet.</p>`;
}

function renderOrganization() {
  renderPeopleTree();
  // A chart wider than the screen starts centred on its top, not cut off at the left.
  requestAnimationFrame(() => document.querySelectorAll(".org-chart").forEach((chart) => {
    chart.scrollLeft = Math.max(0, (chart.scrollWidth - chart.clientWidth) / 2);
  }));
}
