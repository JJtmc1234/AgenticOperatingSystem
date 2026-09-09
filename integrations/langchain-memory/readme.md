# AOS Chroma memory

AOS now uses persistent **Chroma** as its vector database. Chroma performs cosine similarity
search through an HNSW graph index. The package exposes LangChain `VectorStore` and
`as_retriever()` interfaces and an operator CLI. Markdown remains the memory source.
The event log remains the authority for runtime state.

## Run from the repository root

```sh
uv sync --project integrations/langchain-memory --extra local --extra test --python 3.12
uv run --project integrations/langchain-memory aos-memory --namespace shared sync memory-system/shared
uv run --project integrations/langchain-memory aos-memory --namespace shared recall 'How do agents remember lessons?'
uv run --project integrations/langchain-memory aos-memory --namespace shared stats
```

Chroma persists in `run/chroma`. There is no server to start or cloud subscription. The first
embedding request downloads the BGE small English model into `run/embedding-cache`. Later
requests use the local cache. Inference uses two CPU threads and batches of 32. This provider
makes no paid API calls. Chroma anonymized telemetry and ONNX telemetry events are disabled.

## Storage budget

The default is **500 MB**, meaning 500,000,000 bytes for the Chroma persistence directory,
including its database files, vector indexes and the AOS catalog. Expand it for a command with:

```sh
uv run --project integrations/langchain-memory aos-memory --namespace shared --max-mb 1000 sync memory-system/shared
```

The library equivalent is `max_bytes=1_000_000_000`. Supply the desired budget on each invocation.
`stats` reports directory bytes and the configured budget. This is an application admission
budget, not an operating system disk quota. Before embedding or staging a changed snapshot,
AOS checks current disk use plus a conservative allowance for the new vectors, graph, text,
metadata and journals. It measures again before publication and rejects an oversized snapshot.
Chroma background work, temporary files and failed writes can exceed an estimate. A strict
physical ceiling requires a filesystem quota. Model downloads and Python dependencies live
outside this database budget.

| Control | Default |
| --- | --- |
| Chroma storage admission budget | 500 MB |
| Documents across namespaces | 100,000 |
| Source bytes per directory refresh | 8 MiB |
| Chunk size and overlap | 800 and 100 characters |
| Local embedding dimensions | 384 |
| Query embedding cache | 32 vectors in process |
| Recall context including source labels | 4096 UTF8 bytes |

Identical passages share a Chroma document and vector within a namespace, while preserving
all source IDs and metadata. Embeddings are reused for unchanged text. Unchanged sync does
not build a new index or load the embedding model. Chroma owns vector precision and indexing,
replacing the previous custom float16 SQLite storage. Deduplication does not cross namespaces.

## Safe refresh and maintenance

Use one namespace per source directory. Sync replaces that namespace with its current Markdown
chunks, including removing deleted sources. Do not mix manually added documents into a
namespace managed by sync. Refresh after editing memory and before recall when freshness matters.
There is no background watcher.

Changed writes build a complete staging collection with reused vectors, then publish its name
through an atomic catalog replacement. Embedding, staging and admission failures preserve the
previous live snapshot. The wrapper serializes reads and writes across its threads and local
processes using a file lock. All access must use this wrapper, and the Chroma directory must be
reserved for AOS. The catalog must be backed up together with Chroma data. A missing catalog in
an existing database fails closed rather than deleting collections.

This prioritizes recoverable refreshes over write throughput. A changed namespace rebuilds its
HNSW index, even when most embeddings are reused. Batch related edits into one sync. Writes
materialize the namespace in memory and temporarily require both old and new indexes. Queries
use Chroma search and do not fetch every stored vector into Python. HNSW results are approximate.

Old and abandoned staging collections are cleaned up after writes and on open. The `compact`
command retries that cleanup. Chroma's internal SQLite file may retain reusable free pages, so
cleanup does not guarantee that filesystem bytes shrink. For full compaction, stop all users
of this Chroma directory and run Chroma's supported offline maintenance command:

```sh
uv run --project integrations/langchain-memory chroma vacuum --path run/chroma
```

## Migrate the previous database

```sh
uv run --project integrations/langchain-memory aos-memory --namespace shared migrate-sqlite run/memory.sqlite3
```

Migration reads the previous SQLite file in read only mode and imports its stored embeddings,
IDs, text and metadata into Chroma. It makes no embedding requests and leaves the old file
intact. The destination namespace must be empty and the model identity must match. Repeat
with each namespace you want to import. The old file can be kept as a backup.

## LangChain use

```python
from aos_memory import ChromaMemory, sync_directory
from aos_memory.local import LocalEmbeddings, MODEL_ID, DIMENSIONS
from aos_memory.sync import recall

with ChromaMemory(
    "run/chroma", LocalEmbeddings("run/embedding-cache"),
    model_id=MODEL_ID, dimensions=DIMENSIONS, namespace="shared",
    max_bytes=500_000_000,
) as memory:
    sync_directory(memory, "memory-system/shared")
    documents = memory.as_retriever(search_kwargs={"k": 4}).invoke("agent learning")
    context = recall(memory, "agent learning", max_bytes=4096)
```

Async retrieval uses LangChain's executor interface. Close the wrapper after outstanding
requests complete. Chroma's embedded client resources are managed for the process lifetime.
Custom providers implement LangChain `Embeddings`, with an explicit model identity and
dimensions. Include provider revision and preprocessing in that identity. Model changes
require a new database directory. The old `SQLiteMemory` class has been replaced by `ChromaMemory`.

## Integration boundary

These are operator commands and a library for a trusted LangChain host. The Rust daemon does
not automatically inject results into prompts. A host must select namespaces from trusted
agent identity and route operations through its existing policy gate. Never add Python to the
AOS program allowlist to expose this package. Namespaces are not authentication. Mutually
untrusted agents require separate directories and filesystem permissions.

Retrieval is reference material, not instructions or permission grants. Mandatory identity and
safety rules still load through their existing path. Only index approved directories. Sync
path checks protect against accidental escapes in trusted local sources, not hostile concurrent
filesystem changes. Generated databases and model caches belong outside version control.

## Verification

```sh
cd integrations/langchain-memory
uv run --extra test pytest
```

Tests exercise real persistent Chroma with deterministic embeddings. They cover HNSW index
files, indexed queries, persistence, deduplication, isolation, atomic refresh, failed staging,
failed catalog publication, budget admission, async retrieval and SQLite migration.
A separate smoke check migrated the 64 existing shared chunks and retrieved the verification
lesson using the real local model. An unchanged sync loaded no model.

See [Chroma index configuration](https://docs.trychroma.com/docs/collections/configure) and
[LangChain vector stores](https://docs.langchain.com/oss/python/integrations/vectorstores/).
