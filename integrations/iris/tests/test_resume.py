"""Completed investigations must keep their evidence across later requests."""
import unittest
import test_workflow as fixtures
from test_workflow import FakeModel, finding, REPO


class ResumeTests(unittest.TestCase):
    setUp = fixtures.WorkflowTests.setUp
    execute = fixtures.WorkflowTests.execute

    def check_duplicate_resume(self, request):
        url = 'https://github.com/' + REPO + '/issues/7'
        self.github.published = [dict(number=7, title=finding()['title'],
                                     body='Previously fixed', state='closed', url=url)]
        first = self.execute(FakeModel([dict(findings=[finding()]), dict(findings=[])]),
                             selected=REPO, request=request)
        self.assertEqual(first['repositories'][0]['issues'][0]['url'], url)
        # A fresh runner must recover the link from disk, without another investigator.
        repeated = self.execute(FakeModel([]), selected=REPO, request=request)
        self.assertEqual(repeated['repositories'][0]['issues'][0]['url'], url)
        self.assertNotIn('No new verified findings', repeated['repositories'][0]['status'])
        self.assertIn(url, (self.home / 'manual-report.md').read_text())
        self.assertEqual(len(self.github.published), 1)

    def test_repeated_specific_review_keeps_closed_duplicate_link(self):
        self.check_duplicate_resume('Check division')

    def test_repeated_general_review_keeps_closed_duplicate_link(self):
        self.check_duplicate_resume('')

    def test_missing_configured_repository_is_a_failure_not_silently_omitted(self):
        missing = 'JJtmc1234/unavailable'
        self.config['repositories'] = [REPO, missing]
        result = self.execute(FakeModel([dict(findings=[]), dict(findings=[])]))
        rows = {row['repo']: row for row in result['repositories']}
        self.assertIn(missing, rows)
        self.assertTrue(rows[missing]['status'].startswith('Failed:'))
        self.assertIn('Complete.', rows[REPO]['status'])
        self.assertIn(missing, (self.home / 'manual-report.md').read_text())

    def test_finished_holder_releases_kernel_lock_without_deleting_file(self):
        from iris_workflow.ledger import Ledger
        with Ledger(self.home):
            with self.assertRaisesRegex(RuntimeError, 'kernel lock is held'):
                with Ledger(self.home):
                    self.fail('Concurrent investigation was permitted')
        self.assertTrue((self.home/'run.lock').exists())
        with Ledger(self.home) as ledger:
            ledger.append('lock_reacquired')
        self.assertTrue((self.home/'run.lock').exists())
