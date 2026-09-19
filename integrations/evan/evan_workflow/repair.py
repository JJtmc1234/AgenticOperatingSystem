"""A failing regression, minimal repair and independent review precede a commit."""
import json
from iris_workflow.repository import contains_credential
from . import patches, sandbox, source


def prepare(folder, cache, policy, issue, comments, model, settings, key):
    if len(issue['body'])>32000:
        raise ValueError('Issue exceeds the bounded repair context')
    comments=[dict(body=c.get('body','')[:2000]) for c in comments[-10:]]
    head,original,context,instructions=source.read(cache,policy,issue)
    baseline=sandbox.run(original,policy['commands'],settings['test_timeout'])
    if not sandbox.passed(baseline):
        raise RuntimeError('Existing tests fail before any edit. Repair requires investigation.')
    evidence=dict(issue=issue,comments=comments,sources=context,instructions=instructions,
                  test_paths=policy['test_paths'],commands=policy['commands'])
    prompt='Create only a regression test for this specific issue. Use literal before and after replacements. '
    test=model.ask(key+'/regression',prompt+json.dumps(evidence),patches.PATCH)
    reproduced=patches.apply(original,test,policy['test_paths'],allow_new=True)
    red=sandbox.run(reproduced,policy['commands'],settings['test_timeout'])
    if sandbox.passed(red) or any(r['code'] not in (0,1) for r in red) or any(
            word in r['output'] for r in red for word in ('SyntaxError','ImportError','ModuleNotFoundError')):
        raise RuntimeError('Proposed regression did not reproduce a test assertion failure')
    evidence.update(regression=test,red=red)
    fix=model.ask(key+'/fix','Fix the demonstrated mechanism. Only source paths may change. '+json.dumps(evidence),patches.PATCH)
    fixed=patches.apply(reproduced,fix,policy['paths'])
    green=sandbox.run(fixed,policy['commands'],settings['test_timeout'])
    if not sandbox.passed(green):
        raise RuntimeError('Fix failed verification. No commit or PR was prepared.')
    evidence.update(fix=fix,baseline=baseline,green=green)
    review=model.ask(key+'/review','Independently reject unrelated edits, weakened tests, fabricated reproduction, '
        'unsafe changes or an incorrect fix. A passing test alone is insufficient. '+json.dumps(evidence),patches.REVIEW)
    if (not isinstance(review,dict) or set(review)!={'accepted','reason'} or review['accepted'] is not True
            or not isinstance(review['reason'],str) or not review['reason'].strip()):
        raise RuntimeError('Independent review did not accept the fix')
    if contains_credential(json.dumps([test,fix,review,baseline,red,green])):
        raise ValueError('Sensitive repair evidence was excluded')
    branch='evan/issue-'+str(issue['number'])+'-'+key
    checkout=folder/'checkout'
    oid=source.commit(checkout,cache,branch,head,original,fixed)
    result=dict(key=key,repo=issue['repo'],number=issue['number'],issue_url=issue['url'],
                title=issue['title'],base=head,branch=branch,commit=oid,checkout=str(checkout),
                summary=fix['summary'],review=review['reason'],baseline=baseline,red=red,green=green)
    (folder/'evidence.json').write_text(json.dumps(result,indent=2)+'\n')
    return result
