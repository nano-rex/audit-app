# Production readiness

The application is a working prototype that runs as one process over one SQLite database. It has
not been deployed or certified for production. This page records:
- what the server does today;
- how to run it behind a proxy;
- what has been measured;
- what is still open.

## Configuration

```sh
AUDIT_DATA_DIR=/absolute/path/to/audit-data \
AUDIT_WORKERS=8 \
AUDIT_SECURE_COOKIES=1 \
AUDIT_TRUST_PROXY=1 \
PORT=41883 \
./target/release/audit-app
```

The executable needs only the system SQLite library (`libsqlite3.so.0`) at run time. It also needs
the `web/` folder, which it finds next to the executable or in the source tree, or through
`AUDIT_WEB_ROOT`. PDF export uses the Noto Sans fonts in `web/fonts/`.

| Variable | Default | Effect |
| --- | --- | --- |
| `AUDIT_DATA_DIR` | `web/data` | Directory holding the SQLite databases |
| `AUDIT_WORKERS` | `8` | Requests processed at once; further connections wait their turn |
| `AUDIT_SECURE_COOKIES` | off | `1` marks the session cookie HTTPS-only. Use it only behind HTTPS, otherwise browsers will not send the cookie |
| `AUDIT_TRUST_PROXY` | off | `1` takes the client address from the last `X-Forwarded-For` entry. Use it only behind a reverse proxy you control |
| `PORT` | `41883` | Listening port. The server always binds to `127.0.0.1` |
| `AUDIT_CONTROL_DB` | `<data dir>/control/control.db` | The Super accounts' own database; see Super accounts |
| `AUDIT_SESSION_COOKIE` | from `PORT` | Name of the session cookie; see Running several organizations |
| `AUDIT_WEB_ROOT` | found automatically | The static web folder |

## Supply chain

The crate has no dependencies (`Cargo.lock` lists only `audit-app`). JSON, HTTP, cryptography,
DEFLATE, ZIP, PNG, WebP, PDF, and XLSX are implemented in `src/`. SQLite is the system library,
linked through `extern "C"` declarations in `src/sqlite.rs`, so it is updated with the operating
system. Building needs only `rustc`/`cargo` and that library.

## What the server does

- **Static files:** only the HTML entry points and the `js/`, `css/`, and web-font files are
  served. Responses carry:
  - ETags and gzip;
  - `X-Content-Type-Options`;
  - `X-Frame-Options: SAMEORIGIN`;
  - `Referrer-Policy: same-origin`.
- **Passwords and sessions:**
  - Passwords are hashed with PBKDF2-SHA256 at 600,000 iterations.
  - Sessions are stored as token hashes in SQLite, so they survive restarts.
  - Deactivation, password reset, and password change revoke an account's sessions.
  - An account flagged to change its password can only read its own account, change the
    password, and sign out. Every other request is refused.
- **Sign-in limit:**
  - Five failed attempts for one identifier from one client address within 15 minutes return HTTP
    429 with `Retry-After`.
  - Every attempt performs one password hash, so response time does not reveal which accounts
    exist.
  - The limit is held in memory: it resets on restart.
  - Registrations are limited to ten per hour per client address.
- **Requests:** authentication is checked before a request body is read. Bodies are capped at
  20 MiB (64 KiB for sign-in routes) and must be JSON objects.
- **Images:** the browser shrinks photos to about 16 KiB before upload. The server accepts PNG,
  JPEG, and WebP after checking their structure, up to 10 MiB per upload, 2 MiB stored, and 40
  megapixels.
- **Access:** page permissions and outlet limits are enforced by the API, not only by the interface.
- **Errors:** expected failures return JSON with a 4xx status. Unexpected ones are logged and
  return a JSON 500, or a 503 while the database stays busy.
- **SQLite:**
  - WAL mode, a 15-second busy timeout, and foreign keys are enforced.
  - Each request has an explicit commit or rollback.
  - Audit completion and closure run inside a transaction, so concurrent requests cannot create
    duplicates. `tests/api.rs` sends six simultaneous completions to check this.
