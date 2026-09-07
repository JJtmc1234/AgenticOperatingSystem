import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock
from iris_workflow import browser

class BrowserTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.home=self.root/'state'
        self.suite=self.root/'.local/share/aos-iris/browser-tests'
        names=['server.mjs','playwright.config.js','package-lock.json','tests/portal.spec.js',
               'portal/page.js','portal/worker.js','portal/people.js','portal/schema.sql']
        for name in names:
            path=self.suite/name
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_text('reviewed fixture')
        (self.suite/'source-manifest.json').write_text(json.dumps({
            name:hashlib.sha256((self.suite/name).read_bytes()).hexdigest() for name in names}))

    def test_modified_source_is_refused_before_starting_a_process(self):
        (self.suite/'portal/page.js').write_text('changed')
        with patch('pathlib.Path.home',return_value=self.root), patch.object(browser.subprocess,'Popen') as process:
            with self.assertRaisesRegex(RuntimeError,'changed'):
                browser.run(self.home)
            process.assert_not_called()

    def test_empty_or_failed_reports_cannot_claim_browser_success(self):
        for stats,code in [({},0),({'expected':0},0),({'expected':5,'unexpected':1},1),
                           ({'expected':5,'skipped':1},0),({'expected':5,'flaky':1},0)]:
            def execute(argv,**kwargs):
                output=Path(kwargs['env']['IRIS_TEST_OUTPUT'])
                (output/'results.json').write_text(json.dumps({'stats':stats}))
                return Mock(wait=Mock(return_value=code))
            with patch('pathlib.Path.home',return_value=self.root), patch.object(browser.subprocess,'Popen',side_effect=execute):
                self.assertEqual(browser.run(self.home),1)

    def test_success_records_exact_source_and_report_without_shell(self):
        def execute(argv,**kwargs):
            self.assertEqual(argv[0],'node')
            self.assertNotIn('shell',kwargs)
            self.assertTrue(kwargs['start_new_session'])
            events=[json.loads(s) for s in (self.home/'events.jsonl').read_text().splitlines()]
            self.assertEqual(events[-1]['kind'],'browser_test_started')
            output=Path(kwargs['env']['IRIS_TEST_OUTPUT'])
            (output/'results.json').write_text(json.dumps({'stats':{'expected':6,'unexpected':0}}))
            return Mock(wait=Mock(return_value=0))
        with patch('pathlib.Path.home',return_value=self.root), patch.object(browser.subprocess,'Popen',side_effect=execute):
            self.assertEqual(browser.run(self.home),0)
        event=json.loads((self.home/'events.jsonl').read_text().splitlines()[-1])
        self.assertTrue(event['passed'])
        self.assertIn('portal/page.js',event['sources'])
