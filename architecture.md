# Architecture

The server is one Rust crate with no dependencies outside the standard library. `src/main.rs` calls
`audit_app::main_with`, and the library in `src/lib.rs` holds every module. The integration tests
can therefore use the same JSON and XLSX code as the server. The static web root is `web/`, and the
database lives in the configured data directory (`web/data/` by default).

The rewrite keeps the Python server's HTTP API, database schema, and behaviour, including its error
messages. The unchanged browser application in `web/` runs against either server. During the
rewrite, every read endpoint was compared response by response against the Python server on a copy
of the same database, and so were scripted runs of the audit workflow.

## Modules

### Foundations

These modules replace what the Python server took from its standard library and from pip.

| Module | Responsibility |
| --- | --- |
| `sqlite` | `extern "C"` bindings to libsqlite3, statements, rows, and parameters (`params!`). Before a data-changing statement it begins a transaction implicitly, as Python's `sqlite3` module does |
| `json` | An ordered JSON value, parser, and serializer, with Python's truthiness, `str()`, `int()`, and equality rules where the API depends on them |
| `http` | HTTP/1.1 with keep-alive, one thread per connection, and at most `AUDIT_WORKERS` requests processed at once. Bodies are read only when the handler asks, with the limit it chooses |
| `crypto` | SHA-256, HMAC, PBKDF2 (600,000 iterations), base64, and random tokens from `/dev/urandom` |
| `flate`, `zip` | DEFLATE compression and decompression, gzip, zlib, CRC-32, Adler-32; ZIP archives for XLSX |
| `clock` | Wall-clock time, and the local time zone read from TZif files and POSIX `TZ` rules |
| `png`, `webp`, `webp_tables` | PNG decoding and encoding. A WebP decoder (VP8 lossy, VP8L lossless, alpha) for images stored by the Python server |
| `pdf/` | PDF documents (`doc`), TrueType parsing and subsetting (`font`), and a flowing layout of paragraphs, tables, and drawings (`layout`) |
| `xlsx`, `xlsx_read` | Writing workbooks with styles, merged cells, pictures, and charts; reading them for the asset import |

### Application

| Module | Responsibility |
| --- | --- |
| `config`, `error` | Paths and defaults read once at startup. `AppError` (a status with a message, a database error, or an internal error) |
| `db`, `migrate` | Connections, transactions (`with_db` commits on success and rolls back on error), the relational value store, schema creation, seeds, and upgrades |
| `api` | The HTTP boundary: authentication, page permissions, GET routing, static files, throttles, and JSON responses |
| `routes/` | The method/path registry for changes, one file per area: accounts, assets, inspections, findings, locations, setup, and work |
| `accounts`, `permissions`, `control` | Account projection and outlet access. Role inheritance and per-user overrides. The Super accounts' separate database |
| `audits`, `inspections`, `work` | Audit references, scoring, completion, and closure. Findings and work-order state transitions |
| `catalog`, `locations` | Asset queries and bulk changes. Location hierarchy and audit-history retention |
| `changes` | Changes held for approval, applied by replaying the original request when approved |
| `notify`, `todo`, `activity` | Addressed notifications and due reminders. Per-user next steps. The activity log and KPIs |
| `reports`, `exports`, `figures` | Dashboard and report data. Gathering data for PDF and Excel exports. Chart data |
| `pdf_report`, `xlsx_report` | The PDF and Excel documents themselves |
| `media` | Image validation and content-addressed storage in SQLite |
| `databases` | Creating, switching, and removing company databases without redirecting other requests |
| `tools` | `import-assets` and `load-test` |

## Conventions

- Route handlers own the transaction: they run inside `with_db` and pass the connection to domain
  functions that must take part in it.
- Expected failures are `AppError::Status` (built with `bad`, `forbidden`, or `status`). The
  boundary turns them into JSON with that status. Anything else is logged and answered with a JSON
  500, or a 503 when the database stays busy.
- Add a new change route to the registry in `api::find_route`, with its handler in `routes/`. Reads are routed in `api.rs`.
- Responses for the dashboard, reports, setup, and assets are cached for five seconds. The cache is
  cleared whenever a transaction commits.

## Storage

Each structured attribute is stored as typed rows in `value_sets` / `value_nodes`, and domain tables
hold integer foreign keys to them. Structured attributes include checklist snapshots, scores,
settings, permissions, signatures, and evidence metadata. Images are BLOBs in `media_images`,
addressed by content hash and served by `/api/media/...`. Cleanup triggers remove a value set after
its last reference disappears. JSON is used only for HTTP messages and exports.

## Images

The browser shrinks photos before uploading them, to roughly 16 KiB, as JPEG (or PNG when the image
has transparency). The server checks the image's structure, size, and pixel count, then stores the
bytes unchanged. PDF and Excel exports embed JPEG as it is. They embed PNG through its own
compressed data, and WebP after decoding it to pixels. The WebP decoder's output was compared with
libwebp's and matches it exactly on the test images in `tests/data/`.

## Generated code

`src/webp_tables.rs` holds the VP8 probability and coefficient tables, extracted from the text of
RFC 6386 by `tools/vp8_tables.rs`. The RFC gives several of these tables twice, and the tool checks
that the copies agree. To regenerate the file:

```sh
curl -o rfc6386.txt https://www.rfc-editor.org/rfc/rfc6386.txt
rustc --edition 2021 -O tools/vp8_tables.rs -o /tmp/vp8_tables
/tmp/vp8_tables rfc6386.txt > src/webp_tables.rs
```

## Frontend

`web/index.html` is a shell. The server expands its `<!-- include: ... -->` comments from
`web/html/`. Scripts in `web/js/` are plain globals loaded in order, and `state.js` holds shared
state and the page list. Colours are tokens in `web/css/base.css`.

## Verification

```sh
cargo build --release
cargo test
cargo clippy --all-targets
node tests/test_frontend.cjs
./target/release/audit-app load-test --users 100
```
