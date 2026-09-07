import unittest
from iris_workflow.findings import issue_plans

class FindingsTests(unittest.TestCase):
    def test_issue_leads_with_user_visible_problem_and_labels_unexecuted_test(self):
        finding=dict(title='A message appears twice',severity='minor',kind='bug',path='page.js',line=1,
                     impact='One sent message appears twice after a delayed refresh.',
                     mechanism='Two refreshes return the same message and both append it.',
                     excerpt='show(message)',validation='Delay a response and check that one message appears.')
        body=issue_plans('owner/repo','a'*40,[finding])[0]['body']
        self.assertTrue(body.startswith('## What happens\n\n'+finding['impact']))
        self.assertLess(body.index(finding['impact']),body.index('Source evidence'))
        self.assertIn('Runtime reproduction has not been performed.',body)
