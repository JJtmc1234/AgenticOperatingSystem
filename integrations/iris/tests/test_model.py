import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from iris_workflow.config import DEFAULTS, load
from iris_workflow.ledger import Ledger
from iris_workflow.model import Model, FINDINGS


class ModelTests(unittest.TestCase):
    def test_tool_free_argv_and_reservation_before_process(self):
        with tempfile.TemporaryDirectory() as directory, Ledger(directory) as ledger:
            model=Model(DEFAULTS,ledger)
            def execute(argv,**kwargs):
                self.assertEqual(ledger.events[-2]['kind'],'budget_reserved')
                self.assertEqual(argv[argv.index('--tools')+1],'')
                self.assertIn('--strict-mcp-config',argv)
                self.assertNotIn('--dangerously-skip-permissions',argv)
                self.assertIn('--no-session-persistence',argv)
                self.assertEqual(kwargs['input'],'literal $(no execution)')
                return subprocess.CompletedProcess(argv,0,json.dumps({'subtype':'success','structured_output':{'findings':[]}}),'')
            with patch('iris_workflow.model.subprocess.run',side_effect=execute):
                self.assertEqual(model.ask('iris/test','literal $(no execution)',FINDINGS),{'findings':[]})

    def test_budget_and_invalid_response_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory, Ledger(directory) as ledger:
            model=Model(DEFAULTS | {'max_daily_usd':0.5},ledger)
            self.assertFalse(model.can_investigate())
            with patch('iris_workflow.model.subprocess.run',return_value=subprocess.CompletedProcess([],0,'{"result":"not structured"}','')) as process:
                with self.assertRaises(ValueError):model.ask('iris/test','test',FINDINGS)
                with self.assertRaises(RuntimeError):model.ask('iris/test','test',FINDINGS)
                self.assertEqual(process.call_count,1)

    def test_failed_process_reports_budget_reason_without_exposing_credentials(self):
        with tempfile.TemporaryDirectory() as directory, Ledger(directory) as ledger:
            model=Model(DEFAULTS,ledger)
            result=subprocess.CompletedProcess([],1,'{"subtype":"error_max_budget_usd","is_error":true}','')
            with patch('iris_workflow.model.subprocess.run',return_value=result):
                with self.assertRaisesRegex(RuntimeError,'error_max_budget_usd'):
                    model.ask('iris/test','test',FINDINGS)
            result=subprocess.CompletedProcess([],1,'','ghp_'+'a'*32)
            with patch('iris_workflow.model.subprocess.run',return_value=result):
                with self.assertRaisesRegex(RuntimeError,'Sensitive diagnostic excluded'):
                    model.ask('iris/test','test',FINDINGS)

    def test_concurrent_runs_and_corrupt_journal_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            with Ledger(directory):
                with self.assertRaises(RuntimeError):
                    with Ledger(directory):pass
            (Path(directory)/'events.jsonl').write_text('{torn')
            with self.assertRaises(ValueError):
                with Ledger(directory):pass

    def test_invalid_configuration_does_not_enable_a_run(self):
        with tempfile.TemporaryDirectory() as directory:
            home=Path(directory)
            for config in [{'publish':'yes'},{'repositories':['elsewhere/repo']},{'max_call_usd':float('nan')},{'max_batches':1.5},{'surprise':True}]:
                (home/'config.json').write_text(json.dumps(config))
                with self.assertRaises(ValueError):load(home)
