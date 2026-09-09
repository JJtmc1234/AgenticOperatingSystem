"""LangChain memory backed by persistent Chroma HNSW vector search."""

import json
from functools import lru_cache
from pathlib import Path
from threading import RLock

import chromadb
from chromadb.config import Settings
from filelock import FileLock
from langchain_core.documents import Document
from langchain_core.vectorstores import VectorStore

from .database import (DEFAULT_MAX_BYTES, active_collection, cleanup, disk_bytes,
                       locked, normalize, read_catalog, write_catalog)
from .snapshot import commit, load_rows, prepare


class ChromaMemory(VectorStore):
    def __init__(self, path, embedding, *, model_id, dimensions, namespace,
                 max_bytes=DEFAULT_MAX_BYTES, max_documents=100000):
        if not namespace or not model_id or dimensions < 1 or max_documents < 1 or max_bytes < 1:
            raise ValueError("Require model identity, namespace and positive dimensions and budgets")
        self.path = Path(path).resolve()
        if self.path.is_file():
            raise ValueError("Chroma requires a directory. Use migrate-sqlite for the old database")
        self.path.mkdir(parents=True, exist_ok=True)
        self.identity = f"v2:chroma:cosine:{dimensions}:{model_id}"
        self._embedding, self.dimensions, self.namespace = embedding, dimensions, namespace
        self.max_bytes, self.max_documents = max_bytes, max_documents
        self._lock, self._closed = RLock(), False
        self._file_lock = FileLock(self.path / "aos.lock", timeout=60)
        self._query = lru_cache(maxsize=32)(self._embed_query)
        with self._file_lock:
            catalog = read_catalog(self)
            self.client = chromadb.PersistentClient(str(self.path), settings=Settings(anonymized_telemetry=False))
            write_catalog(self, catalog)
            cleanup(self)

    @property
    def embeddings(self):
        return self._embedding

    def _embed_query(self, text):
        return normalize(self.embeddings.embed_query(text), self.dimensions).tolist()

    @locked
    def add_texts(self, texts, metadatas=None, *, ids=None, **kwargs):
        if kwargs:
            raise ValueError(f"Unsupported write options: {sorted(kwargs)}")
        ids, old, rows = prepare(self, texts, metadatas, ids, replace=False)
        commit(self, old, rows)
        return ids

    @locked
    def replace_texts(self, texts, metadatas=None, *, ids=None):
        ids, old, rows = prepare(self, texts, metadatas, ids, replace=True)
        commit(self, old, rows)
        return {"chunks": len(set(ids)), "removed": len(set(old) - set(ids))}

    @locked
    def similarity_search_with_score(self, query, k=4, *, min_score=-1.0, **kwargs):
        if kwargs:
            raise ValueError(f"Unsupported search options: {sorted(kwargs)}")
        if not isinstance(k, int) or not 0 <= k <= 100 or not -1 <= min_score <= 1:
            raise ValueError("Require k in 0..100 and min_score in -1..1")
        collection = active_collection(self)
        if not k or not query.strip() or collection is None or not collection.count():
            return []
        results = collection.query(query_embeddings=[self._query(query)],
                                   n_results=min(k, collection.count()),
                                   include=["documents", "metadatas", "distances"])
        found = []
        for text, metadata, distance in zip(results["documents"][0], results["metadatas"][0], results["distances"][0]):
            score = max(-1.0, min(1.0, 1.0 - distance))
            if score >= min_score:
                for doc_id, user_metadata in sorted(json.loads(metadata["refs"]).items()):
                    found.append((Document(id=doc_id, page_content=text, metadata=user_metadata), score))
        return found[:k]

    def similarity_search(self, query, k=4, **kwargs):
        return [doc for doc, _ in self.similarity_search_with_score(query, k, **kwargs)]

    def _select_relevance_score_fn(self):
        return lambda score: (score + 1) / 2

    @locked
    def delete(self, ids=None, **kwargs):
        if ids is None or isinstance(ids, str) or kwargs:
            raise ValueError("Deletion requires explicit IDs")
        old = load_rows(self)
        removed = set(ids)
        commit(self, old, {i: row for i, row in old.items() if i not in removed})
        return True

    @classmethod
    def from_texts(cls, texts, embedding, metadatas=None, *, ids=None, **kwargs):
        store = cls(embedding=embedding, **kwargs)
        try:
            store.add_texts(texts, metadatas, ids=ids)
            return store
        except BaseException:
            store.close()
            raise

    @locked
    def stats(self):
        catalog = read_catalog(self)
        collection = active_collection(self, catalog)
        return {"backend": "chroma", "index": "hnsw", "storage_bytes": disk_bytes(self.path),
                "max_bytes": self.max_bytes, "namespace": self.namespace,
                "documents": catalog["namespaces"].get(self.namespace, {}).get("count", 0),
                "vectors": 0 if collection is None else collection.count()}

    @locked
    def compact(self):
        cleanup(self)
        return self.stats()

    def close(self):
        with self._lock:
            self._query.cache_clear()
            self._closed = True

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
