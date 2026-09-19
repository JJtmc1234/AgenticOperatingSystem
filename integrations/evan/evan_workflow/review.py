"""Read a prepared repair without starting workers or publishing anything."""
import json
from pathlib import Path
from iris_workflow.repository import contains_credential
from .source import git, verify_prepared


def show(home, repo, number):
    home=Path(home)
    journal=home/'events.jsonl'
    events=[json.loads(line) for line in journal.read_text().splitlines()] if journal.exists() else []
    prepared=next((event for event in reversed(events) if event['kind']=='prepared'
                   and event['repo']==repo and event['number']==number),None)
    if not prepared:
        raise ValueError('No prepared repair for '+repo+' issue '+str(number))
    plan=prepared['plan']
    root=Path(plan['checkout']).resolve()
    if not root.is_relative_to((home/'attempts').resolve()):
        raise ValueError('Prepared checkout is outside the Evan attempts directory')
    verify_prepared(plan)
    published=next((event for event in reversed(events) if event['kind']=='pr_published'
                    and event['key']==prepared['key']),None)
    lines=['# '+repo+' issue '+str(number), '', plan['title'], '',
           'Commit: '+plan['commit'], 'Branch: '+plan['branch'],
           'PR: '+published['url'] if published else 'Local repair. No published PR recorded.', '',
           '## Change', '', plan['summary'], '', '## Verification', '']
    for label,key in [('Before regression','baseline'),('Regression before repair','red'),('After repair','green')]:
        rows=plan[key]
        passed=sum(row['code']==0 for row in rows)
        lines.append(label+': '+str(passed)+'/'+str(len(rows))+' commands passed.')
        for index,row in enumerate(rows,1):
            lines+=['', 'Command '+str(index)+' exit '+str(row['code']),
                    '```text', row['output'].rstrip(), '```']
    lines+=['', '## Independent review', '', plan['review'], '',
            'Evidence: '+plan['evidence'], 'Checkout: '+str(root), '',
            '## Patch', '', '```diff', git(root,'diff','--no-ext-diff','--no-textconv',
                                        plan['base'],plan['commit'],'--'), '```', '']
    report='\n'.join(lines)
    if contains_credential(report):
        raise ValueError('Sensitive review output excluded')
    return report
