//! Settings and fixed lists of the audit application.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use crate::json::{obj, strings, Value};

pub const DEFAULT_PORT: &str = "41883";
pub const DEFAULT_PASSWORD: &str = "password123";
pub const SUPER_ROLE: &str = "Super";
pub const ADMIN_ROLE: &str = "Admin";
pub const DATABASE_FILE: &str = "ottotree_audit_web.db";

pub const LOUDSPEAKER_OUTLETS: [&str; 15] = ["STP", "SBA", "TPG", "AQP", "CCS", "SPK", "BSP", "MYT", "DJM", "KPG", "TSU", "TMA", "PGA", "PSC", "PWS"];

pub const DEFAULT_INSPECTION_CRITERIA: [&str; 4] =
    ["Present and correctly placed", "Clean and free from visible damage", "Operational during inspection", "Label, cable, or accessory is complete"];

// Variable assets (stored as kind "fixture") are the building and everything without a code of its
// own: walls, floors, furniture, fittings. Their asset type comes from VARIABLE_ASSET_TYPES (kept in
// the setting assets.variableTypes); categories were retired in favour of asset types.
pub const DEFAULT_FIXTURE_CRITERIA: [&str; 4] =
    ["Clean and free from stains or marks", "Intact with no cracks, leaks, or loose parts", "Works as intended", "Safe with no hazard to users"];

pub const ITEM_KINDS: [&str; 2] = ["asset", "fixture"];

pub const VARIABLE_ASSET_TYPES: [&str; 15] = ["AV Equipment", "COM Equipment", "Facility", "F&B Equipment", "Electrical", "Plumbing", "Air Conditioning",
                                             "Lighting", "Furniture", "Building", "Safety", "Cleanliness", "IT / Network", "KTV Equipment", "Others"];

/// (name, classification, due days)
pub const DEFAULT_PRIORITY_LEVELS: [(&str, &str, i64); 5] = [
    ("Priority", "Priority", 3),
    ("Non-Priority", "Non-Priority", 14),
    ("High", "Priority", 3),
    ("Medium", "Non-Priority", 14),
    ("Low", "Non-Priority", 14),
];

// How an audit type grades: Detailed checks every asset; Casual checks assets with the same name in
// a location once, as a group, and the result applies to all of them.
pub const AUDIT_STYLES: [&str; 2] = ["Detailed", "Casual"];

/// (name, description, active)
pub const DEFAULT_AUDIT_TYPES: [(&str, &str, bool); 2] = [("Standard", "Full outlet inspection", true), ("Quick", "Short follow-up inspection", true)];

pub fn default_scoring_settings() -> Value {
    obj([
        ("passMark", Value::Int(70)),
        ("weighting", "Equal".into()),
        ("excellentBand", Value::Int(90)),
        ("goodBand", Value::Int(70)),
        ("belowBand", Value::Int(60)),
    ])
}

pub const DEFAULT_REPORT_SETTINGS: [(&str, &str); 8] = [
    ("companyName", "Ottotree"),
    ("departmentHeader", "Facilities Department"),
    ("logoText", "OTTOTREE"),
    ("appTitle", "Ottotree Audit"),
    ("appSubtitle", "Loudspeaker & Mini Studio operations"),
    ("businessUnitLabel", "Ottotree"),
    ("reportHeading", "audit report"),
    ("loginTitle", "Ottotree Audit"),
];

pub fn report_default(key: &str) -> &'static str {
    DEFAULT_REPORT_SETTINGS.iter().find(|(name, _)| *name == key).map(|(_, value)| *value).unwrap_or("")
}

pub fn default_system_settings() -> Value {
    obj([
        ("findingsEnabled", Value::Bool(true)),
        // When off, only assets with a failed check need photo evidence before an inspection can be completed.
        ("requirePhotoEveryAsset", Value::Bool(true)),
    ])
}

// An organization's look. Each company database stores its own; the Super account edits it.
pub const THEME_CHOICES: [(&str, &[&str]); 5] = [
    ("preset", &["default", "ottotree", "ocean", "plum", "ember", "slate"]),
    ("font", &["system", "noto-sans-sc", "serif"]),
    ("corners", &["rounded", "square", "soft"]),
    ("density", &["comfortable", "compact"]),
    ("mode", &["system", "light", "dark"]),
];

pub fn default_theme_settings() -> Value {
    obj([
        ("preset", "default".into()),
        ("accent", "".into()), // "#rrggbb" replaces the preset's accent colour; empty keeps it.
        ("font", "system".into()),
        ("corners", "rounded".into()),
        ("density", "comfortable".into()),
        ("mode", "system".into()),        // The organization's default appearance.
        ("userChoice", Value::Bool(true)), // Whether each user may pick light or dark for themselves.
    ])
}

/// Accent colour of each palette, used where CSS is not available (the PDF report).
pub fn theme_preset_accent(preset: &str) -> &'static str {
    match preset {
        "ottotree" => "#1e99b4",
        "ocean" => "#2563eb",
        "plum" => "#7c3aed",
        "ember" => "#c2410c",
        "slate" => "#475569",
        _ => "#47735f",
    }
}

