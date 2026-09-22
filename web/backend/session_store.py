"""Durable login sessions; only token hashes are persisted in SQLite."""
import hashlib
from contextlib import nullcontext

from backend.relational_values import ACTIVE_CONNECTION


class SessionStore:
    @staticmethod
    def connection():
        active = ACTIVE_CONNECTION.get()
        if active is not None:
            return nullcontext(active)
        from backend.database import connect
        return connect()

    @staticmethod
    def key(token):
        return hashlib.sha256(token.encode()).hexdigest()

    def get(self, token, default=None):
        if not token:
            return default
        with self.connection() as db:
            row = db.execute("SELECT user_id, expires_at FROM auth_sessions WHERE token_hash = ?", (self.key(token),)).fetchone()
        return dict(row) if row else default

    def __setitem__(self, token, value):
        with self.connection() as db:
            db.execute("INSERT OR REPLACE INTO auth_sessions(token_hash, user_id, expires_at) VALUES (?, ?, ?)",
                       (self.key(token), value["user_id"], value["expires_at"]))

    def pop(self, token, default=None):
        if not token:
            return default
        with self.connection() as db:
            row = db.execute("SELECT user_id, expires_at FROM auth_sessions WHERE token_hash = ?", (self.key(token),)).fetchone()
            db.execute("DELETE FROM auth_sessions WHERE token_hash = ?", (self.key(token),))
        return dict(row) if row else default
