# Audit App Web

SQLite-backed web version of the audit application.

See [production readiness and measured performance](../production-readiness.md) for the audit findings, 100-user benchmark, configuration, and remaining release work.

## Structure

- `server.py` serves the SQLite API and static web app.
- `index.html` is the small page shell. It uses `<!-- include: ... -->` comments that are expanded by `server.py`.
- `html/tabs/` contains each main tab panel.
- `html/dialogs/` contains modal form markup.
- `js/` contains browser scripts split by responsibility.
- `css/` contains stylesheets loaded directly by the HTML entry points; `styles.css` remains a compatibility entry point.
- `data/ottotree_audit_web.db` is the local SQLite database. Prototype data can be recreated from setup screens.

Run:

```sh
python3 server.py
```

Python 3.9 or later is required. Optional settings: `AUDIT_DATA_DIR` overrides the database directory, `AUDIT_WORKERS` sets the active request limit (default 8), and `AUDIT_SECURE_COOKIES=1` marks login cookies for HTTPS-only use. The app stays bound to `127.0.0.1`.

Then open:

```text
http://127.0.0.1:41883
```

Data is stored in:

```text
data/ottotree_audit_web.db
```

The first screen is `Today`, backed by SQLite schedules and audit status.

Use `Inspections` for the guided checklist workflow. Submissions write one audit row plus item-level checklist rows to SQLite.

Use `Work Orders` for follow-up issues. Low-scoring inspection items create work orders automatically, and users can also create them manually.

Use `Fixed Assets` for the QR/asset registry and fixed asset condition records. Fixed asset `Code` is the canonical unique key, and QR codes are generated from the same `Code`.

Import XLSX fixed asset listings from the repo-local `Fixed_Assets/` folder with:

```sh
python3 tools/import_fixed_assets.py
```

Or pass a device-specific folder explicitly:

```sh
python3 tools/import_fixed_assets.py /path/to/Fixed_Assets
```

Use `Reports` for the monthly summary, KPI metrics, outlet rankings, critical issues, and CSV/JSON exports.

Use `Categories`, `Departments`, `Outlets`, and `Users` for configurable setup data used by the audit workflow.
