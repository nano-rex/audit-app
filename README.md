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
cd web
python3 server.py
```

Then open `http://127.0.0.1:41883`.

## Data Storage

The Android version uses local SQLite through `ottotree_audit.db`.

The web version uses a local Python API backed by SQLite at `web/data/ottotree_audit_web.db`.
