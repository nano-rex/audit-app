//! Links the system's SQLite library. Distributions ship `libsqlite3.so.0`; the unversioned
//! `libsqlite3.so` comes only with the development package, so the versioned name is used when
//! the plain one is missing. AUDIT_SQLITE_LIB names another library file or directory.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=AUDIT_SQLITE_LIB");
    if let Ok(custom) = std::env::var("AUDIT_SQLITE_LIB") {
        let path = Path::new(&custom);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
            println!("cargo:rustc-link-lib=dylib=sqlite3");
        } else {
            println!("cargo:rustc-link-arg={}", path.display());
        }
        return;
    }
    let directories = ["/usr/lib/x86_64-linux-gnu", "/usr/lib64", "/usr/lib", "/lib/x86_64-linux-gnu", "/usr/local/lib",
                       "/usr/lib/aarch64-linux-gnu", "/opt/homebrew/opt/sqlite/lib", "/usr/local/opt/sqlite/lib"];
    let has_plain = directories.iter().any(|dir| Path::new(dir).join("libsqlite3.so").exists() || Path::new(dir).join("libsqlite3.dylib").exists());
    if has_plain || cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=dylib=sqlite3");
        return;
    }
    for dir in directories {
        let versioned = Path::new(dir).join("libsqlite3.so.0");
        if versioned.exists() {
            println!("cargo:rustc-link-arg={}", versioned.display());
            return;
        }
    }
    println!("cargo:rustc-link-lib=dylib=sqlite3");
}
