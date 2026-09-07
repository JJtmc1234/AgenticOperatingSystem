"""Bounded GitHub access through argument vectors, with recoverable publication."""

import json
import re
import subprocess
import tempfile
from pathlib import Path


class GitHubError(RuntimeError):
    """GitHub could not confirm the requested operation."""


def _identifier(value, repository=False):
    pattern = r"[A-Za-z0-9][A-Za-z0-9_.-]*"
    if repository:
        pattern += "/" + pattern
    if not isinstance(value, str) or not re.fullmatch(pattern, value):
        raise GitHubError("Invalid GitHub repository" if repository else "Invalid GitHub owner")
    return value


class GitHub:
    def __init__(self, executable="gh", timeout=60):
        self.executable = executable
        self.timeout = timeout

    def _run(self, arguments):
        try:
            result = subprocess.run(
                [self.executable, *arguments], capture_output=True, text=True,
                timeout=self.timeout, check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise GitHubError(f"GitHub command failed: {error}") from error
        if result.returncode:
            raise GitHubError(f"GitHub command failed: {result.stderr.strip()[:1000]}")
        return result.stdout

    def _json(self, arguments):
        try:
            output = self._run(arguments)
            if "--paginate" not in arguments:
                return json.loads(output)
            pages, decoder = [], json.JSONDecoder()
            while output.strip():
                output = output.lstrip()
                page, end = decoder.raw_decode(output)
                pages.append(page)
                output = output[end:]
            if not pages:
                raise ValueError("No JSON pages")
            return pages
        except (ValueError, TypeError) as error:
            raise GitHubError("GitHub returned invalid JSON") from error

    def list_repositories(self, owner):
        owner = _identifier(owner)
        # Fail visibly at the cap rather than silently omit repositories.
        repositories = self._json([
            "repo", "list", owner, "--limit", "10000", "--json",
            "nameWithOwner,url,isArchived,isFork,defaultBranchRef",
        ])
        if not isinstance(repositories, list) or any(
            not isinstance(repo, dict) or not isinstance(repo.get("nameWithOwner"), str)
            for repo in repositories
        ):
            raise GitHubError("GitHub returned invalid repositories")
        if len(repositories) >= 10000:
            raise GitHubError("Repository discovery reached its limit, narrow the owner scope")
        for repo in repositories:
            _identifier(repo["nameWithOwner"], repository=True)
            if repo["nameWithOwner"].split("/")[0].lower() != owner.lower():
                raise GitHubError("GitHub returned a repository outside the requested owner")
        return repositories

    def issues(self, repo):
        repo = _identifier(repo, repository=True)
        pages = self._json([
            "api", "--paginate",
            f"repos/{repo}/issues?state=all&per_page=100",
        ])
        if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
            raise GitHubError("GitHub returned invalid issue pages")
        issues = []
        for page in pages:
            for issue in page:
                if not isinstance(issue, dict):
                    raise GitHubError("GitHub returned an invalid issue")
                if "pull_request" in issue:
                    continue
                if (not isinstance(issue.get("number"), int)
                        or not isinstance(issue.get("title"), str)
                        or not isinstance(issue.get("body") or "", str)):
                    raise GitHubError("GitHub returned an invalid issue")
                issues.append({
                    "number": issue["number"], "title": issue["title"],
                    "body": issue.get("body") or "", "state": issue.get("state"),
                    "url": issue.get("html_url"),
                })
        return issues

    def comments(self, repo, number):
        repo = _identifier(repo, repository=True)
        if type(number) is not int or number <= 0:
            raise GitHubError("Invalid GitHub issue number")
        pages = self._json([
            "api", "--paginate",
            f"repos/{repo}/issues/{number}/comments?per_page=100",
        ])
        if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
            raise GitHubError("GitHub returned invalid comment pages")
        comments = [comment for page in pages for comment in page]
        if any(not isinstance(comment, dict) or not isinstance(comment.get("body"), str)
               for comment in comments):
            raise GitHubError("GitHub returned an invalid comment")
        return comments

    def create_issue(self, repo, title, body, marker):
        repo = _identifier(repo, repository=True)
        if not isinstance(marker, str) or not re.fullmatch(r"<!-- aos-iris:[A-Za-z0-9_.:-]+ -->", marker):
            raise GitHubError("Invalid Iris issue marker")
        if not isinstance(title, str) or not title.strip() or not isinstance(body, str):
            raise GitHubError("Issue title and body must be text with a nonempty title")
        existing = next((issue for issue in self.issues(repo) if marker in issue["body"]), None)
        if existing:
            return existing
        content = body if marker in body else f"{body.rstrip()}\n\n{marker}\n"
        try:
            with tempfile.TemporaryDirectory(prefix="iris-issue-") as directory:
                path = Path(directory) / "body.md"
                path.write_text(content, encoding="utf-8")
                output = self._run([
                    "issue", "create", "--repo", repo, "--title", title,
                    "--body-file", str(path),
                ]).strip()
            match = re.fullmatch(rf"https://github\.com/{re.escape(repo)}/issues/([0-9]+)", output)
            if not match:
                raise GitHubError("GitHub did not return the created issue URL")
            created = self._json(["issue", "view", match[1], "--repo", repo,
                                  "--json", "number,url,title,body"])
            if (not isinstance(created, dict) or created.get("number") != int(match[1])
                    or created.get("url") != output or created.get("title") != title
                    or not isinstance(created.get("body"), str)
                    or created.get("body", "").strip() != content.strip()):
                raise GitHubError("Created issue did not match requested content")
            return created
        except GitHubError as error:
            # A timeout may arrive after GitHub has committed the issue.
            try:
                existing = next((issue for issue in self.issues(repo) if marker in issue["body"]), None)
            except GitHubError as reconcile_error:
                raise GitHubError("Issue publication is uncertain, reconcile before retrying") from reconcile_error
            if existing:
                return existing
            raise GitHubError("Issue publication was not confirmed, no create retry was attempted") from error
