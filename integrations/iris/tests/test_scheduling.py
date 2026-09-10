"""Fair progress across revisions without reusing stale source reviews."""
import json
import subprocess
import unittest
import test_workflow as fixtures
from test_workflow import FakeModel


class SchedulingTests(unittest.TestCase):
    setUp = fixtures.WorkflowTests.setUp
    execute = fixtures.WorkflowTests.execute

    def commit(self):
        subprocess.run(['git', '-C', str(self.repo), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(self.repo), '-c', 'user.name=Fixture',
                        '-c', 'user.email=fixture@example.test', '-c', 'commit.gpgsign=false',
                        '-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'Fixture update'], check=True)

    def add_batches(self):
        for index in range(8):
            (self.repo / f'file{index}.py').write_text(f'number = {index}\n')
        self.commit()

    def sources(self, prompt):
        return [source['path'] for source in json.loads(prompt.split('\n', 1)[1])['sources']]

    def test_new_commits_do_not_starve_the_unreviewed_last_batch(self):
        self.add_batches()
        model = FakeModel([dict(findings=[])] * 6)
        self.execute(model)
        self.assertNotIn('sample.py', self.sources(model.prompts[0]))
        (self.repo / 'readme.md').write_text('New documentation\n')
        self.commit()
        result = self.execute(model)
        self.assertEqual(self.sources(model.prompts[2]), ['sample.py'])
        self.assertIn('1/2', result['repositories'][0]['status'])
        # The earlier batch still needs review at the new revision.
        result = self.execute(model)
        self.assertIn('2/2', result['repositories'][0]['status'])
        self.assertIn('Complete.', result['repositories'][0]['status'])
        self.assertEqual(len(model.calls), 6)

    def test_a_focused_review_does_not_reorder_an_unrelated_general_review(self):
        self.add_batches()
        self.execute(FakeModel([dict(findings=[])] * 2), request='Inspect constants')
        (self.repo / 'readme.md').write_text('Documentation\n')
        self.commit()
        model = FakeModel([dict(findings=[])] * 2)
        self.execute(model)
        self.assertNotIn('sample.py', self.sources(model.prompts[0]))
