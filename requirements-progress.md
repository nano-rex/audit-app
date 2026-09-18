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
