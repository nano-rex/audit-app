"""Response cache for the audit application."""
import json
import time
import gzip
from backend import config
from backend.config import EQUIPMENT_CACHE, EQUIPMENT_CACHE_LOCK


class PreparedJson(dict):
    """Immutable-by-convention shared response, encoded once for a burst of readers."""

    def __init__(self, data):
        super().__init__(data)
        self.body = json.dumps(data, separators=(",", ":")).encode("utf-8")
        self.compressed = gzip.compress(self.body, compresslevel=3, mtime=0)


def cached_response(key, loader):
    key = (str(config.DB_PATH), *key)
    with EQUIPMENT_CACHE_LOCK:
        cached = EQUIPMENT_CACHE.get(key)
        if cached and time.monotonic() - cached[0] < 5:
            return cached[1]
        data = PreparedJson(loader())
        EQUIPMENT_CACHE[key] = (time.monotonic(), data)
        EQUIPMENT_CACHE.move_to_end(key)
        while len(EQUIPMENT_CACHE) > 32:
            EQUIPMENT_CACHE.popitem(last=False)
        return data
