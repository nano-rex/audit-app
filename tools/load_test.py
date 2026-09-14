#!/usr/bin/env python3
"""Local synthetic load test; never opens the application's real database."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import http.client
import json
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import types


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--users", type=int, default=100)
    parser.add_argument("--baseline", action="store_true", help="Test server.py from git HEAD")
    args = parser.parse_args()
    if not 1 <= args.users <= 200:
        parser.error("users must be between 1 and 200")
    root = Path(__file__).resolve().parents[1]
    source = subprocess.check_output(["git", "show", "HEAD:web/server.py"], cwd=root, text=True) if args.baseline else (root / "web/server.py").read_text()
    app = types.ModuleType("load_test_server")
    app.__file__ = str(root / "web/server.py")
    exec(compile(source, "<baseline-server>" if args.baseline else app.__file__, "exec"), app.__dict__)

    class QuietHandler(app.Handler):
        def log_message(self, *unused):
            pass

    with tempfile.TemporaryDirectory(prefix="audit-load-") as storage:
        app.DATA_DIR = Path(storage)
        app.DB_PATH = app.DATA_DIR / "load.db"
        app.init_db()
        with app.connect() as db:
            user_id = db.execute("SELECT id FROM users WHERE role = 'Super'").fetchone()[0]
            template = {"asset_id": "", "qr_code": "", "code": "", "name": "", "business_unit": "Ottotree", "outlet": "STP", "zone": "Test", "equipment_type": "AV", "health_status": "Operational", "last_checked": "2026-09-14", "created_at": 0}
            columns = list(template)
            sql = f"INSERT INTO equipment ({','.join(columns)}) VALUES ({','.join('?' for _ in columns)})"
            for number in range(2500):
                record = template | {"asset_id": f"LOAD-{number}", "code": f"LOAD-{number}", "qr_code": f"LOAD-{number}", "name": f"Test asset {number}"}
                db.execute(sql, [record[key] for key in columns])
            evidence = json.dumps([{"item": "Test asset", "passed": True, "images": [{"dataUrl": "data:image/jpeg;base64," + "A" * 16384}]}])
            for number in range(300):
                db.execute("INSERT INTO inspection_sessions(business_unit,outlet,zone,audit_date,auditor,items_json,progress,status,created_at,updated_at) VALUES ('Ottotree','STP','Test','2026-09-14','Load test',?,100,'Draft',?,?)", (evidence, number, number))
        # Older sqlite3 context managers do not close the connection.
        db.close()
        server_class = getattr(app, "AuditHTTPServer", app.ThreadingHTTPServer)
        server = server_class(("127.0.0.1", 0), QuietHandler)
        server_errors = []
        # Summarize failures instead of printing hundreds of interleaved tracebacks.
        server.handle_error = lambda *_: server_errors.append(type(sys.exc_info()[1]).__name__)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        barrier = threading.Barrier(args.users)

        def visit(number):
            token = f"load-{number}"
            app.SESSION_TOKENS[token] = {"user_id": user_id, "expires_at": time.time() + 3600}
            barrier.wait()
            started = time.perf_counter()
            samples = []
            for path in ("/", "/api/dashboard", "/api/equipment", "/api/inspection-sessions"):
                connection = http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=30)
                begin = time.perf_counter()
                try:
                    body = json.dumps({"outlet": "STP", "auditor": f"Load {number}", "items": [{"passed": True}]}) if path == "/api/inspection-sessions" else None
                    connection.request("POST" if body else "GET", path, body, {"Cookie": f"ottotree_session={token}", "Accept-Encoding": "gzip", "Content-Type": "application/json"})
                    response = connection.getresponse()
                    size = len(response.read())
                    samples.append((path, time.perf_counter() - begin, response.status, size))
                except Exception as error:
                    samples.append((path, time.perf_counter() - begin, str(error), 0))
                finally:
                    connection.close()
            return time.perf_counter() - started, samples

        started = time.perf_counter()
        try:
            with ThreadPoolExecutor(max_workers=args.users) as pool:
                results = list(pool.map(visit, range(args.users)))
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
        elapsed = time.perf_counter() - started
        samples = [sample for _, records in results for sample in records]
        def summary(values):
            ordered = sorted(values)
            return {"median_ms": round(statistics.median(ordered) * 1000, 1), "p95_ms": round(ordered[max(0, int(len(ordered) * .95 + .999) - 1)] * 1000, 1)}
        with app.connect() as db:
            saved = db.execute("SELECT COUNT(*) FROM inspection_sessions WHERE auditor != 'Load test' AND auditor LIKE 'Load %'").fetchone()[0]
        db.close()
        output = {"baseline": args.baseline, "users": args.users, "assets_added": 2500, "drafts_seeded": 300, "evidence_bytes_per_draft": 16384, "seconds": round(elapsed, 2), "flow": summary([value for value, _ in results]), "requests": len(samples), "errors": [record for record in samples if record[2] != 200], "drafts_saved": saved, "endpoints": {path: summary([row[1] for row in samples if row[0] == path]) | {"mean_bytes": round(statistics.mean(row[3] for row in samples if row[0] == path))} for path in sorted({row[0] for row in samples})}}
        output["error_count"] = len(output["errors"])
        output["errors"] = output["errors"][:10]
        output["server_errors"] = dict(Counter(server_errors))
        print(json.dumps(output, indent=2))
        return 0 if not output["error_count"] and not server_errors and saved == args.users else 1


if __name__ == "__main__":
    raise SystemExit(main())
