"""Structured literal replacements are checked before any file is changed."""
from iris_workflow.repository import contains_credential
from .config import safe_path

EDIT = {'type':'object','additionalProperties':False,'required':['path','before','after'],
        'properties':{k:{'type':'string','maxLength':16000} for k in ('path','before','after')}}
PATCH = {'type':'object','additionalProperties':False,'required':['summary','edits'],
         'properties':{'summary':{'type':'string','maxLength':1200},
                       'edits':{'type':'array','minItems':1,'maxItems':8,'items':EDIT}}}
REVIEW = {'type':'object','additionalProperties':False,'required':['accepted','reason'],
          'properties':{'accepted':{'type':'boolean'},'reason':{'type':'string','maxLength':1200}}}


def apply(sources, result, allowed, allow_new=False):
    if not isinstance(result,dict) or set(result)!={'summary','edits'}:
        raise ValueError('Invalid patch response')
    if not isinstance(result['summary'],str) or len(result['summary'])>1200 or contains_credential(result['summary']):
        raise ValueError('Invalid patch summary')
    edits=result['edits']
    if not isinstance(edits,list) or not 1<=len(edits)<=8:
        raise ValueError('Patch requires one to eight replacements')
    output=dict(sources)
    seen=set()
    for edit in edits:
        if not isinstance(edit,dict) or set(edit)!={'path','before','after'}:
            raise ValueError('Invalid replacement')
        path,before,after=(edit[k] for k in ('path','before','after'))
        if not safe_path(path) or path not in allowed or path in seen:
            raise ValueError('Replacement outside assigned paths or repeated file')
        if not all(isinstance(s,str) and len(s)<=16000 and not contains_credential(s) for s in (before,after)):
            raise ValueError('Replacement exceeds bounds or contains credentials')
        if before==after:
            raise ValueError('Replacement has no change')
        if path not in output:
            if not allow_new or before or not after:
                raise ValueError('Only assigned new regression files may be created')
            output[path]=after
        else:
            if not before or output[path].count(before)!=1:
                raise ValueError('Replacement must match one exact source fragment')
            output[path]=output[path].replace(before,after,1)
        if len(output[path].encode())>65536:
            raise ValueError('Patched file exceeds source bounds')
        seen.add(path)
    return output
