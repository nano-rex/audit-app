# Audit App

Ottotree Audit application with two platform versions:

- `android/` - native Android app, builds an APK using the local SDK
- `web/` - browser version backed by a local Python SQLite API

The product model is one Ottotree audit system. Loudspeaker and Mini Studio checks are combined into the same inspection workflow:

- Loudspeaker
- Mini Studio

The first screen is `Today`, which is organized for on-site work: scheduled visits, pending uploads, follow-up counts, and quick actions for starting inspections or scanning QR codes.

The `Inspections` screen is the guided field workflow. It records outlet, zone/location, inspector, checklist item scores, evidence status, and notes into SQLite.

The `Work Orders` screen tracks inspection findings and manual issues through assignment, repair, verification, and outlet confirmation.

The `Equipment` screen tracks editable equipment items with name, description, type, operational status, code, model, serial number, brand, outlet Location, and installation date.

The `Reports` screen consolidates outlet rankings, score charts, critical issues, and exportable summaries.

Managers and directors can manage `Departments` and `Outlets`, which supply the selectable department and outlet choices across users, work orders, schedules, inspections, equipment, and reports. Each outlet can also have Locations for inspection and equipment placement.

The `Users` screen supports account creation, editing, deletion, activation, role assignment, department assignment, title, email, password reset, and responsibilities. The `Roles` screen lets admin users configure app-section access for non-admin roles.

## Android

```sh
cd android
./build.sh
```

APK output:

```text
android/build/ottotree-audit-debug.apk
```

## Web

Run the local SQLite-backed web server:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
.venv/bin/python web/server.py
```

Then open `http://127.0.0.1:41883`.

See [backend architecture](backend-architecture.md) for module responsibilities and test commands,
[production readiness](production-readiness.md) for deployment limits and load-test results,
and [requirements progress](requirements-progress.md) for the remaining core work.

## Data Storage

The Android version uses local SQLite through `ottotree_audit.db`.

The web version uses a local Python API backed by SQLite at `web/data/ottotree_audit_web.db`.

All persistent web records and image bytes are stored in `web/data/ottotree_audit_web.db`.
Images use SQLite BLOBs; checklist answers, permissions, settings, and image metadata use typed
relational rows linked to their owning records. JSON is used for HTTP messages and exports, not
as database document columns. Set `AUDIT_DATA_DIR` to use another data directory.

On the first startup of an older database, a SQLite backup is created in `web/data/backups/`,
legacy media files are copied into BLOBs, and JSON columns are migrated and removed. Stop the
old server before starting the new release. Old media files are retained for rollback but are
no longer read by the app. Use SQLite’s backup API for live WAL databases; a complete SQLite
backup includes all images. Rolling back code requires restoring the matching pre-migration
database and legacy media files.
