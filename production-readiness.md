# Audit App performance review — 14 September 2026

This pass implements and tests improvements to the shared web application. It does not certify that every bug has been found, deploy a service, or modify the existing application database. The installed data was inspected read-only: 2,330 assets, one inspection session, no completed audits or work orders, and five users. All write tests used temporary databases.

## Changes implemented

| Problem | Result |
| --- | --- |
| Public static handler could serve database files and Python source | Only the public HTML entry points, JavaScript directory, and CSS directory can be served; resolved paths must remain in their permitted directories |
| Server restart reset built-in account credentials and reactivated accounts | Existing accounts retain their passwords, activation status, roles, and reset flags |
| Fast SHA-256 password hashes | New passwords use PBKDF2-SHA256 with 600,000 iterations; legacy hashes are upgraded after successful login |
| Section permissions were largely enforced only in the UI | API checks section permissions; shared selection lists remain available to permitted workflows; inspectors can create work orders from failed checks |
| SQLite connections survived context-manager exit | Connections explicitly commit or roll back and close; WAL mode, a 15-second busy timeout, and targeted indexes support concurrent access |
| One unrestricted thread per request | Eight active request workers by default, with a 256-connection listen backlog and a 30-second accepted-connection timeout |
| Repeated asset queries and JSON compression | Shared, pre-encoded asset, dashboard, and setup responses; bounded to 32 entries, expiring after five seconds and cleared after local database writes |
| Every screen loaded at startup | Startup loads authentication, branding, setup, and the active tab; other screens load on navigation; duplicate tab requests are combined |
| Thousands of asset rows rendered together | Fixed Assets displays 100 rows per page; filters still search the entire loaded list |
| Static files always downloaded again; CSS discovered through imports | ETags and conditional requests, gzip, cached HTML assembly, and direct stylesheet links; static edits are detected within about one second |
| Dashboard parsed every inspection's saved evidence to count drafts | A SQL count reads no evidence payloads |
| Report and follow-up counts used an eight-row preview | Counts cover all matching open work orders; critical issues include both High and Priority classifications |
| Concurrent completion generated duplicate audits | Completion is serialized in a transaction; completed inspections reject later edits with HTTP 409; the UI prevents duplicate clicks and offers New Inspection |
| Progress could round an unfinished large checklist to 100% | Completion requires every item; incomplete completion requests are rejected consistently |
| UTC dates could show yesterday in Malaysia | Browser date defaults use the device's local calendar day |
| Invalid JSON shapes or unbounded body lengths | Top-level objects and inspection-item arrays are validated; request bodies are capped at 20 MiB |
| Export names could inject response headers | Download filenames are sanitized and quoted |

The five-second response cache is shared only for data that is already shared by these API endpoints. Authentication and permission checks run before cache access. Writes from another process, such as an import tool, become visible after cache expiry. This is not a cross-process cache.

## Verification

From the repository root:

```sh
python3 -m unittest discover -s tests -v
node tests/test_frontend.cjs
python3 tools/load_test.py --users 100
```

The earlier review ran 11 backend regression tests. The current backend suite has 43 tests. Frontend tests parse browser scripts and exercise startup loading, request coalescing, asset pagination, and local dates in a JavaScript VM; they are not full browser or visual tests. Node.js is not installed in the current workspace, so those tests were not rerun in the latest review. The Android APK was not rebuilt or device-tested.

The load tool starts a loopback server and creates its own temporary database. It seeds 2,500 assets and 300 draft inspections, each containing a synthetic 16 KiB evidence string. A barrier starts 100 simulated, already-authenticated clients together. Each requests the HTML shell, dashboard, and full asset list, then saves a draft: 400 requests total. It verifies the number of drafts actually persisted, rather than relying only on HTTP success. `--baseline` uses `web/server.py` from git HEAD with the current static files and the same synthetic workload. The baseline commit for this review was `d4c5012`; after committing these changes, HEAD will no longer refer to that baseline.

Historical result from the original review on this shared ARM64 environment (four reported CPUs, approximately 2.7 GiB RAM):

| Operation | Median | 95th percentile | Mean response bytes |
| --- | ---: | ---: | ---: |
| HTML shell | 1.05 s | 1.56 s | 8,379 |
| Dashboard | 1.03 s | 1.23 s | 1,111 |
| Full asset list | 0.99 s | 1.05 s | 42,654 |
| Save draft | 1.27 s | 1.92 s | 105 |
| Entire four-request client flow | 4.40 s | 5.04 s | — |

