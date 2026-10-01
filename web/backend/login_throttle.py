"""Failed-login limiter held in memory; the application runs as one process."""
import os
import threading
import time
from collections import OrderedDict, deque


class LoginThrottle:
    def __init__(self, limit=5, window=15 * 60, capacity=10000, clock=time.monotonic):
        self.limit = limit
        self.window = window
        self.capacity = capacity
        self.clock = clock
        self.failures = OrderedDict()
        self.lock = threading.Lock()

    def _recent(self, key, now):
        attempts = self.failures.get(key)
        if attempts is None:
            return None
        while attempts and now - attempts[0] >= self.window:
            attempts.popleft()
        if not attempts:
            del self.failures[key]
            return None
        return attempts

    def retry_after(self, key):
        """Seconds until another attempt is accepted; zero when the caller may try now."""
        with self.lock:
            now = self.clock()
            attempts = self._recent(key, now)
            if attempts is None or len(attempts) < self.limit:
                return 0
            return max(1, int(attempts[0] + self.window - now + 0.999))

    def failure(self, key):
        with self.lock:
            now = self.clock()
            attempts = self._recent(key, now)
            if attempts is None:
                attempts = self.failures[key] = deque(maxlen=self.limit)
            attempts.append(now)
            self.failures.move_to_end(key)
            while len(self.failures) > self.capacity:
                self.failures.popitem(last=False)

    def success(self, key):
        with self.lock:
            self.failures.pop(key, None)

    def clear(self):
        with self.lock:
            self.failures.clear()


LOGIN_THROTTLE = LoginThrottle()


def client_address(handler):
    """The caller's address; the reverse proxy's forwarded address only when explicitly trusted."""
    if os.environ.get("AUDIT_TRUST_PROXY") == "1":
        forwarded = [part.strip() for part in handler.headers.get("X-Forwarded-For", "").split(",") if part.strip()]
        if forwarded:
            return forwarded[-1]
    return handler.client_address[0]
