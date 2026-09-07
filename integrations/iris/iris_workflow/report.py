"""Human reports are derived from recorded events and distinguish queued work."""
import json
import os
import tempfile


def write(home, value):
    lines=['# Iris report', '', 'Trigger: '+value['trigger'],
           'Mode: '+('publish' if value['publish'] else 'draft'), '']
    for row in value['repositories']:
        lines.append('## '+row['repo'])
        lines.append(row['status'])
        if row.get('scope'):
            lines.append('Scope: '+row['scope']+'. '+', '.join(row.get('files_requested',[])))
        for item in row.get('issues',[]):
            lines.append('* '+item['title']+' '+item.get('url',item.get('draft','')))
        lines.append('')
    for suffix,text in [('json',json.dumps(value,indent=2)+'\n'),('md','\n'.join(lines))]:
        with tempfile.NamedTemporaryFile('w',dir=home,delete=False) as out:
            out.write(text)
            out.flush()
            os.fsync(out.fileno())
        os.replace(out.name,home/('latest-report.'+suffix))
        if value['trigger']=='manual':
            with tempfile.NamedTemporaryFile('w',dir=home,delete=False) as manual:
                manual.write(text)
            os.replace(manual.name,home/('manual-report.'+suffix))

    from .status import write as overview
    overview(home)


def reviewed_outputs(plans,ledger,repo):
    results=[]
    for plan in plans:
        marker='<!-- aos-iris:'+plan['marker']+' -->'
        published=ledger.latest('published',repo=repo,marker=marker)
        duplicate=ledger.latest('duplicate_skipped',repo=repo,marker=marker)
        draft=ledger.latest('draft_saved',repo=repo,marker=marker)
        if published or duplicate:
            results.append(dict(title='Already reported: '+plan['title'],
                                url=(published or duplicate)['url'],existing=True))
        elif draft:
            results.append(dict(title=plan['title'],draft=draft['path'],existing=True))
    return results
