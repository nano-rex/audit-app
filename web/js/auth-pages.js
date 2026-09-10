function pageFormValue(form, name, fallback = "") {
  const value = new FormData(form).get(name);
  return value ? String(value) : fallback;
}

function pageMessage(message = "", ok = false) {
  const node = document.querySelector("[data-auth-page-message]");
  if (!node) return;
  node.textContent = message;
  node.classList.toggle("success", ok);
}

async function pagePostJson(url, payload) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || "Request failed");
  return data;
}

const params = new URLSearchParams(location.search);
if (params.get("message")) pageMessage(params.get("message"));

fetch("/api/branding").then((response) => response.ok ? response.json() : null).then((branding) => {
  if (!branding) return;
  const title = branding.loginTitle || branding.appTitle || "Audit App";
  document.title = `${location.pathname.endsWith("/register.html") ? "Register" : "Login"} | ${title}`;
  document.querySelectorAll("[data-auth-brand-title]").forEach((node) => {
    node.textContent = title;
  });
}).catch(() => {});

if (location.pathname.endsWith("/login.html")) {
  fetch("/api/auth/me").then((response) => {
    if (response.ok) location.href = "/";
  }).catch(() => {});
}

document.getElementById("login-page-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  try {
    await pagePostJson("/api/auth/login", {
      email: pageFormValue(form, "email"),
      password: pageFormValue(form, "password"),
      remember: Boolean(form.elements.remember.checked),
    });
    location.href = "/";
  } catch (error) {
    pageMessage(error.message || "Login failed");
  }
});

document.querySelector("[data-page-forgot-password]")?.addEventListener("click", async () => {
  const form = document.getElementById("login-page-form");
  try {
    const data = await pagePostJson("/api/auth/forgot-password", {
      email: pageFormValue(form, "email"),
    });
    pageMessage(data.message, true);
  } catch (error) {
    pageMessage(error.message || "Password reset request failed");
  }
});

document.getElementById("register-page-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const password = pageFormValue(form, "password");
  if (password !== pageFormValue(form, "confirmPassword")) {
    pageMessage("Passwords do not match.");
    return;
  }
  try {
    const data = await pagePostJson("/api/auth/register", {
      name: pageFormValue(form, "name"),
      email: pageFormValue(form, "email"),
      password,
    });
    pageMessage(data.message, true);
  } catch (error) {
    pageMessage(error.message || "Registration failed");
  }
});
