"""Current workflow status comes from the journal and kernel lock, not a cached report."""
import fcntl
import json
from pathlib import Path


def running(home):
    try:
        lock=(home/'run.lock').open('rb')
    except FileNotFoundError:
        return False
    with lock:
        try:
            fcntl.flock(lock,fcntl.LOCK_SH | fcntl.LOCK_NB)
        except BlockingIOError:
            return True
    return False


def read(home):
    home=Path(home)
    active=running(home)
    path=home/'events.jsonl'
    if not path.exists():
        return dict(status='Running' if active else 'Not run',rows=[])
    with path.open('rb') as source:
        data=source.read(16*1024*1024+1)
    if len(data)>16*1024*1024:
        raise ValueError('Evan journal exceeds the status reader limit')
    events=[]
    for line in data.splitlines(keepends=True):
        try:
            event=json.loads(line)
        except (ValueError,UnicodeDecodeError):
            if active and not line.endswith(b'\n'):
                break
            raise ValueError('Evan journal is unreadable. Status is unknown.') from None
        if not isinstance(event,dict) or event.get('seq')!=len(events)+1 or not isinstance(event.get('kind'),str):
            raise ValueError('Evan journal is unreadable. Status is unknown.')
        events.append(event)
    boundary=next((e for e in reversed(events) if e['kind'] in ('run_started','run_finished')),None)
    if active:
        start=next((e['seq'] for e in reversed(events) if e['kind']=='run_started'),0)
        stage=next((e for e in reversed(events) if e['kind']=='investigator_started' and e['seq']>start),None)
        phase=stage['identity'].rsplit('/',1)[-1] if stage else 'checking issues'
        return dict(status='Running',phase=phase,rows=[])
    if boundary and boundary['kind']=='run_started':
        # Recheck after reading so a run starting during this observation is not called dead.
        if running(home):
            return dict(status='Running',phase='checking issues',rows=[])
        return dict(status='Interrupted',detail='Inspect the last attempt before using --retry.',rows=[])
    if boundary:
        return boundary['report']
    return dict(status='Not run',rows=[])
