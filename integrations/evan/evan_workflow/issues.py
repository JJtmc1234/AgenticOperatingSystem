"""Select confirmed Iris publications in Iris's original priority order."""
import hashlib
import json
import re
from urllib.parse import unquote
from iris_workflow.repository import contains_credential


def identity(issue, comments):
    value=[issue['number'],issue['title'],issue['body'],comments]
    return hashlib.sha256(json.dumps(value,sort_keys=True).encode()).hexdigest()[:24]


def known_publications(path):
    events=[json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    priorities={}
    for event in events:
        for plan in event.get('plans',[]):
            priorities['<!-- aos-iris:'+plan['marker']+' -->']=plan['priority']
    return {e['url']:(e['marker'],priorities.get(e['marker'],1)) for e in events
            if e['kind']=='published' and e.get('url')}


def select(repo, issues, known):
    selected=[]
    for issue in issues:
        if issue.get('state')!='open' or issue.get('url') not in known:
            continue
        marker,priority=known[issue['url']]
        if marker not in issue['body'] or '[Iris validation]' in issue['title']:
            continue
        if contains_credential(issue['body']+issue['title']):
            continue
        paths=re.findall(r'Source: https://github\.com/'+re.escape(repo)+r'/blob/[a-f0-9]{40}/([^\s#]+)#L\d+',issue['body'])
        selected.append(issue | dict(priority=priority,paths=sorted({unquote(p) for p in paths})))
    return sorted(selected,key=lambda issue:(issue['priority'],issue['number']))
