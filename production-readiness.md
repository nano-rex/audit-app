# Audit and performance review — 14 September 2026

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

The backend suite has 11 regression tests. Four frontend tests parse all browser scripts and exercise startup loading, request coalescing, asset pagination, and local dates in a JavaScript VM. These are not full browser interaction or visual tests. The Android APK was not rebuilt or device-tested.

The load tool starts a loopback server and creates its own temporary database. It seeds 2,500 assets and 300 draft inspections, each containing a synthetic 16 KiB evidence string. A barrier starts 100 simulated, already-authenticated clients together. Each requests the HTML shell, dashboard, and full asset list, then saves a draft: 400 requests total. It verifies the number of drafts actually persisted, rather than relying only on HTTP success. `--baseline` uses `web/server.py` from git HEAD with the current static files and the same synthetic workload. The baseline commit for this review was `d4c5012`; after committing these changes, HEAD will no longer refer to that baseline.

Final observed result on this shared ARM64 environment (four reported CPUs, approximately 2.7 GiB RAM):

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
4. Replace process-local session storage before adding replicas or application processes. Sessions currently disappear on restart, and independent processes cannot authenticate one another's cookies. Also add expiry cleanup, login throttling, and session revocation after administrator password resets. Rotate the prototype's built-in credentials before allowing external access.
5. Keep SQLite on local storage while the workload remains modest. WAL allows readers and a writer to overlap, but still permits only one writer at a time and is unsuitable for a database shared over a network filesystem. See [SQLite WAL constraints](https://www.sqlite.org/wal.html). Move to PostgreSQL when measured write contention, durable job workers, or multi-host operation requires it.
6. Move photo bytes out of JSON fields into private object storage, with size limits and thumbnails. Add database-side pagination and filters to assets, findings, work orders, and inspection history as data grows. Current asset pagination limits browser DOM size; the API still returns the full requested asset set.
7. Run a sustained staging test with realistic photos, audit history, login bursts, and exports. Measure errors, p95 latency, memory, disk latency, and SQLite busy errors. Suggested initial acceptance criteria: no lost or duplicate writes, zero unexpected server errors, and p95 under two seconds for routine API operations. These are proposed targets, not a measured production SLA.

Configuration supported now:

```sh
AUDIT_DATA_DIR=/absolute/path/to/audit-data \
AUDIT_WORKERS=8 \
AUDIT_SECURE_COOKIES=1 \
PORT=41883 \
python3 web/server.py
```

Use `AUDIT_SECURE_COOKIES=1` only behind HTTPS; otherwise browsers will not send the session cookie over HTTP. The default database path remains `web/data/ottotree_audit_web.db`. No external dependency was added to the Python server.

## Remaining correctness and release risks

- The Android app is a separate offline prototype, not a client of the shared web API. Its `AuditDatabase.onUpgrade()` currently drops and recreates tables. Do not ship an Android database-version upgrade before replacing that with reviewed, versioned migrations and testing backups/restores on devices. Android migration work was not included in these web changes.
- Web permissions are section-level, not outlet ownership or tenant isolation. Define those access rules before exposing the application to separate organizations.
- Several reports still label summaries as monthly while aggregating all dates. A date-filtered reporting redesign needs a clear reporting-period definition.
- Full endpoint field validation, persistent session management, and every inspection-to-work-order edge case are not covered by this test suite. Existing prototype account credentials and administrative reset behavior still require a release review.

## Rollout and rollback

Back up the database using SQLite's backup API before restarting with the changes. For a running WAL database, do not back up only the main `.db` file using an ordinary file copy. Keep the backup outside any public document root and test restoring it into a separate directory.

Restarting applies WAL and creates indexes idempotently. The existing live database was not migrated during this audit. The cache is in memory and requires no migration. A restart logs out users because sessions are currently process-local.

Retain the previous code release and a verified database backup. After successful logins or password changes, hashes may use the new PBKDF2 format, so rolling back to the old SHA-only verifier would break those logins. Keep the compatible verifier in any rollback release, or restore the pre-rollout backup with explicit acceptance of losing subsequent data. Do not reset user passwords to prototype defaults as a rollback mechanism.
