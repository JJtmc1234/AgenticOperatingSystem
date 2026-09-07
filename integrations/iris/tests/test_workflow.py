import subprocess
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

from iris_workflow.config import DEFAULTS
from iris_workflow.workflow import run


REPO = "JJtmc1234/example"


def finding(second=False):
    return dict(title="Missing zero divisor check" if not second else "Empty input indexing fails",
                kind="bug", severity="major", path="sample.py", line=4 if second else 2,
                excerpt="    return values[0]" if second else "    return 10 / value",
                mechanism="No empty input check" if second else "No zero input check",
                impact="Invalid input raises an exception instead of returning an error",
                validation="Add regression coverage for invalid input and normal input")


class FakeModel:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []
        self.prompts = []

    def can_investigate(self):
        return True

    def ask(self, identity, prompt, schema):
        self.calls.append(identity)
        self.prompts.append(prompt)
        if not self.responses:
            raise AssertionError("Unexpected repeated model call")
        return self.responses.pop(0)


class FakeGitHub:
    def __init__(self):
        self.published = []

    def list_repositories(self, owner):
        return [dict(nameWithOwner=REPO, isArchived=False, defaultBranchRef=dict(name="main"))]

    def issues(self, repo):
        return list(self.published)

    def comments(self, repo, number):
        return []

    def create_issue(self, repo, title, body, marker):
        result = dict(number=len(self.published) + 1, title=title, body=body + "\n" + marker,
                      state="open", url=f"https://github.com/{repo}/issues/{len(self.published)+1}")
        self.published.append(result)
        return result


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        notifier=patch("iris_workflow.notifications.emit",return_value={"delivered":True})
        notifier.start()
        self.addCleanup(notifier.stop)
        self.temp = tempfile.TemporaryDirectory(prefix="iris-workflow-test-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.home, self.repo = root / "state", root / "repository"
        self.repo.mkdir()
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        (self.repo / "sample.py").write_text(
            "def divide(value):\n    return 10 / value\ndef first(values):\n    return values[0]\n")
        subprocess.run(["git", "-C", str(self.repo), "add", "sample.py"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "-c", "user.name=Fixture",
                        "-c", "user.email=fixture@example.test", "-c", "commit.gpgsign=false",
                        "-c", "core.hooksPath=/dev/null", "commit", "-qm", "Fixture"], check=True)
        self.github = FakeGitHub()
        self.config = DEFAULTS | dict(publish=True)

    def execute(self, model, **kwargs):
        return run(self.home, self.config, github=self.github,
                   snapshot=lambda home, repo, branch: self.repo,
                   model_factory=lambda config, ledger: model, **kwargs)

    def model_with_findings(self, findings):
        return FakeModel([dict(findings=findings), dict(findings=[]),
                          dict(accepted=list(range(len(findings))))])

    def test_no_findings_creates_no_issues(self):
        model = FakeModel([dict(findings=[]), dict(findings=[])])
        result = self.execute(model)
        self.assertIn("Complete", result["repositories"][0]["status"])
        self.assertEqual(len(model.calls), 2)
        self.assertIn('"line": 2, "text": "    return 10 / value"', model.prompts[0])
        self.assertEqual(self.github.published, [])

    def test_validated_findings_publish_once_across_rerun(self):
        model = self.model_with_findings([finding()])
        self.execute(model)
        self.execute(model)
        self.assertEqual(len(self.github.published), 1)
        self.assertEqual(len(model.calls), 3)
        self.assertIn("aos-iris:", self.github.published[0]["body"])

    def test_invalid_excerpt_never_publishes(self):
        bad = finding() | dict(excerpt="This was never in the source")
        result = self.execute(FakeModel([dict(findings=[bad])]))
        self.assertIn("Failed", result["repositories"][0]["status"])
        self.assertEqual(self.github.published, [])

    def test_draft_then_publish_same_head_reuses_reviewed_plan(self):
        model = self.model_with_findings([finding()])
        self.config["publish"] = False
        self.execute(model)
        self.assertEqual(len(list((self.home / "drafts").glob("*.md"))), 1)
        self.assertEqual(self.github.published, [])
        self.config["publish"] = True
        self.execute(model)
        self.assertEqual(len(self.github.published), 1)
        self.assertEqual(len(model.calls), 3)

    def test_issue_cap_resumes_reviewed_plans_without_model_calls(self):
        model = self.model_with_findings([finding(), finding(second=True)])
        self.config["max_issues"] = 1
        first = self.execute(model)
        self.assertIn("queued", first["repositories"][0]["status"])
        self.assertEqual(len(self.github.published), 1)
        second = self.execute(model)
        self.assertIn("Complete", second["repositories"][0]["status"])
        self.assertEqual(len(self.github.published), 2)
        self.assertEqual(len(model.calls), 3)

    def test_focused_request_does_not_suppress_general_scan(self):
        model = FakeModel([dict(findings=[])] * 4)
        self.execute(model, request="Check division")
        result = self.execute(model)
        self.assertIn("Complete", result["repositories"][0]["status"])
        self.assertEqual(len(model.calls), 4)

    def test_poll_checks_new_commit_without_waiting_an_hour(self):
        model = FakeModel([dict(findings=[])] * 4)
        self.execute(model, trigger="poll")
        (self.repo / "sample.py").write_text("answer = 42\n")
        subprocess.run(["git", "-C", str(self.repo), "add", "sample.py"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "-c", "user.name=Fixture",
                        "-c", "user.email=fixture@example.test", "-c", "commit.gpgsign=false",
                        "-c", "core.hooksPath=/dev/null", "commit", "-qm", "Update behavior"], check=True)
        result = self.execute(model, trigger="poll")
        self.assertIn("Complete", result["repositories"][0]["status"])
        self.assertEqual(len(model.calls), 4)

    def test_selected_repository_outside_owner_is_rejected(self):
        model = FakeModel([])
        with self.assertRaisesRegex(ValueError, "outside"):
            self.execute(model, selected="someoneelse/repository")
        self.assertEqual(model.calls, [])
        self.assertEqual(self.github.published, [])


if __name__ == "__main__":
    unittest.main()
