"""Recheck live issue history before publishing a previously reviewed plan."""
import re
from .findings import duplicate


def reconcile(repo, ledger, github, issues=None):
    pending={e['marker']: e for e in ledger.events
             if e['kind']=='publication_requested' and e.get('repo')==repo
             and not ledger.latest('published',repo=repo,marker=e['marker'])}
    if not pending:
        return
    issues=github.issues(repo) if issues is None else issues
    unresolved=[]
    for marker in pending:
        match=next((i for i in issues if marker in i.get('body','')),None)
        if match and match.get('url'):
            ledger.append('published',repo=repo,marker=marker,url=match['url'],reconciled=True)
        else:
            unresolved.append(marker)
    if unresolved:
        raise RuntimeError('Issue publication is uncertain. Iris will only read GitHub until the '
                           'previous write is reconciled. No create retry is allowed, even with '
                           '--force. Check repository issue history for '+', '.join(unresolved))


def existing_issue(plan, issues):
    marker='<!-- aos-iris:'+plan['marker']+' -->'
    findings=set(re.findall(r'<!-- aos-iris-finding:[A-Za-z0-9_.:-]+ -->',plan['body']))
    for issue in issues:
        body=issue.get('body','')
        if (marker in body or any(value in body for value in findings)
                or duplicate(plan,[issue])):
            return issue
    return None


def publish(home,config,repo,plans,ledger,github,remaining,row):
    issues=github.issues(repo) if plans else []
    reconcile(repo,ledger,github,issues)
    for plan in plans:
        marker='<!-- aos-iris:'+plan['marker']+' -->'
        previous=ledger.latest('published',repo=repo,marker=marker)
        match=existing_issue(plan,issues)
        if match or previous:
            found=match or previous
            ledger.append('duplicate_skipped',repo=repo,marker=marker,url=found.get('url'))
            row['issues'].append(dict(title='Already reported: '+plan['title'],
                                      url=found.get('url',''),existing=True))
            continue
        if not config['publish']:
            folder=home/'drafts'
            folder.mkdir(exist_ok=True)
            path=folder/(plan['marker']+'.md')
            path.write_text('# '+plan['title']+'\n\n'+plan['body']+'\n\n'+marker+'\n')
            ledger.append('draft_saved',repo=repo,marker=marker,path=str(path))
            row['issues'].append(dict(title=plan['title'],draft=str(path)))
            issues.append(dict(title=plan['title'],body=plan['body']+'\n'+marker,url=str(path)))
            continue
        if remaining[1]<=0:
            return False
        ledger.append('publication_requested',repo=repo,marker=marker,title=plan['title'])
        created=github.create_issue(repo,plan['title'],plan['body'],marker)
        ledger.append('published',repo=repo,marker=marker,url=created['url'])
        row['issues'].append(dict(title=plan['title'],url=created['url']))
        remaining[1]-=1
        issues=github.issues(repo)
        # Retain confirmed writes even if the next GitHub list is briefly stale.
        issues.append(created)
    return True
