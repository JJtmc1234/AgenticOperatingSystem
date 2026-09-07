#!/usr/bin/env python3
"""Local desktop completion notices. Prompt and answer text never enter notifications."""
import datetime
import html
import json
import os
from pathlib import Path
import subprocess
import sys


def emit(title, body):
    env=dict(os.environ)
    runtime=env.setdefault('XDG_RUNTIME_DIR','/run/user/'+str(os.getuid()))
    env.setdefault('DBUS_SESSION_BUS_ADDRESS','unix:path='+runtime+'/bus')
    try:
        result=subprocess.run(['notify-send','--print-id','--app-name=AOS','--expire-time=15000',
            '--hint=string:sound-name:complete','--',title,html.escape(body[:400])],
            env=env,capture_output=True,text=True,timeout=5)
        return dict(delivered=result.returncode==0,notification_id=result.stdout.strip() if result.returncode==0 else None)
    except (OSError,subprocess.TimeoutExpired):
        return dict(delivered=False,notification_id=None)


def completed(ledger, result):
    rows=result['repositories']
    issues=sum(len(row.get('issues',[])) for row in rows)
    failed=sum(row['status'].startswith('Failed:') for row in rows)
    start=ledger.latest('run_started') or {'seq':0}
    investigated=any(e['seq']>start['seq'] and e['kind']=='investigator_started' for e in ledger.events)
    manual=result['trigger']=='manual'
    if not (manual or issues or failed or investigated):
        return
    title='Iris needs attention' if failed else 'Iris finished'
    body=f'{issues} issues reported. {failed} repositories failed. Run carl iris status for details.'
    previous=ledger.latest('notification_requested')
    if not manual and not issues and not investigated and previous and previous.get('body')==body:
        return
    ledger.append('notification_requested',title=title,body=body)
    ledger.append('notification_result',**emit(title,body))


def browser_finished(ledger, passed, stats):
    title='Iris browser tests passed' if passed else 'Iris browser tests need attention'
    body=f"{stats.get('expected',0)} passed, {stats.get('unexpected',0)} failed. Open the latest Iris browser report for details."
    ledger.append('notification_requested',title=title,body=body)
    ledger.append('notification_result',**emit(title,body))


def main():
    try:
        event=json.loads(sys.argv[1])
        if event.get('type') not in ('agent-turn-complete','aos-work-complete'):
            return 0
        title='Codex finished' if event['type']=='agent-turn-complete' else 'AOS work finished'
        body='Your task is finished. Open the conversation to see the result.'
        result=emit(title,body)
        state=Path.home()/'.local/state/aos-notifications'
        state.mkdir(parents=True,exist_ok=True,mode=0o700)
        with (state/'events.jsonl').open('a') as log:
            log.write(json.dumps(dict(at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                                     type=event['type'],**result))+'\n')
    except (ValueError,IndexError,AttributeError,OSError):
        pass
    return 0


if __name__=='__main__':
    raise SystemExit(main())
