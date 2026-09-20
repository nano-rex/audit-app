# Requirements completion work

Baseline: `req_spec_checklist.md`. A checkbox will be promoted only with implementation and verification evidence.

Work in progress:

1. Preserve finding classification, department, PIC, comments, and its work-order relationship through draft/save/submit.
2. Store original/marked/completion photos and signatures as private image files; render actual images in multi-page PDF reports.
3. Enforce corrective-action transitions, verification, and assigned-user access; make scoring configuration effective.
4. Complete account/session/profile flows, audit metadata, hierarchy, reporting filters and exports, notifications, and mobile access.
5. Verify each core checklist section and record external-service dependencies separately.

Confirmed scope: complete core requirements; section 34 external/future integrations remain separate. Android keeps working offline; synchronization is deferred.

## Implemented and checked in this increment

- Extracted the Python backend into focused modules with an explicit mutation route registry.
- Failed audit criteria produce one linked finding and work order on submission; draft finding
  details retain priority, department, PIC, cause, recommendation, and required action.
- Original and marked photos and signatures can be stored as validated private image files.
  Work-order edits retain evidence and diagnostic fields.
- PDF reports paginate the full checklist and embed actual evidence/signature images.
- Audit submission records a scoring snapshot, excludes N/A criteria, and supports configured
  category weights in the calculation. The category-weight editor remains to be completed.
- Session API saves time, type, and remarks. Completing the corresponding UI remains part of core work.
- Dialog close buttons no longer submit forms.

Evidence: 16 Python tests, 4 frontend tests, and a 100-user/400-request synthetic load run with
zero errors and 100 drafts saved. See `backend-architecture.md` for commands and limitations.

This increment does not certify the full checklist complete. Workflow authorization/transitions,
account flows, hierarchy consistency, remaining report controls/exports, notifications, and
offline Android requirements still need the implementation and verification listed above.

## 2026-09-18 web workflow and navigation increment

- Departments and Roles now live under Users. Edit User opens correctly; its save errors are shown in the dialog.
- User and role editors have a Permissions tab. Users inherit role tab access and Auditor,
  Verifier, and Acknowledger capabilities unless an explicit user override is saved. Restoring
  inheritance applies the current role configuration. Super remains the protected full-access role.
- Account contains profile name/email, profile picture, a reusable signature image, Change Password,
  and Logout. Administrative role/department changes preserve the existing privilege boundary.
- Inspection authors come from the authenticated account. Signature actions enforce their respective
  capabilities and record the authenticated signer and timestamp. Verifiers can sign a completed
  audit without reopening or changing its checklist.
- Progress saves notify active users with effective Verifier or Acknowledger permission. A verifier
  signature notifies the audit creator. New workflow notifications are addressed per user; read/delete
  endpoints reject access to another recipient's notification. Pre-existing broadcast messages remain.
- Guided Inspection initially lists scheduled work. Clicking a schedule atomically creates or resumes
  one linked draft with a real inspection ID. Schedules display `SCH-00001` style references and update
  on completion. Concurrent starts return the same inspection. Completed inspections are retained.
- History & Findings combines inspection history and finding details, with links to an inspection's
  findings. Outlets, zones, locations, categories, findings, inspection history, and scheduled work use
  shared pagination with 10/25/50/100 rows, page jumps, and filter reset behavior.
- The hard-coded performance donut was removed. Distribution is calculated from completed audits,
  using saved rating snapshots where available. Empty results display no fabricated chart. Work-order
  count charts are labeled as counts rather than performance scores.
- Work-order transitions enforce evidence before completion, verifier permission for verification,
  recorded verifier identity, rejection/return to work, and verification before closure. Workflow
  history is retained, and simultaneous closure creates one transition.

Validation: 23 Python HTTP/database/report tests and 8 Node frontend regression tests passed,
along with static Python checks and the synthetic 100-user run documented in `backend-architecture.md`.
Pagination currently limits browser rendering; the APIs still return complete filtered lists.
The remaining broader core checklist and offline Android work are not certified complete by this increment.


