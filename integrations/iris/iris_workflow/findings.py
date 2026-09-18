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
        if not isinstance(item,dict) or set(item) not in (expected,expected|{'direction'}):
            raise ValueError('Invalid finding fields')
        if item['kind'] not in ('bug','performance','maintainability','request') or item['severity'] not in ('major','minor'):
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
        if 'direction' in item and (not isinstance(item['direction'],str) or not item['direction'].strip()
                                    or len(item['direction'])>400 or contains_credential(item['direction'])):
            raise ValueError('Invalid suggested direction')
        source=repository.source(item['path']).splitlines()
        excerpt=item['excerpt'].splitlines()
        actual='\n'.join(source[item['line']-1:item['line']-1+len(excerpt)])
        if actual.strip()!=item['excerpt'].strip():
            matches=[index for index in range(len(source)-len(excerpt)+1)
                     if '\n'.join(source[index:index+len(excerpt)]).strip()==item['excerpt'].strip()]
            if len(matches)!=1:
                raise ValueError('Finding excerpt does not match committed source')
            # Locate an exact unique quotation, never rewrite code to fit a claim.
            start=matches[0]
            item=item | dict(line=start+1,excerpt='\n'.join(source[start:start+len(excerpt)]))
        findings.append(item)
    return findings


def related(findings, issues, limit=5):
    def score(issue):
        body=(issue.get('title','')+' '+issue.get('body','')).lower()
        return max((difflib.SequenceMatcher(None,f['title'].lower(),issue.get('title','').lower()).ratio()
                    + (0.5 if f['path'].lower() in body else 0) for f in findings),default=0)
    ranked=sorted(((score(issue),index,issue) for index,issue in enumerate(issues)),
                  key=lambda entry:entry[0],reverse=True)
    return [issue for relevance,_,issue in ranked if relevance>=0.35][:limit]


def duplicate(finding, issues):
    title=finding['title'].lower().strip()
    return any(difflib.SequenceMatcher(None,title,i.get('title','').lower().strip()).ratio()>=0.90 for i in issues)


def issue_plans(repo, head, findings):
    groups={}
    for f in findings:
        component=f['path']
        key=identifier(repo,f) if f['severity']=='major' else 'minor:'+component+':'+f['kind']
        groups.setdefault(key,[]).append(f)
    plans=[]
    for group in groups.values():
        ids=sorted(identifier(repo,f) for f in group)
        marker=hashlib.sha256('|'.join(ids).encode()).hexdigest()[:24]
        title=group[0]['title'] if len(group)==1 else f"{group[0]['path']}: {len(group)} related {group[0]['kind']} findings"
        body=[]
        for f in group:
            from urllib.parse import quote
            body += ["## What happens" if len(group)==1 else f"### {f['title']}",f['impact'],
                     "**Why this happens**",f['mechanism'],"**Source evidence**",f"Source: https://github.com/{repo}/blob/{head}/{quote(f['path'],safe='/')}#L{f['line']}",
                     '```\n'+f['excerpt'].replace('```','` ` `')+'\n```',
                     '**Suggested direction**',f.get('direction','Use the proposed regression to guide a minimal change at the referenced code.'),
                     '**Acceptance criteria and proposed test**',f['validation'],f"<!-- aos-iris-finding:{identifier(repo,f)} -->"]
        if any(f['kind']=='request' for f in group):
            body.insert(0,'Requested enhancement from JJ. Current limitations are source reviewed, not a reproduced defect.')
        body += ['Evidence level: source analysis with independent review. Runtime reproduction has not been performed.',
                 f'Inspected commit: `{head}`.']
        plans.append(dict(title=title,body='\n\n'.join(body),marker=marker,
                          priority=0 if group[0]['severity']=='major' else 1))
    return sorted(plans,key=lambda p:(p['priority'],p['title']))
