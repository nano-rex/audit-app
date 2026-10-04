// Appearance. The organization sets the palette, font, corner style, density, and default
// light/dark; each user may choose light or dark for themselves when the organization allows it.
// Loaded in <head> so the stored theme is applied before the page is drawn.
const THEME_STORAGE_KEY = "audit-app-theme";          // The user's own choice: light, dark, or system.
const ORG_THEME_STORAGE_KEY = "audit-app-org-theme";  // The organization theme last received from the server.
const defaultOrgTheme = { preset: "default", accent: "", font: "system", corners: "rounded", density: "comfortable", mode: "system", userChoice: true };
const themeModeLabels = { system: "Follow device", light: "Light", dark: "Dark" };

function readThemeStorage(key) {
  try { return localStorage.getItem(key); } catch (error) { return null; }
}

function writeThemeStorage(key, value) {
  try {
    if (value == null || value === "") localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch (error) { /* Private browsing: the theme still applies for this page. */ }
}

function storedOrgTheme() {
  try { return { ...defaultOrgTheme, ...JSON.parse(readThemeStorage(ORG_THEME_STORAGE_KEY) || "{}") }; } catch (error) { return { ...defaultOrgTheme }; }
}

let orgTheme = storedOrgTheme();
let savedOrgTheme = orgTheme;

function userThemeChoice() {
  const value = readThemeStorage(THEME_STORAGE_KEY);
  return ["light", "dark", "system"].includes(value) ? value : "";
}

function effectiveThemeMode(theme = orgTheme) {
  const mode = (theme.userChoice !== false && userThemeChoice()) || theme.mode || "system";
  if (mode !== "system") return mode;
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function hexToRgb(hex) {
  const value = parseInt(hex.slice(1), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

// Mix `hex` toward `other` by `amount` (0 keeps hex, 1 gives other).
function mixColor(hex, other, amount) {
  const a = hexToRgb(hex), b = hexToRgb(other);
  return "#" + a.map((channel, index) => Math.round(channel + (b[index] - channel) * amount).toString(16).padStart(2, "0")).join("");
}

function relativeLuminance(hex) {
  const [r, g, b] = hexToRgb(hex).map((channel) => {
    const value = channel / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

// A custom accent colour replaces the preset's, with the lighter and darker shades derived from it.
function customAccentProperties(accent, mode, palette) {
  if (!/^#[0-9a-f]{6}$/i.test(accent || "")) return {};
  const dark = mode === "dark";
  // Filled controls carry white text in light mode, so a pale accent is darkened until that reads.
  let fill = dark ? mixColor(accent, "#ffffff", 0.2) : accent;
  while (!dark && relativeLuminance(fill) > 0.2) fill = mixColor(fill, "#000000", 0.12);
  const properties = {
    "--accent": dark ? mixColor(accent, "#ffffff", 0.35) : accent,
    "--accent-strong": dark ? mixColor(accent, "#ffffff", 0.5) : mixColor(fill, "#000000", 0.18),
    "--accent-soft": dark ? mixColor(accent, "#111111", 0.75) : mixColor(accent, "#ffffff", 0.88),
    "--accent-fill": fill,
    "--hero": dark ? mixColor(accent, "#000000", 0.55) : fill,
    "--on-accent": relativeLuminance(fill) > 0.4 ? "#111111" : "#ffffff",
  };
  if (palette === "ottotree") properties["--table-head"] = dark ? properties["--hero"] : fill;
  return properties;
}

const customAccentNames = ["--accent", "--accent-strong", "--accent-soft", "--accent-fill", "--hero", "--on-accent", "--table-head"];

function applyTheme(theme = orgTheme) {
  const root = document.documentElement;
  const mode = effectiveThemeMode(theme);
  root.dataset.theme = mode;
  root.dataset.palette = theme.preset || "default";
  root.dataset.font = theme.font || "system";
  root.dataset.corners = theme.corners || "rounded";
  root.dataset.density = theme.density || "comfortable";
  customAccentNames.forEach((name) => root.style.removeProperty(name));
  Object.entries(customAccentProperties(theme.accent, mode, root.dataset.palette)).forEach(([name, value]) => root.style.setProperty(name, value));
  syncThemeControls(theme);
}

// The organization's theme as received from the server; kept so the next page load starts with it.
function setOrgTheme(theme) {
  orgTheme = { ...defaultOrgTheme, ...(theme || {}) };
  savedOrgTheme = orgTheme;
  writeThemeStorage(ORG_THEME_STORAGE_KEY, JSON.stringify(orgTheme));
  applyTheme();
}

// Show a theme being edited without saving it; restoreOrgTheme() undoes the preview.
function previewOrgTheme(theme) {
  orgTheme = { ...defaultOrgTheme, ...theme };
  applyTheme();
}

function restoreOrgTheme() {
  orgTheme = savedOrgTheme;
  applyTheme();
}

function setTheme(choice) {
  writeThemeStorage(THEME_STORAGE_KEY, ["light", "dark", "system"].includes(choice) ? choice : "");
  applyTheme();
}

function syncThemeControls(theme = orgTheme) {
  if (typeof document === "undefined" || !document.querySelectorAll) return;
  const allowed = theme.userChoice !== false;
  document.querySelectorAll("[data-theme-choice]").forEach((select) => {
    const organization = select.querySelector('option[value=""]');
    if (organization) organization.textContent = `Organization default (${themeModeLabels[theme.mode] || "Follow device"})`;
    select.value = allowed ? userThemeChoice() : "";
    select.disabled = !allowed;
  });
  document.querySelectorAll("[data-theme-choice-note]").forEach((note) => {
    note.textContent = allowed
      ? "Saved on this device."
      : `Your organization uses ${(themeModeLabels[theme.mode] || "Follow device").toLowerCase()} appearance for everyone.`;
  });
}

applyTheme();
if (typeof document !== "undefined" && document.addEventListener) {
  document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll("[data-theme-choice]").forEach((select) => {
      select.addEventListener("change", () => setTheme(select.value));
    });
    applyTheme();
  });
  if (typeof matchMedia === "function") matchMedia("(prefers-color-scheme: dark)").addEventListener?.("change", () => applyTheme());
}
