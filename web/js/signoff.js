// Sign-off: completed audits are signed by the auditor, verifier, and acknowledger, then closed.
const SIGNOFF_ROLES = [
  ["auditedBy", "auditor", "Audited by"],
  ["verifiedBy", "verifier", "Verified by"],
  ["acknowledgedBy", "acknowledger", "Acknowledged by"],
];
let signoffSession = null;
let signaturePad = null;
let signatureDialogSave = null;

function awaitingSignoff() {
  return inspectionHistoryCache.filter((row) => row.status === "Completed" && !row.closed_at);
}

async function loadSignoff() {
  await loadInspectionHistory();
  renderSignoffList();
  renderSignoffDetail();
}

function renderSignoffList() {
  const rows = awaitingSignoff();
  const page = paginateList("signoff", rows, "", renderSignoffList);
  setHtml("[data-signoff-list]", (page.items.length ? page.items.map(signoffRow).join("") : `<article><div><b>Nothing to sign off</b><span>Audits appear here once their inspection is completed.</span></div></article>`) + page.controls);
}

function signoffRow(row) {
  const signed = new Set(row.signed || []);
  const pills = SIGNOFF_ROLES.map(([key, , label]) => `<span class="status-pill ${signed.has(key) ? "status-complete" : "status-untouched"}">${escapeHtml(label)}: ${signed.has(key) ? "Signed" : "Not signed"}</span>`).join("");
  return `
    <article>
      <div>
        <b>${escapeHtml(auditTitle(row, ""))}</b>
        <span>${escapeHtml(row.audit_date)} | ${escapeHtml(row.outlet)} | ${escapeHtml(row.zone || "All Locations")} | ${escapeHtml(row.auditor || "No auditor")} | Findings: ${escapeHtml(row.findings_count || 0)}</span>
        <span class="signoff-pills">${pills}</span>
      </div>
      <span class="row-actions">
        <button type="button" class="primary" data-open-signoff="${Number(row.id)}">Open</button>
      </span>
    </article>`;
}

// Whether the signed-in user may give this signature now.
function canSign(session, key, capability) {
  if (session.closed_at || imageSource(session.signatures?.[key])) return false;
  if (!(currentUser?.inspectionPermissions || []).includes(capability)) return false;
  // The auditor's signature belongs to whoever ran the inspection.
  return key !== "auditedBy" || !session.owner_user_id || session.owner_user_id === currentUser.id;
}

async function openSignoff(id) {
  const response = await authFetch(`/api/inspection-sessions/${id}`);
  if (!response.ok) throw new Error(`Audit could not be loaded (${response.status})`);
  signoffSession = await response.json();
  showContextTab("signoff");
  renderSignoffDetail();
}

function renderSignoffDetail() {
  const session = signoffSession;
  document.querySelector("[data-signoff-list-panel]").hidden = Boolean(session);
  document.querySelector("[data-signoff-detail]").hidden = !session;
  if (!session) return;
  const findings = (session.items || []).filter((item) => !item.passed && !item.notApplicable).length;
  setText("[data-signoff-title]", auditTitle(session, ""));
  setText("[data-signoff-summary]", `${session.audit_date} | ${session.outlet} | ${session.zone || "All Locations"} | Auditor: ${session.auditor || "None"} | Findings: ${findings}${session.closed_at ? " | Closed" : ""}`);
  const saved = imageSource(currentUser?.signatureImage);
  setHtml("[data-signoff-cards]", SIGNOFF_ROLES.map(([key, capability, label]) => {
    const signature = session.signatures?.[key];
    const signedAt = signature?.signedAt ? new Date(signature.signedAt).toLocaleString() : "";
    const mine = canSign(session, key, capability);
    return `
      <article class="signoff-card">
        <h3>${escapeHtml(label)}</h3>
        <div class="signoff-signature">${imageSource(signature) ? `<img src="${escapeAttr(imageSource(signature))}" alt="${escapeAttr(label)} signature">` : `<span>Not signed</span>`}</div>
        <p class="muted">${imageSource(signature) ? `${escapeHtml(signature.name || "")}${signedAt ? ` · ${escapeHtml(signedAt)}` : ""}` : `Needs a person with ${escapeHtml(capability)} permission`}</p>
        ${mine ? `<div class="signoff-card-actions">
          ${saved ? `<button type="button" class="primary" data-sign-saved="${key}">Use my saved signature</button>` : ""}
          <button type="button" class="${saved ? "outline" : "primary"}" data-sign-draw="${key}">Draw signature</button>
        </div>` : ""}
      </article>`;
  }).join(""));
  const allSigned = SIGNOFF_ROLES.every(([key]) => imageSource(session.signatures?.[key]));
  const verifier = (currentUser?.inspectionPermissions || []).includes("verifier");
  setHtml("[data-signoff-actions]", `
    <a class="button-link outline" data-download href="/api/inspection-sessions/${Number(session.id)}/export.pdf">Export PDF</a>
    <a class="button-link outline" data-download href="/api/inspection-sessions/${Number(session.id)}/export.xlsx">Export Excel</a>
    ${!session.closed_at && verifier ? `<button type="button" class="primary" data-close-inspection-session="${Number(session.id)}" ${allSigned ? "" : "disabled title=\"All three signatures are needed first\""}>Close audit</button>` : ""}`);
}

