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
        for item in row.get('issues',[]):
            lines.append('* '+item['title']+' '+item.get('url',item.get('draft','')))
        lines.append('')
    for suffix,text in [('json',json.dumps(value,indent=2)+'\n'),('md','\n'.join(lines))]:
        with tempfile.NamedTemporaryFile('w',dir=home,delete=False) as out:
            out.write(text)
            out.flush()
            os.fsync(out.fileno())
        os.replace(out.name,home/('latest-report.'+suffix))

    from .status import write as overview
    overview(home)
