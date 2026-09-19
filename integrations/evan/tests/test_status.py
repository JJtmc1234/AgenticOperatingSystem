import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from iris_workflow.ledger import Ledger
from evan_workflow.__main__ import main


class StatusTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.home=Path(temp.name)/'evan'

    def status(self):
        output=io.StringIO()
        with patch('sys.argv',['aos-evan','--home',str(self.home),'status']), contextlib.redirect_stdout(output):
            self.assertEqual(main(),0)
        return json.loads(output.getvalue())

    def old_report(self):
        (self.home/'latest-report.json').write_text(json.dumps(dict(status='Finished',rows=[])))

    def test_status_does_not_show_old_success_while_a_new_run_holds_the_lock(self):
        with Ledger(self.home) as ledger:
            self.old_report()
            ledger.append('run_started',publish=False)
            ledger.append('investigator_started',identity='key/regression',parent='evan',tools=[])
            result=self.status()
            self.assertEqual(result['status'],'Running')
            self.assertEqual(result['phase'],'regression')

    def test_status_reports_an_interrupted_run_after_the_lock_is_released(self):
        with Ledger(self.home) as ledger:
            self.old_report()
            ledger.append('run_started',publish=False)
        self.assertEqual(self.status()['status'],'Interrupted')

    def test_status_recovers_completed_report_from_journal_if_cache_is_stale(self):
        report=dict(status='Finished',rows=[dict(repo='example/repo',status='Prepared. Publication disabled.')])
        with Ledger(self.home) as ledger:
            self.old_report()
            ledger.append('run_started',publish=False)
            ledger.append('run_finished',report=report)
        self.assertEqual(self.status(),report)

    def test_status_of_unused_home_does_not_create_state(self):
        self.assertEqual(self.status()['status'],'Not run')
        self.assertFalse(self.home.exists())
