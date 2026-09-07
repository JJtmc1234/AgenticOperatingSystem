"""Two scoped investigators followed by independent evidence review."""
import json
from .findings import validated, related, duplicate, identifier, issue_plans
from .model import FINDINGS, REVIEW
from .repository import contains_credential


def investigate(repo, repository, batch, issues, request, model, github, identity):
    if contains_credential(request):
        raise ValueError('Request may contain credentials')
    clean=lambda text: '[Sensitive text excluded]' if contains_credential(text) else text
    context=dict(repository=repo,revision=repository.head,request=request,
                 sources=[dict(path=f['path'], numbered_lines=[dict(line=n,text=line)
                     for n,line in enumerate(f['content'].splitlines(),1)]) for f in batch],issue_titles=[clean(i['title']) for i in issues][:200])
    candidates=[]
    for role in ['correctness','efficiency']:
        prompt=('Inspect this source batch for '+
                ('bugs and broken boundaries.' if role=='correctness' else 'algorithmic waste and materially unnecessary complexity.')+
                ' Return concrete findings only. Do not claim runtime testing. Use the supplied line numbers. Copy a contiguous excerpt exactly, without line numbers or ellipses.\n'+json.dumps(context))
        result=model.ask(identity+'/'+role,prompt,FINDINGS)
        for finding in validated(result,repository):
            if finding['path'] not in {f['path'] for f in batch}:
                raise ValueError('Finding is outside the assigned source batch')
            if not any(identifier(repo,f)==identifier(repo,finding) for f in candidates):
                candidates.append(finding)
    candidates=[f for f in candidates if not duplicate(f,issues) and not any(
        '<!-- aos-iris-finding:'+identifier(repo,f)+' -->' in i.get('body','') for i in issues)]
    if not candidates:
        return []
    matches=related(candidates,issues)
    existing=[]
    for issue in matches:
        comments=github.comments(repo,issue['number'])
        existing.append(dict(title=clean(issue['title']),body=clean(issue.get('body',''))[:6000],state=issue['state'],
                             comments=[clean(c.get('body',''))[:2000] for c in comments[-10:]]))
    prompt='''Independently review these findings using the committed source. Return accepted zero-based
candidate indexes only. Reject unsupported mechanisms, trivial preferences, duplicates of existing
issues including fixes recorded in comments, invented measurements and unactionable proposals.
Accept only findings you can justify directly from the source. Empty accepted is correct if unsure.
'''+json.dumps(dict(candidates=candidates,sources=batch,existing=existing))
    review=model.ask(identity+'/review',prompt,REVIEW)
    accepted=review.get('accepted')
    if set(review)!={'accepted'} or not isinstance(accepted,list) or len(set(map(str,accepted)))!=len(accepted) or any(
        type(i) is not int or i<0 or i>=len(candidates) for i in accepted):
        raise ValueError('Reviewer returned invalid candidate indexes')
    return issue_plans(repo,repository.head,[candidates[i] for i in accepted])
