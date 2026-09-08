# Ottotree Audit Web

SQLite-backed web version of the Ottotree Audit prototype.

## Structure

- `server.py` serves the SQLite API and static web app.
- `index.html` is the small page shell. It uses `<!-- include: ... -->` comments that are expanded by `server.py`.
- `html/tabs/` contains each main tab panel.
- `html/dialogs/` contains modal form markup.
- `js/` contains browser scripts split by responsibility.
- `css/` contains the stylesheet sections imported by `styles.css`.
- `data/ottotree_audit_web.db` is the local SQLite database. Prototype data can be recreated from setup screens.

Run:

```sh
python3 server.py
```

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

Use `Equipment` for the QR/asset registry and equipment condition records.

Use `Reports` for the monthly summary, KPI metrics, outlet rankings, critical issues, and CSV/JSON exports.

Use `Categories`, `Departments`, `Outlets`, and `Users` for configurable setup data used by the audit workflow.
