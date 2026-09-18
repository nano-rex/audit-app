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

function showLogin(message = "") {
  if (!location.pathname.endsWith("/login.html")) {
    const suffix = message ? `?message=${encodeURIComponent(message)}` : "";
    location.href = `/login.html${suffix}`;
    return;
  }
  document.body.classList.add("auth-locked");
  setAuthMessage("[data-login-message]", message);
  const dialog = document.getElementById("login-dialog");
  if (dialog && !dialog.open) dialog.showModal();
}

function hideLogin() {
  document.body.classList.remove("auth-locked");
  const dialog = document.getElementById("login-dialog");
  if (dialog?.open) dialog.close();
}

function renderCurrentUser() {
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
  hideLogin();
  renderCurrentUser();
  if (currentUser?.resetRequired) {
    document.getElementById("change-password-dialog")?.showModal();
  }
  return true;
}

function wireAuth() {
  document.getElementById("login-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    setAuthMessage("[data-login-message]");
    const response = await fetch("/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        email: formValue(form, "email", ""),
        password: formValue(form, "password", ""),
        remember: Boolean(form.elements.remember.checked),
      }),
    });
    const data = await response.json();
    if (!response.ok) {
      setAuthMessage("[data-login-message]", data.error || "Login failed");
      return;
    }
    currentUser = data.user;
    hideLogin();
    renderCurrentUser();
    loadApp();
  });

  document.querySelector("[data-forgot-password]")?.addEventListener("click", async () => {
    const form = document.getElementById("login-form");
    const response = await fetch("/api/auth/forgot-password", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email: formValue(form, "email", "") }),
    });
    const data = await response.json();
    setAuthMessage("[data-login-message]", data.message || "Ask an administrator to reset this password.");
  });

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
    form.closest("dialog").close();
  });

  document.querySelector("[data-logout]")?.addEventListener("click", async () => {
    await fetch("/api/auth/logout", { method: "POST" });
    currentUser = null;
    renderCurrentUser();
    location.href = "/login.html";
  });
}
