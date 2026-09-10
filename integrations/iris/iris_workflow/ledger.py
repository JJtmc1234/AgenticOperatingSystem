"""Append before action. Reports and cursors are derived from this journal."""
import datetime
import fcntl
import json
import os
from pathlib import Path


class Ledger:
    def __init__(self, home):
        self.home = Path(home)
        self.home.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.path = self.home / 'events.jsonl'
        self.lock = None
        self.events = []

    def __enter__(self):
        self.lock = (self.home / 'run.lock').open('a')
        try:
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            self.lock.close()
            raise RuntimeError('Iris already has a running investigation. The kernel lock is held by '
                               'another process, which may finish before a process listing runs. '
                               'Retry later without deleting run.lock. This is not evidence of a stale lock file.') from None
        try:
            if self.path.exists():
                with self.path.open() as source:
                    for line in source:
                        self.events.append(json.loads(line))
        except Exception:
            self.lock.close()
            raise
        return self

    def __exit__(self, *args):
        self.lock.close()

    def append(self, kind, **values):
        event = dict(seq=len(self.events)+1, at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                     kind=kind, **values)
        with self.path.open('a') as out:
            out.write(json.dumps(event, ensure_ascii=True)+'\n')
            out.flush()
            os.fsync(out.fileno())
        self.events.append(event)
        return event

    def reserve(self, amount, daily_limit):
        day = datetime.datetime.now(datetime.timezone.utc).date().isoformat()
        spent = sum(e['amount'] for e in self.events if e['kind']=='budget_reserved' and e['day']==day)
        if spent + amount > daily_limit + 1e-9:
            raise RuntimeError('Iris daily model budget exhausted. Work remains queued.')
        self.append('budget_reserved', day=day, amount=amount)

    def latest(self, kind, **match):
        return next((e for e in reversed(self.events) if e['kind']==kind and
                     all(e.get(k)==v for k,v in match.items())), None)
