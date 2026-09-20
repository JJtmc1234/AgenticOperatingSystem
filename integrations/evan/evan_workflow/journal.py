"""Bounded, read-only journal observations shared by status and review."""
import json


def read(home, *, active=False):
    path=home/'events.jsonl'
    if not path.exists():
        return []
    with path.open('rb') as source:
        data=source.read(16*1024*1024+1)
    if len(data)>16*1024*1024:
        raise ValueError('Evan journal exceeds the reader limit')
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
    return events
