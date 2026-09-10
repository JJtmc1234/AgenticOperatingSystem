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
from .publication import publish
from .selection import select_batches, scope_key


def run(home, config, selected=None, request='', trigger='manual', force=False,
        github=None, snapshot=cache.snapshot, model_factory=Model, paths=(), specific=False):
    home=Path(home)
    github=github or GitHub()
    with Ledger(home) as ledger:
        ledger.append('run_started',trigger=trigger,request_hash=hashlib.sha256(request.encode()).hexdigest(),publish=config['publish'])
        try:
            if specific and (not selected or not request.strip()):
                raise ValueError('A specific issue requires a repository and a nonempty request')
            if paths and not selected:
                raise ValueError('File selection requires one configured repository')
            if len(request)>4000:
                raise ValueError('Request exceeds 4000 characters')
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
            if allowed and not selected:
                missing=set(allowed)-{r['nameWithOwner'] for r in repos}
                for repo in sorted(missing):
                    error='Configured repository was not found in GitHub discovery'
                    result['repositories'].append(dict(repo=repo,status='Failed: '+error,issues=[]))
                    ledger.append('repository_failed',repo=repo,error=error)
            remaining=[config['max_batches'],config['max_issues']]
            for entry in repos:
                repo=entry['nameWithOwner']
                try:
                    row=process(home,config,entry,request,trigger,force,ledger,model,github,snapshot,remaining,paths,specific)
                except Exception as error:
                    row=dict(repo=repo,status='Failed: '+str(error)[:600])
                    ledger.append('repository_failed',repo=repo,error=str(error)[:600])
                result['repositories'].append(row)
            ledger.append('run_finished',report=result)
            report.write(home,result)
            notifications.completed(ledger,result)
            return result
        except Exception as error:
            ledger.append('run_failed',error=str(error)[:600])
            from .status import write
            write(home)
            raise


def process(home,config,entry,request,trigger,force,ledger,model,github,snapshot,remaining,paths=(),specific=False):
    repo=entry['nameWithOwner']
    row=dict(repo=repo,issues=[])
    branch=(entry.get('defaultBranchRef') or {}).get('name')
    if entry.get('isArchived') or not branch:
        return row | dict(status='Skipped: archived or empty repository')
    repository=Repository(snapshot(home,repo,branch))
    info=repository.inspect()
    head=info['head']
    scope=scope_key(request,paths,specific)
    complete=ledger.latest('repo_complete',repo=repo,head=head,publish=config['publish'],scope=scope)
    if complete and not force and not request and not paths:
        row['issues']=complete.get('issues',[])
        return row | dict(status='Unchanged committed revision, already inspected')
    if trigger=='poll' and not force:
        seen=ledger.latest('repo_checked',repo=repo)
        if seen:
            elapsed=(datetime.datetime.now(datetime.timezone.utc)-datetime.datetime.fromisoformat(seen['at'])).total_seconds()
            changed=head!=seen['head']
            if elapsed<config['scan_interval'] and not changed:
                return row | dict(status='Waiting for hourly check or feature commit')
    ledger.append('repo_checked',repo=repo,head=head,trigger=trigger)
    batches=select_batches(repository,paths)
    eligible=sum(len(batch) for batch in batches)
    row['excluded_files']=info['tracked_count']-eligible
    issues=github.issues(repo)
    done=0
    blocked=''
    verified=0
    for batch in batches:
        key=hashlib.sha256(json.dumps([head,scope if paths or specific else request,batch],sort_keys=True).encode()).hexdigest()[:24]
        if ledger.latest('batch_done',repo=repo,key=key,publish=config['publish']) and not force:
            previous=ledger.latest('batch_reviewed',repo=repo,key=key) or {'plans':[]}
            verified+=len(previous['plans'])
            row['issues']+=previous.get('existing_links',[])
            row['issues']+=report.reviewed_outputs(previous['plans'],ledger,repo)
            done+=1
            continue
        if remaining[0]<=0:
            blocked='Run batch limit reached.'
            break
        plan=ledger.latest('batch_reviewed',repo=repo,key=key)
        if plan is None or force:
            if not model.can_investigate():
                blocked=model.blocked_reason() if hasattr(model,'blocked_reason') else 'Model budget cannot cover a review.'
                break
            ledger.append('batch_started',repo=repo,key=key,head=head)
            existing_links=[]
            plans=investigate(repo,repository,batch,issues,request,model,github,'iris/'+repo+'/'+key,specific=specific,existing_links=existing_links)
            plan=ledger.append('batch_reviewed',repo=repo,key=key,head=head,plans=plans,existing_links=existing_links)
        row['issues']+=plan.get('existing_links',[])
        verified+=len(plan['plans'])
        if not publish(home,config,repo,plan['plans'],ledger,github,remaining,row):
            blocked='Run issue limit reached.'
            break
        ledger.append('batch_done',repo=repo,key=key,head=head,publish=config['publish'],files=[f['path'] for f in batch])
        remaining[0]-=1
        done+=1
    if done==len(batches):
        ledger.append('repo_complete',repo=repo,head=head,publish=config['publish'],scope=scope,issues=row['issues'])
    row['scope']='Requested files' if paths else 'Eligible repository source'
    row['files_requested']=list(paths)
    row['blocked_reason']=blocked
    row['status']=f'Inspected {done}/{len(batches)} source batches at {head[:12]}. '+('Complete.' if done==len(batches) else 'Remaining work queued.')
    if blocked:
        row['status']+=' '+blocked
    elif not batches:
        row['status']+=' No eligible source was available. This is not a clean bill of health.'
    elif not row['issues'] and verified==0:
        row['status']+=' No new verified findings in the completed batches.'
    return row
