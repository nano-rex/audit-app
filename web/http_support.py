"""Http support for the audit application."""
import sqlite3
import hashlib
import os
import gzip
import threading
from functools import lru_cache, wraps
from http.server import ThreadingHTTPServer
from config import INCLUDE_PATTERN, ROOT


@lru_cache(maxsize=128)
def static_fingerprint(target, second):
    dependencies = [target]
    if target.name == "index.html":
        dependencies.extend(sorted((ROOT / "html").rglob("*.html")))
    return tuple((str(file), file.stat().st_mtime_ns, file.stat().st_size) for file in dependencies)


@lru_cache(maxsize=128)
def static_content(target, fingerprint):
    # The fingerprint includes partial mtimes, so edits invalidate the rendered shell.
    if target.name == "index.html":
        def include(match, parents=()):
            partial = (ROOT / match.group(1)).resolve()
            if not partial.is_relative_to(ROOT / "html") or not partial.is_file():
                return ""
            if partial in parents or len(parents) >= 8:
                raise ValueError("Circular or excessively nested HTML include")
            return INCLUDE_PATTERN.sub(lambda child: include(child, (*parents, partial)), partial.read_text(encoding="utf-8"))
        body = INCLUDE_PATTERN.sub(include, target.read_text(encoding="utf-8")).encode("utf-8")
    else:
        body = target.read_bytes()
    return body, gzip.compress(body, compresslevel=5, mtime=0), '"' + hashlib.sha256(body).hexdigest() + '"'


class AuditHTTPServer(ThreadingHTTPServer):
    request_queue_size = 256
    daemon_threads = True

    def __init__(self, *args, **kwargs):
        self.slots = threading.BoundedSemaphore(max(1, int(os.environ.get("AUDIT_WORKERS", "8"))))
        super().__init__(*args, **kwargs)

    def process_request(self, request, client_address):
        self.slots.acquire()
        try:
            super().process_request(request, client_address)
        except BaseException:
            self.slots.release()
            raise

    def process_request_thread(self, request, client_address):
        try:
            super().process_request_thread(request, client_address)
        finally:
            self.slots.release()


def api_errors(method):
    @wraps(method)
    def guarded(self):
        try:
            return method(self)
        except PermissionError as error:
            if not getattr(self, "response_started", False):
                self.json({"error": str(error)}, 403)
        except (ValueError, TypeError) as error:
            if not getattr(self, "response_started", False):
                self.json({"error": str(error) or "Invalid request fields"}, getattr(error, "status", 400))
        except sqlite3.IntegrityError:
            if not getattr(self, "response_started", False):
                self.json({"error": "This record conflicts with an existing record"}, 409)
        except sqlite3.OperationalError:
            if not getattr(self, "response_started", False):
                self.json({"error": "Database is busy; retry the request"}, 503)
    return guarded
