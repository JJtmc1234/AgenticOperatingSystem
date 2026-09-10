"""A one-off review may lower its model allowance, never raise configured limits."""
import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from iris_workflow.__main__ import main
from iris_workflow.config import DEFAULTS


class CallBudgetTests(unittest.TestCase):
    def invoke(self, amount):
        with tempfile.TemporaryDirectory() as directory:
            home=Path(directory)
            config=json.dumps(DEFAULTS)
            (home/'config.json').write_text(config)
            (home/'latest-report.md').write_text('Result')
            argv=['iris','--home',str(home),'issue','--repo','JJtmc1234/example',
                  '--request','Check division','--draft','--call-budget',amount]
            with patch('sys.argv',argv), contextlib.redirect_stdout(io.StringIO()), \
                    contextlib.redirect_stderr(io.StringIO()), \
                    patch('iris_workflow.workflow.run',return_value={'repositories':[]}) as run:
                result=main()
            self.assertEqual((home/'config.json').read_text(),config)
            return result,run

    def test_lower_budget_is_per_run_and_keeps_daily_limit(self):
        result,run=self.invoke('0.25')
        self.assertEqual(result,0)
        settings=run.call_args.args[1]
        self.assertEqual(settings['max_call_usd'],0.25)
        self.assertEqual(settings['max_daily_usd'],DEFAULTS['max_daily_usd'])
        self.assertFalse(settings['publish'])

    def test_raised_or_invalid_budget_never_starts_review(self):
        for amount in ['0.51','0','-1','nan','inf']:
            with self.subTest(amount=amount):
                result,run=self.invoke(amount)
                self.assertEqual(result,1)
                run.assert_not_called()
