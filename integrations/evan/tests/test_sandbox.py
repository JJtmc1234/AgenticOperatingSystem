import os
import unittest
from evan_workflow import sandbox
from evan_workflow.source import git
from fixtures import Fixture


@unittest.skipUnless(os.environ.get('EVAN_SANDBOX_TESTS')=='1','Set EVAN_SANDBOX_TESTS=1 where user namespaces are available')
class SandboxTests(Fixture,unittest.TestCase):
    def test_actual_red_green_flow_prepares_a_real_git_commit(self):
        result=self.execute()
        self.assertIn('Prepared.',result['rows'][0]['status'])
        plan=next(e['plan'] for e in self.events() if e['kind']=='prepared')
        self.assertTrue(all(r['code']==0 for r in plan['baseline']))
        self.assertTrue(any(r['code']==1 for r in plan['red']))
        self.assertTrue(all(r['code']==0 for r in plan['green']))
        self.assertEqual(git(plan['checkout'],'rev-parse','HEAD'),plan['commit'])
        self.assertEqual((self.repo/'sample.py').read_text(),'def add(a,b):\n    return a-b\n')
        self.assertEqual(git(self.repo,'rev-parse','HEAD'),self.head)

    def test_network_credentials_and_host_files_are_unavailable(self):
        script='''import os, pathlib, socket
assert 'GITHUB_TOKEN' not in os.environ
assert not pathlib.Path('/home/jj_tmc').exists()
assert not pathlib.Path('/work/.git').exists()
try:
 socket.create_connection(('1.1.1.1',443),timeout=1)
except OSError:
 pass
else:
 raise AssertionError('network accessible')
'''
        self.assertTrue(sandbox.passed(sandbox.run({'check.py':script},[['python3','check.py']],10)))

    def test_timeout_stops_the_test_process(self):
        with self.assertRaisesRegex(RuntimeError,'timed out'):
            sandbox.run({'slow.py':'import time\ntime.sleep(60)'},[['python3','slow.py']],1)