async function signoffSign(key, image) {
  const message = (text, ok = false) => {
    setText("[data-signoff-message]", text);
    document.querySelector("[data-signoff-message]")?.classList.toggle("success", ok);
  };
  try {
    const signatures = { ...(signoffSession.signatures || {}), [key]: image };
    await requestJson(`/api/inspection-sessions/${signoffSession.id}`, "PATCH", { signatures });
    await openSignoff(signoffSession.id);
    message("Signature saved.", true);
    loadAttention().catch(() => {});
    loadInspectionHistory().then(renderSignoffList).catch(() => {});
  } catch (error) {
    message(error.message);
  }
}

// The signature dialog: draw or upload, then Save hands the image to whoever opened it.
function openSignatureDialog(title, onSave, existing = null) {
  const dialog = document.getElementById("signature-dialog");
  const form = document.getElementById("signature-form");
  signaturePad ||= setupSignaturePad(document.querySelector("[data-signature-canvas]"));
  form.reset();
  form.elements.signatureName.value = currentUser?.name || "";
  setText("[data-signature-title]", title);
  signaturePad.show(imageSource(existing));
  signatureDialogSave = onSave;
  dialog.showModal();
}

document.querySelector("[data-signature-clear]")?.addEventListener("click", () => signaturePad?.clear());

document.querySelector("[data-signature-upload]")?.addEventListener("change", async (event) => {
  try {
    const [image] = await readFilesAsStoredImages(event.target.files);
    if (imageSource(image)) signaturePad.show(imageSource(image));
  } catch (error) {
    alert(error.message);
  } finally {
    event.target.value = "";
  }
});

document.getElementById("signature-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!signatureDialogSave) return;
  if (signaturePad.isBlank()) {
    alert("Draw or upload a signature first.");
    return;
  }
  const dialog = event.currentTarget.closest("dialog");
  const button = event.submitter;
  if (button) button.disabled = true;
  try {
    const image = await uploadImage({ name: `${currentUser?.name || "Signature"}.png`, dataUrl: signaturePad.toDataUrl() });
    const save = signatureDialogSave;
    signatureDialogSave = null;
    dialog.close();
    await save(image);
  } catch (error) {
    alert(error.message);
  } finally {
    if (button) button.disabled = false;
  }
});

document.addEventListener("click", (event) => {
  const open = event.target.closest("[data-open-signoff]");
  if (open) {
    openSignoff(Number(open.dataset.openSignoff)).catch(showLoadError);
    return;
  }
  if (event.target.closest("[data-signoff-back]")) {
    signoffSession = null;
    setText("[data-signoff-message]", "");
    renderSignoffDetail();
    document.querySelector("[data-signoff-list-panel]").hidden = false;
    renderSignoffList();
    return;
  }
  const saved = event.target.closest("[data-sign-saved]");
  if (saved) {
    signoffSign(saved.dataset.signSaved, currentUser.signatureImage);
    return;
  }
  const draw = event.target.closest("[data-sign-draw]");
  if (draw) {
    const key = draw.dataset.signDraw;
    const label = SIGNOFF_ROLES.find(([role]) => role === key)?.[2] || "Signature";
    openSignatureDialog(label, (image) => signoffSign(key, image), currentUser?.signatureImage);
  }
});
