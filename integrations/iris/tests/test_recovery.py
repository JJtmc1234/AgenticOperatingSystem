"""Restart safety with real journals and committed repository fixtures."""
import unittest
import datetime
from unittest.mock import patch

import test_workflow as fixtures
from test_workflow import FakeModel, REPO, finding
from iris_workflow.ledger import Ledger


class RecoveryTests(unittest.TestCase):
    setUp = fixtures.WorkflowTests.setUp
    execute = fixtures.WorkflowTests.execute
    model_with_findings = fixtures.WorkflowTests.model_with_findings

    def test_uncertain_write_is_not_repeated_after_restart_or_force(self):
        model = self.model_with_findings([finding()])
        with patch.object(self.github, 'create_issue', side_effect=RuntimeError('Response lost')) as create:
            first = self.execute(model)
            self.assertIn('Failed:', first['repositories'][0]['status'])
            second = self.execute(FakeModel([]))
            self.assertIn('uncertain', second['repositories'][0]['status'])
            self.execute(FakeModel([]), force=True)
            self.assertEqual(create.call_count, 1)

    def test_delayed_issue_visibility_reconciles_saved_publication(self):
        model = self.model_with_findings([finding()])
        create = self.github.create_issue

        def lost_response(*args):
            create(*args)
            raise RuntimeError('Response lost')

        with patch.object(self.github, 'create_issue', side_effect=lost_response):
            self.execute(model)
        result = self.execute(FakeModel([]))
        self.assertIn('Complete.', result['repositories'][0]['status'])
        self.assertEqual(len(self.github.published), 1)
        with Ledger(self.home) as ledger:
            event = ledger.latest('published', repo=REPO)
        self.assertIsNotNone(event)
        self.assertEqual(event['url'], self.github.published[0]['url'])

    def test_torn_final_append_recovers_without_losing_completed_review(self):
        self.execute(FakeModel([dict(findings=[])] * 2))
        path = self.home / 'events.jsonl'
        original = path.read_bytes()
        with path.open('ab') as out:
            out.write(b'{"seq":999,"kind":"run_')
        result = self.execute(FakeModel([]))
        self.assertIn('already inspected', result['repositories'][0]['status'])
        self.assertTrue(path.read_bytes().startswith(original))
        with Ledger(self.home) as ledger:
            self.assertEqual([e['seq'] for e in ledger.events], list(range(1, len(ledger.events)+1)))

    def test_focused_manual_review_does_not_delay_general_poll(self):
        self.execute(FakeModel([dict(findings=[])] * 2), selected=REPO, request='Check division', paths=['sample.py'])
        model = FakeModel([dict(findings=[])] * 2)
        result = self.execute(model, trigger='poll')
        self.assertIn('Complete.', result['repositories'][0]['status'])
        self.assertEqual(len(model.calls), 2)

    def test_no_findings_still_reads_related_closed_issue_context(self):
        self.github.published = [dict(number=7, title='Prior input validation', body='sample.py prior fix',
                                     state='closed', url='https://github.com/'+REPO+'/issues/7')]
        model = FakeModel([dict(findings=[])] * 2)
        with patch.object(self.github, 'comments', return_value=[dict(body='Fix preserves zero handling')]) as comments:
            self.execute(model)
        comments.assert_called_once_with(REPO, 7)
        self.assertIn('Fix preserves zero handling', model.prompts[0])
        self.assertIn('sample.py prior fix', model.prompts[0])

    def test_hourly_poll_resumes_queued_batches_and_skips_finished_revision(self):
        from test_scheduling import SchedulingTests
        SchedulingTests.add_batches(self)
        first=self.execute(FakeModel([dict(findings=[])] * 2), trigger='poll')
        self.assertIn('Remaining work queued.', first['repositories'][0]['status'])
        waiting=self.execute(FakeModel([]), trigger='poll')
        self.assertIn('Waiting for hourly', waiting['repositories'][0]['status'])
        real_datetime=datetime.datetime
        now=real_datetime.now(datetime.timezone.utc)+datetime.timedelta(hours=1,seconds=1)
        with patch('iris_workflow.workflow.datetime.datetime') as clock:
            clock.now.return_value=now
            clock.fromisoformat=real_datetime.fromisoformat
            completed=self.execute(FakeModel([dict(findings=[])] * 2), trigger='poll')
        self.assertIn('Complete.', completed['repositories'][0]['status'])
        repeated=self.execute(FakeModel([]), trigger='poll')
        self.assertIn('already inspected', repeated['repositories'][0]['status'])

    def commit(self):
        from test_scheduling import SchedulingTests
        SchedulingTests.commit(self)

    def test_terminated_process_releases_lock_and_preserves_committed_events(self):
        import subprocess
        import sys
        process=subprocess.Popen([sys.executable,'-u','-c',
            'import sys,time; from iris_workflow.ledger import Ledger; '
            'l=Ledger(sys.argv[1]); l.__enter__(); l.append("held"); print("ready"); time.sleep(60)',
            str(self.home)],stdout=subprocess.PIPE,text=True)
        try:
            self.assertEqual(process.stdout.readline().strip(),'ready')
            with self.assertRaisesRegex(RuntimeError,'kernel lock is held'):
                with Ledger(self.home):
                    self.fail('Overlapping scan acquired lock')
        finally:
            process.terminate()
            process.wait(timeout=5)
            process.stdout.close()
        with Ledger(self.home) as ledger:
            self.assertIsNotNone(ledger.latest('held'))

    def test_complete_last_record_without_newline_preserves_publication_intent(self):
        with Ledger(self.home) as ledger:
            ledger.append('publication_requested',repo=REPO,marker='<!-- aos-iris:lost -->',title='Example')
        path=self.home/'events.jsonl'
        path.write_bytes(path.read_bytes().rstrip(b'\n'))
        result=self.execute(FakeModel([]))
        self.assertIn('uncertain',result['repositories'][0]['status'])
        with Ledger(self.home) as ledger:
            self.assertIsNotNone(ledger.latest('publication_requested'))

    def test_complete_corruption_after_valid_prefix_is_never_truncated(self):
        with Ledger(self.home) as ledger:
            ledger.append('healthy')
        path=self.home/'events.jsonl'
        with path.open('ab') as out:
            out.write(b'{broken}\n')
        corrupt=path.read_bytes()
        with self.assertRaisesRegex(ValueError,'journal is corrupt'):
            with Ledger(self.home):
                self.fail('Corrupt committed record was accepted')
        self.assertEqual(path.read_bytes(),corrupt)

    def test_unrelated_human_issue_cannot_clear_uncertain_publication(self):
        with Ledger(self.home) as ledger:
            ledger.append('publication_requested',repo=REPO,marker='<!-- aos-iris:lost -->',title=finding()['title'])
        self.github.published=[dict(number=7,title=finding()['title'],body='A human report with the same title',
                                    state='closed',url='https://github.com/'+REPO+'/issues/7')]
        result=self.execute(FakeModel([]))
        self.assertIn('uncertain',result['repositories'][0]['status'])
        with Ledger(self.home) as ledger:
            self.assertIsNone(ledger.latest('published'))
