"""Validated private image bytes stored as BLOBs in the application SQLite database."""
import base64
import hashlib
from io import BytesIO
from pathlib import Path
import re
import sqlite3
import time
from contextlib import nullcontext
from relational_values import ACTIVE_CONNECTION

from PIL import Image, UnidentifiedImageError


class MediaStore:
    formats = {"PNG": ("png", "image/png"), "JPEG": ("jpg", "image/jpeg"), "WEBP": ("webp", "image/webp")}
    identifier = re.compile(r"^[0-9a-f]{64}\.(?:png|jpg|webp)$")
    max_bytes = 10 * 1024 * 1024

    def __init__(self, database_path):
        self.database_path = Path(database_path)
        self.database_path.parent.mkdir(parents=True, exist_ok=True)
        with self.connect() as db:
            db.execute("""CREATE TABLE IF NOT EXISTS media_images (
                id TEXT PRIMARY KEY, mime_type TEXT NOT NULL, content BLOB NOT NULL,
                byte_size INTEGER NOT NULL, created_at INTEGER NOT NULL,
                CHECK(typeof(content) = 'blob'), CHECK(length(content) = byte_size)
            )""")

    def connect(self):
        active = ACTIVE_CONNECTION.get()
        if active is not None and Path(active.execute("PRAGMA database_list").fetchone()[2]) == self.database_path:
            return nullcontext(active)
        from database import DatabaseConnection
        return sqlite3.connect(self.database_path, timeout=15, factory=DatabaseConnection)

    def validate_identifier(self, identifier):
        if not isinstance(identifier, str) or not self.identifier.fullmatch(identifier):
            raise ValueError("Invalid image reference")
        return identifier

    def read(self, identifier):
        self.validate_identifier(identifier)
        with self.connect() as db:
            row = db.execute("SELECT content, mime_type FROM media_images WHERE id = ?", (identifier,)).fetchone()
        return (bytes(row[0]), row[1]) if row else None

    def exists(self, identifier):
        self.validate_identifier(identifier)
        with self.connect() as db:
            return db.execute("SELECT 1 FROM media_images WHERE id = ?", (identifier,)).fetchone() is not None

    def put(self, content):
        if len(content) > self.max_bytes:
            raise ValueError("Each image must be at most 10 MiB")
        try:
            with Image.open(BytesIO(content)) as picture:
                if picture.format not in self.formats or picture.width * picture.height > 40_000_000:
                    raise ValueError("Use PNG, JPEG, or WebP images under 40 megapixels")
                extension, mime = self.formats[picture.format]
                picture.verify()
        except (UnidentifiedImageError, OSError, Image.DecompressionBombError) as error:
            raise ValueError("The uploaded file is not a valid image") from error
        identifier = hashlib.sha256(content).hexdigest() + "." + extension
        with self.connect() as db:
            db.execute("INSERT OR IGNORE INTO media_images(id, mime_type, content, byte_size, created_at) VALUES (?, ?, ?, ?, ?)",
                       (identifier, mime, sqlite3.Binary(content), len(content), int(time.time() * 1000)))
        return identifier, mime

    def migrate_directory(self, directory):
        """Copy legacy images into SQLite; retain originals for rollback only."""
        directory = Path(directory)
        if not directory.is_dir():
            return 0
        imported = 0
        for source in sorted(directory.iterdir()):
            if not source.is_file() or not self.identifier.fullmatch(source.name) or self.exists(source.name):
                continue
            content = source.read_bytes()
            if hashlib.sha256(content).hexdigest() != source.stem:
                raise ValueError(f"Legacy image integrity check failed: {source.name}")
            identifier, _ = self.put(content)
            if identifier != source.name:
                raise ValueError(f"Legacy image format does not match its filename: {source.name}")
            imported += 1
        return imported

    def store(self, value, name="photo"):
        if not isinstance(value, str) or not value.startswith("data:image/") or ";base64," not in value:
            raise ValueError("Upload a PNG, JPEG, or WebP image")
        encoded = value.split(";base64,", 1)[1]
        if len(encoded) > self.max_bytes * 4 // 3 + 4:
            raise ValueError("Each image must be at most 10 MiB")
        content = base64.b64decode(encoded, validate=True)
        identifier, mime = self.put(content)
        return {"id": identifier, "url": "/api/media/" + identifier, "name": Path(str(name)).name[:200], "type": mime, "size": len(content)}

    def normalize(self, value):
        if isinstance(value, list):
            return [self.normalize(item) for item in value]
        if not isinstance(value, dict):
            if isinstance(value, str) and value.startswith("data:image/"):
                return self.store(value)
            return value
        result = {key: self.normalize(item) for key, item in value.items() if key not in {"dataUrl", "markedDataUrl"}}
        if value.get("dataUrl"):
            result.update(self.store(value["dataUrl"], value.get("name", "photo")))
        if value.get("markedDataUrl"):
            marked = self.store(value["markedDataUrl"], value.get("markedName", "marked-photo.png"))
            result["markedUrl"] = marked["url"]
            result["markedId"] = marked["id"]
            result["markedName"] = marked["name"]
        for key in ("url", "markedUrl"):
            if key in result:
                url = result[key]
                if not isinstance(url, str) or not url.startswith("/api/media/") or not self.exists(url.removeprefix("/api/media/")):
                    raise ValueError("Image reference was not found; upload the image again")
        return result

    def image_bytes(self, image, marked=False):
        if not isinstance(image, dict):
            return None
        url = image.get("markedUrl" if marked else "url")
        if url and url.startswith("/api/media/"):
            stored = self.read(url.removeprefix("/api/media/"))
            return stored[0] if stored else None
        raw = image.get("markedDataUrl" if marked else "dataUrl")
        if raw:
            stored = self.store(raw, image.get("name", "photo"))
            return self.read(stored["id"])[0]
        return None
