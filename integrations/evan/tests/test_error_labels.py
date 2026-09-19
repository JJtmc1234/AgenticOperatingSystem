import subprocess
import unittest
from unittest.mock import patch
from fixtures import Fixture
from iris_workflow.ledger import Ledger
from evan_workflow.model import Model
from evan_workflow.patches import PATCH


class ErrorLabelTests(Fixture,unittest.TestCase):
    def test_evan_model_failure_names_evan_and_keeps_sensitive_details_private(self):
        with Ledger(self.home) as ledger:
            result=subprocess.CompletedProcess([],1,'','ghp_'+'a'*32)
            with patch('iris_workflow.model.subprocess.run',return_value=result):
                with self.assertRaisesRegex(RuntimeError,'Evan.*Sensitive diagnostic excluded'):
                    Model(self.settings,ledger).ask('check/fix','private input',PATCH)

    def test_evan_timeout_names_evan_without_echoing_the_prompt(self):
        with Ledger(self.home) as ledger:
            with patch('iris_workflow.model.subprocess.run',side_effect=subprocess.TimeoutExpired('private command',240)):
                with self.assertRaisesRegex(RuntimeError,'Evan.*timed out') as result:
                    Model(self.settings,ledger).ask('check/fix','private input',PATCH)
            self.assertNotIn('private',str(result.exception))

    def test_overlapping_evan_run_does_not_claim_iris_is_running(self):
        with Ledger(self.home):
            with self.assertRaisesRegex(RuntimeError,'Evan already has a running'):
                self.execute()
