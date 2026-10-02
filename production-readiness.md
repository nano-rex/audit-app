# Production readiness

The application is a working prototype that runs as one Python process over one SQLite
database. It has not been deployed or certified for production. This page records what the
server does today, how to run it behind a proxy, what has been measured, and what is still
open. Earlier review notes are in the git history of this file.

## Configuration

```sh
AUDIT_DATA_DIR=/absolute/path/to/audit-data \
AUDIT_WORKERS=8 \
AUDIT_SECURE_COOKIES=1 \
AUDIT_TRUST_PROXY=1 \
PORT=41883 \
python3 web/server.py
```

| Variable | Default | Effect |
| --- | --- | --- |
| `AUDIT_DATA_DIR` | `web/data` | Directory holding the SQLite database and its backups |
| `AUDIT_WORKERS` | `8` | Requests handled at once; more threads made the measured workload slower |
| `AUDIT_SECURE_COOKIES` | off | `1` marks the session cookie HTTPS-only. Use it only behind HTTPS, otherwise browsers will not send the cookie |
| `AUDIT_TRUST_PROXY` | off | `1` takes the client address from the last `X-Forwarded-For` entry. Use it only behind a reverse proxy you control |
| `PORT` | `41883` | Listening port. The server always binds to `127.0.0.1` |

## What the server does

- **Static files:** only the HTML entry points and the `js/` and `css/` directories are served.
  Responses carry ETags, gzip, `X-Content-Type-Options`, `X-Frame-Options: SAMEORIGIN`, and
  `Referrer-Policy: same-origin`.
- **Passwords and sessions:** PBKDF2-SHA256 with 600,000 iterations; older hashes are upgraded
  at sign-in. Sessions are stored as token hashes in SQLite and survive restarts. Deactivation,
  password reset, and password change revoke sessions. An account flagged to change its
  password (created on the default password, or reset by an administrator) is refused every
  request except reading its own account, changing the password, and signing out.
- **Sign-in limit:** five failed attempts for one identifier from one client address within 15
  minutes return HTTP 429 with `Retry-After`. Every attempt performs one password hash, so
  response time does not reveal which accounts exist. The limiter is held in memory: it
  resets on restart and is not shared between processes.
- **Company databases:** the Super account can create, switch, and remove databases. The
  selected one is recorded in the data directory and reopened after a restart; a database is
  brought up to the current schema when it is selected.
- **Requests:** authentication is checked before a request body is read. Bodies are capped at
  20 MiB (64 KiB for sign-in routes) and must be JSON objects.
- **Access:** page permissions are enforced by the API, not only by the interface. Work-order
  steps, verification, signatures, and audit closure check the caller's capabilities.
- **Errors:** expected failures return JSON with a 4xx status; unexpected ones are logged and
  return a JSON 500.
- **SQLite:** WAL mode, a 15-second busy timeout, foreign keys enforced, and explicit
  commit/rollback/close per request. Audit completion and closure are serialised in a
  transaction, so concurrent requests cannot create duplicates.
- **Caching:** dashboard, report, setup, and asset responses are cached in memory for five
  seconds and cleared after a local write. Writes from another process appear after expiry.

## Deployment path

1. Run one application process on a Linux host with local SSD storage. Four vCPUs and 4–8 GiB
   of RAM are a starting estimate; validate it with production-shaped data.
2. Put TLS termination, request buffering, request-size limits, and rate limiting in a reverse
   proxy, and keep the application bound to loopback. Set `AUDIT_SECURE_COOKIES=1` and
   `AUDIT_TRUST_PROXY=1`. Do not expose the repository or data directory through the proxy.
   Without `AUDIT_TRUST_PROXY=1`, every client appears to come from the proxy, so five failed
   sign-ins for one username lock that username for everyone for 15 minutes.
3. The application limits registrations to ten per hour per client address, and password-reset
   requests to one per account per 15 minutes. Add proxy rate limits if you need tighter ones.
4. In a database created by this release, each starter account must change its password at
   first sign-in. A database created earlier keeps whatever passwords its accounts have;
   change any that are still the published ones.
5. Before a public launch, move HTTP handling to a maintained WSGI/ASGI server. The code uses
   `http.server`, which Python [does not recommend for production](https://docs.python.org/3/library/http.server.html).
6. Keep SQLite on local storage while the workload is modest. WAL allows one writer at a time
   and is unsuitable for network filesystems ([SQLite WAL](https://www.sqlite.org/wal.html)).
   Move to PostgreSQL when measured write contention or multi-host operation requires it.

## Backup and rollback

Back up with SQLite's backup API; do not copy only the main `.db` file of a running WAL
database. A complete backup includes all images. Keep backups outside any public document root
and test restoring one into a separate directory.

Starting a new release applies schema changes and indexes idempotently. On the first start
with a database from before the SQLite-only storage change, a verified backup is written to
`backups/` under the data directory before conversion; rolling back past that point means
restoring that backup. After a sign-in or password change, hashes use PBKDF2, so a rollback
release must keep the compatible verifier.

## Measured

The load tool (`tools/load_test.py --users 100`) starts a loopback server on a temporary
database with 2,500 assets and 300 draft inspections, then has 100 already-authenticated
clients each request the page shell, dashboard, and asset list and save a draft.

Latest run on this shared ARM64 host (four CPUs, about 2.7 GiB RAM), on 2 October 2026 with
the current code: all 400 requests succeeded and all 100 drafts were stored. The
95th-percentile times were about 1.6 s for the page shell, 1.3 s for the dashboard, 1.4 s for
the asset list, and 3.6 s for saving a draft; runs on this host vary by several tenths of a
second. Draft saves therefore miss a two-second target in this burst test.

These figures exclude sign-in hashing, TLS, network latency, image uploads, PDF generation,
sustained traffic, and browser rendering, and the client shares the machine with the server.
They are a regression signal, not a capacity guarantee.

## Verified, and not

- 68 backend tests and 16 frontend tests pass; pyflakes and `git diff --check` pass.
- The web workflow in [USER_GUIDE.txt](USER_GUIDE.txt) was walked end to end in headless
  Chromium on 2 October 2026, including dark mode and phone width. That walk was run by hand
  from outside the repository and is not an automated test here.
- Not verified: the photo marking tool, QR scanning, real phone cameras, browsers other than
  Chromium, sustained or production-shaped load, and the Android app (not rebuilt or
  device-tested; its SDK and JDK binaries do not run on this host).

## Open items

- **Committed data:** `web/data/ottotree_audit_web.db` is tracked in git with user rows and
  password hashes. Its accounts were created before starter accounts were required to change
  their passwords, and those starter passwords are published in `web/backend/seed_data.py`.
- **One intermittent test failure** was seen once in about ten full runs of the backend suite
  on 2 October 2026: after the database create/switch test, every later request in that test
  class was answered 401. It did not recur in repeated runs and its cause was not found.
- **Access scope** is by page, not by outlet or tenant. Department/PIC accounts see only their
  own work orders and findings; other roles see every outlet.
- **Lists are paginated in the browser.** The API returns the full asset, finding, work-order,
  and history lists.
- **Draft saves** rewrite the whole checklist on every save. Re-saving a 400-check draft
  takes about 0.3 s here (2.1 s before the `value_nodes` parent index was added) and a
  2,000-check draft about 1.1 s. The load test does not exercise this: its 100 simultaneous
  saves are small new drafts, and their time is commit and file-close cost on this host's
  disk, queued one writer at a time. Keeping an idle connection open and
  `synchronous=NORMAL` were both tried and made no measurable difference here.
