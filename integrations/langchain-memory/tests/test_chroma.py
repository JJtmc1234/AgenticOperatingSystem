import errno
import json
import sqlite3

import numpy as np
import pytest
from chromadb.api.models.Collection import Collection

from aos_memory import ChromaMemory
from aos_memory.database import active_collection, read_catalog
from aos_memory.migrate import migrate_sqlite
from test_memory import memory


def test_chroma_cosine_index_and_indexed_query(memory, monkeypatch):
    store, _ = memory
    store.add_texts([f"cat {i}" for i in range(80)])
    collection = active_collection(store)
    assert collection.configuration["hnsw"]["space"] == "cosine"
    assert collection.count() == 80
    monkeypatch.setattr(Collection, "get", lambda *a, **kw: pytest.fail("Search scanned stored vectors"))
    assert len(store.similarity_search("cat", k=3)) == 3
    assert any(store.path.rglob("*.bin")), "No persisted HNSW index files"


def test_failed_chroma_write_keeps_live_snapshot(memory, monkeypatch):
    store, _ = memory
    store.add_texts(["cat"], ids=["old"])
    catalog = read_catalog(store)
    original = Collection.add

    def fail_after_write(self, *args, **kwargs):
        original(self, *args, **kwargs)
        raise OSError(errno.ENOSPC, "No space left on device")

    monkeypatch.setattr(Collection, "add", fail_after_write)
    with pytest.raises(OSError, match="No space"):
        store.replace_texts(["dog"], ids=["new"])
    assert read_catalog(store) == catalog
    assert store.similarity_search("cat")[0].page_content == "cat"
    assert len(store.client.list_collections()) == 1


def test_failed_publication_keeps_live_snapshot(memory, monkeypatch):
    import aos_memory.snapshot as snapshots
    store, _ = memory
    store.add_texts(["cat"])
    original = read_catalog(store)

    def fail(*args):
        raise OSError("catalog write failed")

    monkeypatch.setattr(snapshots, "write_catalog", fail)
    with pytest.raises(OSError, match="catalog"):
        store.replace_texts(["dog"])
    assert read_catalog(store) == original
    assert store.similarity_search("cat")[0].page_content == "cat"


def test_reopen_observes_refresh_and_noop_at_capacity(memory):
    store, embedding = memory
    store.add_texts(["cat"])
    with ChromaMemory(store.path, embedding, model_id="test-v1", dimensions=3,
                      namespace="shared") as other:
        other.replace_texts(["dog"])
        assert store.similarity_search("cat")[0].page_content == "dog"
    catalog = read_catalog(store)
    store.max_bytes = 1
    store.replace_texts(["dog"])
    assert read_catalog(store) == catalog
    assert embedding.documents == 2
    store.delete([store.similarity_search("dog")[0].id])
    assert store.stats()["documents"] == 0


def test_default_budget_expansion_and_cleanup(memory):
    store, embedding = memory
    assert store.max_bytes == 500_000_000
    store.max_bytes = 1
    with pytest.raises(ValueError, match="Storage budget"):
        store.add_texts(["cat"])
    assert embedding.documents == 0
    store.max_bytes = 1_000_000_000
    store.add_texts(["cat"])
    store.client.create_collection("aos-snapshot-abandoned", embedding_function=None)
    store.compact()
    assert len(store.client.list_collections()) == 1


def test_sqlite_migration_reuses_vectors(memory, tmp_path):
    store, embedding = memory
    path = tmp_path / "old.sqlite3"
    db = sqlite3.connect(path)
    db.executescript("""
        CREATE TABLE config(identity TEXT);
        CREATE TABLE vectors(hash TEXT, text TEXT, vector BLOB);
        CREATE TABLE documents(namespace TEXT, id TEXT, hash TEXT, metadata TEXT);
    """)
    db.execute("INSERT INTO config VALUES ('v1:float16:3:test-v1')")
    db.execute("INSERT INTO vectors VALUES (?, ?, ?)", ("hash", "cat", np.array([2, 1, 1], dtype='<f2').tobytes()))
    db.execute("INSERT INTO documents VALUES ('shared', 'one', 'hash', ?)", (json.dumps({"source": "facts.md"}),))
    db.commit()
    db.close()
    original = path.read_bytes()
    assert migrate_sqlite(store, path)["migrated"] == 1
    assert embedding.documents == 0
    assert store.similarity_search("cat")[0].metadata == {"source": "facts.md"}
    assert path.read_bytes() == original
    with pytest.raises(ValueError, match="empty destination"):
        migrate_sqlite(store, path)


def test_missing_catalog_never_deletes_existing_memory(memory):
    store, embedding = memory
    store.add_texts(["cat"])
    path = store.path / "aos-catalog.json"
    original = path.read_bytes()
    path.unlink()
    with pytest.raises(ValueError, match="catalog is missing"):
        ChromaMemory(store.path, embedding, model_id="test-v1", dimensions=3, namespace="shared")
    assert len(store.client.list_collections()) == 1
    path.write_bytes(original)
    assert store.similarity_search("cat")[0].page_content == "cat"
