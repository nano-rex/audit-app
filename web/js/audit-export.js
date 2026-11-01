// Export PDF of one audit: the overall report, or chosen locations with full audit details.
const auditPdfPattern = /\/api\/inspection-sessions\/(\d+)\/export\.pdf$/;
let auditPdfLocations = [];

async function openAuditPdfDialog(id) {
  const response = await authFetch(`/api/inspection-sessions/${id}`);
  if (!response.ok) throw new Error(`Audit could not be loaded (${response.status})`);
  const session = await response.json();
  auditPdfLocations = [...new Set((session.items || []).map((item) => item.location || session.zone || "Unassigned"))];
  const form = document.getElementById("audit-pdf-form");
  form.dataset.sessionId = String(id);
  form.elements.pdfScope.value = "overall";
  setText("[data-audit-pdf-title]", auditTitle(session, ""));
  form.querySelector("[data-audit-pdf-location-options]").innerHTML = `<label class="zone-location-option"><input type="checkbox" data-audit-pdf-all checked><span>All locations</span></label>`
    + auditPdfLocations.map((name) => `<label class="zone-location-option"><input type="checkbox" name="pdfLocation" value="${escapeAttr(name)}"><span>${escapeHtml(name)}</span></label>`).join("");
  updateAuditPdfLink();
  document.getElementById("audit-pdf-dialog").showModal();
}

// "All locations" means every location of the audit in full; ticking one narrows to the ticked.
function chosenAuditPdfLocations(form) {
  if (form.querySelector("[data-audit-pdf-all]")?.checked) return auditPdfLocations;
  return [...form.querySelectorAll('input[name="pdfLocation"]:checked')].map((input) => input.value);
}

function updateAuditPdfLink() {
  const form = document.getElementById("audit-pdf-form");
  const byLocation = form.elements.pdfScope.value === "locations";
  form.querySelector("[data-audit-pdf-locations]").hidden = !byLocation;
  const query = new URLSearchParams();
  if (byLocation) chosenAuditPdfLocations(form).forEach((name) => query.append("location", name));
  const link = form.querySelector("[data-audit-pdf-link]");
  const ready = !byLocation || query.has("location");
  link.classList.toggle("disabled", !ready);
  link.href = ready ? `/api/inspection-sessions/${form.dataset.sessionId}/export.pdf${query.size ? `?${query}` : ""}` : "#";
}

// Every Export PDF of an audit (History, Sign-off, an open inspection) asks which report first.
document.addEventListener("click", (event) => {
  const link = event.target.closest("a[data-download]");
  if (!link || link.closest("#audit-pdf-dialog") || event.button !== 0 || event.metaKey || event.ctrlKey) return;
  const match = (link.getAttribute("href") || "").match(auditPdfPattern);
  if (!match || link.classList.contains("disabled")) return;
  event.preventDefault();
  openAuditPdfDialog(match[1]).catch(showActionError);
}, true);

document.getElementById("audit-pdf-form")?.addEventListener("change", (event) => {
  const form = event.currentTarget;
  const all = form.querySelector("[data-audit-pdf-all]");
  const picks = [...form.querySelectorAll('input[name="pdfLocation"]')];
  if (event.target === all) picks.forEach((input) => { input.checked = false; });
  else if (event.target.name === "pdfLocation") all.checked = !picks.some((input) => input.checked);
  updateAuditPdfLink();
});

// The dialog's link downloads as every export does ("Preparing…" while the report is built,
// which takes a while with photos); the dialog closes once the download is done.
document.querySelector("[data-audit-pdf-link]")?.addEventListener("click", (event) => {
  const link = event.currentTarget;
  if (link.classList.contains("disabled")) return;
  const wait = setInterval(() => {
    if (link.dataset.busy) return;
    clearInterval(wait);
    document.getElementById("audit-pdf-dialog").close();
  }, 300);
});
