# Backend architecture

The backend is one Python application with explicit module boundaries. Splitting it improves
reviewability and testing; it does not by itself increase request throughput.

| Module | Responsibility |
| --- | --- |
| `web/server.py` | Startup, data-directory configuration, compatibility exports for maintenance tools |
| `web/config.py` | Paths, defaults, shared runtime state |
| `web/database.py` | Connection lifetime, transactions, shared database lookups |
| `web/migrations.py`, `web/seed_data.py` | Schema upgrades and seed records |
| `web/api.py`, `web/http_support.py` | HTTP boundary, access checks, reads, static responses, bounded workers |
| `web/routes.py` | Explicit method/path registry for mutations |
| `web/routes_*.py` | Account, asset, inspection, location, setup, and work-order mutations |
| `web/accounts.py`, `web/catalog.py`, `web/work_orders.py` | Account projection, catalog queries, finding/work-order queries |
| `web/permissions.py` | Role inheritance, per-user overrides, inspection action/signature permissions |
| `web/workflow.py` | Work-order state transitions, completion evidence, verification identity |
| `web/inspection_notifications.py` | Notifications addressed to effective verifier/acknowledger permissions and audit creators |
| `web/inspections.py`, `web/scoring.py` | Audit finalization, linked findings, score snapshots |
| `web/media_store.py` | Image validation and content-addressed SQLite BLOBs |
| `web/relational_values.py` | Typed relational child rows for structured attributes; no JSON columns |
| `web/storage_migration.py` | Verified SQLite backup before destructive schema conversion |
| `web/reports.py`, `web/pdf_report.py` | Report data, exports, paginated PDF rendering |
| `web/response_cache.py` | Bounded response cache and pre-encoded JSON |
| `web/common.py` | Shared date, identifier, password, and workflow helpers |

Route handlers own transactions and call domain functions. Domain functions receive an existing
database connection when they must participate in the same atomic operation. Keep HTTP response
formatting out of domain functions. Add new routes to the explicit registry rather than growing
another large conditional handler. Keep configuration changes at startup; use
`server.configure_data_directory()` before opening connections in isolated tools/tests.

No schema or data files were deleted during the extraction. Public endpoint paths and the
`python web/server.py` entry point remain available. New photo/report functionality requires
the dependencies in `requirements.txt`.

## Verification

From the repository root, using Python 3.11+ and Node.js 22:

```sh
.venv/bin/python -m pip install -r requirements-dev.txt
.venv/bin/python -m unittest discover -s tests -v
.venv/bin/python -m pyflakes web tests tools/load_test.py
node tests/test_frontend.cjs
.venv/bin/python tools/load_test.py --users 100
git diff --check
```

The HTTP tests and load test use temporary databases and require permission to bind a loopback
socket. Tests cover transaction rollback, authentication/section permissions, concurrent audit
completion, caching, route create/update/delete behavior, private evidence, weighted scores,
finding/work-order links, and PDF pagination with actual images/signatures.

On 2026-09-15, the refactored application passed 16 Python tests and 4 frontend tests. A synthetic
100-user run completed 400 requests in 4.84 seconds with zero errors and all 100 drafts saved.
Its p95 four-request user flow was 4.16 seconds. This is a local synthetic result, not a
production capacity guarantee; it excludes concurrent logins and large uploads/PDF downloads.

The 2026-09-18 feature run (permissions, signatures, targeted notifications, linked schedules)
completed 400 requests from 100 simultaneous users in 6.0 seconds, with zero errors and
100 drafts saved. The p95 four-request flow was 5.29 seconds. This run uses the same fixture
sizes and has the same limits as the earlier benchmark.

## Rollback

The SQLite-only storage migration removes legacy JSON columns. Stop the old server before
upgrading. Startup first creates a verified SQLite backup under the data directory's `backups/`
folder, imports legacy media files, then converts nested data to `value_sets` / `value_nodes`.
Each attribute has its own typed SQL row; domain tables hold integer foreign keys. Image
references store BLOB identifiers, and `/api/media/...` streams database bytes without redirects.
Cleanup triggers remove replaced value sets after their last domain reference disappears.

For rollback, restore the matching pre-migration database and retained media folder together
with the previous code. Do not run older code against the converted schema. New backups need
only SQLite's backup API, which includes all images and structured records.
