import json
import subprocess
import unittest
from unittest.mock import patch
from iris_workflow.config import DEFAULTS
from iris_workflow.model import Model
from iris_workflow.ledger import Ledger
from iris_workflow.status import render
import test_workflow as fixtures
from test_workflow import FakeModel, finding, REPO


class ManualTests(unittest.TestCase):
    setUp=fixtures.WorkflowTests.setUp
    execute=fixtures.WorkflowTests.execute
    model_with_findings=fixtures.WorkflowTests.model_with_findings

    def test_specific_issue_checks_only_selected_committed_source_and_reuses_draft(self):
        self.config['publish']=False
        model=self.model_with_findings([finding()])
        result=self.execute(model,selected=REPO,request='Report the zero division failure',
                            specific=True,paths=['sample.py'])
        row=result['repositories'][0]
        self.assertEqual(row['files_requested'],['sample.py'])
        self.assertEqual(len(row['issues']),1)
        self.assertIn('Review only the problem or behavior JJ requested',model.prompts[0])
        repeated=self.execute(model,selected=REPO,request='Report the zero division failure',
                              specific=True,paths=['sample.py'])
        self.assertEqual(repeated['repositories'][0]['issues'][0]['draft'],row['issues'][0]['draft'])
        self.assertEqual(len(model.calls),3)
        self.assertEqual(self.github.published,[])

    def test_explicit_enhancement_is_labelled_a_request(self):
        model=self.model_with_findings([finding() | dict(kind='request')])
        self.config['publish']=False
        result=self.execute(model,selected=REPO,request='Request a safe division result type',specific=True)
        from pathlib import Path
        draft=Path(result['repositories'][0]['issues'][0]['draft']).read_text()
        self.assertIn('Requested enhancement from JJ',draft)
        self.assertIn('Acceptance criteria',draft)

    def test_general_review_cannot_invent_requested_enhancements(self):
        result=self.execute(FakeModel([dict(findings=[finding() | dict(kind='request')])]))
        self.assertIn('Failed:',result['repositories'][0]['status'])
        self.assertEqual(self.github.published,[])

    def test_missing_or_uncommitted_selected_source_fails_without_model_calls(self):
        (self.repo/'uncommitted.py').write_text('answer = 42\n')
        for path in ['../sample.py','missing.py','uncommitted.py']:
            model=FakeModel([])
            result=self.execute(model,selected=REPO,paths=[path])
            self.assertIn('Failed:',result['repositories'][0]['status'])
            self.assertEqual(model.calls,[])

    def test_discovery_failure_finishes_status_and_preserves_failure(self):
        with patch.object(self.github,'list_repositories',side_effect=RuntimeError('GitHub unavailable')):
            with self.assertRaisesRegex(RuntimeError,'GitHub unavailable'):
                self.execute(FakeModel([]))
        text=render(self.home)
        self.assertIn('Latest investigation failed: GitHub unavailable',text)
        self.assertNotIn('in progress',text)
        self.assertEqual(json.loads((self.home/'events.jsonl').read_text().splitlines()[-1])['kind'],'run_failed')

    def test_daily_budget_reports_actual_reason_without_calling_model(self):
        def factory(config,ledger):
            ledger.reserve(4.5,5)
            return Model(config,ledger)
        from iris_workflow.workflow import run
        result=run(self.home,self.config,selected=REPO,github=self.github,
                   snapshot=lambda *args:self.repo,model_factory=factory)
        row=result['repositories'][0]
        self.assertIn('$4.50 reserved of $5.00',row['status'])
        self.assertIn('Next review needs $1.50',row['status'])
        self.assertNotIn('No new verified findings',row['status'])
        self.assertIn('00:00 UTC',render(self.home))

    def test_no_findings_is_explicit_and_manual_report_survives_poll(self):
        self.execute(FakeModel([dict(findings=[])]*2))
        self.assertTrue((self.home/'manual-report.md').exists(),'Manual results must survive timer reports')
        manual=(self.home/'manual-report.md').read_text()
        self.assertIn('No new verified findings',manual)
        self.execute(FakeModel([]),trigger='poll')
        self.assertEqual((self.home/'manual-report.md').read_text(),manual)
        self.assertEqual(self.github.published,[])

    def test_explicit_scope_refuses_other_owned_repository_before_snapshot(self):
        self.config['repositories']=['JJtmc1234/allowed']
        with self.assertRaisesRegex(ValueError,'outside'):
            self.execute(FakeModel([]),selected=REPO)

    def test_file_selection_requires_repository(self):
        with self.assertRaisesRegex(ValueError,'requires one'):
            self.execute(FakeModel([]),paths=['sample.py'])

    def test_empty_specific_request_is_refused(self):
        with self.assertRaisesRegex(ValueError,'nonempty'):
            self.execute(FakeModel([]),selected=REPO,specific=True,request=' ')

    def test_existing_closed_finding_reports_link_without_new_issue(self):
        self.github.published=[dict(number=7,title=finding()['title'],body='Previously fixed',
                                    state='closed',url='https://github.com/'+REPO+'/issues/7')]
        model=FakeModel([dict(findings=[finding()]),dict(findings=[])])
        result=self.execute(model,selected=REPO,request='Check division')
        row=result['repositories'][0]
        self.assertTrue(row['issues'][0]['existing'])
        self.assertTrue(row['issues'][0]['url'].endswith('/7'))
        self.assertEqual(len(self.github.published),1)
        self.assertEqual(len(model.calls),2)

    def test_issue_cli_routes_request_paths_and_draft_policy(self):
        import contextlib,io
        from iris_workflow.__main__ import main
        self.home.mkdir()
        (self.home/'config.json').write_text(json.dumps(self.config))
        (self.home/'latest-report.md').write_text('Draft report')
        with patch('sys.argv',['aos-iris','--home',str(self.home),'issue','--repo',REPO,
                              '--request','Report division failure','--path','sample.py','--draft']), \
             patch('iris_workflow.workflow.run',return_value={'repositories':[]}) as runner, \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(main(),0)
        self.assertFalse(runner.call_args.args[1]['publish'])
        self.assertEqual(runner.call_args.kwargs['paths'],['sample.py'])
        self.assertTrue(runner.call_args.kwargs['specific'])
