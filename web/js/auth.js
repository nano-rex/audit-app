let currentUser = null;

async function authFetch(url, options = {}) {
  const response = await fetch(url, options);
  if (response.status === 401) {
    currentUser = null;
    showLogin();
  }
  if (!response.ok && (!options.method || options.method === "GET")) {
    const data = await response.json().catch(() => ({}));
    throw new Error(data.error || `Request failed: ${response.status}`);
  }
  return response;
}

function setAuthMessage(selector, message = "") {
  const node = document.querySelector(selector);
  if (node) node.textContent = message;
}

// Signing in happens on login.html; an expired session returns there.
function showLogin(message = "") {
  location.href = `/login.html${message ? `?message=${encodeURIComponent(message)}` : ""}`;
}

function renderCurrentUser() {
  document.querySelectorAll('[data-open="new-audit"]').forEach((button) => {
    button.hidden = !(currentUser?.permissions || []).includes("inspections") || !(currentUser?.inspectionPermissions || []).includes("auditor");
  });
  const inspection = document.getElementById("inspection-form");
  if (inspection && !inspection.elements.inspectionSessionId.value) inspection.elements.auditor.value = currentUser?.name || "";
  const audit = document.getElementById("new-audit-form");
  if (audit) audit.elements.auditor.value = currentUser?.name || "";
  const strip = document.querySelector("[data-account-strip]");
  if (strip) strip.hidden = !currentUser;
  const label = document.querySelector("[data-auth-user]");
  if (label && currentUser) {
    label.textContent = `${currentUser.name} | ${currentUser.role}`;
  }
}

async function requireLogin() {
  const response = await fetch("/api/auth/me");
  if (!response.ok) {
    showLogin();
    return false;
  }
  const data = await response.json();
  currentUser = data.user;
  renderCurrentUser();
  if (currentUser?.resetRequired) {
    // The server refuses everything else until the temporary password is replaced.
    forcePasswordChange();
    return false;
  }
  return true;
}

function forcePasswordChange() {
  const dialog = document.getElementById("change-password-dialog");
  if (!dialog) return;
  dialog.dataset.forced = "true";
  document.getElementById("change-password-form")?.reset();
  setAuthMessage("[data-change-password-message]", "Your password is temporary. Choose a new one to continue.");
  if (!dialog.open) dialog.showModal();
}

function wireAuth() {
  document.querySelector("[data-open-change-password]")?.addEventListener("click", () => {
    setAuthMessage("[data-change-password-message]");
    document.getElementById("change-password-form")?.reset();
    document.getElementById("change-password-dialog")?.showModal();
  });

  document.getElementById("change-password-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const newPassword = formValue(form, "newPassword", "");
    if (newPassword !== formValue(form, "confirmPassword", "")) {
      setAuthMessage("[data-change-password-message]", "New passwords do not match.");
      return;
    }
    const response = await fetch("/api/auth/change-password", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        oldPassword: formValue(form, "oldPassword", ""),
        newPassword,
      }),
    });
    const data = await response.json();
    if (!response.ok) {
      setAuthMessage("[data-change-password-message]", data.error || "Password change failed.");
      return;
    }
    const dialog = form.closest("dialog");
    const forced = dialog.dataset.forced === "true";
    delete dialog.dataset.forced;
    dialog.close();
    if (forced) loadApp();
  });

  // A required password change cannot be dismissed with Escape.
  document.getElementById("change-password-dialog")?.addEventListener("cancel", (event) => {
    if (event.currentTarget.dataset.forced === "true") event.preventDefault();
  });

  document.querySelector("[data-forced-logout]")?.addEventListener("click", async () => {
    await fetch("/api/auth/logout", { method: "POST" });
    location.href = "/login.html";
  });

  document.querySelector("[data-logout]")?.addEventListener("click", async () => {
    await fetch("/api/auth/logout", { method: "POST" });
    currentUser = null;
    renderCurrentUser();
    location.href = "/login.html";
  });
}
