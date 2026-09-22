"""Migration preserves typed data and media without JSON columns or file dependencies."""
import base64
from pathlib import Path
import json
import sqlite3
import tempfile
import unittest

from test_server import app
from test_media_reports import photo_data_url
from backend.media_store import MediaStore
from backend.relational_values import FIELDS, hydrate, hydrate_many, load_value, migrate_columns, save_value
from backend.storage_migration import backup_legacy_database
from backend import config


class RelationalStorageTests(unittest.TestCase):
    def setUp(self):
        self.previous = config.DATA_DIR
        self.directory = tempfile.TemporaryDirectory(prefix="audit-storage-")
        app.configure_data_directory(self.directory.name)

    def tearDown(self):
        app.configure_data_directory(self.previous)
        self.directory.cleanup()

    def test_legacy_migration_preserves_every_type_and_embedded_images(self):
        media = MediaStore(config.DB_PATH)
        value = {"items": [{"passed": False, "notApplicable": True, "score": 2.5, "notes": "中文\nRemarks", "count": 0}],
                 "empty": [], "object": {}, "null": None, "image": {"dataUrl": photo_data_url(), "name": "signature.png"}}
        with app.connect() as db:
            db.execute("CREATE TABLE inspection_sessions(id INTEGER PRIMARY KEY, items_json TEXT, signatures_json TEXT)")
            db.execute("INSERT INTO inspection_sessions VALUES (1, ?, '{}')", (json.dumps(value),))
        backup = backup_legacy_database(config.DB_PATH)
        self.assertTrue(backup.is_file())
        with app.connect() as db:
            db.execute("BEGIN IMMEDIATE")
            migrate_columns(db)
            columns = {row[1] for row in db.execute("PRAGMA table_info(inspection_sessions)")}
            self.assertNotIn("items_json", columns)
            record = db.execute("SELECT * FROM inspection_sessions").fetchone()
            saved = load_value(record["items_data_id"])
            self.assertEqual(saved["items"], value["items"])
            for key in ("empty", "object", "null"):
                self.assertEqual(saved[key], value[key])
            self.assertNotIn("dataUrl", saved["image"])
            self.assertEqual(hydrate(record)["signatures_json"], {})
            self.assertEqual(db.execute("SELECT typeof(content) FROM media_images").fetchone()[0], "blob")
            self.assertEqual(db.execute("SELECT COUNT(*) FROM value_nodes WHERE text_value LIKE 'data:image/%'").fetchone()[0], 0)
        self.assertEqual(media.image_bytes(saved["image"]), base64.b64decode(photo_data_url().split(",")[1]))
        # A single SQLite backup is sufficient to restore both records and images.
        restored = Path(self.directory.name) / "restored.db"
        with sqlite3.connect(config.DB_PATH) as source, sqlite3.connect(restored) as target:
            source.backup(target)
        self.assertEqual(MediaStore(restored).image_bytes(saved["image"]), media.image_bytes(saved["image"]))
        self.assertIsNone(backup_legacy_database(config.DB_PATH))
        with sqlite3.connect(backup) as original:
            self.assertEqual(json.loads(original.execute("SELECT items_json FROM inspection_sessions").fetchone()[0]), value)

    def test_file_media_migration_and_database_only_reads(self):
        import hashlib
        content = base64.b64decode(photo_data_url().split(",")[1])
        identifier = hashlib.sha256(content).hexdigest() + ".png"
        folder = Path(self.directory.name) / "media"
        folder.mkdir()
        image = folder / identifier
        image.write_bytes(content)
        media = MediaStore(config.DB_PATH)
        self.assertEqual(media.migrate_directory(folder), 1)
        self.assertEqual(media.migrate_directory(folder), 0)
        image.unlink()
        self.assertEqual(media.read(identifier), (content, "image/png"))
        self.assertIsNone(media.read("a" * 64 + ".png"))

    def test_bulk_hydration_uses_one_database_connection_and_preserves_nulls(self):
        app.init_db()
        with app.connect() as db:
            references = [save_value(db, {"items": [{"index": index}], "image": "/api/media/" + str(index)})
                          for index in range(25)]
        from unittest.mock import patch
        with patch("backend.database.connect", wraps=app.connect) as connect_spy:
            rows = hydrate_many([
                {"id": index, "items_data_id": reference, "signatures_data_id": None}
                for index, reference in enumerate(references)
            ])
        self.assertEqual(connect_spy.call_count, 1)
        self.assertEqual(rows[7]["items_json"], {"items": [{"index": 7}], "image": "/api/media/7"})
        self.assertIsNone(rows[7]["signatures_json"])
        self.assertNotIn("items_data_id", rows[7])

    def test_schema_and_shared_value_cleanup_and_rollback(self):
        app.init_db()
        with app.connect() as db:
            for table, fields in FIELDS.items():
                columns = {row[1]: row[2] for row in db.execute(f'PRAGMA table_info("{table}")')}
                self.assertFalse(set(fields).intersection(columns), table)
                self.assertTrue(all(columns[column] == "INTEGER" for column in fields.values()), table)
            shared = save_value(db, ["shared"])
            for key in ("test.one", "test.two"):
                db.execute("INSERT INTO app_settings VALUES (?, ?)", (key, shared))
            db.execute("DELETE FROM app_settings WHERE key = 'test.one'")
            self.assertEqual(load_value(shared), ["shared"])
            db.execute("DELETE FROM app_settings WHERE key = 'test.two'")
        with app.connect() as db:
            self.assertEqual(db.execute("SELECT count(*) FROM value_nodes WHERE set_id = ?", (shared,)).fetchone()[0], 0)
        with self.assertRaises(RuntimeError):
            with app.connect() as db:
                reference = save_value(db, {"rollback": True})
                db.execute("INSERT INTO app_settings VALUES ('test.rollback', ?)", (reference,))
                raise RuntimeError("rollback")
        with app.connect() as db:
            self.assertIsNone(db.execute("SELECT 1 FROM app_settings WHERE key = 'test.rollback'").fetchone())
