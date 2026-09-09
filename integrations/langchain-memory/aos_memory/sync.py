"""Refresh one trusted Markdown directory in its dedicated namespace."""

from pathlib import Path

from .database import locked


@locked
def sync_directory(store, root, *, chunk_chars=800, overlap=100, max_source_bytes=8 * 1024 * 1024):
    root = Path(root).resolve(strict=True)
    if not root.is_dir() or not 0 <= overlap < chunk_chars <= 2000:
        raise ValueError("Require a directory and 0 <= overlap < chunk_chars <= 2000")
    texts, metadata, ids = [], [], []
    total = 0
    for path in sorted(root.rglob("*.md")):
        if path.is_symlink() or not path.resolve().is_relative_to(root):
            raise ValueError(f"Memory path escapes its root: {path}")
        with path.open("rb") as source:
            data = source.read(max_source_bytes - total + 1)
        total += len(data)
        if total > max_source_bytes:
            raise ValueError("Source byte budget exceeded")
        text = data.decode("utf-8")
        relative = path.relative_to(root).as_posix()
        for start in range(0, len(text), chunk_chars - overlap):
            chunk = text[start:start + chunk_chars]
            if chunk.strip():
                ids.append(f"{relative}:{start}")
                texts.append(chunk)
                metadata.append({"source": relative, "offset": start})
    return store.replace_texts(texts, metadata, ids=ids)


def recall(store, query, *, max_bytes=4096, k=6, min_score=0.25):
    """Bound the complete context, including citations, by UTF8 bytes."""
    if max_bytes < 0:
        raise ValueError("Context budget cannot be negative")
    parts = []
    remaining = max_bytes
    for doc, _ in store.similarity_search_with_score(query, k, min_score=min_score):
        block = f"[{doc.metadata.get('source', doc.id)}]\n{doc.page_content}\n\n"
        encoded = block.encode()[:remaining]
        part = encoded.decode("utf-8", errors="ignore")
        parts.append(part)
        remaining -= len(part.encode())
        if remaining < 4:
            break
    return "".join(parts)
