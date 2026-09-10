"""Finish saved work, then rotate source batches without trusting stale reviews."""
import hashlib
import json


def batch_key(head, request, scope, paths, specific, batch):
    value=[head,scope if paths or specific else request,batch]
    return hashlib.sha256(json.dumps(value,sort_keys=True).encode()).hexdigest()[:24]


def ordered_batches(batches, ledger, repo, head, request, scope, paths, specific, publish):
    recent={}
    for event in ledger.events:
        if event['kind']=='batch_done' and event.get('repo')==repo and event.get('scope')==scope:
            for path in event.get('files',[]):
                recent[path]=event['seq']

    def priority(batch):
        key=batch_key(head,request,scope,paths,specific,batch)
        if ledger.latest('batch_done',repo=repo,key=key,publish=publish):
            return (0,0)
        if ledger.latest('batch_reviewed',repo=repo,key=key):
            return (1,0)
        # A new revision invalidates reviews, but must not restart the queue at file one.
        return (2,max((recent.get(source['path'],0) for source in batch),default=0))

    return sorted(batches,key=priority)
