# Audit App

A facilities audit application: schedule an audit, walk a guided checklist of an outlet's
fixed and variable assets, record findings with photos, assign and close work orders, sign off, and
report.

There are two separate versions:

- `web/` — the browser application, backed by a Python server and SQLite. This is the main one.
- `android/` — a native offline Android app with its own local database. It does not
  synchronise with the web application.

## Run the web application

Python 3.9 or newer:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
.venv/bin/python web/server.py
```

Then open `http://127.0.0.1:41883`. The repository includes a database in `web/data/`; when
none exists, the first start creates one with seed records (outlets, roles, sample accounts).
The starter accounts and their passwords are defined in `web/backend/seed_data.py` and
`web/backend/control.py`; each must choose a new password at first sign-in. The Super account
is kept in its own database (`web/data/control/`), separate from every organization.

Settings are environment variables: `AUDIT_DATA_DIR`, `AUDIT_WORKERS`, `AUDIT_SECURE_COOKIES`,
`AUDIT_TRUST_PROXY`, and `PORT`. They are described in
[production-readiness.md](production-readiness.md).

## The workflow

1. An administrator sets up users and roles, outlets and their locations, fixed and variable assets
   with inspection criteria, and the priorities and audit types used by audits and findings.
2. An auditor schedules an audit (or chooses **Schedule and start now**), optionally limited to some locations, and works through
   the checklist: tick what passes, or use **Pass all** for an asset; record a remark and
   finding details for what fails; attach photos.
3. Completing the inspection calculates the score and records a finding per failed check. **Findings**
   lists each failed item once, with **Create work request**.
4. On **Maintenance > Work Requests**, a request becomes a work order (or is declined when no work is
   needed). On **Maintenance > Work Orders**, the assignee closes the order when done; its request and
   findings close with it.
5. On **Sign-off**, the auditor, verifier, and acknowledger sign (drawn, uploaded, or the signature saved
   on their account). Once every linked work order is closed, a verifier closes the audit, which becomes read-only.
6. Reports, charts, and CSV, Excel, JSON, and PDF exports are available throughout.

The dashboard lists every scheduled audit not yet finished, and each tab shows a number when
something there waits for the signed-in user. Full instructions are in [USER_GUIDE.txt](USER_GUIDE.txt).

## Documentation

| File | Contents |
| --- | --- |
| [USER_GUIDE.txt](USER_GUIDE.txt) | Step-by-step use of the web application |
| [scope.md](scope.md) | What the application covers, where it differs from the original specification, and what is deferred |
| [backend-architecture.md](backend-architecture.md) | Server modules, conventions, storage, and how to run the checks |
| [production-readiness.md](production-readiness.md) | Configuration, deployment, measurements, and open items |
| [web/README.md](web/README.md) | Layout of the web folder and the fixed-asset import tool |
| [android/README.md](android/README.md) | Building the Android app |

## Tests

```sh
.venv/bin/python -m pip install -r requirements-dev.txt
.venv/bin/python -m unittest discover -s tests
node tests/test_frontend.cjs
```

## Android

```sh
cd android
./build.sh
```

The APK is written to `android/build/audit-app-debug.apk`. The build expects the SDK and JDK
layout described in [android/README.md](android/README.md).

## Data storage

The web application keeps every persistent record, and the bytes of every image, in the
SQLite database in the data directory. A SQLite backup is therefore a complete backup; take
it with SQLite's backup API while the server is running. JSON is used for HTTP messages and
exports, not as stored documents.