## Section 4 — Main Dashboard (2026-09-18)

All fourteen section 4 requirements are implemented on the landing Dashboard (formerly To-do):
eight summary cards and six charts, populated by the existing cached dashboard endpoint.

- Total audits = completed inspection-backed audits + saved drafts + unstarted schedules.
  A schedule with an inspection is counted through that inspection only.
- Overall score and outlet score bars use completed audits only. An empty set displays
  “No completed audits”; an actual score of zero remains zero.
- Priority and non-priority counts use the saved finding classification, falling back to
  configured priority classification for older records. Issue grouping charts count findings.
- Outstanding issues are findings not Completed, Verified, or Closed. Completed corrective
  actions count work orders in those three statuses; linked findings are not counted again.
- All counts use the selected business-unit scope. The dashboard is all-time; monthly trend
  bars show completed audit counts by audit month. Count bars scale relative to the largest
  count, while score bars use a fixed 0–100 scale. Values remain visible as text.
- Latest outlet scores exclude unfinished/unlinked audit rows. The Reports charts also now
  read the monthly `audits` count correctly and use the shared chart renderer.

Validation: 25 Python tests and 9 frontend regression tests, including empty datasets,
custom priorities, cross-unit exclusion, schedule/draft deduplication, chart scaling, and
zero scores. Section 5 remains unchanged after the requested scope correction.


## SQLite-only persistence

Images now live in `media_images.content` BLOBs inside the application database. Authenticated
image endpoints stream those bytes directly; no redirect or external media path is involved.
Former JSON columns are replaced with integer foreign keys to normalized `value_sets` and
`value_nodes`: each key, list position, scalar type, and scalar value is a relational row.
This includes checklist snapshots, scoring, settings, role/user permissions, profile images,
signatures, asset criteria, zone membership, and evidence metadata. API JSON and user-requested
JSON exports remain transport/output formats only.

Startup takes a verified SQLite backup before converting an older database and imports legacy
media files without deleting the originals. Replaced attributes are cleaned up after their last
record reference disappears. A restore test confirms one SQLite backup includes the image bytes.
The working database was also checked through a test-copy migration before conversion.

The 100-user storage run completed 400 requests in 8.29 seconds with no failures and all 100
drafts saved (p95 four-request flow 7.65 seconds). This fixture uses 2,500 assets, 300 drafts,
and valid shared PNG evidence stored as a BLOB. It does not simulate large simultaneous uploads
or PDF generation, and is not a production capacity guarantee.


## Core audit creation, scoring, and account recovery

- Section 5 is complete: + New Audit creates a scheduled draft, reserves a unique
  `AUD-YYYY-NNNN` reference, records the authenticated auditor, and validates outlet,
  date, time, audit type, and remarks. Open the created schedule to enter the guided checklist.
  References survive draft editing and completion; concurrent creates receive distinct references.
- Section 17 is complete: category weights are editable in Settings, thresholds and weights
  are validated, N/A is excluded, and completion retains the exact scoring snapshot. A pass
  mark of zero is handled correctly rather than being replaced with a fallback value.
- Login sessions now persist as hashed tokens and expiry columns in SQLite, so Remember Me
  survives server restarts. Logout revokes the stored session, password changes revoke other
  sessions, and administrator password resets revoke the user's sessions.
- Forgot Password creates a rate-limited request and an addressed administrator notification.
  Administrators see a reset-request indicator in Users; resolving it requires the existing
  password reset action and identity verification outside the app. Email delivery remains
  part of the deferred external integrations.

Validation: 31 Python tests and 9 frontend tests passed. New checks exercise concurrent audit
references, invalid headers, metadata persistence through completion, weighted snapshots,
invalid scoring settings, reset-request deduplication, and durable hashed sessions.


## Reporting and in-app reminders

- Reports now filter by business unit, outlet, and audit date range. CSV, native XLSX,
  and JSON exports use the same filters. Detailed exports exclude other units/outlets;
  spreadsheet cells treat user-entered formulas as text. XLSX contains Summary and
  Findings sheets with audit metadata and corrective-action fields.
