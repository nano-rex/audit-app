# Ottotree Audit Android

Native Android prototype based on the Canva screenshots in `../../Screenshots`, with Ottotree as one audit app and Loudspeaker plus Mini Studio checks in one workflow.

## Build

From this directory:

```sh
./build.sh
```

The APK is written to:

```text
build/ottotree-audit-debug.apk
```

## Requirements

This project builds without Gradle. It expects the local workspace SDK/JDK layout:

- `../../android-sdk`
- `../../toolchains/jdk-17.0.20+8`

## Screens

- Compact Ottotree navigation row
- Today workspace for scheduled visits and on-site inspection flow
- Guided Inspections workspace with item-level SQLite storage
- Work Orders workspace for issue assignment and outlet confirmation tracking
- Equipment registry with add, edit, delete, operational status, code, model, serial number, brand, Location, and installation date
- Reports workspace for summaries, rankings, critical issues, and replacement planning
- Role-based Account screen for switching prototype access
- Manager/director Departments and Outlets setup screens
- Outlet Locations with add, edit, delete, size, and equipment assignment
- Users screen with role, department, email, title, and responsibilities
- Schedule audit visit dialog
- Captain login dialog

## Data Storage

The Android app uses a local SQLite database named `ottotree_audit.db`.

Tables:

- `audits` - saved audit entries and scores
- `schedules` - scheduled audit visits
- `captain_logins` - captain login records
- `inspection_items` - item-level checklist scoring
- `work_orders` - follow-up assignments and issue status
- `equipment` - asset registry and replacement planning
- `locations` - outlet Location records
- `admin_records` - configurable department and outlet setup records
- `users` - local user access records

The database is private to the installed app and persists across app restarts. It is removed when the app is uninstalled or app data is cleared.
