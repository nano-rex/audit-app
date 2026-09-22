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

applyTheme(localStorage.getItem(THEME_STORAGE_KEY) || "light");
document.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll("[data-theme-toggle]").forEach((toggle) => {
    toggle.addEventListener("change", () => setTheme(toggle.checked ? "dark" : "light"));
  });
  applyTheme(localStorage.getItem(THEME_STORAGE_KEY) || "light");
});
