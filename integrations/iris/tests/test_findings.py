import unittest
from iris_workflow.findings import issue_plans

class FindingsTests(unittest.TestCase):
    def test_unrelated_issue_history_does_not_fill_the_review_context(self):
        from iris_workflow.findings import related
        findings=[dict(title='',path='new/component.py')]
        self.assertEqual(related(findings,[dict(title='Exam homework',body='Answer the questions')]),[])
        relevant=dict(title='Earlier fix',body='Changed new/component.py')
        self.assertEqual(related(findings,[relevant]),[relevant])

    def test_unique_exact_source_excerpt_repairs_only_its_line_number(self):
        from iris_workflow.findings import validated
        from test_workflow import finding
        from types import SimpleNamespace
        repository=SimpleNamespace(source=lambda path:'def divide(value):\n    return 10 / value\n')
        result=validated(dict(findings=[finding() | dict(line=1)]),repository)
        self.assertEqual(result[0]['line'],2)
        self.assertEqual(result[0]['excerpt'],'    return 10 / value')

    def test_misnumbered_ambiguous_excerpt_is_rejected(self):
        from iris_workflow.findings import validated
        from test_workflow import finding
        from types import SimpleNamespace
        repository=SimpleNamespace(source=lambda path:'header\n    return 10 / value\n    return 10 / value\n')
        with self.assertRaisesRegex(ValueError,'excerpt does not match'):
            validated(dict(findings=[finding() | dict(line=1)]),repository)

    def test_issue_leads_with_user_visible_problem_and_labels_unexecuted_test(self):
        finding=dict(title='A message appears twice',severity='minor',kind='bug',path='page.js',line=1,
                     impact='One sent message appears twice after a delayed refresh.',
                     mechanism='Two refreshes return the same message and both append it.',
                     excerpt='show(message)',validation='Delay a response and check that one message appears.')
        body=issue_plans('owner/repo','a'*40,[finding])[0]['body']
        self.assertTrue(body.startswith('## What happens\n\n'+finding['impact']))
        self.assertLess(body.index(finding['impact']),body.index('Source evidence'))
        self.assertIn('Runtime reproduction has not been performed.',body)

    def test_minor_findings_group_by_file_without_burying_major_findings(self):
        from test_workflow import finding
        first=finding() | dict(severity='minor',path='carl/chat.py')
        second=first | dict(title='A related failure',mechanism='A second mechanism')
        unrelated=first | dict(path='carl/storage.py',mechanism='Different operation')
        major=first | dict(severity='major',path='carl/guard.py',mechanism='Destructive access')
        plans=issue_plans('owner/repo','a'*40,[first,second,unrelated,major])
        self.assertEqual(len(plans),3)
        self.assertEqual(plans[0]['priority'],0)
        grouped=next(p for p in plans if '2 related' in p['title'])
        self.assertIn('carl/chat.py',grouped['title'])
        self.assertNotIn('storage.py',grouped['body'])

    def test_suggested_direction_is_distinct_from_acceptance_criteria(self):
        from test_workflow import finding
        item=finding() | dict(direction='Return a typed error for zero before dividing.')
        body=issue_plans('owner/repo','a'*40,[item])[0]['body']
        self.assertIn('**Suggested direction**\n\n'+item['direction'],body)
        self.assertIn('**Acceptance criteria and proposed test**',body)
