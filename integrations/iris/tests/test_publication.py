import tempfile
import unittest
from pathlib import Path
from iris_workflow.ledger import Ledger
from iris_workflow.publication import publish


REPO='JJtmc1234/example'
PLAN=dict(title='Slow chat refresh displays a message twice',marker='one',
          body='Actual source evidence\n<!-- aos-iris-finding:individual -->')


class GitHub:
    def __init__(self,issues=()):
        self.current=list(issues)
        self.writes=[]
        self.reads=0

    def issues(self,repo):
        self.reads+=1
        return list(self.current)

    def create_issue(self,repo,title,body,marker):
        created=dict(title=title,body=body+'\n'+marker,url=f'https://github.com/{repo}/issues/2')
        self.current.append(created)
        self.writes.append(created)
        return created


class PublicationTests(unittest.TestCase):
    def execute(self,github,plans=None,publishing=True):
        with tempfile.TemporaryDirectory() as folder:
            home=Path(folder)
            with Ledger(home) as ledger:
                row=dict(issues=[])
                completed=publish(home,dict(publish=publishing),REPO,plans or [PLAN],
                                  ledger,github,[1,3],row)
                return completed,row,list(ledger.events),list(home.glob('drafts/*.md'))

    def test_new_closed_issue_blocks_stale_reviewed_plan(self):
        issue=dict(title=PLAN['title'],body='Reported after the draft was reviewed',
                   state='closed',url=f'https://github.com/{REPO}/issues/1')
        github=GitHub([issue])
        completed,row,events,_=self.execute(github)
        self.assertTrue(completed)
        self.assertEqual(github.writes,[])
        self.assertEqual(row['issues'][0]['url'],issue['url'])
        self.assertTrue(row['issues'][0]['existing'])
        self.assertEqual(events[-1]['kind'],'duplicate_skipped')

    def test_individual_marker_blocks_renamed_grouped_duplicate(self):
        github=GitHub([dict(title='A differently titled report',body=PLAN['body'],state='open',url='existing')])
        self.execute(github)
        self.assertEqual(github.writes,[])

    def test_fresh_history_also_blocks_duplicate_draft(self):
        github=GitHub([dict(title=PLAN['title'],body='Human report',url='existing')])
        _,_,_,drafts=self.execute(github,publishing=False)
        self.assertEqual(drafts,[])
        self.assertEqual(github.writes,[])

    def test_refresh_after_creation_prevents_intra_run_duplicate(self):
        second=PLAN | dict(marker='two',body='Other evidence')
        github=GitHub()
        _,row,_,_=self.execute(github,[PLAN,second])
        self.assertEqual(len(github.writes),1)
        self.assertEqual(github.reads,2)
        self.assertTrue(row['issues'][1]['existing'])

    def test_no_match_publishes_verified_plan(self):
        github=GitHub([dict(title='Unrelated startup error',body='Different source',url='other')])
        completed,row,events,_=self.execute(github)
        self.assertTrue(completed)
        self.assertEqual(len(github.writes),1)
        self.assertNotIn('existing',row['issues'][0])
        self.assertEqual([e['kind'] for e in events],['publication_requested','published'])