pub const APP_TABS: [(&str, &str); 14] = [
    ("today", "Dashboard"),
    ("inspections", "Inspections"),
    ("history", "History"),
    ("findings", "Findings"),
    ("work-orders", "Maintenance"),
    ("equipment", "Fixed Assets"),
    ("reports", "Reports"),
    ("categories", "Assets"),
    ("departments", "Departments"),
    ("outlets", "Outlets"),
    ("users", "Users"),
    ("roles", "Roles"),
    ("notifications", "Notifications"),
    ("settings", "Settings"),
];

// These pages are intentionally outside the permission model. They are only exposed to the
// built-in Super account and can never be granted to a role.
pub const SUPER_TABS: [(&str, &str); 2] = [("super-dashboard", "Super Dashboard"), ("super-settings", "Super Settings")];

pub fn tab_ids() -> Vec<&'static str> {
    APP_TABS.iter().map(|(id, _)| *id).collect()
}

pub fn tabs_value() -> Value {
    Value::Array(APP_TABS.iter().map(|(id, label)| obj([("id", (*id).into()), ("label", (*label).into())])).collect())
}

pub fn all_tabs() -> Value {
    strings(&tab_ids())
}

struct Paths {
    root: PathBuf,
    data_dir: PathBuf,
    db_path: RwLock<PathBuf>,
    session_cookie: String,
}

static PATHS: OnceLock<Paths> = OnceLock::new();

thread_local! {
    /// Lets one request initialize another database without redirecting concurrent requests.
    static DB_PATH_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

fn absolute(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| std::env::current_dir().map(|dir| dir.join(path)).unwrap_or_else(|_| path.to_path_buf()))
}

/// The web root: AUDIT_WEB_ROOT, else `web/` beside the executable or in the source tree.
fn find_root() -> PathBuf {
    if let Ok(root) = std::env::var("AUDIT_WEB_ROOT") {
        if !root.trim().is_empty() {
            return absolute(Path::new(root.trim()));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1).take(4) {
            if dir.join("web").join("index.html").is_file() {
                return absolute(&dir.join("web"));
            }
        }
    }
    absolute(&Path::new(env!("CARGO_MANIFEST_DIR")).join("web"))
}

// Browsers share cookies between ports of the same host, so instances running side by side (one
// per organization) each need their own session cookie name. The default port keeps the original
// name so existing sign-ins survive.
fn session_cookie_name() -> Result<String, String> {
    let mut name = std::env::var("AUDIT_SESSION_COOKIE").unwrap_or_default().trim().to_string();
    if name.is_empty() {
        let port = std::env::var("PORT").unwrap_or_else(|_| DEFAULT_PORT.into()).trim().to_string();
        name = if port == DEFAULT_PORT { "ottotree_session".into() } else { format!("ottotree_session_{port}") };
    }
    if name.is_empty() || name.len() > 64 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err("AUDIT_SESSION_COOKIE may use only letters, digits, hyphens, and underscores".into());
    }
    Ok(name)
}

/// Read the environment once at startup.
pub fn init() -> Result<(), String> {
    let root = find_root();
    let data_dir = match std::env::var("AUDIT_DATA_DIR") {
        Ok(dir) if !dir.trim().is_empty() => absolute(Path::new(dir.trim())),
        _ => root.join("data"),
    };
    std::fs::create_dir_all(&data_dir).map_err(|error| format!("Cannot create the data directory {}: {error}", data_dir.display()))?;
    let data_dir = absolute(&data_dir);
    let db_path = data_dir.join(DATABASE_FILE);
    let session_cookie = session_cookie_name()?;
    let _ = PATHS.set(Paths { root, data_dir, db_path: RwLock::new(db_path), session_cookie });
    Ok(())
}

fn paths() -> &'static Paths {
    PATHS.get().expect("config::init runs at startup")
}

pub fn root() -> &'static Path {
    &paths().root
}

pub fn data_dir() -> &'static Path {
    &paths().data_dir
}

pub fn session_cookie() -> &'static str {
    &paths().session_cookie
}

/// The organization database requests use.
pub fn db_path() -> PathBuf {
    paths().db_path.read().unwrap_or_else(|poison| poison.into_inner()).clone()
}

pub fn set_db_path(path: PathBuf) {
    *paths().db_path.write().unwrap_or_else(|poison| poison.into_inner()) = path;
}

/// The database this thread works on: an override while one is being created, else the active one.
pub fn active_db_path() -> PathBuf {
    DB_PATH_OVERRIDE.with(|cell| cell.borrow().clone()).unwrap_or_else(db_path)
}

/// Run `work` with this thread's database set to `path`.
pub fn with_db_override<T>(path: PathBuf, work: impl FnOnce() -> T) -> T {
    let previous = DB_PATH_OVERRIDE.with(|cell| cell.replace(Some(path)));
    struct Restore(Option<PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            DB_PATH_OVERRIDE.with(|cell| *cell.borrow_mut() = previous);
        }
    }
    let _restore = Restore(previous);
    work()
}

pub fn env_flag(name: &str) -> bool {
    std::env::var(name).map(|value| value == "1").unwrap_or(false)
}
