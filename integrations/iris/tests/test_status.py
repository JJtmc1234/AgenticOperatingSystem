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

    def test_published_draft_is_not_reported_as_unpublished_with_legacy_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            home=Path(directory)
            events=[dict(kind='draft_saved',repo='owner/repo',marker='abc',path='/draft.md'),
                    dict(kind='published',repo='owner/repo',marker='<!-- aos-iris:abc -->',url='https://github.com/owner/repo/issues/1')]
            (home/'events.jsonl').write_text(''.join(json.dumps(dict(e,seq=i))+'\n' for i,e in enumerate(events)))
            text=render(home)
            self.assertNotIn('/draft.md',text)
            self.assertIn('https://github.com/owner/repo/issues/1',text)
            self.assertIn('Budget limits queue new reviews',text)

    def test_revision_checks_are_not_reported_as_completed_source_reviews(self):
        with tempfile.TemporaryDirectory() as directory:
            home=Path(directory)
            rows=[{'status':'Inspected 1/4 source batches. Remaining work queued.'},
                  {'status':'Waiting for hourly check or feature commit'},
                  {'status':'Unchanged committed revision, already inspected'},
                  {'status':'Failed: GitHub unavailable'}]
            event=dict(seq=1,kind='run_finished',report={'trigger':'poll','repositories':rows})
            (home/'events.jsonl').write_text(json.dumps(event)+'\n')
            text=render(home)
            self.assertIn('1 complete, 1 queued, 1 waiting, 1 failed',text)
            self.assertNotIn('4 repositories checked,',text)
