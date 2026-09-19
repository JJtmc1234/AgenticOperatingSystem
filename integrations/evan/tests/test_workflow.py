import datetime
import unittest
from unittest.mock import patch
from iris_workflow.ledger import Ledger
from evan_workflow import workflow
from fixtures import Fixture, REPO

GREEN=[dict(code=0,output='OK',argv=['test'])]
RED=[dict(code=1,output='AssertionError: 2 != 5',argv=['test'])]


class WorkflowTests(Fixture,unittest.TestCase):
    def test_empty_queue_spends_no_model_calls_or_source_fetches(self):
        self.github.items=[]
        result=self.execute()
        self.assertIn('Idle',result['rows'][0]['status'])
        self.assertEqual(self.models[0].calls,[])

    def test_test_reports_and_unconfirmed_markers_are_not_actionable(self):
        self.github.items[0]['title']='[Iris validation] Addition'
        self.assertIn('Idle',self.execute()['rows'][0]['status'])
        self.github.items[0]['title']='Addition'
        self.github.items[0]['url']='https://github.com/'+REPO+'/issues/100'
        self.assertIn('Idle',self.execute()['rows'][0]['status'])

    def test_prepare_then_restart_does_not_repeat_models_or_commit(self):
        with patch('evan_workflow.sandbox.run',side_effect=[GREEN,RED,GREEN]):
            result=self.execute()
        self.assertIn('Prepared.',result['rows'][0]['status'])
        self.assertEqual(len(self.models[0].calls),3)
        self.assertIn('Prepared.',self.execute()['rows'][0]['status'])
        self.assertEqual(self.models[1].calls,[])
        self.assertEqual(len([e for e in self.events() if e['kind']=='prepared']),1)
        self.assertEqual(self.github.creates,0)

    def test_failed_baseline_is_blocked_and_retries_only_after_two_hours(self):
        with patch('evan_workflow.sandbox.run',return_value=RED) as runner:
            self.assertIn('Existing tests fail',self.execute()['rows'][0]['status'])
            self.assertIn('Waiting for two hour',self.execute(trigger='poll')['rows'][0]['status'])
            now=datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(hours=2,seconds=1)
            self.execute(trigger='poll',now=now)
            self.assertEqual(runner.call_count,2)
        self.assertEqual(self.models[0].calls,[])

    def test_new_issue_bypasses_previous_issue_retry_interval(self):
        with patch('evan_workflow.sandbox.run',return_value=RED) as runner:
            self.execute()
            self.github.items[0]['body']+=' New evidence'
            self.execute(trigger='poll')
            self.assertEqual(runner.call_count,2)

    def test_no_reproduction_never_creates_a_commit(self):
        with patch('evan_workflow.sandbox.run',return_value=GREEN):
            self.assertIn('did not reproduce',self.execute()['rows'][0]['status'])
        self.assertFalse(any(e['kind']=='prepared' for e in self.events()))

    def test_failed_fix_never_prepares_publication(self):
        with patch('evan_workflow.sandbox.run',side_effect=[GREEN,RED,RED]):
            self.assertIn('failed verification',self.execute()['rows'][0]['status'])
        self.assertFalse(any(e['kind']=='prepared' for e in self.events()))

    def test_changed_issue_during_repair_is_not_publishable(self):
        original=workflow.repair.prepare
        def changed(*args):
            result=original(*args)
            self.github.items[0]['state']='closed'
            return result
        with patch('evan_workflow.sandbox.run',side_effect=[GREEN,RED,GREEN]), patch.object(workflow.repair,'prepare',side_effect=changed):
            self.assertIn('Issue changed',self.execute()['rows'][0]['status'])
        self.assertFalse(any(e['kind']=='prepared' for e in self.events()))

    def test_overlapping_run_is_refused(self):
        with Ledger(self.home):
            with self.assertRaisesRegex(RuntimeError,'kernel lock'):
                self.execute()

    def test_interrupted_attempt_requires_explicit_retry(self):
        with patch('evan_workflow.repair.prepare',side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                self.execute()
        self.assertIn('Interrupted',self.execute()['rows'][0]['status'])
        with patch('evan_workflow.sandbox.run',return_value=RED):
            self.assertIn('Existing tests fail',self.execute(retry=True)['rows'][0]['status'])

    def test_unauthorized_repository_is_refused(self):
        with self.assertRaisesRegex(ValueError,'policy'):
            self.execute(selected='JJtmc1234/other')

    def test_new_commit_triggers_without_waiting_for_retry_interval(self):
        from evan_workflow.source import git
        with patch('evan_workflow.sandbox.run',return_value=RED) as runner:
            self.execute(trigger='poll')
            (self.repo/'sample.py').write_text('def add(a,b):\n    return a-b+0\n')
            git(self.repo,'add','sample.py')
            git(self.repo,'commit','-m','Changed source')
            self.execute(trigger='poll')
            self.assertEqual(runner.call_count,2)

    def test_priorities_apply_across_repositories(self):
        import json
        from fixtures import POLICY
        other='JJtmc1234/other'
        self.settings['repositories'][other]=json.loads(json.dumps(POLICY))
        original=self.github.items[0]
        urgent=original | dict(number=2,url='https://github.com/'+other+'/issues/2',
                               body=original['body'].replace(REPO,other).replace('fixture','urgent'))
        with Ledger(self.iris) as ledger:
            ledger.append('batch_reviewed',plans=[dict(marker='fixture',priority=1),dict(marker='urgent',priority=0)])
            ledger.append('published',repo=other,url=urgent['url'],marker='<!-- aos-iris:urgent -->')
        with patch.object(self.github,'issues',side_effect=lambda repo:[urgent] if repo==other else [original]), patch('evan_workflow.sandbox.run',return_value=RED):
            rows=self.execute()['rows']
        self.assertEqual(rows[0]['repo'],other)
        self.assertIn('Run issue limit',rows[1]['status'])
