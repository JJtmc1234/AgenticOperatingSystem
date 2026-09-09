"""Lazy CPU embeddings. No model download until the first embedding request."""

from langchain_core.embeddings import Embeddings

MODEL_ID = "BAAI/bge-small-en-v1.5"
DIMENSIONS = 384


class LocalEmbeddings(Embeddings):
    def __init__(self, cache_dir=None):
        self.cache_dir = cache_dir
        self._model = None

    def _load(self):
        if self._model is None:
            import onnxruntime
            onnxruntime.disable_telemetry_events()
            from fastembed import TextEmbedding
            self._model = TextEmbedding(model_name=MODEL_ID, threads=2,
                                        cache_dir=self.cache_dir)
        return self._model

    def embed_documents(self, texts):
        return [v.tolist() for v in self._load().passage_embed(texts, batch_size=32)]

    def embed_query(self, text):
        return next(iter(self._load().query_embed(text))).tolist()
