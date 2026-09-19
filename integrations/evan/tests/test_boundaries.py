import unittest
from unittest.mock import patch
from iris_workflow.ledger import Ledger
from evan_workflow import config, issues, patches, publication
from fixtures import Fixture, REPO, POLICY
from test_workflow import GREEN, RED


class BoundaryTests(Fixture,unittest.TestCase):
    def test_path_escape_and_unassigned_file_are_refused(self):
        for path in ('../escape.py','/tmp/escape.py','.git/config','other.py'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                patches.apply({},dict(summary='test',edits=[dict(path=path,before='',after='x')]),['test_sample.py'],True)

    def test_source_and_tests_cannot_share_authority(self):
        with self.assertRaisesRegex(ValueError,'disjoint'):
            config.validate(dict(repositories={REPO:POLICY | dict(test_paths=['sample.py'])}))

    def test_invented_and_ambiguous_replacements_are_refused(self):
        for before in ('missing','a'):
            with self.assertRaises(ValueError):
                patches.apply({'sample.py':'aa'},dict(summary='test',edits=[dict(path='sample.py',before=before,after='b')]),['sample.py'])

    def test_priority_is_taken_from_confirmed_iris_plan(self):
        first=self.github.items[0]
        second=first | dict(number=2,url=first['url']+'2',body=first['body'].replace('fixture','minor'))
        known={first['url']:('<!-- aos-iris:fixture -->',0),second['url']:('<!-- aos-iris:minor -->',1)}
        result=issues.select(REPO,[second,first],known)
        self.assertEqual([i['number'] for i in result],[1,2])

    def test_uncertain_pr_is_never_recreated_even_after_restart(self):
        with patch('evan_workflow.sandbox.run',side_effect=[GREEN,RED,GREEN]):
            self.execute()
        plan=next(e['plan'] for e in self.events() if e['kind']=='prepared')
        with Ledger(self.home) as ledger:
            ledger.append('pr_requested',key=plan['key'])
        self.settings['publish']=True
        result=self.execute()
        self.assertIn('uncertain',result['rows'][0]['status'])
        self.assertEqual(self.github.creates,0)
        self.github.pulls=[dict(url='https://github.com/'+REPO+'/pull/2',headRefOid=plan['commit'],body='<!-- aos-evan:'+plan['key']+' -->')]
        result=self.execute()
        self.assertEqual(result['rows'][0]['pr'],self.github.pulls[0]['url'])
        self.assertEqual(self.github.creates,0)

    def test_changed_prepared_checkout_is_not_pushed(self):
        with patch('evan_workflow.sandbox.run',side_effect=[GREEN,RED,GREEN]):
            self.execute()
        plan=next(e['plan'] for e in self.events() if e['kind']=='prepared')
        from pathlib import Path
        (Path(plan['checkout'])/'sample.py').write_text('changed')
        with Ledger(self.home) as ledger, self.assertRaisesRegex(ValueError,'changed'):
            publication.publish(plan,ledger,self.github)
        self.assertEqual(self.github.creates,0)

    def test_model_transport_has_evan_authority_and_no_tools(self):
        import subprocess
        from evan_workflow.model import Model
        with Ledger(self.home) as ledger, patch('iris_workflow.model.subprocess.run',
                return_value=subprocess.CompletedProcess([],0,'{"structured_output":{"accepted":true,"reason":"checked"}}','')) as call:
            model=Model(self.settings,ledger)
            model.ask('fixture/review','Only the assigned issue',patches.REVIEW)
            args=call.call_args.args[0]
            self.assertEqual(args[args.index('--tools')+1],'')
            self.assertIn('managed by Evan',args[args.index('--system-prompt')+1])
            self.assertEqual(ledger.latest('investigator_started')['parent'],'evan')
            self.assertEqual(ledger.latest('budget_reserved')['amount'],0.5)

    def test_run_and_daily_model_budgets_stop_work_before_calls(self):
        from evan_workflow.model import Model
        with Ledger(self.home) as ledger:
            ledger.reserve(5,5)
            model=Model(self.settings,ledger)
            self.assertIn('Daily model budget',model.blocked_reason())
            with self.assertRaisesRegex(RuntimeError,'budget exhausted'):
                model.ask('fixture/test','prompt',patches.PATCH)
