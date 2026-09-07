import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from iris_workflow.repository import Repository


class RepositoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "repo"
        self.root.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Test")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args],
                                       stderr=subprocess.PIPE).decode().strip()

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def commit(self, message="initial"):
        self.git("add", ".")
        self.git("commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def test_dirty_files_are_read_from_pinned_head(self):
        self.write("main.py", "original")
        head = self.commit()
        self.write("main.py", "working secret")
        self.write("untracked.py", "untracked")
        repo = Repository(self.root)
        self.assertEqual(repo.inspect(), {"head": head, "files": ["main.py"], "dirty": True, "tracked_count": 1})
        self.assertEqual(repo.source("main.py"), "original")
        self.commit("feat: change")
        self.assertEqual(repo.source("main.py"), "original")

    def test_traversal_symlinks_secrets_binary_and_oversize_are_refused(self):
        self.write("main.py", "ok")
        self.write(".env", "SECRET=x")
        self.write("credentials.json", "secret")
        self.write("innocent.txt", "-----BEGIN PRIVATE KEY-----")
        self.write("large.txt", "a" * 65537)
        (self.root / "binary").write_bytes(b"hello\0world")
        os.symlink("main.py", self.root / "link")
        self.commit()
        repo = Repository(self.root)
        for path in ("../main.py", "/etc/passwd", "link", ".env", "credentials.json",
                     "innocent.txt", "large.txt", "binary", "missing"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                repo.source(path)

    def test_every_eligible_file_is_batched_and_build_output_is_excluded(self):
        expected = [f"src/{i:03}.py" for i in range(67)]
        for path in expected:
            self.write(path, "a" * 4000)
        for path in ("vendor/lib.py", "node_modules/lib.js", "target/out.rs", "Cargo.lock"):
            self.write(path, "excluded")
        self.commit()
        batches = Repository(self.root).batches(max_bytes=65536, max_files=12)
        self.assertEqual([entry["path"] for batch in batches for entry in batch], expected)
        self.assertTrue(all(len(batch) <= 12 for batch in batches))
        self.assertTrue(all(sum(len(item["content"].encode()) for item in batch) <= 65536
                            for batch in batches))

    def test_default_batches_fit_the_live_model_budget(self):
        for i in range(9):
            self.write(f"file{i}.py", "x" * 10000)
        self.commit()
        batches = Repository(self.root).batches()
        self.assertEqual(sum(len(batch) for batch in batches), 9)
        self.assertTrue(all(len(batch) <= 8 for batch in batches))
        self.assertTrue(all(sum(len(f["content"].encode()) for f in batch) <= 65536
                            for batch in batches))

    def test_factorio_and_other_source_languages_are_included(self):
        from iris_workflow.repository import eligible
        for name in ['control.lua','src/main.rs','boot/start.S','game.gd','panel.vue']:
            self.assertTrue(eligible(name),name)
        self.assertFalse(eligible('memory/contacts.md'))

    def test_bare_repository_snapshot(self):
        self.write("main.py", "committed")
        self.commit()
        bare = Path(self.temp.name) / "bare"
        subprocess.run(["git", "clone", "--bare", str(self.root), str(bare)],
                       check=True, capture_output=True)
        repo = Repository(bare)
        self.assertFalse(repo.inspect()["dirty"])
        self.assertEqual(repo.source("main.py"), "committed")

    def test_inline_credentials_never_enter_model_batches(self):
        samples = ["ghp_" + "a" * 30, "github_pat_" + "a" * 30,
                   "sk-proj-" + "a" * 30, "AKIA" + "A" * 16,
                   "xoxb-" + "1" * 20, "sk_live_" + "a" * 20,
                   "AIza" + "a" * 32, 'API_KEY = "' + "a" * 24 + '"',
                   '"client_secret": "' + "a" * 24 + '"']
        for index, sample in enumerate(samples):
            self.write(f"sample{index}.py", sample)
        self.write("safe.py", 'api_key = os.environ["API_KEY"]')
        self.commit()
        repo = Repository(self.root)
        for index in range(len(samples)):
            with self.subTest(index=index), self.assertRaisesRegex(ValueError, "credential"):
                repo.source(f"sample{index}.py")
        self.assertEqual([item["path"] for batch in repo.batches() for item in batch], ["safe.py"])

    def test_feature_commit_detection_and_invalid_baseline(self):
        self.write("main.py", "one")
        previous = self.commit()
        self.write("main.py", "two")
        self.commit("fix: repair")
        self.assertFalse(Repository(self.root).feature_commits(previous))
        self.write("main.py", "three")
        self.commit("feat(ui): add panel")
        self.assertTrue(Repository(self.root).feature_commits(previous))
        self.assertTrue(Repository(self.root).feature_commits("unknown"))
        self.assertTrue(Repository(self.root).feature_commits("f" * 40))


if __name__ == "__main__":
    unittest.main()
