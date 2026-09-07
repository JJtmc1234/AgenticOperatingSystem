"""Build a durable overview from the journal without hiding results behind idle polls."""
import datetime
import json
import os
import tempfile
from pathlib import Path


def render(home):
    home=Path(home)
    path=home/'events.jsonl'
    events=[]
    if path.exists():
        raw=path.read_text()
        lines=raw.splitlines()
        for index,line in enumerate(lines):
            try:
                events.append(json.loads(line))
            except ValueError:
                if index!=len(lines)-1 or raw.endswith('\n'):
                    raise ValueError('Iris journal is corrupt') from None
    from .config import load
    settings=load(home)
    day=datetime.datetime.now(datetime.timezone.utc).date().isoformat()
    reserved=sum(e['amount'] for e in events if e['kind']=='budget_reserved' and e['day']==day)
    lines=['# Iris', '', f"Model allowance: ${reserved:.2f} reserved of ${settings['max_daily_usd']:.2f} today. Resets at 00:00 UTC.", '']
    lines+=['Budget limits queue new reviews. Status checks do not spend review allowance or kill handoff processes.', '']
    starts=[e for e in events if e['kind']=='run_started']
    finishes=[e for e in events if e['kind'] in ('run_finished','run_failed')]
    if starts and (not finishes or starts[-1]['seq']>finishes[-1]['seq']):
        lines+=['Investigation in progress.', '']
    published=[e for e in events if e['kind']=='published']
    lines+=['## Published issues', '', 'Titles are recorded locally and may differ from later GitHub edits.', '']
    seen=set()
    for event in reversed(published):
        url=event['url']
        if url in seen:continue
        seen.add(url)
        title=next((e.get('title') for e in reversed(events) if e['kind']=='publication_requested'
                    and e.get('repo')==event['repo'] and e.get('marker')==event['marker']),None)
        lines.append(f'- [{title or url}]({url})')
        if len(seen)==10:break
    if not seen:lines.append('No published issues yet.')
    canonical=lambda marker: marker.removeprefix('<!-- aos-iris:').removesuffix(' -->')
    published_markers={(e['repo'],canonical(e.get('marker',''))) for e in published}
    drafts=[e for e in events if e['kind']=='draft_saved' and
            (e['repo'],canonical(e.get('marker',''))) not in published_markers]
    if drafts:
        lines+=['', '## Recent drafts', '']
        shown=set()
        for event in reversed(drafts):
            if event['path'] in shown:continue
            shown.add(event['path'])
            lines.append(f"* [{event['repo']}]({event['path']})")
            if len(shown)==5:break
    if (home/'manual-report.md').exists():
        lines+=['', f"[Latest manual review]({home/'manual-report.md'})"]
    tests=[e for e in events if e['kind']=='browser_test_finished']
    if tests:
        test=tests[-1]
        stats=test['stats']
        output=Path(test['output'])
        lines+=['', '## Latest browser test', '',
                f"{stats.get('expected',0)} passed, {stats.get('unexpected',0)} failed, {stats.get('skipped',0)} skipped.",
                f"[Open report]({output/'html/index.html'})", f"[Run log]({output/'run.log'})"]
    if finishes and finishes[-1]['kind']=='run_failed':
        lines+=['', 'Latest investigation failed: '+finishes[-1]['error'], '']
    completed=[e for e in finishes if e['kind']=='run_finished']
    if completed:
        report=completed[-1]['report']
        failed=sum(row['status'].startswith('Failed:') for row in report['repositories'])
        lines+=['', '## Latest repository check', '',
                f"{len(report['repositories'])} repositories checked, {failed} failed. Trigger: {report['trigger']}.",
                f"[Full report]({home/'latest-report.md'})"]
    if completed:
        reasons=sorted({r.get('blocked_reason','') for r in report['repositories']} - {''})
        lines+=['', *reasons]
    return '\n'.join(lines)+'\n'


def write(home):
    home=Path(home)
    text=render(home)
    home.mkdir(parents=True,exist_ok=True)
    with tempfile.NamedTemporaryFile('w',dir=home,delete=False) as out:
        out.write(text)
    os.replace(out.name,home/'overview.md')
    return text
