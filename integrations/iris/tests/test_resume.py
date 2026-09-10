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
