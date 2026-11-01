# Audit App

A facilities audit application. You schedule an audit, walk a guided checklist of an outlet's fixed
and variable assets, and record findings with photos. Then you assign and close work orders, sign
off, and report.

This is the Rust rewrite of the web application. It has one server executable and the browser
application in `web/`. The server uses only the Rust standard library: it has no third-party crates,
which reduces supply-chain risk. The only native dependency is the system SQLite library, called
through a small hand-written FFI layer. The Python server and the native Android app remain in
[audit-app-old](https://github.com/nano-rex/audit-app-old).

## Build and run

You need Rust 1.85 or newer and the SQLite 3 shared library (`libsqlite3.so.0`; the `-dev`
package is not required):

```sh
cargo build --release
./target/release/audit-app
```

Then open `http://127.0.0.1:41883`. On first start the server creates `web/data/ottotree_audit_web.db`
with seed records (outlets, roles, sample accounts). The starter accounts' passwords are in
`src/config.rs` (organization accounts) and `src/control.rs` (Super), and each account must choose a new password at first
sign-in. The Super account (`super`) is kept in its own database (`web/data/control/control.db`),
separate from every organization.

Databases written by the Python server's current release open unchanged. A database old enough to
still have JSON columns must first be opened once with audit-app-old, which converts it.

```text
audit-app                                  Serve the application on 127.0.0.1:$PORT (41883)
audit-app import-assets [SOURCE] [--db PATH] [--limit N]
                                           Import Fixed Asset Listing workbooks (.xlsx file or folder)
audit-app load-test [--users N]            Run a local load test on a temporary database
audit-app --version | --help
```

Settings are environment variables: `AUDIT_DATA_DIR`, `AUDIT_WORKERS`, `AUDIT_SECURE_COOKIES`,
`AUDIT_TRUST_PROXY`, `AUDIT_CONTROL_DB`, `AUDIT_SESSION_COOKIE`, `AUDIT_WEB_ROOT`, and `PORT`. They
are described in [production-readiness.md](production-readiness.md). To build against a SQLite
library in another location, set `AUDIT_SQLITE_LIB` to its directory at build time.

## The workflow

1. An administrator sets up the organization:
   - users and roles;
   - outlets and their locations;
   - fixed and variable assets with their inspection criteria;
   - the priorities and audit types used by audits and findings.
2. An auditor schedules an audit, or chooses **Schedule and start now**. The audit can be limited
   to some locations. The auditor works through the checklist:
   - tick what passes, or use **Pass all** for an asset;
   - record a remark and finding details for what fails;
   - attach photos.
3. Completing the inspection calculates the score and records one finding per failed check.
   **Findings** lists each failed item once, with **Create work request**.
4. On **Maintenance > Work Requests**, a request becomes a work order. It can instead be declined
   when no work is needed. On **Maintenance > Work Orders**, the assignee closes the order when the
   work is done, and its request and findings close with it.
5. On **Sign-off**, the auditor, verifier, and acknowledger sign. A signature can be drawn,
   uploaded, or taken from the one saved on the account. Once every linked work order is closed, a
   verifier closes the audit, which becomes read-only.
6. Reports, charts, and CSV, Excel, JSON, and PDF exports are available throughout.

Full instructions are in [USER_GUIDE.txt](USER_GUIDE.txt).

## Tests

```sh
cargo test                  # unit tests and the HTTP suite in tests/api.rs
node tests/test_frontend.cjs
```

`tests/api.rs` starts the real server for each test, on its own temporary data directory and port.
It then drives the server over HTTP the way the browser does.

## Documentation

| File | Contents |
| --- | --- |
| [USER_GUIDE.txt](USER_GUIDE.txt) | Step-by-step use of the web application |
| [scope.md](scope.md) | What the application covers, and what changed in the Rust rewrite |
| [architecture.md](architecture.md) | Server modules, conventions, storage, and the self-written formats |
| [production-readiness.md](production-readiness.md) | Configuration, deployment, measurements, and open items |
| [web/README.md](web/README.md) | Layout of the web folder |

## Data storage

Every persistent record, and the bytes of every image, is kept in the SQLite database in the data
directory. A SQLite backup is therefore a complete backup. Take it with SQLite's backup API while
the server is running, for example `sqlite3 data.db ".backup copy.db"`.
