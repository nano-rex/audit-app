# Audit App Web

The browser application, served by the Rust server in `src/`. For running, configuration, and the workflow,
see the [repository README](../README.md); for server modules, see
[architecture](../architecture.md).

## Layout

- `index.html` — the page shell. Its `<!-- include: ... -->` comments are expanded by the server.
- `login.html`, `register.html` — sign-in and registration pages.
- `html/tabs/` — one file per page; `html/dialogs/` — dialog forms.
- `js/` — browser scripts, loaded as plain globals in the order listed in `index.html`.
- `css/` — stylesheets. Colours, font, corner radius, and density are tokens in `base.css`;
  palettes are selected by attributes that `js/theme.js` sets from the organization's theme.
  `shell.css` is the app shell (a sidebar listing every page from 1100px wide, a top bar and
  drawer below that) and the shared look of lists, sub-page tabs, and phone filters.
- `fonts/` — Noto Sans SC (Latin range, regular and bold) under the SIL Open Font License; see
  `fonts/OFL-NotoSansSC.txt`.
- `data/` — the SQLite database and its backups (override with `AUDIT_DATA_DIR`).

## Pages

| Page | Sub-pages | Use |
| --- | --- | --- |
| Dashboard | Reports | What is waiting on you, scheduled audits, overview figures and charts; filtered reports and exports |
| Inspections | History, Findings | Guided checklist; past inspections, PDFs, and audit closure; findings raised by audits |
| Work Orders | | Corrective actions and manually raised issues |
| Notifications | | In-app notices addressed to you |
| Assets | Assets & Fixtures, Categories | The register of fixed assets and of fixtures and finishes, with inspection criteria; categories and their responsible departments |
| Outlets | Zones, Locations | Outlets and the places inside them |
| Users | Departments, Roles | Accounts, departments, and role permissions |
| Settings | | Theme, items per page, and workflow options |
| Account | | Profile, signature, password, sign out |

The Super account also has Super Dashboard and Super Settings (company databases, priority
levels, audit types, scoring, and report branding).

## The asset register

The register can hold tens of thousands of items, so the Assets page asks the server for one page
at a time (`/api/equipment?page=…&size=…&kind=…` with the filters as parameters). The server
filters and orders in SQLite and returns the page, the total, and, when asked with `facets=1`, the
choices for the filter lists. Bulk Edit asks for its groups (`groups=1`) and for one group's
items; the location editor asks for a brief list (`view=brief`). Requests without `page` return the
whole register as before, which the guided inspection uses per outlet (`view=inspection`).

## Importing fixed assets

A fixed asset's `Code` is its unique key, and its QR code is generated from it. To import XLSX
asset listings:

```sh
./target/release/audit-app import-assets /path/to/Fixed_Assets
```
