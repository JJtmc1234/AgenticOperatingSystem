"""Real Git fixtures with deterministic tool-free worker responses."""
import json
import tempfile
from pathlib import Path
from iris_workflow.ledger import Ledger
from evan_workflow.config import validate
from evan_workflow.source import git
from evan_workflow.workflow import run

REPO='JJtmc1234/fixture'
BUG='def add(a,b):\n    return a-b\n'
TEST='import unittest\nfrom sample import add\nclass Check(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2,3),5)\n'
POLICY=dict(paths=['sample.py'],test_paths=['test_sample.py'],commands=[['python3','-m','unittest','discover','-v']])


class FakeGitHub:
    def __init__(self,head):
        self.items=[dict(number=1,state='open',title='Addition subtracts',url='https://github.com/'+REPO+'/issues/1',
            body='Source: https://github.com/'+REPO+'/blob/'+head+'/sample.py#L2\n<!-- aos-iris:fixture -->')]
        self.pulls=[]
        self.creates=0
        self.comment_rows=[]

    def issues(self,repo):
        return [dict(i) for i in self.items]

    def comments(self,repo,number):
        return self.comment_rows

    def _json(self,args):
        if args[0]=='repo':
            return dict(defaultBranchRef=dict(name='master'),isArchived=False)
        return self.pulls

    def _run(self,args):
        self.creates+=1
        raise RuntimeError('Fake publication lost its response')


class FakeModel:
    def __init__(self,settings,ledger):
        self.calls=[]

    def blocked_reason(self):
        return ''

    def ask(self,key,prompt,schema):
        self.calls.append((key,prompt))
        if key.endswith('/regression'):
            return dict(summary='Reproduce incorrect sum',edits=[dict(path='test_sample.py',before='',after=TEST)])
        if key.endswith('/fix'):
            return dict(summary='Add both operands',edits=[dict(path='sample.py',before='return a-b',after='return a+b')])
        return dict(accepted=True,reason='The regression demonstrates the arithmetic error and the fix corrects it')


class Fixture:
    def setUp(self):
        temp=tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root=Path(temp.name)
        self.repo=self.root/'repo'
        self.repo.mkdir()
        git(self.repo,'init','-b','master')
        (self.repo/'sample.py').write_text(BUG)
        (self.repo/'test_existing.py').write_text('import unittest\nfrom sample import add\nclass Existing(unittest.TestCase):\n def test_zero(self):self.assertEqual(add(0,0),0)\n')
        git(self.repo,'add','sample.py','test_existing.py')
        git(self.repo,'commit','-m','Fixture')
        self.head=git(self.repo,'rev-parse','HEAD')
        self.home=self.root/'state'
        self.iris=self.root/'iris'
        with Ledger(self.iris) as ledger:
            ledger.append('batch_reviewed',plans=[dict(marker='fixture',priority=0)])
            ledger.append('published',repo=REPO,url='https://github.com/'+REPO+'/issues/1',marker='<!-- aos-iris:fixture -->')
        self.github=FakeGitHub(self.head)
        self.settings=validate(json.loads(json.dumps(dict(repositories={REPO:POLICY}))))
        self.models=[]

    def factory(self,*args):
        model=FakeModel(*args)
        self.models.append(model)
        return model

    def execute(self,**kwargs):
        return run(self.home,self.settings,github=self.github,fetch=lambda *a:self.repo,
                   model_factory=self.factory,iris_home=self.iris,**kwargs)

    def events(self):
        return [json.loads(x) for x in (self.home/'events.jsonl').read_text().splitlines()]
