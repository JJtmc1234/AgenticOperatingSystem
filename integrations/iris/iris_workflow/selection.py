"""Exact source selection never expands a repository's permitted files."""
import hashlib
import json


def scope_key(request, paths, specific):
    value=json.dumps([request,sorted(set(paths)),specific]) if paths or specific else request
    return hashlib.sha256(value.encode()).hexdigest()


def select_batches(repository, paths):
    if not paths:
        return repository.batches()
    selected=[]
    for path in sorted(set(paths)):
        selected.append(dict(path=path,content=repository.source(path)))
    batches, batch, size=[],[],0
    for source in selected:
        length=len(source['content'].encode())
        if batch and (len(batch)>=8 or size+length>65536):
            batches.append(batch)
            batch,size=[],0
        batch.append(source)
        size+=length
    if batch:
        batches.append(batch)
    return batches
