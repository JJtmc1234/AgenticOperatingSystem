"""Read bounded, committed repository snapshots without running repository code."""

import os
from pathlib import Path, PurePosixPath
import re
import subprocess


EXCLUDED = {".git", "node_modules", "vendor", "target", "build", "dist", ".venv",
            "venv", "__pycache__", ".ssh", ".aws", ".gnupg", "credentials", "secrets"}
LOCKS = {"cargo.lock", "package-lock.json", "yarn.lock", "pnpm-lock.yaml",
         "poetry.lock", "uv.lock", "composer.lock", "gemfile.lock"}
MAX_BYTES = 65536
SOURCE_SUFFIXES = {'.rs','.py','.js','.ts','.tsx','.jsx','.cs','.go','.c','.h','.cpp',
                   '.hpp','.sh','.bash','.zsh','.json','.toml','.yaml','.yml','.sql',
                   '.xml','.html','.css','.scss','.service','.timer','.conf','.cmake',
                   '.lua','.luau','.rb','.php','.swift','.kt','.kts','.scala','.r','.jl',
                   '.s','.asm','.ino','.ps1','.psm1','.bat','.cmd','.vue','.svelte',
                   '.mdx','.wgsl','.glsl','.vert','.frag','.gd'}
CREDENTIALS = re.compile(
    r"PRIVATE KEY-----|(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}"
    r"|sk-[A-Za-z0-9_-]{20,}|(?:AKIA|ASIA)[A-Z0-9]{16}"
    r"|xox[baprs]-[A-Za-z0-9-]{15,}|[sr]k_live_[A-Za-z0-9]{16,}"
    r"|AIza[A-Za-z0-9_-]{30,})"
    r"|(?i:[\"']?(?:api[_-]?key|access[_-]?token|auth[_-]?token|client[_-]?secret"
    r"|password|secret[_-]?key)[\"']?\s*[:=]\s*[\"'][^\"'\r\n]{12,}[\"'])"
)


def contains_credential(text):
    return bool(CREDENTIALS.search(text))


def eligible(path):
    """Refuse sensitive names even when they were accidentally committed."""
    parts = PurePosixPath(path).parts
    if not parts or path.startswith("/") or any(p in {".", ".."} for p in parts):
        return False
    lowered = [p.lower() for p in parts]
    name = lowered[-1]
    if PurePosixPath(name).suffix not in SOURCE_SUFFIXES and name not in {'makefile','dockerfile','cmakelists.txt','justfile'}:
        return False
    return not (set(lowered) & EXCLUDED or name in LOCKS or name.endswith(".lock")
                or any(p.startswith(".env") for p in lowered)
                or any(word in name for word in ("credential", "privatekey", "private_key"))
                or name.startswith(("secrets.", "secret.", "token.", "tokens."))
                or name in {"id_rsa", "id_ed25519", "id_dsa", ".netrc", ".npmrc"}
                or name.endswith((".pem", ".key", ".p12", ".pfx")))


class Repository:
    def __init__(self, root: Path):
        self.root = Path(root).resolve(strict=True)
        self.head = None
        self.entries = {}

    def _git(self, *args):
        env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                   GIT_TERMINAL_PROMPT="0", GIT_OPTIONAL_LOCKS="0", GIT_NO_REPLACE_OBJECTS="1")
        result = subprocess.run(
            ["git", "--no-pager", "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null",
             "-c", "core.untrackedCache=false", "-C", str(self.root), *args],
            env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
            check=True)
        return result.stdout

    def inspect(self) -> dict:
        self.head = self._git("rev-parse", "--verify", "HEAD^{commit}").decode().strip()
        self.entries = {}
        tracked_count = 0
        for entry in self._git("ls-tree", "-r", "-z", self.head).split(b"\0"):
            if not entry:
                continue
            tracked_count += 1
            metadata, raw_path = entry.split(b"\t", 1)
            mode, kind, oid = metadata.decode("ascii").split()
            try:
                path = raw_path.decode("utf-8")
            except UnicodeDecodeError:
                continue
            if kind == "blob" and mode in {"100644", "100755"} and eligible(path):
                self.entries[path] = oid
        bare = self._git("rev-parse", "--is-bare-repository").strip() == b"true"
        dirty = False if bare else bool(self._git("status", "--porcelain", "--untracked-files=normal"))
        return {"head": self.head, "files": sorted(self.entries), "dirty": dirty, "tracked_count": tracked_count}

    def source(self, path: str) -> str:
        if self.head is None:
            self.inspect()
        if not eligible(path) or path not in self.entries:
            raise ValueError("Source path is not an eligible committed regular file")
        oid = self.entries[path]
        if int(self._git("cat-file", "-s", oid)) > MAX_BYTES:
            raise ValueError("Source exceeds 64 KiB")
        content = self._git("cat-file", "blob", oid)
        if b"\0" in content:
            raise ValueError("Binary content is excluded")
        try:
            text = content.decode("utf-8")
        except UnicodeDecodeError as error:
            raise ValueError("Source is not UTF-8 text") from error
        if contains_credential(text):
            raise ValueError("Potential credential content is excluded")
        return text

    def batches(self, max_bytes=65536, max_files=8) -> list:
        if max_bytes < MAX_BYTES or max_files < 1:
            raise ValueError("Batch limits must fit one eligible file and at least one file")
        if self.head is None:
            self.inspect()
        batches, batch, size = [], [], 0
        for path in sorted(self.entries):
            try:
                content = self.source(path)
            except ValueError:
                continue
            length = len(content.encode("utf-8"))
            if batch and (size + length > max_bytes or len(batch) >= max_files):
                batches.append(batch)
                batch, size = [], 0
            batch.append({"path": path, "content": content})
            size += length
        if batch:
            batches.append(batch)
        return batches

    def feature_commits(self, previous: str) -> bool:
        if self.head is None:
            self.inspect()
        if not re.fullmatch(r"[0-9a-fA-F]{40,64}", previous or ""):
            return True
        try:
            self._git("merge-base", "--is-ancestor", previous, self.head)
            subjects = self._git("log", "--format=%s", f"{previous}..{self.head}").decode()
        except subprocess.CalledProcessError:
            return True
        return any(re.match(r"^(?:feat(?:\([^)]*\))?!?:|feature\b)", line, re.I)
                   for line in subjects.splitlines())
