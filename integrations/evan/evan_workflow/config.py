"""Only operator configuration grants repository, test and publication authority."""
import json
import math
import re
from pathlib import PurePosixPath
from iris_workflow.repository import eligible

DEFAULTS = dict(owner='JJtmc1234', repositories={}, publish=False, model='sonnet',
                max_call_usd=0.50, max_run_usd=1.50, max_daily_usd=5.0,
                timeout=240, test_timeout=120, scan_interval=7200, max_issues=1)


def safe_path(path):
    return (isinstance(path,str) and path == str(PurePosixPath(path)) and
            not path.startswith(('/', '-')) and eligible(path) and
            not any(p.startswith('.') for p in PurePosixPath(path).parts))


def validate(value):
    if not isinstance(value,dict) or set(value)-set(DEFAULTS):
        raise ValueError('Unknown Evan configuration keys')
    result=DEFAULTS | value
    if not isinstance(result['owner'],str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}',result['owner']):
        raise ValueError('Invalid owner')
    if type(result['publish']) is not bool or not isinstance(result['repositories'],dict):
        raise ValueError('Invalid publication or repository configuration')
    for repo,policy in result['repositories'].items():
        if not re.fullmatch(re.escape(result['owner'])+r'/[A-Za-z0-9][A-Za-z0-9_.-]*',repo):
            raise ValueError('Repository must belong to configured owner')
        if not isinstance(policy,dict) or set(policy)!={'paths','test_paths','commands'}:
            raise ValueError('Each repository needs exact paths, test_paths and commands')
        for key in ('paths','test_paths'):
            if not isinstance(policy[key],list) or not policy[key] or not all(safe_path(p) for p in policy[key]):
                raise ValueError('Only explicit safe source and test paths are allowed')
        if set(policy['paths']) & set(policy['test_paths']):
            raise ValueError('Source and test paths must be disjoint')
        commands=policy['commands']
        if not isinstance(commands,list) or not 1<=len(commands)<=4 or any(
                not isinstance(c,list) or not c or not all(isinstance(a,str) and a and '\0' not in a for a in c)
                for c in commands):
            raise ValueError('Tests must be fixed argument arrays')
    for key in ('max_call_usd','max_run_usd','max_daily_usd','timeout','test_timeout','scan_interval','max_issues'):
        n=result[key]
        if type(n) not in (int,float) or not math.isfinite(n) or n<=0:
            raise ValueError(key+' must be positive and finite')
    for key in ('timeout','test_timeout','scan_interval','max_issues'):
        if type(result[key]) is not int:
            raise ValueError(key+' must be an integer')
    if result['max_run_usd']<3*result['max_call_usd']:
        raise ValueError('Run allowance must cover three bounded workers')
    if not isinstance(result['model'],str) or not result['model'].strip():
        raise ValueError('Model must be named')
    return result


def load(home):
    path=home/'config.json'
    return validate(json.loads(path.read_text()) if path.exists() else {})
