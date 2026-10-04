# Backend architecture

The backend is one Python application with explicit module boundaries. All application
modules live in `web/backend/`; `web/server.py` is the entry point. Regression tests are in
`tests/` and maintenance commands in `tools/`. The static web root is `web/`, and the
database is under the configured data directory (`web/data/` by default).

| Module | Responsibility |
| --- | --- |
| `web/server.py` | Startup, data-directory configuration, compatibility exports for maintenance tools |
| `web/backend/config.py` | Paths, defaults, shared runtime state |
| `web/backend/database.py` | Connection lifetime, transactions, shared database lookups |
| `web/backend/migrations.py`, `web/backend/seed_data.py` | Schema upgrades and seed records |
| `web/backend/api.py`, `web/backend/http_support.py` | HTTP boundary, access checks, reads, static responses, bounded workers, error responses |
| `web/backend/routes.py` | Explicit method/path registry for mutations |
| `web/backend/routes_*.py` | Account, asset, inspection, location, setup, and work-order mutations |
| `web/backend/accounts.py`, `web/backend/catalog.py`, `web/backend/work_orders.py` | Account projection, catalog queries, finding/work-order queries |
| `web/backend/login_throttle.py`, `web/backend/session_store.py` | Failed sign-in limits held in memory; durable hashed session tokens |
| `web/backend/database_manager.py` | Creating, switching, and removing company databases without redirecting other requests |
| `web/backend/permissions.py` | Role inheritance, per-user overrides, inspection action/signature permissions |
| `web/backend/workflow.py` | Work-order state transitions, completion evidence, verification identity |
| `web/backend/inspection_notifications.py` | Notifications addressed to effective verifier/acknowledger permissions and audit creators |
| `web/backend/inspections.py`, `web/backend/scoring.py` | Audit finalization, linked findings, score snapshots |
| `web/backend/media_store.py` | Image validation and content-addressed SQLite BLOBs |
| `web/backend/relational_values.py` | Typed relational child rows for structured attributes; no JSON columns |
| `web/backend/storage_migration.py` | Verified SQLite backup before destructive schema conversion |
| `web/backend/reports.py`, `web/backend/pdf_report.py` | Report data, exports, paginated PDF rendering |
| `web/backend/response_cache.py` | Bounded response cache and pre-encoded JSON |
| `web/backend/location_integrity.py` | Hierarchy validation, membership updates, and audit-history retention |
| `web/backend/audit_closure.py` | Final audit closure and immutable completion records |
| `web/backend/control.py` | The Super accounts' own database: sign-in, sessions, history, page order, and moving Super accounts out of organizations |
| `web/backend/todo.py` | Per-user next steps for the dashboard and the unread notification count |
| `web/backend/reminders.py` | Targeted assignment notifications and daily due reminders |
| `web/backend/common.py` | Shared date, identifier, password, and workflow helpers |

## Conventions

- Route handlers own transactions and call domain functions. Domain functions receive an
  existing database connection when they must take part in the same atomic operation.
- Keep HTTP response formatting out of domain functions.
- Add new mutation routes to the registry in `routes.py`; reads are handled in `api.py`.
- Raise `ValueError` (400), `PermissionError` (403), or `WorkflowError` (its own status) for
  expected failures; `api_errors` turns them into JSON. Anything else is logged and answered
  with a JSON 500.
- Keep configuration changes at startup; isolated tools and tests call
  `server.configure_data_directory()` before opening connections.

## Storage

Each structured attribute (checklist snapshots, scores, settings, permissions, signatures,
evidence metadata) is stored as typed rows in `value_sets` / `value_nodes`; domain tables hold
integer foreign keys to them. Images are BLOBs in `media_images`, addressed by content hash and
streamed by `/api/media/...`. Cleanup triggers remove a value set after its last reference
disappears. JSON is used for HTTP messages and exports only.

On the first start with an older database, a verified SQLite backup is written to the data
directory's `backups/` folder, legacy media files are imported, and JSON columns are converted
and dropped. Rolling the code back therefore requires restoring that backup.

## Frontend

`web/index.html` is a shell whose `<!-- include: ... -->` comments are expanded by the server
from `web/html/`. Scripts in `web/js/` are plain globals loaded in order; `state.js` holds
shared state and the page list. Colours are tokens in `web/css/base.css`, with one dark set.

## Verification

From the repository root, with Python 3.9+ and Node.js 22+:

```sh
.venv/bin/python -m pip install -r requirements-dev.txt
.venv/bin/python -m unittest discover -s tests
.venv/bin/python -m pyflakes web tests tools/load_test.py
node tests/test_frontend.cjs
.venv/bin/python tools/load_test.py --users 100
git diff --check
```

The HTTP tests and the load test use temporary databases and bind a loopback socket. The
frontend tests run the browser scripts in a JavaScript VM with stubbed DOM objects; they are
not browser tests. See [production-readiness.md](production-readiness.md) for the latest
results and what has not been verified.
