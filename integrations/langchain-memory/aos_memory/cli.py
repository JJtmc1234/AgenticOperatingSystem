"""Operator commands, not a runtime tool or a permission bypass."""

import argparse
import json

from .local import DIMENSIONS, MODEL_ID, LocalEmbeddings
from .migrate import migrate_sqlite
from .store import ChromaMemory
from .sync import recall, sync_directory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", default="run/chroma", help="Chroma persistence directory")
    parser.add_argument("--namespace", required=True)
    parser.add_argument("--max-mb", type=int, default=500, help="Storage admission budget in decimal MB")
    parser.add_argument("--cache-dir", default="run/embedding-cache")
    commands = parser.add_subparsers(dest="command", required=True)
    sync = commands.add_parser("sync")
    sync.add_argument("root")
    search = commands.add_parser("recall")
    search.add_argument("query")
    search.add_argument("--max-bytes", type=int, default=4096)
    migrate = commands.add_parser("migrate-sqlite")
    migrate.add_argument("source")
    commands.add_parser("compact", help="Remove abandoned snapshots. Full vacuum requires offline Chroma maintenance")
    commands.add_parser("stats")
    args = parser.parse_args()
    with ChromaMemory(args.db, LocalEmbeddings(args.cache_dir), model_id=MODEL_ID,
                      dimensions=DIMENSIONS, namespace=args.namespace,
                      max_bytes=args.max_mb * 1_000_000) as store:
        if args.command == "sync":
            print(json.dumps(sync_directory(store, args.root)))
        elif args.command == "recall":
            print(recall(store, args.query, max_bytes=args.max_bytes))
        elif args.command == "migrate-sqlite":
            print(json.dumps(migrate_sqlite(store, args.source)))
        else:
            print(json.dumps(store.compact() if args.command == "compact" else store.stats()))
