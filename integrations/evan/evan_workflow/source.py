"""Safe source materialization and fixed Git operations."""
import os
from pathlib import Path
import subprocess
from iris_workflow.repository import Repository


def git(root,*args):
    env={k:v for k,v in os.environ.items() if not k.startswith('GIT_')}
    env.update(GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL=os.devnull,GIT_TERMINAL_PROMPT='0',
               GIT_AUTHOR_NAME='Evan',GIT_AUTHOR_EMAIL='evan@localhost',
               GIT_COMMITTER_NAME='Evan',GIT_COMMITTER_EMAIL='evan@localhost')
    result=subprocess.run(['git','-c','core.hooksPath=/dev/null','-c','core.fsmonitor=false',
        '-c','credential.helper=','-c','credential.helper=!gh auth git-credential',
        '-C',str(root),*args],env=env,capture_output=True,text=True,timeout=120)
    if result.returncode:
        raise RuntimeError('Git '+args[0]+' failed: '+result.stderr[:1000])
    return result.stdout.strip()


def read(root, policy, issue):
    repository=Repository(root)
    repository.inspect()
    if not issue['paths'] or not set(issue['paths'])<=set(policy['paths']):
        raise ValueError('Issue source paths need an explicit repository policy')
    sources={}
    total=0
    for path in sorted(repository.entries):
        try:
            content=repository.source(path)
        except ValueError:
            continue
        total+=len(content.encode())
        if total>20_000_000:
            raise ValueError('Source exceeds the 20 MB test fixture limit')
        sources[path]=content
    allowed=policy['paths']+policy['test_paths']
    context={p:sources[p] for p in allowed if p in sources}
    if not set(policy['paths'])<=set(sources) or sum(len(v.encode()) for v in context.values())>65536:
        raise ValueError('Assigned source is unavailable or exceeds 64 KiB')
    instructions={}
    for p in ('CLAUDE.md','AGENTS.md'):
        try:
            value=repository._git('show',repository.head+':'+p).decode()
            from iris_workflow.repository import contains_credential
            if contains_credential(value):
                raise ValueError('Sensitive repository instructions')
            instructions[p]=value[:8000]
        except subprocess.CalledProcessError:
            pass
    return repository.head,sources,context,instructions


def commit(folder, cache, branch, head, before, after):
    git(folder.parent,'clone','--no-hardlinks','--no-checkout','--',str(cache),str(folder))
    git(folder,'checkout','-b',branch,head)
    changed=[p for p in after if before.get(p)!=after[p]]
    for path in changed:
        dest=folder/path
        if dest.is_symlink() or any(p.is_symlink() for p in dest.parents if p!=folder.parent):
            raise ValueError('Refusing symlink in prepared checkout')
        dest.parent.mkdir(parents=True,exist_ok=True)
        dest.write_text(after[path])
    git(folder,'add','--',*changed)
    git(folder,'commit','-m','Fix Iris issue with verified regression')
    return git(folder,'rev-parse','HEAD')


def verify_prepared(plan):
    root=plan['checkout']
    if git(root,'status','--porcelain') or git(root,'rev-parse','HEAD')!=plan['commit']:
        raise ValueError('Prepared checkout changed. Saved verification no longer describes it.')
    return root
