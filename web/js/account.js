let accountPhoto = {};
let accountSignature = {};

function renderAccountSignature() {
  const image = document.querySelector("[data-account-signature]");
  const source = imageSource(accountSignature);
  image.hidden = !source;
  if (source) image.src = source;
  else image.removeAttribute("src");
}

function renderAccountPhoto() {
  const image = document.querySelector("[data-account-photo]");
  const source = imageSource(accountPhoto);
  image.hidden = !source;
  if (source) image.src = source;
  else image.removeAttribute("src");
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
  accountPhoto = currentUser.profilePhoto || {};
  accountSignature = currentUser.signatureImage || {};
  renderAccountPhoto();
  renderAccountSignature();
  renderCurrentUser();
  const databaseManagement = document.querySelector("[data-database-management]");
  const isSuper = currentUser.role === "Super";
  databaseManagement.hidden = !isSuper;
  if (isSuper) await loadDatabases();
  setText("[data-account-message]", "");
}

async function loadDatabases() {
  const response = await authFetch("/api/account/databases");
  const data = await response.json();
  const select = document.querySelector("[data-database-select]");
  select.innerHTML = (data.databases || []).map((database) => `<option value="${escapeAttr(database.name)}"${database.active ? " selected" : ""}>${escapeHtml(database.name)}${database.active ? " (active)" : ""}</option>`).join("");
}

document.querySelector("[data-refresh-databases]")?.addEventListener("click", () => loadDatabases().catch((error) => setText("[data-database-message]", error.message)));
document.querySelector("[data-create-database]")?.addEventListener("click", async () => {
  const input = document.querySelector("[data-new-database-name]");
  try {
    await requestJson("/api/account/databases", "POST", { name: input.value.trim() });
    input.value = "";
    await loadDatabases();
    setText("[data-database-message]", "Database created.");
  } catch (error) { setText("[data-database-message]", error.message); }
});

document.querySelector("[data-switch-database]")?.addEventListener("click", async () => {
  const name = document.querySelector("[data-database-select]").value;
  if (!name || !confirm(`Switch to ${name}? All users must sign in again.`)) return;
  try {
    await requestJson("/api/account/databases", "PATCH", { name });
    await requestJson("/api/auth/logout", "POST", {});
    window.location.href = "login.html";
  } catch (error) { setText("[data-database-message]", error.message); }
});

document.querySelector("[data-remove-database]")?.addEventListener("click", async () => {
  const name = document.querySelector("[data-database-select]").value;
  if (!name || !confirm(`Remove ${name}? This cannot be undone.`)) return;
  try {
    await requestJson(`/api/account/databases/${encodeURIComponent(name)}`, "DELETE");
    await loadDatabases();
    setText("[data-database-message]", "Database removed.");
  } catch (error) { setText("[data-database-message]", error.message); }
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

document.querySelector("[data-remove-account-photo]").addEventListener("click", () => {
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
      department: form.elements.department.value, role: form.elements.role.value, profilePhoto: accountPhoto,
      signatureImage: accountSignature,
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

document.querySelector("[data-remove-account-signature]").addEventListener("click", () => {
  accountSignature = {};
  renderAccountSignature();
  setText("[data-account-message]", "Save Account to remove your signature.");
});
