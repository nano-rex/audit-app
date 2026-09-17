"""Validated, content-addressed private image storage for audit evidence."""
import base64
import hashlib
from io import BytesIO
import os
from pathlib import Path
import re
import tempfile

from PIL import Image, UnidentifiedImageError


class MediaStore:
    formats = {"PNG": ("png", "image/png"), "JPEG": ("jpg", "image/jpeg"), "WEBP": ("webp", "image/webp")}
    identifier = re.compile(r"^[0-9a-f]{64}\.(?:png|jpg|webp)$")
    max_bytes = 10 * 1024 * 1024

    def __init__(self, directory):
        self.directory = Path(directory)

    def path(self, identifier):
        if not self.identifier.fullmatch(identifier):
            raise ValueError("Invalid image reference")
        return self.directory / identifier

    def store(self, value, name="photo"):
        if not isinstance(value, str) or not value.startswith("data:image/") or ";base64," not in value:
            raise ValueError("Upload a PNG, JPEG, or WebP image")
        encoded = value.split(";base64,", 1)[1]
        if len(encoded) > self.max_bytes * 4 // 3 + 4:
            raise ValueError("Each image must be at most 10 MiB")
        try:
            content = base64.b64decode(encoded, validate=True)
            with Image.open(BytesIO(content)) as picture:
                if picture.format not in self.formats or picture.width * picture.height > 40_000_000:
                    raise ValueError("Use PNG, JPEG, or WebP images under 40 megapixels")
                extension, mime = self.formats[picture.format]
                picture.verify()
        except (UnidentifiedImageError, OSError, Image.DecompressionBombError) as error:
            raise ValueError("The uploaded file is not a valid image") from error
        identifier = hashlib.sha256(content).hexdigest() + "." + extension
        target = self.path(identifier)
        self.directory.mkdir(parents=True, exist_ok=True)
        if not target.exists():
            with tempfile.NamedTemporaryFile(dir=self.directory, delete=False) as temporary:
                temporary.write(content)
                temporary_path = temporary.name
            os.replace(temporary_path, target)
        return {"id": identifier, "url": "/api/media/" + identifier, "name": Path(str(name)).name[:200], "type": mime, "size": len(content)}

    def normalize(self, value):
        if isinstance(value, list):
            return [self.normalize(item) for item in value]
        if not isinstance(value, dict):
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
                if not isinstance(url, str) or not url.startswith("/api/media/") or not self.path(url.removeprefix("/api/media/")).is_file():
                    raise ValueError("Image reference was not found; upload the image again")
        return result

    def image_bytes(self, image, marked=False):
        if not isinstance(image, dict):
            return None
        url = image.get("markedUrl" if marked else "url")
        if url and url.startswith("/api/media/"):
            target = self.path(url.removeprefix("/api/media/"))
            return target.read_bytes() if target.is_file() else None
        raw = image.get("markedDataUrl" if marked else "dataUrl")
        if raw:
            stored = self.store(raw, image.get("name", "photo"))
            return self.path(stored["id"]).read_bytes()
        return None