- Added previous/current outlet comparisons, monthly audit scores, room score trends,
  and priority trends. Department/location/category performance shows the percentage
  of completed corrective actions, with no fabricated values. Room scores are the
  percentage of applicable checks passed; completed audit scores retain their weighting.
- Assignment and completion notices target assignees, with completion also notifying
  verifiers. Reassignment generates a notice for the new assignee. Due-soon and overdue
  reminders run every minute while the server is running, at most once per recipient,
  work order, due date, reminder kind, and day. Completed orders stop receiving reminders.
- Email, WhatsApp, and mobile push remain explicitly deferred external integrations.

Validation: 33 Python tests and 9 frontend tests passed, including scoped exports,
valid XLSX parsing, spreadsheet formula handling, room trends, date validation, and
addressed/deduplicated reminders. No browser/device acceptance test has been run.


## Final audit closure

History & Findings now offers **Close audit** to verifiers. Closure requires a completed
inspection, all three recorded signatures, and all linked findings/work orders closed.
The database transaction prevents concurrent closure from creating duplicate history.
Closed audits retain their completed scores and reports, display a Closed status, and
reject subsequent checklist/signature changes or deletion. PDF reports record the
closure timestamp and the authenticated verifier's name.

Room-level trend calculations run only for Reports, so the landing Dashboard does not
load all completed checklist snapshots to render its six overview charts.

Validation for audit closure: 34 Python regression tests and 9 frontend checks passed.
The closure test covers capability denial, incomplete audits, missing signatures, open
corrective actions, concurrent closure, one retained history event, and blocked edits.
The filtered ranking regression also checks that an audit outside the selected dates
cannot become the displayed latest audit.

The latest isolated load run used 100 concurrent users, 2,500 assets and 300 seeded drafts:
400 requests, zero failures, and 100 drafts saved in 14.91 seconds. The p95 four-request
flow was 11.43 seconds. This is slower than the earlier 8.29-second run; timings vary
with the shared host and session persistence now also uses SQLite. It is not evidence
that production meets a particular response-time target.


## User administration and location integrity (2026-09-20)

- User edits preserve omitted fields. Removing, demoting, or deactivating the last active
  Super account is rejected inside a serialized transaction. Deactivation revokes stored
  sessions so reactivation does not restore old access. Accounts with login/audit history
  are retained and can be deactivated; unused accounts can be deleted with session and
  reset-request cleanup.
- Users now includes an administrator-only Login activity dialog showing successful login
  timestamps, email, Remember Me, and browser information. The API returns 50 records per
  page and uses a user/time index; it does not download the complete activity table.
- Section 6 is complete for the web application's outlet → floor/area → room hierarchy.
  Floor and area are editable location fields; room order is configurable. Duplicate
  outlet/location/zone creation returns a conflict instead of replacing an existing ID.
- Renaming unused locations updates asset placement and zone membership atomically.
  Renaming unused outlet codes updates child locations, zones, assets, and generated
  room QR codes. Custom QR values are retained.
- Names referenced by schedules or saved audits cannot be renamed or deleted; other
  details remain editable. Remove/relocate dependent assets and child entries before
  deleting an unused master entry. Zone locations and location equipment must belong to
  the selected outlet. Invalid changes roll back, and the UI displays the server reason.

Validation: 36 Python tests passed, plus the subsequently added login-activity test
(37 distinct tests), 9 frontend regression checks, Python static checks, and diff checks.
New coverage includes last-Super protection, session revocation across reactivation,
partial user updates, history retention, duplicate master data, cross-outlet rejection,
rename propagation, rollback, and paginated login activity with authorization checks.
No device/browser acceptance test was performed. Offline Android remains incomplete.


## Backend folder organization

