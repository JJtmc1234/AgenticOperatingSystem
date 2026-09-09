"""Publish complete Chroma snapshots so failed refreshes never replace live memory."""

import hashlib
import json
import uuid

from .database import (active_collection, cleanup, disk_bytes, normalize,
                       read_catalog, write_catalog)


def load_rows(store):
    collection = active_collection(store)
    rows = {}
    if collection is not None:
        for offset in range(0, collection.count(), 1000):
            page = collection.get(limit=1000, offset=offset,
                                  include=["documents", "metadatas", "embeddings"])
            for text, metadata, vector in zip(page["documents"], page["metadatas"], page["embeddings"]):
                for doc_id, user_metadata in json.loads(metadata["refs"]).items():
                    rows[doc_id] = (text, user_metadata, vector)
    return rows


def prepare(store, texts, metadatas, ids, replace):
    texts = list(texts)
    if any(not isinstance(t, str) or not t.strip() or len(t.encode()) > 8192 for t in texts):
        raise ValueError("Memory chunks must contain 1 to 8192 UTF8 bytes")
    hashes = [hashlib.sha256(t.encode()).hexdigest() for t in texts]
    ids = hashes if ids is None else list(ids)
    metadatas = [{} for _ in texts] if metadatas is None else list(metadatas)
    if len(ids) != len(texts) or len(metadatas) != len(texts):
        raise ValueError("Texts, IDs and metadata must have equal lengths")
    if any(not isinstance(i, str) or not i or len(i) > 1024 for i in ids):
        raise ValueError("IDs must be nonempty strings of at most 1024 characters")
    for metadata in metadatas:
        if not isinstance(metadata, dict) or len(json.dumps(metadata, allow_nan=False).encode()) > 8192:
            raise ValueError("Metadata must be an object of at most 8192 bytes")
    old = load_rows(store)
    rows = {} if replace else old.copy()
    cache = {text: vector for text, _, vector in old.values()}
    for doc_id, text, metadata in zip(ids, texts, metadatas):
        rows[doc_id] = (text, metadata, cache.get(text))
    if {i: (t, m) for i, (t, m, _) in old.items()} == {i: (t, m) for i, (t, m, _) in rows.items()}:
        return ids, old, old
    admit(store, rows)
    missing = list(dict.fromkeys(text for text, _, vector in rows.values() if vector is None))
    for start in range(0, len(missing), 32):
        batch = missing[start:start + 32]
        vectors = store.embeddings.embed_documents(batch)
        if len(vectors) != len(batch):
            raise ValueError("Embedding provider returned the wrong batch size")
        cache.update((text, normalize(vector, store.dimensions)) for text, vector in zip(batch, vectors))
    rows = {i: (text, metadata, cache[text]) for i, (text, metadata, _) in rows.items()}
    return ids, old, rows


def admit(store, rows):
    catalog = read_catalog(store)
    other_count = sum(v["count"] for n, v in catalog["namespaces"].items() if n != store.namespace)
    if other_count + len(rows) > store.max_documents:
        raise ValueError("Document budget exceeded")
    if not rows:
        return
    unique = {text for text, _, _ in rows.values()}
    # Account for vectors, graph, documents, metadata, journal and index growth.
    estimate = 2_000_000 + sum(4 * len(t.encode()) + 8 * store.dimensions + 4096 for t in unique)
    estimate += sum(4 * len(json.dumps({i: m}).encode()) for i, (_, m, _) in rows.items())
    if disk_bytes(store.path) + estimate > store.max_bytes:
        raise ValueError("Storage budget exceeded. Increase --max-mb or reclaim storage offline")


def publish(store, rows):
    admit(store, rows)
    catalog = read_catalog(store)
    name = "aos-snapshot-" + uuid.uuid4().hex
    grouped = {}
    for doc_id, (text, metadata, vector) in rows.items():
        digest = hashlib.sha256(text.encode()).hexdigest()
        if digest not in grouped:
            grouped[digest] = {"text": text, "vector": normalize(vector, store.dimensions), "refs": {}}
        grouped[digest]["refs"][doc_id] = metadata
    if rows:
        collection = store.client.create_collection(
            name, embedding_function=None,
            configuration={"hnsw": {"space": "cosine", "num_threads": 2,
                                     "batch_size": 32, "sync_threshold": 32}},
        )
        items = list(grouped.items())
        batch_size = min(1000, store.client.get_max_batch_size())
        for start in range(0, len(items), batch_size):
            batch = items[start:start + batch_size]
            collection.add(ids=[h for h, _ in batch],
                           documents=[v["text"] for _, v in batch],
                           embeddings=[v["vector"].tolist() for _, v in batch],
                           metadatas=[{"refs": json.dumps(v["refs"], sort_keys=True)} for _, v in batch])
        if disk_bytes(store.path) > store.max_bytes:
            raise ValueError("Storage budget exceeded while staging. Live memory was not changed")
        catalog["namespaces"][store.namespace] = {"collection": name, "count": len(rows)}
    else:
        catalog["namespaces"].pop(store.namespace, None)
    write_catalog(store, catalog)
    # Cleanup is separate from publication. A crash here leaves only reclaimable old snapshots.


def commit(store, old, rows):
    if {i: (t, m) for i, (t, m, _) in old.items()} == {i: (t, m) for i, (t, m, _) in rows.items()}:
        return
    try:
        publish(store, rows)
    finally:
        # Do not mask an embedding, disk or publication error with a cleanup failure.
        try:
            cleanup(store)
        except Exception:
            pass
