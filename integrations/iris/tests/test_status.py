import json
from pathlib import Path
import tempfile
import unittest
from iris_workflow.status import render

class StatusTests(unittest.TestCase):
    def test_idle_poll_does_not_hide_published_issues_or_browser_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            home=Path(directory)
            events=[dict(kind='publication_requested',repo='owner/repo',marker='m',title='Duplicate messages'),
                    dict(kind='published',repo='owner/repo',marker='m',url='https://github.com/owner/repo/issues/45'),
                    dict(kind='browser_test_finished',stats={'expected':5,'unexpected':1},output=directory),
                    dict(kind='run_started'),
                    dict(kind='run_finished',report={'trigger':'poll','repositories':[{'status':'Waiting for hourly check'}]})]
            (home/'events.jsonl').write_text(''.join(json.dumps(dict(event,seq=i))+'\n' for i,event in enumerate(events)))
            text=render(home)
            self.assertIn('[Duplicate messages](https://github.com/owner/repo/issues/45)',text)
            self.assertIn('5 passed, 1 failed',text)
            self.assertNotIn('in progress',text)

    def test_active_run_is_visible_and_incomplete_last_append_is_tolerated(self):
        with tempfile.TemporaryDirectory() as directory:
            home=Path(directory)
            (home/'events.jsonl').write_text('{"seq":1,"kind":"run_started"}\n{"seq":')
            self.assertIn('in progress',render(home))
