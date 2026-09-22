"""Publish one operator-selected prepared repair without granting recurring publication."""
import hashlib
import json
from pathlib import Path
from iris_workflow.cache import snapshot
from iris_workflow.github import GitHub
from iris_workflow.ledger import Ledger
from iris_workflow.repository import Repository, contains_credential
from . import issues, publication, source


def submit(home, settings, repo, number, *, github=None, fetch=snapshot, iris_home=None):
    home = Path(home)
    if repo not in settings['repositories'] or type(number) is not int or number <= 0:
        raise ValueError('Select a positive issue number in an operator approved repository')
    github = github or GitHub()
    with Ledger(home, label='Evan') as ledger:
        prepared = ledger.latest('prepared', repo=repo, number=number)
        if not prepared:
            raise ValueError('No prepared repair for this issue. Run Evan in draft mode first.')
        plan = prepared['plan']
        row = dict(repo=repo, issue=number, url=plan['issue_url'])
        ledger.append('run_started', trigger='submit', publish=True)
        try:
            published = ledger.latest('pr_published', key=plan['key'])
            if published:
                url = published['url']
            elif ledger.latest('pr_requested', key=plan['key']):
                # An uncertain write may only be reconciled, never repeated.
                url = publication.publish(plan, ledger, github)
            else:
                root = Path(plan['checkout']).resolve()
                if not root.is_relative_to((home/'attempts').resolve()):
                    raise ValueError('Prepared checkout is outside the Evan attempts directory')
                source.verify_prepared(plan)
                known = issues.known_publications((iris_home or home.parent/'iris')/'events.jsonl')
                current = next((item for item in issues.select(repo, github.issues(repo), known)
                                if item['number'] == number), None)
                if not current:
                    raise ValueError('Issue is closed or no longer a confirmed Iris repair')
                comments = github.comments(repo, number)
                if contains_credential(json.dumps(comments)):
                    raise ValueError('Sensitive issue comments excluded')
                signature = issues.identity(current, comments)
                metadata = github._json(['repo', 'view', repo, '--json', 'defaultBranchRef,isArchived'])
                if metadata['isArchived'] or not metadata.get('defaultBranchRef'):
                    raise ValueError('Repository is archived or empty')
                branch = metadata['defaultBranchRef']['name']
                head = Repository(fetch(home, repo, branch)).inspect()['head']
                key = hashlib.sha256((repo+signature+head+json.dumps(
                    settings['repositories'][repo], sort_keys=True)).encode()).hexdigest()[:24]
                if key != plan['key'] or branch != plan['base_branch']:
                    raise ValueError('Issue, source or policy changed. Run Evan in draft mode to revalidate.')
                url = publication.publish(plan, ledger, github)
            row.update(status='Submitted for review.', pr=url)
        except Exception as error:
            detail = str(error)
            row['status'] = 'Blocked. '+('Sensitive diagnostic excluded' if contains_credential(detail) else detail[:1500])
        report = dict(rows=[row], status='Finished', publish=True)
        ledger.append('run_finished', report=report)
        (home/'latest-report.json').write_text(json.dumps(report, indent=2)+'\n')
        return report
