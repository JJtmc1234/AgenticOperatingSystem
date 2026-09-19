import json
import unittest
from pathlib import Path
from unittest.mock import patch
from iris_workflow.ledger import Ledger
from evan_workflow import publication
from evan_workflow.source import git
from fixtures import Fixture, REPO
from test_workflow import GREEN, RED


class PublicationTests(Fixture,unittest.TestCase):
    def prepare(self):
        with patch('evan_workflow.sandbox.run',side_effect=[GREEN,RED,GREEN]):
            self.execute()
        return next(e['plan'] for e in self.events() if e['kind']=='prepared')

    def test_real_branch_push_and_dry_github_create_are_idempotent(self):
        plan=self.prepare()
        remote=self.root/'remote.git'
        git(self.root,'init','--bare',str(remote))
        commands=[]
        def local_git(root,*args):
            args=list(args)
            if args[0] in ('push','ls-remote'):
                args[1]=str(remote)
            return git(root,*args)
        def create(args):
            commands.append(args)
            body=Path(args[args.index('--body-file')+1]).read_text()
            self.github.pulls=[dict(url='https://github.com/'+REPO+'/pull/2',body=body,headRefOid=plan['commit'])]
            return self.github.pulls[0]['url']
        with patch.object(publication,'git',side_effect=local_git), patch.object(self.github,'_run',side_effect=create):
            with Ledger(self.home) as ledger:
                first=publication.publish(plan,ledger,self.github)
            with Ledger(self.home) as ledger:
                self.assertEqual(publication.publish(plan,ledger,self.github),first)
        self.assertEqual(len(commands),1)
        self.assertIn('--draft',commands[0])
        self.assertEqual(git(remote,'rev-parse','refs/heads/'+plan['branch']),plan['commit'])
        self.assertNotIn('Closes',self.github.pulls[0]['body'])

    def test_policy_change_invalidates_old_prepared_authority(self):
        self.prepare()
        self.settings['repositories'][REPO]['commands']=[['python3','-m','unittest','test_sample']]
        with patch('evan_workflow.sandbox.run',return_value=RED):
            result=self.execute()
        self.assertIn('Existing tests fail',result['rows'][0]['status'])

    def test_existing_pr_for_wrong_commit_is_not_adopted(self):
        plan=self.prepare()
        self.github.pulls=[dict(url='wrong',headRefOid='0'*40,body='<!-- aos-evan:'+plan['key']+' -->')]
        with Ledger(self.home) as ledger, self.assertRaisesRegex(RuntimeError,'does not match'):
            publication.publish(plan,ledger,self.github)

    def test_test_changes_cannot_be_smuggled_into_fix(self):
        from evan_workflow.patches import apply
        with self.assertRaisesRegex(ValueError,'outside assigned'):
            apply({'test_sample.py':'assert False'},dict(summary='hide failure',
                  edits=[dict(path='test_sample.py',before='False',after='True')]),['sample.py'])
