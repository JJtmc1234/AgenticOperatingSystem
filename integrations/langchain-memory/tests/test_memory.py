import asyncio

import pytest
from langchain_core.embeddings import Embeddings

from aos_memory import ChromaMemory, sync_directory
from aos_memory.sync import recall


class CountingEmbeddings(Embeddings):
    def __init__(self):
        self.documents = 0
        self.queries = 0
        self.fail = False

    def embed_documents(self, texts):
        self.documents += len(texts)
        if self.fail:
            raise RuntimeError("provider failed")
        return [self.vector(t) for t in texts]

    def embed_query(self, text):
        self.queries += 1
        return self.vector(text)

    def vector(self, text):
        return [1 + text.count("cat"), 1 + text.count("dog"), 1]


@pytest.fixture
def memory(tmp_path):
    embedding = CountingEmbeddings()
    with ChromaMemory(tmp_path / "memory.db", embedding, model_id="test-v1",
                      dimensions=3, namespace="shared") as store:
        yield store, embedding


def test_persistence_deduplication_and_query_cache(memory):
    store, embedding = memory
    store.add_texts(["cat", "cat"], ids=["a", "b"])
    store.add_texts(["cat"], ids=["a"])
    assert embedding.documents == 1
    assert store.stats()["vectors"] == 1
    assert store.as_retriever().invoke("cat")[0].page_content == "cat"
    store.similarity_search("cat")
    assert embedding.queries == 1
    path = store.path
    with ChromaMemory(path, embedding, model_id="test-v1", dimensions=3,
                      namespace="shared") as reopened:
        assert len(reopened.similarity_search("cat")) == 2


def test_namespace_isolation_and_gc(memory):
    store, embedding = memory
    store.add_texts(["cat"], ids=["same"])
    path = store.path
    with ChromaMemory(path, embedding, model_id="test-v1", dimensions=3,
                      namespace="private") as private:
        assert private.similarity_search("cat") == []
        private.add_texts(["cat"], ids=["same"])
        private.delete(["same"])
        assert len(store.similarity_search("cat")) == 1
    store.delete(["same"])
    assert store.stats()["vectors"] == 0


def test_atomic_sync_edits_deletions_and_failure(memory, tmp_path):
    store, embedding = memory
    root = tmp_path / "sources"
    root.mkdir()
    source = root / "facts.md"
    source.write_text("cat")
    sync_directory(store, root)
    sync_directory(store, root)
    assert embedding.documents == 1
    source.unlink()
    replacement = root / "new.md"
    replacement.write_text("dog")
    embedding.fail = True
    with pytest.raises(RuntimeError):
        sync_directory(store, root)
    assert store.similarity_search("cat")[0].page_content == "cat"
    embedding.fail = False
    store.max_documents = 1
    sync_directory(store, root)
    assert store.similarity_search("dog")[0].page_content == "dog"
    replacement.unlink()
    sync_directory(store, root)
    assert store.similarity_search("dog") == []


def test_budget_and_validation(memory):
    store, embedding = memory
    store.max_documents = 1
    with pytest.raises(ValueError):
        store.add_texts(["cat", "dog"])
    assert embedding.documents == 0
    with pytest.raises(ValueError):
        store.add_texts(["cat"], metadatas=[])
    store.add_texts(["cat 🐈"], ids=["one"])
    assert len(recall(store, "cat", max_bytes=17).encode()) <= 17
    with pytest.raises(ValueError):
        store.similarity_search("cat", filter={"namespace": "private"})
    with pytest.raises(ValueError):
        store.delete()


def test_model_mismatch(memory):
    store, embedding = memory
    path = store.path
    with pytest.raises(ValueError, match="model or schema"):
        ChromaMemory(path, embedding, model_id="other", dimensions=3, namespace="shared")


def test_symlink_and_source_limit(memory, tmp_path):
    store, _ = memory
    root = tmp_path / "root"
    root.mkdir()
    outside = tmp_path / "secret.md"
    outside.write_text("secret")
    link = root / "link.md"
    link.symlink_to(outside)
    with pytest.raises(ValueError, match="escapes"):
        sync_directory(store, root)
    link.unlink()
    (root / "big.md").write_text("cat" * 100)
    with pytest.raises(ValueError, match="byte budget"):
        sync_directory(store, root, max_source_bytes=10)


def test_disk_full_is_atomic(tmp_path):
    with ChromaMemory(tmp_path / "small.db", CountingEmbeddings(), model_id="test",
                      dimensions=3, namespace="shared", max_bytes=65536) as store:
        with pytest.raises(ValueError, match="Storage budget"):
            store.add_texts([str(i) + "x" * 8000 for i in range(20)])
        assert store.stats()["documents"] == 0


def test_bad_vectors_roll_back(memory):
    store, embedding = memory
    embedding.vector = lambda _: [float("nan"), 0, 0]
    with pytest.raises(ValueError, match="nonfinite"):
        store.add_texts(["cat"])
    assert store.stats()["vectors"] == 0


def test_async_retriever(memory):
    store, _ = memory
    store.add_texts(["cat"])
    assert asyncio.run(store.as_retriever().ainvoke("cat"))[0].page_content == "cat"


def test_sync_disk_full_preserves_previous_snapshot(tmp_path):
    root = tmp_path / "source"
    root.mkdir()
    source = root / "facts.md"
    source.write_text("cat")
    with ChromaMemory(tmp_path / "tiny.db", CountingEmbeddings(), model_id="test",
                      dimensions=3, namespace="shared") as store:
        sync_directory(store, root)
        source.write_text("".join(f"{i:08d}" for i in range(20000)))
        store.max_bytes = store.stats()["storage_bytes"] + 1000
        with pytest.raises(ValueError, match="Storage budget"):
            sync_directory(store, root)
        assert store.similarity_search("cat")[0].page_content == "cat"
