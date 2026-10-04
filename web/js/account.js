let accountPhoto = {};
let accountSignature = {};

function renderAccountSignature() {
  renderImageTile(document.querySelector("[data-account-signature-tile]"), accountSignature, "data-remove-account-signature", "Signature");
}

function renderAccountPhoto() {
  renderImageTile(document.querySelector("[data-account-photo-tile]"), accountPhoto, "data-remove-account-photo", "Profile picture");
}

async function loadAccount() {
  setText("[data-account-message]", "Loading account…");
  const response = await authFetch("/api/account");
  const data = await response.json();
  currentUser = data.user;
  const form = document.getElementById("account-form");
  form.elements.name.value = currentUser.name || "";
  form.elements.username.value = currentUser.username || "";
  form.elements.email.value = currentUser.email || "";
  updateSelectOptions(form.elements.department, setupOptions.departments, true, "Select department");
  const roles = setupOptions.roles.filter((role) => currentUser.role === "Super" || role !== "Super");
  updateSelectOptions(form.elements.role, roles, true, "Select role");
  form.elements.department.value = currentUser.department || "";
  form.elements.role.value = currentUser.role || "";
  const administrator = ["Super", "Admin"].includes(currentUser.role);
  form.elements.department.disabled = !administrator;
  form.elements.role.disabled = !administrator;
  document.querySelector("[data-account-access-note]").hidden = administrator;
  // A Super account lives in its own database, outside every organization.
  const superAccount = currentUser.accountScope === "control";
  form.querySelectorAll("[data-organization-account-only]").forEach((node) => { node.hidden = superAccount; });
  document.querySelector("[data-super-account-note]").hidden = !superAccount;
  accountPhoto = currentUser.profilePhoto || {};
  accountSignature = currentUser.signatureImage || {};
  renderAccountPhoto();
  renderAccountSignature();
  renderCurrentUser();
  setText("[data-account-message]", "");
}

async function loadSuperDatabases() {
  const response = await authFetch("/api/account/databases");
  const data = await response.json();
  const select = document.querySelector("[data-super-database-select]");
  if (!select) return data;
  select.innerHTML = (data.databases || []).map((database) => `<option value="${escapeAttr(database.name)}"${database.active ? " selected" : ""}>${escapeHtml(database.name)}${database.active ? " (active)" : ""}</option>`).join("");
  return data;
}

document.querySelector("[data-super-refresh-databases]")?.addEventListener("click", () => loadSuperDatabases().catch((error) => setText("[data-super-database-message]", error.message)));
document.querySelector("[data-super-create-database]")?.addEventListener("click", async () => {
  const input = document.querySelector("[data-super-new-database-name]");
  try { await requestJson("/api/account/databases", "POST", { name: input.value.trim() }); input.value = ""; await loadSuperDatabases(); setText("[data-super-database-message]", "Database created."); }
  catch (error) { setText("[data-super-database-message]", error.message); }
});
document.querySelector("[data-super-switch-database]")?.addEventListener("click", async () => {
  const name = document.querySelector("[data-super-database-select]")?.value;
  if (!name || !confirm(`Switch to ${name}? Everyone signed in to the current organization must sign in again.`)) return;
  // Organization users are signed out; the Super account stays signed in and reopens on the new database.
  try { await requestJson("/api/account/databases", "PATCH", { name }); window.location.reload(); }
  catch (error) { setText("[data-super-database-message]", error.message); }
});
document.querySelector("[data-super-remove-database]")?.addEventListener("click", async () => {
  const name = document.querySelector("[data-super-database-select]")?.value;
  if (!name || !confirm(`Remove database ${name}? This cannot be undone.`)) return;
  try { await requestJson(`/api/account/databases/${encodeURIComponent(name)}`, "DELETE"); await loadSuperDatabases(); setText("[data-super-database-message]", "Database removed."); }
  catch (error) { setText("[data-super-database-message]", error.message); }
});

document.querySelector("[data-account-photo-upload]").addEventListener("change", async (event) => {
  const input = event.target;
  const save = document.querySelector('#account-form button[type="submit"]');
  save.disabled = true;
  try {
    const [photo] = await readFilesAsStoredImages(input.files);
    if (photo) accountPhoto = photo;
    renderAccountPhoto();
    setText("[data-account-message]", "Picture uploaded. Save Account to apply it.");
  } catch (error) {
    setText("[data-account-message]", error.message);
  } finally {
    save.disabled = false;
    input.value = "";
  }
});

document.querySelector("[data-account-photo-tile]").addEventListener("click", (event) => {
  if (!event.target.closest("[data-remove-account-photo]")) return;
  accountPhoto = {};
  renderAccountPhoto();
  setText("[data-account-message]", "Save Account to remove your picture.");
});

document.getElementById("account-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const button = form.querySelector('button[type="submit"]');
  button.disabled = true;
  try {
    const data = await requestJson("/api/account", "PATCH", {
      name: form.elements.name.value, email: form.elements.email.value,
      username: form.elements.username.value,
      ...(currentUser?.accountScope === "control" ? {} : {
        department: form.elements.department.value, role: form.elements.role.value, profilePhoto: accountPhoto,
        signatureImage: accountSignature,
      }),
    });
    currentUser = data.user;
    renderCurrentUser();
    applyNavbarTabs();
    await loadAccount();
    setText("[data-account-message]", "Account saved.");
  } catch (error) {
    setText("[data-account-message]", error.message);
  } finally {
    button.disabled = false;
  }
});

document.querySelector("[data-account-signature-upload]").addEventListener("change", async (event) => {
  const input = event.target;
  const button = document.querySelector('#account-form button[type="submit"]');
  button.disabled = true;
  try {
    const [signature] = await readFilesAsStoredImages(input.files);
    if (signature) accountSignature = signature;
    renderAccountSignature();
    setText("[data-account-message]", "Signature uploaded. Save Account to apply it.");
  } catch (error) {
    setText("[data-account-message]", error.message);
  } finally { button.disabled = false; input.value = ""; }
});

document.querySelector("[data-account-signature-tile]").addEventListener("click", (event) => {
  if (!event.target.closest("[data-remove-account-signature]")) return;
  accountSignature = {};
  renderAccountSignature();
  setText("[data-account-message]", "Save Account to remove your signature.");
});

const accountSignaturePad = setupSignaturePad(document.querySelector("[data-account-signature-pad]"));

document.querySelector("[data-account-pad-clear]")?.addEventListener("click", () => accountSignaturePad?.clear());

document.querySelector("[data-account-pad-use]")?.addEventListener("click", async (event) => {
  if (!accountSignaturePad || accountSignaturePad.isBlank()) {
    setText("[data-account-message]", "Draw your signature in the box first.");
    return;
  }
  const button = event.currentTarget;
  button.disabled = true;
  try {
    accountSignature = await uploadImage({ name: "Signature.png", dataUrl: accountSignaturePad.toDataUrl() });
    renderAccountSignature();
    accountSignaturePad.clear();
    setText("[data-account-message]", "Signature drawn. Save Account to apply it.");
  } catch (error) {
    setText("[data-account-message]", error.message);
  } finally {
    button.disabled = false;
  }
});
