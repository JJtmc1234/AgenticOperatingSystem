"""Serialize runs, resume bounded scans, and publish only reviewed issue plans."""
import datetime
import hashlib
import json
from pathlib import Path
from . import cache, report, notifications
from .github import GitHub
from .ledger import Ledger
from .model import Model
from .repository import Repository
from .investigate import investigate


def run(home, config, selected=None, request='', trigger='manual', force=False,
        github=None, snapshot=cache.snapshot, model_factory=Model):
    home=Path(home)
    github=github or GitHub()
    with Ledger(home) as ledger:
        ledger.append('run_started',trigger=trigger,request_hash=hashlib.sha256(request.encode()).hexdigest(),publish=config['publish'])
        model=model_factory(config,ledger)
        repos=github.list_repositories(config['owner'])
        allowed=config['repositories']
        if selected and (not selected.startswith(config['owner']+'/') or (allowed and selected not in allowed)):
            raise ValueError('Requested repository is outside configured Iris scope')
        repos=[r for r in repos if (not selected or r['nameWithOwner']==selected) and
               (not allowed or r['nameWithOwner'] in allowed)]
        if selected and not repos:
            raise ValueError('Requested repository was not found in GitHub discovery')
        # Rotate deferred repositories so a large first repository cannot starve the rest.
        repos.sort(key=lambda r:(ledger.latest('batch_started',repo=r['nameWithOwner']) or {}).get('seq',0))
        result=dict(trigger=trigger,publish=config['publish'],repositories=[])
        remaining=[config['max_batches'],config['max_issues']]
        for entry in repos:
            repo=entry['nameWithOwner']
            try:
                row=process(home,config,entry,request,trigger,force,ledger,model,github,snapshot,remaining)
            except Exception as error:
                row=dict(repo=repo,status='Failed: '+str(error)[:600])
                ledger.append('repository_failed',repo=repo,error=str(error)[:600])
            result['repositories'].append(row)
        ledger.append('run_finished',report=result)
        report.write(home,result)
        notifications.completed(ledger,result)
        return result


def process(home,config,entry,request,trigger,force,ledger,model,github,snapshot,remaining):
    repo=entry['nameWithOwner']
    row=dict(repo=repo,issues=[])
    branch=(entry.get('defaultBranchRef') or {}).get('name')
    if entry.get('isArchived') or not branch:
        return row | dict(status='Skipped: archived or empty repository')
    repository=Repository(snapshot(home,repo,branch))
    info=repository.inspect()
    head=info['head']
    scope=hashlib.sha256(request.encode()).hexdigest()
    complete=ledger.latest('repo_complete',repo=repo,head=head,publish=config['publish'],scope=scope)
    if complete and not force and not request:
        return row | dict(status='Unchanged committed revision, already inspected')
    if trigger=='poll' and not force:
        seen=ledger.latest('repo_checked',repo=repo)
        if seen:
            elapsed=(datetime.datetime.now(datetime.timezone.utc)-datetime.datetime.fromisoformat(seen['at'])).total_seconds()
            changed=head!=seen['head']
            if elapsed<config['scan_interval'] and not changed:
                return row | dict(status='Waiting for hourly check or feature commit')
    ledger.append('repo_checked',repo=repo,head=head,trigger=trigger)
    batches=repository.batches()
    eligible=sum(len(batch) for batch in batches)
    row['excluded_files']=info['tracked_count']-eligible
    issues=github.issues(repo)
    done=0
    for batch in batches:
        key=hashlib.sha256(json.dumps([head,request,batch],sort_keys=True).encode()).hexdigest()[:24]
        if ledger.latest('batch_done',repo=repo,key=key,publish=config['publish']) and not force:
            done+=1
            continue
        if remaining[0]<=0:
            break
        plan=ledger.latest('batch_reviewed',repo=repo,key=key)
        if plan is None or force:
            if not model.can_investigate():
                break
            ledger.append('batch_started',repo=repo,key=key,head=head)
            plans=investigate(repo,repository,batch,issues,request,model,github,'iris/'+repo+'/'+key)
            plan=ledger.append('batch_reviewed',repo=repo,key=key,head=head,plans=plans)
        if not publish(home,config,repo,plan['plans'],ledger,github,remaining,row):
            break
        ledger.append('batch_done',repo=repo,key=key,head=head,publish=config['publish'],files=[f['path'] for f in batch])
        remaining[0]-=1
        done+=1
    if done==len(batches):
        ledger.append('repo_complete',repo=repo,head=head,publish=config['publish'],scope=scope)
    row['status']=f'Inspected {done}/{len(batches)} source batches at {head[:12]}. '+('Complete.' if done==len(batches) else 'Remaining work queued.')
    return row


def publish(home,config,repo,plans,ledger,github,remaining,row):
    for plan in plans:
        marker='<!-- aos-iris:'+plan['marker']+' -->'
        previous=ledger.latest('published',repo=repo,marker=marker)
        if previous:
            continue
        if not config['publish']:
            folder=home/'drafts'
            folder.mkdir(exist_ok=True)
            path=folder/(plan['marker']+'.md')
            path.write_text('# '+plan['title']+'\n\n'+plan['body']+'\n\n'+marker+'\n')
            ledger.append('draft_saved',repo=repo,marker=marker,path=str(path))
            row['issues'].append(dict(title=plan['title'],draft=str(path)))
            continue
        if remaining[1]<=0:
            return False
        ledger.append('publication_requested',repo=repo,marker=marker,title=plan['title'])
        created=github.create_issue(repo,plan['title'],plan['body'],marker)
        ledger.append('published',repo=repo,marker=marker,url=created['url'])
        row['issues'].append(dict(title=plan['title'],url=created['url']))
        remaining[1]-=1
    return True