All application Python modules now live in `web/backend/`, imported through the
`backend` package. `web/server.py` remains the entry point. Regression tests stay in
`tests/` and maintenance scripts in `tools/`; their imports were updated too.
The static asset root and existing SQLite database path remain under `web/`.
The reorganization does not migrate, copy, or reset application data.


## Responsive account navigation

- The menu always lists every page permitted for the current account; Departments and
  Roles retain their existing location under Users. Up/Down buttons reorder the pages.
- The navbar shows a prefix of that order based on measured available width and button
  widths. Viewports up to 480px show at most two tabs; larger screens show as many as fit.
  Resize/orientation changes recalculate the visible tabs. Every page remains accessible
  from the menu, including the active page if it is outside the visible prefix.
- Order is saved in relational SQLite `user_navigation` rows, scoped to the authenticated
  user. Login/profile responses restore it across devices. New or newly permitted pages
  are appended, and saved ordering never grants permission to a restricted page.
- Failed saves restore the previous order and show an error. Keyboard focus is retained
  after moving a page, and Escape/outside clicks close the menu.

Validation: 38 Python tests and 11 frontend checks passed, including account isolation,
invalid/duplicate page rejection, permission filtering, width-based tab limits, and
failed-save rollback. No real browser/device acceptance test was available.


## Requirements sections 10–20 — priorities, findings, workflow, and reports (2026-09-20)

- Priority setup controls the displayed priority wording, Priority/Non-Priority classification,
  and due days. Finding and work-order filters use configured priority names, and report
  classification now honors the configured class even when a custom label is `High`.
- Department/PIC users now receive only work orders assigned to their PIC identity or
  department, and findings assigned to their PIC identity or department. Existing update
  authorization remains enforced. A regression test covers both assignment paths and confirms
  records assigned to another PIC stay hidden.
- Scheduled remarks, failed-criterion notes, work-order comments, and separate cause,
  recommendation, and required-action fields persist with findings and corrective actions.
  Completed audits generate linked finding and work-order references, persist original and
  marked evidence, and retain audit date/time, auditor, location, department, PIC, and status.
- Corrective actions require action taken, PIC, completion date, remark, and completion photo.
  Status transitions enforce verification permission, verifier identity and remarks, rejection
  back to work, and closure rules; linked findings stay synchronized.
- PDF reports include branding/logo, audit metadata, score/rating, checklist totals, priority
  and non-priority finding counts, completed/outstanding actions, checklist and finding rows,
  photos, corrective details, and signatures. Configured classifications are reflected in the
  PDF summary. Dashboard and report charts use saved audit/finding/work-order data.

Validation: all 39 Python tests pass, including Department/PIC list scoping, work-order
transitions, finding linkage, scoring, report filtering/exports, and PDF pagination/content;
Python static checks and `git diff --check` pass. Frontend regression checks could not run
because Node.js is not installed in this workspace. Sections 10–20 are checked complete in
`req_spec_checklist.md` for the implemented web scope.


## Remaining core checklist closure (2026-09-20)

- Verified and checked the remaining web workflow items in sections 1–9 and 21–35.
  Completed inspection history is deliberately retained; the history checklist now says
  unfinished drafts can be deleted while completed audit records remain immutable.
- The PDF regression test now verifies finding/checklist content, branding, original and
  marked evidence, completion photos, corrective action details, and summary counts. PDF
  priority counts honor the configured classification.
- Department/PIC work-order and finding list endpoints are scoped to PIC identity or
  department assignment.
- The Android database is version 12. Upgrades now create missing tables without dropping
  local audit data or reseeding sample records. Android remains a separate offline SQLite
  app; synchronization is deferred.
- Section 34 external integrations and AI features remain unchecked and explicitly deferred,
  in line with the confirmed project scope.

Validation: all 39 Python tests pass, Python static checks pass, and `git diff --check` passes.
The Android build could not run in this ARM64 workspace because the configured Android SDK
and JDK binaries are x86-64 executables. No Android device acceptance test was available.
Frontend regression checks also could not run because Node.js is absent. These are environment
verification limits; they are recorded rather than treated as successful tests.