- **Caching:** dashboard, report, setup, and asset responses are cached for five seconds and cleared
  after a local write.

## Deployment path

1. Run one application process on a Linux host with local SSD storage.
2. Put TLS termination, request buffering, request-size limits, and rate limiting in a reverse
   proxy, and keep the application bound to loopback.
   - Set `AUDIT_SECURE_COOKIES=1` and `AUDIT_TRUST_PROXY=1`.
   - Without `AUDIT_TRUST_PROXY=1`, every client appears to come from the proxy. Five failed
     sign-ins for one username would then lock that username for everyone for 15 minutes.
3. Each starter account must change its published password at first sign-in.
4. Keep SQLite on local storage, not a network filesystem
   ([SQLite WAL](https://www.sqlite.org/wal.html)).

## Super accounts

Super accounts, their sessions, sign-in history, and page order are kept in a control database,
separate from every organization's database. It is `control/control.db` under the data directory,
or the file named by `AUDIT_CONTROL_DB`.
- An organization database holds only its own people.
- The Super role cannot be assigned in an organization database.
- Organization users cannot take a Super account's username or email.
- Back up the control database with the organization databases. Without it, nobody can sign in as
  Super.

## Running several organizations

Each organization can run as its own instance: a separate process with its own data directory and
port.

```sh
AUDIT_DATA_DIR=/srv/audit/company-a PORT=41891 ./audit-app
AUDIT_DATA_DIR=/srv/audit/company-b PORT=41892 ./audit-app
```

- Browsers share cookies between ports of one host, so each instance names its session cookie after
  its port (`ottotree_session_41891`). Set `AUDIT_SESSION_COOKIE` to choose a name.
- Behind a reverse proxy, give each instance its own host name. Serving instances under paths of one
  host name is not supported.

## Backup and rollback

Back up with SQLite's backup API; do not copy only the main `.db` file of a running WAL database. A
complete backup includes all images.

The schema is the one written by audit-app-old's last release, so rolling back should mean running
the old server on the same data directory. This has not been tested; take a backup first.

## Measured

`audit-app load-test --users 100` starts a loopback server on a temporary database with 2,500
assets and 300 draft inspections. It then has 100 already-authenticated clients each request the
page shell, dashboard, and asset list, and save a draft.

Run on 1 November 2026, on a four-CPU x86-64 host with 15 GiB RAM, using the release build:
- All 400 requests succeeded, and all 100 drafts were stored, in 4.97 s.
- 95th-percentile times:

  | Request | 95th percentile |
  | --- | --- |
  | Page shell | 0.64 s |
  | Dashboard | 2.1 s |
  | Asset list (59 KB each) | 3.4 s |
  | Draft save | 2.7 s |

- An earlier run of the Python server's load tool on the same host took 7.53 s for the same
  workload; this build took 5.26 s on that run.

These figures exclude sign-in hashing, TLS, network latency, image uploads, PDF generation,
sustained traffic, and browser rendering. The client also shares the machine with the server. They
are a regression signal, not a capacity guarantee.

## Verified, and not

- `cargo test` passes: 20 unit tests and the 17 HTTP tests in `tests/api.rs`. The 19 frontend tests
  pass too.
- During the rewrite, every read endpoint was compared with the Python server's response on a copy
  of the same database, and they matched. So did scripted runs of the audit workflow, apart from
  timestamps.
- The PDF and Excel exports were checked:
  - PDFs pass `pdfinfo` and `pdffonts`.
  - Workbooks open in openpyxl and render, charts included, in LibreOffice.
  - WebP decoding matches libwebp exactly on the test images.
- Not verified:
  - the browser workflow end to end against this server;
  - the browser-side photo compression on real phone cameras;
  - any particular browser or device;
  - sustained or production-shaped load.

## Open items

- **Thumbnails** are the stored images themselves, no longer server-made 240-pixel versions.
  Photos uploaded through this release are small. Large photos stored by the Python server are sent
  at full size wherever a thumbnail is shown.
- **Lists are paginated in the browser.** The API returns the full asset, finding, work-order, and
  history lists.
- **Draft saves** rewrite the whole checklist on every save.
