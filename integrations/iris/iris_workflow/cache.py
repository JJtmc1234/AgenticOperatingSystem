"""Fetch GitHub source into disposable bare repositories, never a working checkout."""
import os
from pathlib import Path
import re
import subprocess


def snapshot(home, repo, branch):
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*',repo):
        raise ValueError('Invalid source repository')
    if not isinstance(branch,str) or not branch or branch.startswith('-'):
        raise ValueError('Repository has no default branch')
    subprocess.run(['git','check-ref-format','refs/heads/'+branch],check=True,capture_output=True)
    root=Path(home)/'repos'/repo
    root.parent.mkdir(parents=True,exist_ok=True)
    env={k:v for k,v in os.environ.items() if not k.startswith('GIT_')}
    env.update(GIT_TERMINAL_PROMPT='0',GIT_CONFIG_GLOBAL=os.devnull,GIT_CONFIG_NOSYSTEM='1')
    base=['git','-c','core.hooksPath=/dev/null','-c','core.fsmonitor=false',
          '-c','credential.helper=','-c','credential.helper=!gh auth git-credential']
    url='https://github.com/'+repo+'.git'
    if not root.exists():
        args=base+['clone','--bare','--depth=100','--single-branch','--branch',branch,'--',url,str(root)]
    else:
        args=base+['-C',str(root),'fetch','--depth=100','--no-tags',url,'refs/heads/'+branch]
    result=subprocess.run(args,env=env,capture_output=True,text=True,timeout=120)
    if result.returncode:
        raise RuntimeError('Could not fetch committed GitHub source for '+repo)
    if (root/'FETCH_HEAD').exists():
        subprocess.run(base+['-C',str(root),'update-ref','HEAD','FETCH_HEAD'],env=env,
                       check=True,capture_output=True,timeout=30)
    return root
