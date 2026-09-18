"""Independent review can lower severity without weakening source verification."""
import unittest
import test_workflow as fixtures
from test_workflow import FakeModel, finding
from iris_workflow.ledger import Ledger


class ReviewTests(unittest.TestCase):
    setUp = fixtures.WorkflowTests.setUp
    execute = fixtures.WorkflowTests.execute

    def test_reviewer_can_downgrade_a_real_finding_without_discarding_it(self):
        result=self.execute(FakeModel([dict(findings=[finding()]),dict(findings=[]),
                                       dict(accepted=[0],minor=[0])]))
        self.assertIn('Complete.',result['repositories'][0]['status'])
        self.assertEqual(len(self.github.published),1)
        with Ledger(self.home) as ledger:
            self.assertEqual(ledger.latest('batch_reviewed')['plans'][0]['priority'],1)

    def test_reviewer_cannot_downgrade_an_unaccepted_index(self):
        result=self.execute(FakeModel([dict(findings=[finding()]),dict(findings=[]),
                                       dict(accepted=[],minor=[0])]))
        self.assertIn('Failed:',result['repositories'][0]['status'])
        self.assertEqual(self.github.published,[])

