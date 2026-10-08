// The Activity Log: who did what and when, filtered by dates, outlet, and person, and searchable.
let activityEntries = [];
const activityFilters = { person: "", search: "" };

async function loadActivityLog() {
  const form = document.querySelector("[data-activity-filter]");
  if (!form) return;
  const outlet = form.elements.activityOutlet;
  if (outlet.options.length <= 1) updateSelectOptions(outlet, setupOptions.outlets || [], true, "All outlets");
  const query = new URLSearchParams({ unit: currentUnit });
  if (form.elements.from.value) query.set("from", form.elements.from.value);
  if (form.elements.to.value) query.set("to", form.elements.to.value);
  if (outlet.value) query.set("outlet", outlet.value);
  const response = await authFetch(`/api/activity?${query}`);
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || "The activity log could not be loaded");
  activityEntries = data.items || [];
  setText("[data-activity-total]", data.total > activityEntries.length
    ? `The latest ${activityEntries.length} of ${data.total} entries; narrow the dates to see earlier ones.`
    : `${data.total} entr${data.total === 1 ? "y" : "ies"}`);
  const people = document.querySelector("[data-activity-person]");
  updateSelectOptions(people, [...new Set(activityEntries.map((row) => row.user_name || "Unknown"))].sort(), true, "Everyone");
  people.value = activityFilters.person;
  renderActivityLog();
}

function renderActivityLog() {
  const search = activityFilters.search.toLowerCase();
  const rows = activityEntries.filter((row) => (!activityFilters.person || (row.user_name || "Unknown") === activityFilters.person)
    && (!search || [row.label, row.record_ref, row.detail, row.outlet, row.user_name].join(" ").toLowerCase().includes(search)));
  const page = paginateList("activity-log", rows, activityFilters, renderActivityLog);
  setHtml("[data-activity-log]", (rows.length ? page.items.map((row) => `
    <article><div>
      <b>${escapeHtml(row.user_name || "Unknown")} · ${escapeHtml(row.label)}${row.record_ref ? ` ${escapeHtml(row.record_ref)}` : ""}</b>
      <span>${escapeHtml(new Date(row.created_at).toLocaleString())}${row.outlet ? ` | ${escapeHtml(row.outlet)}` : ""}${row.duration_ms != null ? ` | Took ${escapeHtml(durationText(Math.round(row.duration_ms / 1000)))}` : ""}</span>
      ${row.detail ? `<span>${escapeHtml(row.detail)}</span>` : ""}
    </div></article>`).join("") : `<article><div><b>No activity</b><span>Audits, sign-offs, work requests, and work orders are logged here as people act on them.</span></div></article>`) + page.controls);
}

document.querySelector("[data-activity-filter]")?.addEventListener("change", (event) => {
  if (event.target.matches("[data-activity-person]")) {
    activityFilters.person = event.target.value;
    renderActivityLog();
    return;
  }
  if (event.target.matches("[data-activity-search]")) return;
  loadActivityLog().catch(showLoadError);
});

document.querySelector("[data-activity-search]")?.addEventListener("input", (event) => {
  activityFilters.search = event.target.value;
  renderActivityLog();
});

document.querySelector("[data-activity-filter]")?.addEventListener("submit", (event) => event.preventDefault());
