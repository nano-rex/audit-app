"""Typed relational storage for ordered lists and nested record attributes.

Each scalar occupies a typed SQL column. Containers contain child rows, never a
serialized JSON document. Domain records hold foreign keys to immutable value sets.
"""
import json
from contextvars import ContextVar

ACTIVE_CONNECTION = ContextVar("audit_active_connection", default=None)

FIELDS = {
    "audits": {"scoring_json": "scoring_data_id"},
    "inspection_sessions": {"items_json": "items_data_id", "signatures_json": "signatures_data_id"},
    "findings": {"images_json": "images_data_id", "completion_photo": "completion_photo_data_id"},
    "work_orders": {"images_json": "images_data_id", "completion_photo": "completion_photo_data_id"},
    "equipment": {"photos": "photos_data_id", "inspection_criteria": "inspection_criteria_data_id"},
    "zones": {"locations_json": "locations_data_id"},
    "roles": {"permissions_json": "permissions_data_id", "inspection_permissions": "inspection_permissions_data_id"},
    "users": {"permission_overrides": "permission_overrides_data_id", "profile_photo": "profile_photo_data_id", "signature_image": "signature_image_data_id"},
    "app_settings": {"value": "value_data_id"},
}


def initialize(db):
    db.execute("CREATE TABLE IF NOT EXISTS value_sets(id INTEGER PRIMARY KEY AUTOINCREMENT)")
    db.execute("""CREATE TABLE IF NOT EXISTS value_nodes(
        set_id INTEGER NOT NULL REFERENCES value_sets(id) ON DELETE CASCADE,
        node_id INTEGER NOT NULL, parent_id INTEGER, position INTEGER NOT NULL,
        field_name TEXT, kind TEXT NOT NULL CHECK(kind IN ('null','object','array','text','integer','real','boolean','image')),
        text_value TEXT, integer_value INTEGER, real_value REAL,
        PRIMARY KEY(set_id, node_id),
        FOREIGN KEY(set_id, parent_id) REFERENCES value_nodes(set_id, node_id)
    )""")


def save_value(db, value):
    set_id = db.execute("INSERT INTO value_sets DEFAULT VALUES").lastrowid
    rows = []

    def visit(item, parent=None, position=0, key=None):
        node = len(rows)
        text = integer = real = None
        if item is None:
            kind = "null"
        elif isinstance(item, bool):
            kind, integer = "boolean", int(item)
        elif isinstance(item, int):
            kind, integer = "integer", item
        elif isinstance(item, float):
            kind, real = "real", item
        elif isinstance(item, str):
            kind, text = "text", item
            if item.startswith("/api/media/"):
                kind, text = "image", item.removeprefix("/api/media/")
        elif isinstance(item, dict):
            kind = "object"
        elif isinstance(item, (list, tuple)):
            kind = "array"
        else:
            raise ValueError("Unsupported record attribute")
        rows.append((set_id, node, parent, position, key, kind, text, integer, real))
        if kind == "object":
            for index, (name, child) in enumerate(item.items()):
                if not isinstance(name, str):
                    raise ValueError("Record attribute names must be text")
                visit(child, node, index, name)
        elif kind == "array":
            for index, child in enumerate(item):
                visit(child, node, index)
    visit(value)
    db.executemany("INSERT INTO value_nodes VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)", rows)
    return set_id


def load_value(reference):
    # JSON is accepted only at compatibility/import boundaries, never saved here.
    if not isinstance(reference, int):
        return json.loads(reference) if isinstance(reference, str) else reference
    db = ACTIVE_CONNECTION.get()
    owned = db is None
    if owned:
        from backend.database import connect
        db = connect()
    try:
        rows = db.execute("SELECT node_id, parent_id, field_name, kind, text_value, integer_value, real_value FROM value_nodes WHERE set_id = ? ORDER BY node_id", (reference,)).fetchall()
        if not rows:
            raise ValueError("Saved record attributes are missing")
        nodes = {}
        for row in rows:
            node, parent, key, kind, text, integer, real = tuple(row)
            value = {"null": None, "object": {}, "array": [], "text": text, "integer": integer,
                     "boolean": bool(integer), "real": real, "image": "/api/media/" + (text or "")}[kind]
            nodes[node] = value
            if parent is not None:
                if isinstance(nodes[parent], dict):
                    nodes[parent][key] = value
                else:
                    nodes[parent].append(value)
        return nodes[0]
    finally:
        if owned:
            db.close()


