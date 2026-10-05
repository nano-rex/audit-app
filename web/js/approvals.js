// Approvals: changes to managed records that wait for someone who approves them.
let changeCache = [];

async function loadChanges() {
  const response = await authFetch("/api/changes");
  changeCache = (await response.json()).items || [];
  renderChanges();
}

// The fields a change sets, beside what the record had (field names differ only in case style).
function changeDetails(row) {
  const snake = (key) => key.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
  const shown = (value) => value === null || value === undefined || value === "" ? "(empty)" : typeof value === "object" ? "(updated)" : String(value);
  if (row.action === "delete") return `<li>Delete ${escapeHtml(row.before.name || row.before.code || row.before.email || "this record")}</li>`;
  return Object.entries(row.payload || {})
    .filter(([key, value]) => !["businessUnit"].includes(key) && value !== undefined)
    .map(([key, value]) => {
      const before = row.before?.[snake(key)];
      if (row.action === "edit" && before !== undefined && shown(before) === shown(value)) return "";
      const label = key.replace(/([A-Z])/g, " $1").replace(/^./, (letter) => letter.toUpperCase());
      return `<li><b>${escapeHtml(label)}</b>: ${row.action === "edit" && before !== undefined ? `${escapeHtml(shown(before))} → ` : ""}${escapeHtml(shown(value))}</li>`;
    }).filter(Boolean).join("") || "<li>No visible field changes</li>";
}

function renderChanges() {
  const filter = document.querySelector("[data-change-filter]")?.value ?? "pending";
  const rows = changeCache.filter((row) => !filter || (filter === "pending" ? row.status === "Pending" : row.mine));
  const page = paginateList("changes", rows, filter, renderChanges);
  const pill = { Pending: "status-untouched", Approved: "status-complete", Rejected: "status-progress" };
  setHtml("[data-changes]", (rows.length ? page.items.map((row) => `
    <article class="change-row">
      <div>
        <b>CHG-${String(row.id).padStart(5, "0")} · ${escapeHtml(row.summary || "")}</b>
        <span>${escapeHtml(row.recordLabel)} | Requested by ${escapeHtml(row.requested_by || "someone")} on ${escapeHtml(new Date(row.created_at).toLocaleString())}${row.decided_by ? ` | ${escapeHtml(row.status)} by ${escapeHtml(row.decided_by)}` : ""}${row.remark ? ` | ${escapeHtml(row.remark)}` : ""}</span>
        <ul class="change-fields">${changeDetails(row)}</ul>
      </div>
      <span class="row-actions">
        <span class="status-pill ${pill[row.status] || ""}">${escapeHtml(row.status)}</span>
        ${row.canDecide ? `<button type="button" class="primary" data-decide-change="approve" data-change-id="${row.id}">Approve</button>
        <button type="button" class="outline" data-decide-change="reject" data-change-id="${row.id}">Reject</button>` : ""}
      </span>
    </article>`).join("") : `<article><div><b>Nothing here</b><span>${filter === "pending" ? "No changes are waiting." : "No changes yet."}</span></div></article>`) + page.controls);
}

document.querySelector("[data-change-filter]")?.addEventListener("change", () => renderChanges());

document.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-decide-change]");
  if (!button) return;
  const decision = button.dataset.decideChange;
  const remark = decision === "reject" ? prompt("Why is this change rejected? The requester is told.") : "";
  if (remark === null) return;
  button.disabled = true;
  try {
    await requestJson(`/api/changes/${button.dataset.changeId}`, "PATCH", { decision, remark });
    await loadChanges();
    loadAttention().catch(() => {});
    if (decision === "approve") loadSetup().then(updateSetupSelects).catch(() => {});
  } catch (error) {
    alert(error.message);
    button.disabled = false;
  }
});

// Add, edit, and delete buttons are hidden where the person may not change those records.
const CHANGE_BUTTONS = {
  assets: "equipment", users: "user", outlets: "outlet", zones: "zone",
  locations: "location", departments: "department", roles: "role",
};

function applyActionPermissions() {
  let style = document.getElementById("action-permissions");
  if (!style) {
    style = document.createElement("style");
    style.id = "action-permissions";
    document.head.appendChild(style);
  }
  const actions = new Set(currentUser?.actions || []);
  const hidden = Object.entries(CHANGE_BUTTONS)
    .filter(([record]) => currentUser?.role !== "Super" && !actions.has(`${record}.manage`))
    .flatMap(([, name]) => [`[data-open-${name}]`, `[data-edit-${name}]`, `[data-delete-${name}]`]);
  style.textContent = hidden.length ? `${hidden.join(",\n")} { display: none !important; }` : "";
}

// A short message that a change was sent for approval rather than applied.
function showNotice(message) {
  let notice = document.querySelector("[data-notice]");
  if (!notice) {
    notice = document.createElement("div");
    notice.className = "notice";
    notice.dataset.notice = "";
    notice.setAttribute("role", "status");
    document.body.appendChild(notice);
  }
  notice.textContent = message;
  notice.hidden = false;
  clearTimeout(showNotice.timer);
  showNotice.timer = setTimeout(() => { notice.hidden = true; }, 7000);
}
