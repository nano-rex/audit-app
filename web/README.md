# Audit App Web

The browser application and its Python server. For running, configuration, and the workflow,
see the [repository README](../README.md); for server modules, see
[backend architecture](../backend-architecture.md).

## Layout

- `server.py` — entry point; serves the API and the static application on `127.0.0.1`.
- `backend/` — the server's Python modules.
- `index.html` — the page shell. Its `<!-- include: ... -->` comments are expanded by the server.
- `login.html`, `register.html` — sign-in and registration pages.
- `html/tabs/` — one file per page; `html/dialogs/` — dialog forms.
- `js/` — browser scripts, loaded as plain globals in the order listed in `index.html`.
- `css/` — stylesheets. Colours are tokens in `base.css`, with a light and a dark set.
- `data/` — the SQLite database and its backups (override with `AUDIT_DATA_DIR`).

## Pages

| Page | Sub-pages | Use |
| --- | --- | --- |
| Dashboard | Reports | What is waiting on you, scheduled audits, overview figures and charts; filtered reports and exports |
| Inspections | History, Findings | Guided checklist; past inspections, PDFs, and audit closure; findings raised by audits |
| Work Orders | | Corrective actions and manually raised issues |
| Notifications | | In-app notices addressed to you |
| Assets | Fixed Assets, Categories | The asset register with inspection criteria; finding categories |
| Outlets | Zones, Locations | Outlets and the places inside them |
| Users | Departments, Roles | Accounts, departments, and role permissions |
| Settings | | Theme, items per page, and workflow options |
| Account | | Profile, signature, password, sign out |

The Super account also has Super Dashboard and Super Settings (company databases, priority
levels, audit types, scoring, and report branding).

## Importing fixed assets

A fixed asset's `Code` is its unique key, and its QR code is generated from it. To import
XLSX asset listings from a `Fixed_Assets/` folder in the repository, run from the repository root:

```sh
python3 tools/import_fixed_assets.py
```

or from another folder:

```sh
python3 tools/import_fixed_assets.py /path/to/Fixed_Assets
```
