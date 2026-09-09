"""Chroma persistence, an atomic namespace catalog and storage admission."""

import json
import os
from functools import wraps
from pathlib import Path

import numpy as np

DEFAULT_MAX_BYTES = 500_000_000


def normalize(vector, dimensions):
    value = np.asarray(vector, dtype=np.float32)
    if value.shape != (dimensions,) or not np.isfinite(value).all():
        raise ValueError("Embedding has invalid dimensions or nonfinite values")
    norm = np.linalg.norm(value)
    if not np.isfinite(norm) or norm == 0:
        raise ValueError("Embedding must have a finite, nonzero norm")
    return value / norm


def locked(method):
    @wraps(method)
    def call(store, *args, **kwargs):
        with store._lock, store._file_lock:
            if store._closed:
                raise ValueError("Memory store is closed")
            return method(store, *args, **kwargs)
    return call


def disk_bytes(path):
    return sum(p.stat().st_size for p in Path(path).rglob("*") if p.is_file())


def read_catalog(store):
    path = store.path / "aos-catalog.json"
    if not path.exists():
        if (store.path / "chroma.sqlite3").exists():
            raise ValueError("AOS catalog is missing from an existing Chroma directory. Restore the catalog before opening")
        return {"identity": store.identity, "namespaces": {}}
    value = json.loads(path.read_text())
    if value["identity"] != store.identity:
        raise ValueError("Embedding model or schema changed. Use a new database directory")
    return value


def write_catalog(store, catalog):
    target = store.path / "aos-catalog.json"
    temporary = target.with_suffix(".tmp")
    with temporary.open("w") as output:
        json.dump(catalog, output, sort_keys=True)
        output.flush()
        os.fsync(output.fileno())
    os.replace(temporary, target)
    descriptor = os.open(store.path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def active_collection(store, catalog=None):
    catalog = read_catalog(store) if catalog is None else catalog
    entry = catalog["namespaces"].get(store.namespace)
    if entry is None:
        return None
    return store.client.get_collection(entry["collection"], embedding_function=None)


def cleanup(store):
    active = {v["collection"] for v in read_catalog(store)["namespaces"].values()}
    for collection in store.client.list_collections():
        if collection.name.startswith("aos-snapshot-") and collection.name not in active:
            store.client.delete_collection(collection.name)
