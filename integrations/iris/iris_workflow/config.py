"""Operator configuration. A model cannot alter publication scope or budgets."""
import json
import math
import re

DEFAULTS = dict(owner='JJtmc1234', repositories=[], publish=False, model='sonnet',
                max_call_usd=0.50, max_run_usd=1.50, max_daily_usd=5.0,
                max_batches=1, max_issues=3, timeout=240, scan_interval=3600)


def load(home):
    path = home / 'config.json'
    value = json.loads(path.read_text()) if path.exists() else {}
    if not isinstance(value, dict) or set(value)-set(DEFAULTS):
        raise ValueError('Unknown Iris configuration keys')
    result = DEFAULTS | value
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}', result['owner']):
        raise ValueError('Invalid GitHub owner')
    if not isinstance(result['publish'], bool):
        raise ValueError('publish must be true or false')
    if not isinstance(result['repositories'], list) or any(
        not isinstance(repo, str) or not re.fullmatch(re.escape(result['owner'])+r'/[A-Za-z0-9_.-]+',repo)
        for repo in result['repositories']):
        raise ValueError('Repositories must belong to the configured owner')
    for key in ['max_call_usd','max_run_usd','max_daily_usd','max_batches','max_issues','timeout','scan_interval']:
        n=result[key]
        if isinstance(n,bool) or not isinstance(n,(float,int)) or not math.isfinite(n) or n<=0:
            raise ValueError(key+' must be a positive finite number')
    for key in ['max_batches','max_issues','timeout','scan_interval']:
        if not isinstance(result[key],int):
            raise ValueError(key+' must be an integer')
    if result['max_call_usd']*3 > result['max_run_usd']:
        raise ValueError('Run budget must cover two investigators and one reviewer')
    if not isinstance(result['model'],str) or not result['model'].strip():
        raise ValueError('model must be named')
    return result
