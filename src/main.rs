//! The audit-app command: serve the application, or run a maintenance tool.

fn main() {
    audit_app::main_with(std::env::args().skip(1).collect());
}
