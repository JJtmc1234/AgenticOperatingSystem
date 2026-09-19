"""A locked journal drives polling, repair preparation and reviewed publication."""
import datetime
import hashlib
import json
from pathlib import Path
from iris_workflow.cache import snapshot
from iris_workflow.github import GitHub
from iris_workflow.ledger import Ledger
from iris_workflow.repository import Repository, contains_credential
from . import issues, publication, repair, source
from .model import Model


def recent(event, interval, now):
    return event and (now-datetime.datetime.fromisoformat(event['at'])).total_seconds()<interval


def process(home, settings, repo, policy, ledger, model, github, fetch, candidates, trigger, retry, now):
    rows=[]
    metadata=github._json(['repo','view',repo,'--json','defaultBranchRef,isArchived'])
    if metadata['isArchived'] or not metadata.get('defaultBranchRef'):
        return [dict(repo=repo,status='Blocked. Repository is archived or empty.')]
    branch=metadata['defaultBranchRef']['name']
    for issue in candidates:
        row=dict(repo=repo,issue=issue['number'],url=issue['url'])
        rows.append(row)
        key=None
        try:
            previous=ledger.latest('prepared',repo=repo,number=issue['number'])
            if previous and ledger.latest('pr_published',key=previous['key']):
                row.update(status='Already submitted for review.',pr=ledger.latest('pr_published',key=previous['key'])['url'])
                continue
            comments=github.comments(repo,issue['number'])
            if contains_credential(json.dumps(comments)):
                raise ValueError('Sensitive issue comments excluded')
            signature=issues.identity(issue,comments)
            cache=fetch(home,repo,branch)
            repository=Repository(cache)
            head=repository.inspect()['head']
            key=hashlib.sha256((repo+signature+head+json.dumps(policy,sort_keys=True)).encode()).hexdigest()[:24]
            prepared=ledger.latest('prepared',key=key)
            if prepared:
                source.verify_prepared(prepared['plan'])
                if settings['publish']:
                    row.update(status='Submitted for review.',pr=publication.publish(prepared['plan'],ledger,github))
                else:
                    row.update(status='Prepared. Publication disabled.',evidence=prepared['plan']['evidence'])
                continue
            started=ledger.latest('attempt_started',key=key)
            if started and not ledger.latest('attempt_finished',key=key) and not retry:
                row['status']='Blocked. Interrupted attempt requires an explicit --retry.'
                continue
            if not retry and recent(ledger.latest('attempt_finished',key=key),settings['scan_interval'],now):
                row['status']='Waiting for two hour retry or changed issue or source.'
                continue
            if model.attempts>=settings['max_issues'] or model.blocked_reason():
                row['status']='Queued. '+(model.blocked_reason() or 'Run issue limit reached.')
                continue
            model.attempts+=1
            ledger.append('attempt_started',key=key,repo=repo,number=issue['number'],trigger=trigger)
            folder=home/'attempts'/(key+'-'+str(len(ledger.events)))
            folder.mkdir(parents=True)
            plan=repair.prepare(folder,cache,policy,issue | dict(repo=repo),comments[-10:],model,settings,key)
            fresh=next((i for i in github.issues(repo) if i['number']==issue['number']),None)
            if not fresh or fresh['state']!='open' or issues.identity(fresh,github.comments(repo,issue['number']))!=signature:
                raise RuntimeError('Issue changed during repair. Prepared code is not publishable.')
            if Repository(fetch(home,repo,branch)).inspect()['head']!=head:
                raise RuntimeError('Default branch changed during repair. Revalidation required.')
            plan.update(base_branch=branch,signature=signature,evidence=str(folder/'evidence.json'))
            ledger.append('prepared',key=key,repo=repo,number=issue['number'],plan=plan)
            ledger.append('attempt_finished',key=key,outcome='prepared')
            if settings['publish']:
                row.update(status='Submitted for review.',pr=publication.publish(plan,ledger,github))
            else:
                row.update(status='Prepared. Publication disabled.',evidence=plan['evidence'])
        except Exception as error:
            detail=str(error)
            if contains_credential(detail):
                detail='Sensitive diagnostic excluded'
            row['status']='Blocked. '+detail[:1500]
            if key and ledger.latest('attempt_started',key=key):
                ledger.append('attempt_finished',key=key,outcome='blocked',reason=row['status'])
    return rows


def run(home, settings, selected=None, trigger='manual', retry=False, *, github=None, fetch=snapshot,
        model_factory=Model, iris_home=None, now=None):
    home=Path(home)
    github=github or GitHub()
    now=now or datetime.datetime.now(datetime.timezone.utc)
    configured=settings['repositories']
    if selected and selected not in configured:
        raise ValueError('Repository has no operator approved Evan policy')
    with Ledger(home,label='Evan') as ledger:
        ledger.append('run_started',trigger=trigger,publish=settings['publish'])
        known=issues.known_publications((iris_home or home.parent/'iris')/'events.jsonl')
        model=model_factory(settings,ledger)
        model.attempts=0
        rows=[]
        queue=[]
        for repo in ([selected] if selected else configured):
            try:
                for event in list(ledger.events):
                    if (event['kind']=='prepared' and event['repo']==repo and
                            ledger.latest('pr_requested',key=event['key'])):
                        publication.publish(event['plan'],ledger,github)
                candidates=issues.select(repo,github.issues(repo),known)
                if not candidates:
                    rows.append(dict(repo=repo,status='Idle. No actionable Iris issues.'))
                queue.extend((i['priority'],i['number'],repo,i) for i in candidates)
            except Exception as error:
                detail=str(error)
                rows.append(dict(repo=repo,status='Blocked. '+('Sensitive diagnostic excluded' if contains_credential(detail) else detail[:1500])))
        for _,_,repo,issue in sorted(queue,key=lambda item:item[:3]):
            try:
                rows+=process(home,settings,repo,configured[repo],ledger,model,github,fetch,[issue],trigger,retry,now)
            except Exception as error:
                detail=str(error)
                rows.append(dict(repo=repo,status='Blocked. '+('Sensitive diagnostic excluded' if contains_credential(detail) else detail[:1500])))
        report=dict(rows=rows,status='No repositories configured.' if not configured else 'Finished',publish=settings['publish'])
        ledger.append('run_finished',report=report)
        (home/'latest-report.json').write_text(json.dumps(report,indent=2)+'\n')
        return report
