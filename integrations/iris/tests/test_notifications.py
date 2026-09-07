import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from iris_workflow import notifications
from iris_workflow.ledger import Ledger

class NotificationTests(unittest.TestCase):
    def test_routine_poll_is_quiet_but_real_work_notifies(self):
        with tempfile.TemporaryDirectory() as directory, Ledger(directory) as ledger:
            ledger.append('run_started')
            result={'trigger':'poll','repositories':[{'status':'Waiting for hourly check'}]}
            with patch.object(notifications,'emit',return_value={'delivered':True}) as send:
                notifications.completed(ledger,result)
                send.assert_not_called()
                ledger.append('investigator_started')
                notifications.completed(ledger,result)
                send.assert_called_once()
                self.assertEqual(ledger.events[-2]['kind'],'notification_requested')

    def test_repeated_idle_failures_do_not_spam(self):
        with tempfile.TemporaryDirectory() as directory, Ledger(directory) as ledger:
            result={'trigger':'poll','repositories':[{'status':'Failed: fixture'}]}
            with patch.object(notifications,'emit',return_value={'delivered':True}) as send:
                for _ in range(2):
                    ledger.append('run_started')
                    notifications.completed(ledger,result)
                send.assert_called_once()

    def test_desktop_failure_is_nonfatal_and_arguments_are_literal(self):
        with patch.object(notifications.subprocess,'run',return_value=subprocess.CompletedProcess([],1,'','')) as run:
            self.assertFalse(notifications.emit('Iris finished','<b>$(not code)</b>')['accepted_by_service'])
            self.assertNotIn('shell',run.call_args.kwargs)
            self.assertIn('&lt;b&gt;$(not code)&lt;/b&gt;',run.call_args.args[0])
        with patch.object(notifications.subprocess,'run',side_effect=FileNotFoundError):
            self.assertFalse(notifications.emit('Iris finished','Done')['accepted_by_service'])

    def test_acceptance_does_not_claim_visibility_and_notice_is_persistent(self):
        with patch.object(notifications.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'42','')) as run:
            result=notifications.emit('Iris finished','Done')
            self.assertTrue(result['accepted_by_service'])
            self.assertIsNone(result['visible_to_user'])
            self.assertNotIn('delivered',result)
            self.assertIn('--expire-time=0',run.call_args.args[0])
            self.assertIn('--hint=boolean:resident:true',run.call_args.args[0])

    def test_codex_payload_does_not_expose_prompt_or_answer(self):
        event={'type':'agent-turn-complete','last-assistant-message':'private answer','input-messages':['private prompt']}
        with tempfile.TemporaryDirectory() as directory, patch('pathlib.Path.home',return_value=Path(directory)), \
             patch('sys.argv',['aos-notify',json.dumps(event)]), \
             patch.object(notifications,'emit',return_value={'delivered':True}) as send:
            self.assertEqual(notifications.main(),0)
            self.assertNotIn('private',str(send.call_args))
            self.assertNotIn('private',(Path(directory)/'.local/state/aos-notifications/events.jsonl').read_text())
