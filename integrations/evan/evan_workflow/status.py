"""Current workflow status comes from the journal and kernel lock, not a cached report."""
import fcntl
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
    from .journal import read as read_events
    events=read_events(home,active=active)
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
