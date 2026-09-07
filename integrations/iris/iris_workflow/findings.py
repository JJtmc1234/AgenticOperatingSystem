"""Validate source evidence before a finding can reach GitHub."""
import difflib
import hashlib
from .repository import contains_credential


def identifier(repo, finding):
    identity='|'.join([repo, finding['path'], finding['kind'], finding['mechanism'].strip().lower()])
    return hashlib.sha256(identity.encode()).hexdigest()[:24]


def validated(raw, repository):
    if not isinstance(raw,dict) or set(raw)!={'findings'} or not isinstance(raw['findings'],list) or len(raw['findings'])>6:
        raise ValueError('Invalid investigator findings')
    findings=[]
    for item in raw['findings']:
        expected={'title','kind','severity','path','line','excerpt','mechanism','impact','validation'}
        if not isinstance(item,dict) or set(item)!=expected:
            raise ValueError('Invalid finding fields')
        if item['kind'] not in ('bug','performance','maintainability') or item['severity'] not in ('major','minor'):
            raise ValueError('Invalid finding category')
        if type(item['line']) is not int or item['line']<1:
            raise ValueError('Invalid source line')
        for key in expected-{'line'}:
            if not isinstance(item[key],str) or not item[key].strip() or len(item[key])>1600:
                raise ValueError('Invalid finding text')
        if any(len(item[key])>limit for key,limit in [('title',140),('path',300),('excerpt',800),('mechanism',700),('impact',400),('validation',600)]):
            raise ValueError('Finding is too verbose')
        if any(contains_credential(item[key]) for key in expected-{'line'}):
            raise ValueError('Finding may contain a credential')
        source=repository.source(item['path']).splitlines()
        excerpt=item['excerpt'].splitlines()
        actual='\n'.join(source[item['line']-1:item['line']-1+len(excerpt)])
        if actual.strip()!=item['excerpt'].strip():
            raise ValueError('Finding excerpt does not match committed source')
        findings.append(item)
    return findings


def related(findings, issues, limit=5):
    def score(issue):
        body=(issue.get('title','')+' '+issue.get('body','')).lower()
        return max((difflib.SequenceMatcher(None,f['title'].lower(),issue.get('title','').lower()).ratio()
                    + (0.5 if f['path'].lower() in body else 0) for f in findings),default=0)
    return sorted(issues,key=score,reverse=True)[:limit]


def duplicate(finding, issues):
    title=finding['title'].lower().strip()
    return any(difflib.SequenceMatcher(None,title,i.get('title','').lower().strip()).ratio()>=0.90 for i in issues)


def issue_plans(repo, head, findings):
    groups={}
    for f in findings:
        component=f['path'].split('/')[0]
        key=identifier(repo,f) if f['severity']=='major' else 'minor:'+component+':'+f['kind']
        groups.setdefault(key,[]).append(f)
    plans=[]
    for group in groups.values():
        ids=sorted(identifier(repo,f) for f in group)
        marker=hashlib.sha256('|'.join(ids).encode()).hexdigest()[:24]
        title=group[0]['title'] if len(group)==1 else f"{group[0]['path'].split('/')[0]}: {len(group)} related {group[0]['kind']} findings"
        body=[]
        for f in group:
            from urllib.parse import quote
            body += ["## What happens" if len(group)==1 else f"### {f['title']}",f['impact'],
                     "**Why this happens**",f['mechanism'],"**Source evidence**",f"Source: https://github.com/{repo}/blob/{head}/{quote(f['path'],safe='/')}#L{f['line']}",
                     '```\n'+f['excerpt'].replace('```','` ` `')+'\n```',
                     '**Completion criteria and proposed test**',f['validation'],f"<!-- aos-iris-finding:{identifier(repo,f)} -->"]
        body += ['Evidence level: source analysis with independent review. Runtime reproduction has not been performed.',
                 f'Inspected commit: `{head}`.']
        plans.append(dict(title=title,body='\n\n'.join(body),marker=marker,
                          priority=0 if group[0]['severity']=='major' else 1))
    return sorted(plans,key=lambda p:(p['priority'],p['title']))