All 400 requests succeeded and all 100 drafts persisted. Total harness time was 5.78 seconds. The baseline timed out on dashboard and asset reads, reported SQLite lock failures, and persisted zero test drafts. The baseline result is a failure under this scenario, not a reliable completed-work throughput measurement.

These figures exclude login hashing, TLS, internet latency, actual image uploads, PDF generation, sustained traffic, and browser rendering. The repeated synthetic evidence string is highly compressible. The client and server share one machine. This establishes a reproducible improvement, not a guarantee for 100 real users in every workflow.

## Deployment path

1. Stage one application process on a Linux host with local SSD storage. Four vCPUs and 4–8 GiB RAM are a starting capacity estimate; validate it using production-shaped data. Begin with `AUDIT_WORKERS=8`. Increasing threads without measurements made this workload worse.
2. Put TLS termination, request buffering, request-size limits, and login/registration rate limiting in a reverse proxy. Keep the application bound to loopback. Set `AUDIT_SECURE_COOKIES=1` when the site is served only over HTTPS. Do not expose the repository or database directory through the proxy's static-file root. Nginx supports request-rate limits through its [limit_req module](https://nginx.org/en/docs/http/ngx_http_limit_req_module.html); tune limits for users sharing an outlet's public IP address.
3. Before a public production launch, move HTTP handling into a maintained WSGI/ASGI application and server. This code still uses `http.server`, which Python explicitly [does not recommend for production](https://docs.python.org/3/library/http.server.html). The local changes and load result do not remove that limitation.
4. Sessions are stored in SQLite and survive restarts; processes using the same local database can read them. Expired rows are cleaned during startup and when an expired token is used. Add login throttling before public launch, and keep the database on local storage. Rotate prototype credentials before external access.
5. Keep SQLite on local storage while the workload remains modest. WAL allows readers and a writer to overlap, but still permits only one writer at a time and is unsuitable for a database shared over a network filesystem. See [SQLite WAL constraints](https://www.sqlite.org/wal.html). Move to PostgreSQL when measured write contention, durable job workers, or multi-host operation requires it.
6. Move photo bytes out of JSON fields into private object storage, with size limits and thumbnails. Add database-side pagination and filters to assets, findings, work orders, and inspection history as data grows. Current asset pagination limits browser DOM size; the API still returns the full requested asset set.
7. Run a sustained staging test with realistic photos, audit history, login bursts, and exports. Measure errors, p95 latency, memory, disk latency, and SQLite busy errors. Suggested initial acceptance criteria: no lost or duplicate writes, zero unexpected server errors, and p95 under two seconds for routine API operations. These are proposed targets, not a measured production SLA.

Configuration supported now:

```sh
AUDIT_DATA_DIR=/absolute/path/to/audit-data \
AUDIT_WORKERS=8 \
AUDIT_SECURE_COOKIES=1 \
AUDIT_TRUST_PROXY=1 \
PORT=41883 \
python3 web/server.py
```

Use `AUDIT_SECURE_COOKIES=1` only behind HTTPS; otherwise browsers will not send the session cookie over HTTP. Set `AUDIT_TRUST_PROXY=1` only when a reverse proxy that you control appends `X-Forwarded-For`; the sign-in limiter then counts failures per client address instead of treating every request as coming from the proxy. The default database remains under `web/data/`. No external dependency was added to the Python server.

## Remaining correctness and release risks

- The Android app is separate and offline, with synchronization deferred. Its current schema upgrade preserves existing records and creates missing tables, but still needs an APK build and device upgrade/restore test before release.
- Web permissions are section-level, not outlet ownership or tenant isolation. Define those access rules before exposing the application to separate organizations.
- Full endpoint field validation, login throttling, and every inspection-to-work-order edge case are not covered by this test suite. Existing prototype account credentials and administrative reset behavior still require a release review.

## Rollout and rollback

Back up the database using SQLite's backup API before restarting with the changes. For a running WAL database, do not back up only the main `.db` file using an ordinary file copy. Keep the backup outside any public document root and test restoring it into a separate directory.

Restarting applies WAL and creates indexes idempotently. The existing live database was not migrated during this review. The response cache is in memory and requires no migration; SQLite-backed sessions remain valid across restart.

Retain the previous code release and a verified database backup. After successful logins or password changes, hashes may use the new PBKDF2 format, so rolling back to the old SHA-only verifier would break those logins. Keep the compatible verifier in any rollback release, or restore the pre-rollout backup with explicit acceptance of losing subsequent data. Do not reset user passwords to prototype defaults as a rollback mechanism.

## Follow-up code review — 20 September 2026

- Asset, dashboard, and report value hydration now batches relational value reads instead of opening a connection for each populated row. Media-table setup now runs once per database file per process rather than on every image-bearing request.
- SQLite foreign-key enforcement is enabled on every application connection, and the initialized test database passes `PRAGMA foreign_key_check`.
- Authentication now resolves the stored session, active user, and effective permissions with one SQLite connection per request. Logout also reads and deletes a session in one connection; administrator deactivation and password reset share one session-revocation operation.
- Android Manager/Director role switching now opens a registered dashboard tab, and its audit-area cards switch the active area.
- Verification: 43 backend tests pass; pyflakes and `git diff --check` pass. The 100-user harness completed 400 requests with no HTTP/server errors and persisted all 100 drafts. Latest measured p95s were 1.21 s for the HTML shell, 1.29 s for dashboard, 1.53 s for the full asset list, and 3.03 s for saving a draft; the four-request client flow had a 6.91 s p95. Authentication reuse and batched relational hydration improved the burst flow, while SQLite draft writes still exceed a two-second target. This is a synthetic regression/load signal, not a production guarantee.
- The current workspace lacks Node.js and the Android SDK/JDK executables are x86-64 on ARM64, so frontend and Android build/device checks could not be rerun here.

## Reliability and sign-in review — 2 October 2026

| Problem | Result |
| --- | --- |
| Switching the company database dropped the connection after the switch had already happened | The switch returns normally, signs out sessions stored in the target database, and refuses files that are not initialized audit databases |
| Creating a database pointed every concurrent request at the new, empty database while it was being initialized | Only the creating request sees the new database; other users stay signed in |
| Removing a database left its WAL and shared-memory files behind | The sidecar files are removed with the database |
| Unexpected route faults closed the connection without a response | The fault is logged and the client receives a JSON 500 |
| No limit on failed sign-ins; unknown accounts answered about twenty times faster than real ones | Five failures per client address and identifier within 15 minutes return HTTP 429 with `Retry-After`; every attempt performs one password hash |
| Request bodies up to 20 MiB were read before authentication | Unauthenticated requests are refused first; sign-in bodies are capped at 64 KiB |
| "Reset password" in User Setup reactivated deactivated accounts and did not require a password change | The account keeps its activation state and must change the temporary password |
| The username typed in User Setup was never sent | The form sends it; a blank value keeps the current username |
| Administrator-created accounts accepted any field values; malformed values dropped the connection | Name, email, role, department, username, and password length are validated; duplicates return 409; accounts created with the shared default password must change it |
| Accounts whose address has no dotted domain, including the built-in Super account, could not save their own profile | Only a changed email or username is format-checked |
| Every notification read took the database write lock to look for due reminders | The check reads first and locks only when a reminder is owed |
| Each draft save added another "Audit progress updated" notification per reviewer | An unread notice is refreshed in place; a new one is added after the reviewer has read the previous one |
| HTML and API responses lacked framing and referrer headers | All responses send `X-Content-Type-Options`, `X-Frame-Options: SAMEORIGIN`, and `Referrer-Policy: same-origin` |

The sign-in limiter is held in memory by the single application process: it resets on restart and is not shared between processes. Without `AUDIT_TRUST_PROXY=1` behind a reverse proxy, all clients share the proxy's address, so five failures for one identifier lock that identifier for everyone until the window passes. Keep proxy-level rate limiting for registration and password-reset requests, which are not limited here.

Verification: 55 backend tests pass (12 new, in `tests/test_account_security.py`; 11 of them fail against the previous commit); pyflakes and `git diff --check` pass. The 100-user harness completed 400 requests with no errors and persisted all 100 drafts; its timings were unchanged within run-to-run variation (draft save p95 3.26 s against 3.22 s for the previous commit on the same machine), so the two-second target for draft saves is still not met. Node.js is not installed in this workspace, so the frontend tests were not run; the one frontend change is the added `username` field in the User Setup payload.

Still open:

- A required password change is enforced only by the browser dialog. The API accepts other requests from an account flagged for reset.
- The selected company database is process state. A restart returns to `ottotree_audit_web.db`, and a database created by an older release is not migrated when it is selected.
- The seeded accounts use short published passwords and are not flagged for change. Rotate them before any deployment that is reachable by others.

## Workflow and interface review — 2 October 2026

The web workflow in `USER_GUIDE.txt` was driven end to end in headless Chromium against a temporary database: admin setup (location, fixed asset), New Audit, guided inspection with a photo and one failed criterion, signatures, corrective action by the assigned person, verification, work-order and audit closure, PDF and report exports, and database create/switch as Super. Pages were also checked in dark mode and at phone width.

| Problem | Result |
| --- | --- |
| Saving a finding from a failed criterion threw a script error (it wrote to a category control that no longer exists), so the dialog never closed and findings could not be recorded | The finding saves to the draft, keeps its category, shows a summary under the criterion, and reopens with its saved details; each failed criterion has a Finding details button |
| The Super account's pages resolved to panels that do not exist: Super Dashboard was blank, Super Settings showed the personal settings page, and the bar had no buttons for the other pages | Super has every regular page plus Super Dashboard and Super Settings, matching the list the server validates, so saving the page order works for Super |
| The Super account's role list contained only the Super role | Super lists and manages every role; other accounts still never see the Super role |
| Any save reloaded the app onto the parent page (for example Fixed Assets back to Categories) with the wrong sub-tab highlighted | The active page and sub-page are restored |
| Work-order rule failures (missing completion evidence, verifier permission) produced no message | The reason is shown in the dialog |
| Closed work orders offered Edit and Delete and read "Closed - Awaiting outlet confirmation" | Closed orders show View only and open without a save button |
| A completed inspection still offered a disabled "Complete Inspection" and editable-looking controls | The button reads "Inspection Completed" and the checklist is disabled |
| Locations without fixed assets showed a red 0% | They show "No assets" |
| Finished schedules were mixed in date order with open ones | Open schedules are listed first |
| The PDF repeated each signer line and drew signatures large enough to push one onto a new page | One signer line with the signing date; signatures share a page |
| Secondary text (`.muted`) was painted as a grey bar; labels sat beside their fields outside dialogs; admin list rows, dialog headers, pagers, and settings cards were misaligned or overflowing | Fixed in the stylesheets |
| Dark mode was a list of per-component overrides and missed dialogs, pills, and several panels; sign-in pages ignored the theme | Colours are tokens in `web/css/base.css` with one dark set; all pages follow the saved theme, or the device setting when none is saved |
| Eight of the eleven frontend tests had been failing since the navigation and pagination changes | All pass again, with two added for the fixes above |

Verification (including the redesign below): 59 backend tests and 14 frontend tests pass; pyflakes and `git diff --check` pass. The browser walk is not an automated test in this repository; it was run by hand with Playwright from outside the project. Not covered: the photo marking tool, QR scanning, real phone cameras, the Android app, and browsers other than Chromium.

## Workflow redesign — 2 October 2026

Changes made for speed and clarity rather than to match the specification, each verified in the same browser walk:

| Before | Now |
| --- | --- |
| + New Audit created a scheduled draft that then had to be found in the list and opened | The checklist opens as soon as the audit is created |
| Every criterion of every passing asset was ticked one by one | Pass all ticks an asset's criteria, leaving any with a remark untouched |
| Every asset needed a photo before an inspection could be completed | Unchanged by default. Admins can switch off "Require a photo of every inspected asset" under Settings > Workflow options; assets with a failed check always need one |
| The dashboard showed all-time figures and a static "On-site Flow" card | "Waiting on you" lists the signed-in user's next steps (continue or sign an inspection, complete or verify a corrective action, close an audit) with an Open button, from `GET /api/todo` |
| The work order dialog offered all seven statuses and every section at every stage | Only allowed next statuses are offered; corrective and verification sections appear when the order reaches them; the completion date and PIC are prefilled |
| A verifier had to set Verified, reopen the order, then set Closed | Completed can go straight to Closed; the verifier and date are recorded as before |
| Fixed Assets was a sub-page of Categories | The page is named Assets and opens on Fixed Assets; Categories is its second sub-page |
| Unread notifications were visible only on the Notifications page | The unread count is shown on the bar |

The photo rule is enforced in the browser only, as before; the API accepts a completed inspection without photos.
