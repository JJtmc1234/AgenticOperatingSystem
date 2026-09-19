"""Recover publication intent without duplicate PRs or automatic merges."""
from .source import git


def existing(github, plan):
    pulls=github._json(['pr','list','--repo',plan['repo'],'--state','all','--head',plan['branch'],
                        '--json','url,headRefOid,body'])
    marker='<!-- aos-evan:'+plan['key']+' -->'
    match=next((p for p in pulls if marker in p['body'] and p['headRefOid']==plan['commit']),None)
    if pulls and not match:
        raise RuntimeError('Existing branch PR does not match the prepared commit and marker')
    return match


def publish(plan, ledger, github):
    old=ledger.latest('pr_published',key=plan['key'])
    if old:
        return old['url']
    match=existing(github,plan)
    if match:
        ledger.append('pr_published',key=plan['key'],url=match['url'])
        return match['url']
    if ledger.latest('pr_requested',key=plan['key']):
        raise RuntimeError('PR publication is uncertain. No create retry is permitted.')
    root=plan['checkout']
    if git(root,'status','--porcelain') or git(root,'rev-parse','HEAD')!=plan['commit']:
        raise ValueError('Prepared checkout changed. Publication refused.')
    url='https://github.com/'+plan['repo']+'.git'
    ledger.append('push_requested',key=plan['key'],commit=plan['commit'])
    git(root,'push',url,plan['commit']+':refs/heads/'+plan['branch'])
    remote=git(root,'ls-remote',url,'refs/heads/'+plan['branch']).split()
    if not remote or remote[0]!=plan['commit']:
        raise RuntimeError('Pushed commit could not be confirmed')
    body=('Fix for '+plan['issue_url']+'\n\n'+plan['summary']+
          '\n\nExisting tests passed. The new regression failed before the fix and passed afterward. '
          'Independent review accepted the change. Review and merge remain human decisions.\n\n'
          '<!-- aos-evan:'+plan['key']+' -->')
    from pathlib import Path
    path=Path(root).parent/'pr-body.md'
    path.write_text(body)
    ledger.append('pr_requested',key=plan['key'],repo=plan['repo'],branch=plan['branch'])
    github._run(['pr','create','--repo',plan['repo'],'--base',plan['base_branch'],
                 '--head',plan['branch'],'--title','[Evan] '+plan['title'][:180],
                 '--body-file',str(path),'--draft'])
    match=existing(github,plan)
    if not match:
        raise RuntimeError('PR publication is uncertain. No create retry is permitted.')
    ledger.append('pr_published',key=plan['key'],url=match['url'])
    return match['url']
