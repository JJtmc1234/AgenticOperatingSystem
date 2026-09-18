"""Two scoped investigators followed by independent evidence review."""
import json
from .findings import validated, related, duplicate, identifier, issue_plans
from .model import FINDINGS, REVIEW
from .repository import contains_credential


def investigate(repo, repository, batch, issues, request, model, github, identity, specific=False, existing_links=None):
    if contains_credential(request):
        raise ValueError('Request may contain credentials')
    clean=lambda text: '[Sensitive text excluded]' if contains_credential(text) else text
    history={}

    def issue_context(matches):
        for issue in matches:
            if issue['number'] not in history:
                comments=github.comments(repo,issue['number'])
                history[issue['number']]=dict(title=clean(issue['title']),body=clean(issue.get('body',''))[:6000],
                                             state=issue['state'],comments=[clean(c.get('body',''))[:2000] for c in comments[-10:]])
        return [history[i['number']] for i in matches]

    prior=related([dict(title=request,path=f['path']) for f in batch],issues)
    context=dict(repository=repo,revision=repository.head,request=request,
                 sources=[dict(path=f['path'], numbered_lines=[dict(line=n,text=line)
                     for n,line in enumerate(f['content'].splitlines(),1)]) for f in batch],
                 existing=issue_context(prior),issue_titles=[clean(i['title']) for i in issues][:200])
    focus=('Review only the problem or behavior JJ requested. Do not substitute unrelated findings. '
           'If this is an enhancement, use kind request and describe the current limitation, not an invented bug. '
           'Suggest a small direction and measurable acceptance criteria in validation. ') if specific else ''
    candidates=[]
    for role in ['correctness','efficiency']:
        prompt=(focus+'Inspect this source batch for '+
                ('bugs and broken boundaries.' if role=='correctness' else 'algorithmic waste and materially unnecessary complexity.')+
                ' Return concrete findings only. Do not claim runtime testing. Use the supplied line numbers. Copy a contiguous excerpt exactly, without line numbers or ellipses. Prefer one exact source line demonstrating the mechanism.\n'+json.dumps(context))
        result=model.ask(identity+'/'+role,prompt,FINDINGS)
        for finding in validated(result,repository):
            if finding['kind']=='request' and not specific:
                raise ValueError('General reviews cannot invent feature requests')
            if finding['path'] not in {f['path'] for f in batch}:
                raise ValueError('Finding is outside the assigned source batch')
            if not any(identifier(repo,f)==identifier(repo,finding) for f in candidates):
                candidates.append(finding)
    fresh=[]
    for finding in candidates:
        match=next((i for i in issues if duplicate(finding,[i]) or
                    '<!-- aos-iris-finding:'+identifier(repo,finding)+' -->' in i.get('body','')),None)
        if match:
            if existing_links is not None and not any(i.get('url')==match.get('url') for i in existing_links):
                existing_links.append(dict(title='Already reported: '+match['title'],url=match.get('url',''),existing=True))
        else:
            fresh.append(finding)
    candidates=fresh
    if not candidates:
        return []
    matches=related(candidates,issues)
    existing=issue_context(matches)
    prompt='''Independently review these findings using the committed source. Return accepted zero-based
candidate indexes in accepted. Use minor for accepted indexes whose severity needs downgrading.
If both investigators found the same underlying problem, accept only the clearest candidate.
Reject unsupported mechanisms, trivial preferences, duplicates of existing
issues including fixes recorded in comments, invented measurements and unactionable proposals.
Trace each proposed reproduction through the source. Reject examples that do not distinguish
current behavior from the required behavior, and unsupported downstream claims.
Major severity requires security exposure, data loss or an unusable core operation. If the defect
is real but its severity is overstated, accept it with a downgrade in minor instead of discarding it.
Do not invent consequences to justify severity. Require a small suggested change.
Accept only findings you can justify directly from the source. Empty accepted is correct if unsure.
'''+json.dumps(dict(candidates=candidates,sources=batch,existing=existing))
    review=model.ask(identity+'/review',focus+prompt+json.dumps(dict(request=request)),REVIEW)
    accepted=review.get('accepted')
    minor=review.get('minor',[])
    if (set(review) not in ({'accepted'},{'accepted','minor'}) or
            any(not isinstance(indexes,list) or len(set(map(str,indexes)))!=len(indexes) or
                any(type(i) is not int or i<0 or i>=len(candidates) for i in indexes)
                for indexes in (accepted,minor)) or not set(minor)<=set(accepted)):
        raise ValueError('Reviewer returned invalid candidate indexes')
    return issue_plans(repo,repository.head,[candidates[i] | (dict(severity='minor') if i in minor else {})
                                           for i in accepted])
