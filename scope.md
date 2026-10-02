# Scope

What the application does today, where it deliberately differs from the original
specification, and what is deferred. The specification was the starting point, not the
design authority: where it conflicted with a shorter or clearer workflow, the workflow won.
For how to use the application, see [USER_GUIDE.txt](USER_GUIDE.txt).

## What the web application does

| Area | Capability |
| --- | --- |
| Accounts | Sign-in by email or username, Remember Me, registration awaiting activation, administrator-reviewed password reset, change password, profile picture and reusable signature |
| Administration | Users, departments, roles with page access and inspection capabilities (auditor, verifier, acknowledger), per-user overrides, login activity |
| Master data | Outlets, locations (floor, area, order, QR code), zones, fixed assets and fixtures & finishes (parts of the building) with their inspection criteria, categories with a responsible department, priority levels with due days, audit types |
| Auditing | A checklist that can be narrowed to one kind of item or one category; scheduled visits, + New Audit with an automatic `AUD-YYYY-NNNN` reference, guided checklist by zone and location, photos with marking tools, draft saving and resuming |
| Evidence | Photo thumbnails throughout; a viewer with zoom, pan, stepping through a set, and marked-versus-original comparison |
| Findings | One finding and one linked work order per failed criterion, with category, priority, department, PIC, cause, recommendation, required action, and evidence |
| Corrective action | Work orders with enforced status steps, completion evidence, verifier identity, comments timeline, due dates, vendor and cost |
| Sign-off | Auditor, verifier, and acknowledger signatures; audit closure once signed and all linked work is closed; closed audits are read-only |
| Reporting | Dashboard and report charts, outlet rankings, filtered CSV, Excel, and JSON exports, per-audit PDF with evidence and signatures |
| Scoring | Automatic score from the checklist, configurable pass mark, rating bands, and category weights; the score snapshot is kept with the audit |
| Notifications | In-app assignment, progress, verification, due-soon, and overdue notices addressed per user; unread count on the navigation bar |
| Per-user view | "Waiting on you" on the dashboard, a reorderable page menu saved to the account, light and dark themes |
| Installation | Company databases that the Super account can create, switch, and remove |

All persistent records and image bytes are stored in SQLite.

## Deliberate differences from the specification

| Specification | What the application does instead | Why |
| --- | --- | --- |
| Pass, fail, and N/A per checklist item | A tick means pass; an unticked item with a remark is a failure. There is no N/A control (the API and reports still accept it) | Removed as unnecessary; one control per criterion is faster in the field |
| Category chosen on the checklist row | Category is chosen in the finding dialog | Only failed criteria need one |
| New Audit, then open it from a list | The checklist opens as soon as the audit is created | One step fewer |
| Verify, then close, as two separate edits | A verifier may close a completed work order directly; the verification is still recorded | One step fewer |
| A fixed list of statuses on every work order | Only the statuses allowed from the current one are offered | Prevents choosing a step the server would refuse |
| Separate Findings, Reports, and Fixed Assets menu entries | Sub-pages of Inspections, Dashboard, and Assets | Fits the navigation bar on a phone |
| A Corrective Actions page beside Work Orders | One Work Orders page; the old page's permission was folded into it on upgrade | Both listed the same records with the same access |
| A signature drawn for each role | One step signs every role you hold with the signature saved on your account; drawing is still available | Most signers hold a saved signature |
| Email reset link for forgotten passwords | A reset request that an administrator reviews | No email integration |
| Photo of every inspected asset | The default, but an administrator can limit it to assets with a failed check | Large outlets have thousands of assets |

## Deferred

- Email, WhatsApp, and mobile push notifications.
- CMMS and preventive-maintenance integration.
- AI photo defect detection, audit summaries, and corrective-action recommendations.
- Synchronisation between the Android app and the web application. The Android app is a
  separate offline application with its own local database; see [android/README.md](android/README.md).
