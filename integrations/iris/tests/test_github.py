import json
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch

from iris_workflow.github import GitHub, GitHubError


MARKER = "<!-- aos-iris:abc123 -->"
REPO = "JJtmc1234/AgenticOperatingSystem"


def response(value, code=0):
    return subprocess.CompletedProcess([], code, json.dumps(value), "failure" if code else "")


def pages(*values):
    return subprocess.CompletedProcess([], 0, "\n".join(json.dumps(value) for value in values), "")


def issue(body=MARKER, state="closed"):
    return {"number": 42, "title": "Fix", "body": body, "state": state,
            "html_url": f"https://github.com/{REPO}/issues/42"}


class GitHubTests(unittest.TestCase):
    @patch("iris_workflow.github.subprocess.run")
    def test_discovery_requests_large_paginated_cli_limit(self, run):
        run.return_value = response([{"nameWithOwner": REPO}])
        self.assertEqual(GitHub().list_repositories("JJtmc1234")[0]["nameWithOwner"], REPO)
        args = run.call_args.args[0]
        self.assertIn("10000", args)
        self.assertNotIn("shell", run.call_args.kwargs)

    @patch("iris_workflow.github.subprocess.run")
    def test_discovery_limit_fails_instead_of_truncating(self, run):
        run.return_value = response([{"nameWithOwner": REPO}] * 10000)
        with self.assertRaisesRegex(GitHubError, "limit"):
            GitHub().list_repositories("JJtmc1234")

    @patch("iris_workflow.github.subprocess.run")
    def test_all_issue_pages_include_closed_and_exclude_pull_requests(self, run):
        run.return_value = pages([issue()], [issue("other", "open"), {"pull_request": {}}])
        results = GitHub().issues(REPO)
        self.assertEqual([item["state"] for item in results], ["closed", "open"])
        self.assertIn("--paginate", run.call_args.args[0])
        self.assertNotIn("--slurp", run.call_args.args[0])
        self.assertIn("state=all", run.call_args.args[0][-1])

    @patch("iris_workflow.github.subprocess.run")
    def test_comments_include_all_pages(self, run):
        run.return_value = pages([{"body": "First"}], [{"body": "Last"}])
        self.assertEqual([item["body"] for item in GitHub().comments(REPO, 42)], ["First", "Last"])
        self.assertEqual(run.call_args.args[0][-1], f"repos/{REPO}/issues/42/comments?per_page=100")
        self.assertIn("--paginate", run.call_args.args[0])
        self.assertNotIn("--slurp", run.call_args.args[0])

    @patch("iris_workflow.github.subprocess.run")
    def test_invalid_comment_numbers_never_run_gh(self, run):
        for number in [True, False, 0, -1, "42", 42.0, None]:
            with self.assertRaises(GitHubError):
                GitHub().comments(REPO, number)
        run.assert_not_called()

    @patch("iris_workflow.github.subprocess.run")
    def test_invalid_comment_responses_fail_closed(self, run):
        for value in [{}, ["page"], [None], [{"body": 1}]]:
            run.return_value = response(value)
            with self.assertRaises(GitHubError):
                GitHub().comments(REPO, 42)

    @patch("iris_workflow.github.subprocess.run")
    def test_closed_marker_prevents_duplicate_creation(self, run):
        run.return_value = response([issue()])
        self.assertEqual(GitHub().create_issue(REPO, "Fix", "Details", MARKER)["number"], 42)
        self.assertEqual(run.call_count, 1)

    @patch("iris_workflow.github.subprocess.run")
    def test_create_preserves_literal_body_and_title_without_shell(self, run):
        title = "--body=$(touch SECRET)"
        body = "Literal `echo hi`\n$(touch SECRET)\n"
        files = []

        def execute(args, **kwargs):
            if args[1] == "api":
                return response([])
            if args[1:3] == ["issue", "view"]:
                return response(dict(number=43, url=f"https://github.com/{REPO}/issues/43",
                                     title=title, body=body.rstrip() + "\n\n" + MARKER + "\n"))
            self.assertEqual(args[args.index("--title") + 1], title)
            path = Path(args[args.index("--body-file") + 1])
            self.assertEqual(path.read_text(), body.rstrip() + "\n\n" + MARKER + "\n")
            self.assertFalse(kwargs.get("shell", False))
            files.append(path)
            return subprocess.CompletedProcess(args, 0, f"https://github.com/{REPO}/issues/43\n", "")

        run.side_effect = execute
        self.assertEqual(GitHub().create_issue(REPO, title, body, MARKER)["number"], 43)
        self.assertFalse(files[0].exists())
        self.assertEqual(run.call_args.args[0][1:3], ["issue", "view"])

    @patch("iris_workflow.github.subprocess.run")
    def test_successful_create_with_invalid_readback_requires_reconciliation(self, run):
        url = f"https://github.com/{REPO}/issues/43"
        run.side_effect = [response([]), subprocess.CompletedProcess([], 0, url, ""),
                           response(dict(number=43, url=url, title="Fix", body="Wrong")), response([])]
        with self.assertRaisesRegex(GitHubError, "no create retry"):
            GitHub().create_issue(REPO, "Fix", "Details", MARKER)
        self.assertEqual(run.call_count, 4)

    @patch("iris_workflow.github.subprocess.run")
    def test_timeout_reconciles_without_another_create(self, run):
        run.side_effect = [response([]), subprocess.TimeoutExpired("gh", 60), response([issue()])]
        self.assertEqual(GitHub().create_issue(REPO, "Fix", "Details", MARKER)["number"], 42)
        self.assertEqual(sum(call.args[0][1:3] == ["issue", "create"] for call in run.call_args_list), 1)

    @patch("iris_workflow.github.subprocess.run")
    def test_unconfirmed_failure_never_retries_create(self, run):
        run.side_effect = [response([]), response(None, 1), response([])]
        with self.assertRaisesRegex(GitHubError, "no create retry"):
            GitHub().create_issue(REPO, "Fix", "Details", MARKER)
        self.assertEqual(run.call_count, 3)

    @patch("iris_workflow.github.subprocess.run")
    def test_invalid_identifiers_and_marker_never_run_gh(self, run):
        for repo in ["--help", "user/repo;pwd", "user/repo/issues", "user/../repo"]:
            with self.assertRaises(GitHubError):
                GitHub().issues(repo)
        with self.assertRaises(GitHubError):
            GitHub().list_repositories("--help")
        with self.assertRaises(GitHubError):
            GitHub().create_issue(REPO, "Fix", "Details", "anything")
        run.assert_not_called()

    @patch("iris_workflow.github.subprocess.run")
    def test_invalid_responses_fail_closed(self, run):
        for payload in [{}, [{"number": 1}], ["not a page"], [issue(["bad body"]) ]]:
            run.return_value = response(payload)
            with self.assertRaises(GitHubError):
                GitHub().issues(REPO)


if __name__ == "__main__":
    unittest.main()
