import unittest
import copy
from unittest.mock import patch
from iris_workflow.ledger import Ledger
from evan_workflow import submit
from evan_workflow.source import git
from fixtures import Fixture, REPO
from test_workflow import GREEN, RED


class SubmitTests(Fixture, unittest.TestCase):
    def prepare(self):
        with patch('evan_workflow.sandbox.run', side_effect=[GREEN, RED, GREEN]):
            self.execute()
        return next(e['plan'] for e in self.events() if e['kind']=='prepared')

    def submit(self):
        return submit.submit(self.home,self.settings,REPO,1,github=self.github,
                             fetch=lambda *a:self.repo,iris_home=self.iris)

    def test_submit_one_prepared_repair_does_not_enable_recurring_writes_or_call_models(self):
        plan=self.prepare()
        with patch('evan_workflow.publication.publish',return_value='https://github.com/'+REPO+'/pull/2') as publish:
            report=self.submit()
            publish.assert_called_once()
            self.assertEqual(publish.call_args.args[0],plan)
        self.assertEqual(report['rows'][0]['status'],'Submitted for review.')
        self.assertFalse(self.settings['publish'])
        self.assertEqual(len(self.models),1)
        self.assertEqual(len(self.models[0].calls),3)
        self.assertEqual(self.events()[-1]['kind'],'run_finished')

    def test_changed_issue_source_policy_and_checkout_prevent_publication(self):
        plan=self.prepare()
        original_body=self.github.items[0]['body']
        original_policy=copy.deepcopy(self.settings['repositories'][REPO])
        for changed in ['issue','policy']:
            with self.subTest(changed=changed), patch('evan_workflow.publication.publish') as publish:
                self.github.items[0]['body']=original_body
                self.settings['repositories'][REPO]=copy.deepcopy(original_policy)
                if changed=='issue':
                    self.github.items[0]['body']+=' new evidence'
                else:
                    self.settings['repositories'][REPO]['commands'].append(['python3','--version'])
                self.assertIn('changed',self.submit()['rows'][0]['status'])
                publish.assert_not_called()
        # A dirty prepared checkout is rejected before any remote write.
        from pathlib import Path
        (Path(plan['checkout'])/'sample.py').write_text('changed')
        with patch('evan_workflow.publication.publish') as publish:
            self.assertIn('changed',self.submit()['rows'][0]['status'])
            publish.assert_not_called()

    def test_changed_default_branch_commit_prevents_publication(self):
        self.prepare()
        (self.repo/'sample.py').write_text('def add(a,b): return a-b+0\n')
        git(self.repo,'add','sample.py')
        git(self.repo,'commit','-m','Source changed')
        with patch('evan_workflow.publication.publish') as publish:
            self.assertIn('changed',self.submit()['rows'][0]['status'])
            publish.assert_not_called()

    def test_closed_issue_cannot_be_submitted(self):
        self.prepare()
        self.github.items[0]['state']='closed'
        with patch('evan_workflow.publication.publish') as publish:
            self.assertIn('closed',self.submit()['rows'][0]['status'])
            publish.assert_not_called()

    def test_repeated_submit_uses_recorded_pr_without_another_write(self):
        plan=self.prepare()
        with Ledger(self.home) as ledger:
            ledger.append('pr_published',key=plan['key'],url='existing-pr')
        with patch('evan_workflow.publication.publish') as publish:
            self.assertEqual(self.submit()['rows'][0]['pr'],'existing-pr')
            publish.assert_not_called()

    def test_uncertain_submit_reconciles_without_creating_again(self):
        plan=self.prepare()
        with Ledger(self.home) as ledger:
            ledger.append('pr_requested',key=plan['key'])
        self.assertIn('uncertain',self.submit()['rows'][0]['status'])
        self.assertEqual(self.github.creates,0)
        self.github.pulls=[dict(url='existing-pr',headRefOid=plan['commit'],body='<!-- aos-evan:'+plan['key']+' -->')]
        self.assertEqual(self.submit()['rows'][0]['pr'],'existing-pr')
        self.assertEqual(self.github.creates,0)