def data_value(db, value, fallback=None):
    if value is None:
        value = fallback if fallback is not None else []
    elif isinstance(value, str):
        try:
            value = json.loads(value)
        except ValueError:
            value = [value]
    return save_value(db, value)


def hydrate(row):
    """Expose API-compatible values, not database foreign-key identifiers."""
    result = dict(row)
    for mapping in FIELDS.values():
        for old, new in mapping.items():
            if new in result:
                reference = result.pop(new)
                result[old] = load_value(reference) if reference is not None else None
    return result


def migrate_columns(db):
    initialize(db)
    from backend.media_store import MediaStore
    from backend import config
    media = MediaStore(config.DB_PATH)
    for table, fields in FIELDS.items():
        columns = {row[1] for row in db.execute(f'PRAGMA table_info("{table}")')}
        if not columns:
            continue
        for old, new in fields.items():
            if new not in columns:
                db.execute(f'ALTER TABLE "{table}" ADD COLUMN "{new}" INTEGER REFERENCES value_sets(id)')
            if old not in columns:
                continue
            key = "key" if table == "app_settings" else "id"
            for row in db.execute(f'SELECT "{key}", "{old}" FROM "{table}"').fetchall():
                value = row[1]
                if value is not None:
                    try:
                        value = json.loads(value)
                    except (ValueError, TypeError):
                        # Older photo imports sometimes contain plain filenames.
                        value = value
                    if table == "app_settings" and row[0] == "report.logoUrl" and value:
                        value = media.store(value)["url"] if value.startswith("data:image/") else value
                        media.normalize({"url": value})
                    value = media.normalize(value)
                    reference = save_value(db, value)
                    db.execute(f'UPDATE "{table}" SET "{new}" = ? WHERE "{key}" = ?', (reference, row[0]))
            db.execute(f'ALTER TABLE "{table}" DROP COLUMN "{old}"')


def collect_pending_values(db):
    if not db.execute("SELECT 1 FROM pending_value_cleanup LIMIT 1").fetchone():
        return
    references = [(table, new) for table, fields in FIELDS.items() for new in fields.values()]
    unused = " AND ".join(f'NOT EXISTS(SELECT 1 FROM "{table}" WHERE "{column}" = value_sets.id)' for table, column in references)
    db.execute(f"DELETE FROM value_sets WHERE id IN (SELECT value_id FROM pending_value_cleanup) AND {unused}")
    db.execute("DELETE FROM pending_value_cleanup")


def install_reference_cleanup(db):
    """Queue replaced sets; remove unreferenced attributes at transaction commit."""
    references = [(table, new) for table, fields in FIELDS.items() for new in fields.values()]
    db.execute("CREATE TABLE IF NOT EXISTS pending_value_cleanup(value_id INTEGER PRIMARY KEY)")
    db.execute("CREATE TRIGGER IF NOT EXISTS delete_value_nodes AFTER DELETE ON value_sets BEGIN DELETE FROM value_nodes WHERE set_id = OLD.id; END")
    for table, column in references:
        db.execute(f'CREATE INDEX IF NOT EXISTS "idx_{table}_{column}" ON "{table}"("{column}")')
        for action in ("DELETE", f'UPDATE OF "{column}"'):
            name = f'cleanup_{table}_{column}_{action.split()[0].lower()}'
            db.execute(f'DROP TRIGGER IF EXISTS "{name}"')
            db.execute(f'CREATE TRIGGER "{name}" AFTER {action} ON "{table}" WHEN OLD."{column}" IS NOT NULL BEGIN INSERT OR IGNORE INTO pending_value_cleanup(value_id) VALUES (OLD."{column}"); END')
    db.execute("INSERT OR IGNORE INTO pending_value_cleanup SELECT id FROM value_sets")
    collect_pending_values(db)


def hydrate_many(rows):
    from backend.database import connect
    with connect():
        return [hydrate(row) for row in rows]
