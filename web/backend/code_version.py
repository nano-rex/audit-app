"""Notice when the program files on disk are newer than the ones this server started with.

Python keeps the code it loaded, so after an update the server must be restarted. Until it
is, the pages (read fresh from disk) are newer than the server and changes they make can be
lost; the page says so instead.
"""
import threading
import time

from backend import config

_lock = threading.Lock()
_checked = {"at": 0.0, "stale": False}


def fingerprint():
    files = sorted((config.ROOT / "backend").glob("*.py")) + [config.ROOT / "server.py"]
    return tuple((path.name, path.stat().st_mtime_ns) for path in files if path.exists())


STARTED_WITH = fingerprint()


def outdated():
    """True once the server's program files have changed since it started; checked at most every 30 s."""
    with _lock:
        if not _checked["stale"] and time.monotonic() - _checked["at"] > 30:
            _checked["at"] = time.monotonic()
            _checked["stale"] = fingerprint() != STARTED_WITH
        return _checked["stale"]
