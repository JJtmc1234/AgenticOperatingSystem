import contextlib
import io
import json
import unittest
from unittest.mock import patch
from fixtures import Fixture, REPO
from test_workflow import GREEN, RED
from evan_workflow.review import show
from evan_workflow.__main__ import main


class ReviewTests(Fixture,unittest.TestCase):
    def prepare(self):
        with patch('evan_workflow.sandbox.run',side_effect=[GREEN,RED,GREEN]):
            self.execute()
        return next(e['plan'] for e in self.events() if e['kind']=='prepared')

    def test_review_displays_patch_evidence_and_preserves_the_journal(self):
        plan=self.prepare()
        before=(self.home/'events.jsonl').read_bytes()
        report=show(self.home,REPO,1)
        self.assertIn(plan['commit'],report)
        self.assertIn('-    return a-b',report)
        self.assertIn('+    return a+b',report)
        self.assertIn('Regression before repair: 0/1 commands passed.',report)
        self.assertIn('After repair: 1/1 commands passed.',report)
        self.assertIn('Independent review',report)
        self.assertIn('No published PR recorded',report)
        self.assertEqual((self.home/'events.jsonl').read_bytes(),before)
        self.assertEqual(self.github.creates,0)
        self.assertEqual(len(self.models[0].calls),3)

    def test_missing_review_does_not_create_state(self):
        missing=self.root/'unused'
        with self.assertRaisesRegex(ValueError,'No prepared repair'):
            show(missing,REPO,1)
        self.assertFalse(missing.exists())

    def test_changed_checkout_cannot_reuse_old_verification(self):
        from pathlib import Path
        plan=self.prepare()
        (Path(plan['checkout'])/'sample.py').write_text('unverified edit\n')
        with self.assertRaisesRegex(ValueError,'checkout changed'):
            show(self.home,REPO,1)

    def test_cli_can_review_without_runtime_configuration(self):
        self.prepare()
        output=io.StringIO()
        with patch('sys.argv',['aos-evan','--home',str(self.home),'review','--repo',REPO,'--issue','1']), contextlib.redirect_stdout(output):
            self.assertEqual(main(),0)
        self.assertIn('## Patch',output.getvalue())

    def test_review_rejects_a_journal_with_a_missing_sequence(self):
        self.prepare()
        journal=self.home/'events.jsonl'
        events=[json.loads(line) for line in journal.read_text().splitlines()]
        events.pop(0)
        journal.write_text(''.join(json.dumps(event)+'\n' for event in events))
        with self.assertRaisesRegex(ValueError,'journal is unreadable'):
            show(self.home,REPO,1)

    def test_review_ignores_only_an_incomplete_append_during_a_running_workflow(self):
        from iris_workflow.ledger import Ledger
        self.prepare()
        with Ledger(self.home) as ledger:
            ledger.append('run_started',publish=False)
            with (self.home/'events.jsonl').open('ab') as journal:
                journal.write(b'{"seq":')
            before=(self.home/'events.jsonl').read_bytes()
            self.assertIn('Independent review',show(self.home,REPO,1))
            self.assertEqual((self.home/'events.jsonl').read_bytes(),before)
        with self.assertRaisesRegex(ValueError,'journal is unreadable'):
            show(self.home,REPO,1)
