const THEME_STORAGE_KEY = "audit-app-theme";

function applyTheme(theme) {
  const dark = theme === "dark";
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  document.querySelectorAll("[data-theme-toggle]").forEach((toggle) => { toggle.checked = dark; });
}

function setTheme(theme) {
  const value = theme === "dark" ? "dark" : "light";
  localStorage.setItem(THEME_STORAGE_KEY, value);
  applyTheme(value);
}

// Without a saved choice, follow the device setting.
function preferredTheme() {
  const saved = localStorage.getItem(THEME_STORAGE_KEY);
  if (saved) return saved;
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

applyTheme(preferredTheme());
document.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll("[data-theme-toggle]").forEach((toggle) => {
    toggle.addEventListener("change", () => setTheme(toggle.checked ? "dark" : "light"));
  });
  applyTheme(preferredTheme());
});
