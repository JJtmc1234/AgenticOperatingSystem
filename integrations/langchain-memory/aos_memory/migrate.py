"""Read the previous SQLite format without modifying it or recomputing vectors."""

import json
import sqlite3
from pathlib import Path

import numpy as np

from .database import locked, normalize
from .snapshot import commit, load_rows


@locked
def migrate_sqlite(store, source):
    path = Path(source).resolve(strict=True)
    expected = store.identity.replace("v2:chroma:cosine:", "v1:float16:", 1)
    db = sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)
    try:
        if db.execute("SELECT identity FROM config").fetchall() != [(expected,)]:
            raise ValueError("Source embedding model or schema does not match")
        old = load_rows(store)
        if old:
            raise ValueError("Migration requires an empty destination namespace")
        rows = {}
        for doc_id, text, metadata, vector in db.execute(
            """SELECT d.id, v.text, d.metadata, v.vector FROM documents d
               JOIN vectors v ON d.hash=v.hash WHERE d.namespace=?""", (store.namespace,)
        ):
            rows[doc_id] = (text, json.loads(metadata), normalize(np.frombuffer(vector, dtype='<f2'), store.dimensions))
        commit(store, old, rows)
        return {"migrated": len(rows), "namespace": store.namespace}
    finally:
        db.close()
