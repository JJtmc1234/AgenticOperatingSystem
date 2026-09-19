"""Repository tests execute without host credentials, network or writable host paths."""
import os
from pathlib import Path
import signal
import resource
import subprocess
import tempfile


def command(work, argv):
    result=['bwrap','--unshare-all','--die-with-parent','--new-session','--clearenv',
            '--ro-bind','/usr','/usr','--symlink','usr/bin','/bin',
            '--symlink','usr/lib','/lib','--symlink','usr/lib64','/lib64',
            '--proc','/proc','--dev','/dev','--tmpfs','/tmp','--dir','/home/evan',
            '--bind',str(work),'/work','--chdir','/work',
            '--setenv','HOME','/home/evan','--setenv','PATH','/usr/bin:/bin',
            '--setenv','LANG','C.UTF-8','--setenv','PYTHONDONTWRITEBYTECODE','1',
            '--setenv','PYTHONPATH','/work']
    return result+['--',*argv]


def limits():
    resource.setrlimit(resource.RLIMIT_FSIZE,(2_000_000,2_000_000))
    resource.setrlimit(resource.RLIMIT_AS,(2_000_000_000,2_000_000_000))
    resource.setrlimit(resource.RLIMIT_NOFILE,(128,128))


def run(sources, commands, timeout):
    records=[]
    with tempfile.TemporaryDirectory(prefix='evan-tests-') as folder:
        root=Path(folder)
        for path,content in sources.items():
            dest=root/path
            dest.parent.mkdir(parents=True,exist_ok=True)
            dest.write_text(content)
        for argv in commands:
            with tempfile.TemporaryFile() as log:
                process=subprocess.Popen(command(root,argv),stdout=log,stderr=subprocess.STDOUT,start_new_session=True,preexec_fn=limits)
                try:
                    code=process.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid,signal.SIGKILL)
                    process.wait()
                    raise RuntimeError('Test timed out. No fix is verified.') from None
                log.seek(0)
                output=log.read(16000).decode(errors='replace')
                from iris_workflow.repository import contains_credential
                if contains_credential(output):
                    raise ValueError('Sensitive test output excluded')
                if 'bwrap:' in output:
                    raise RuntimeError('Test isolation unavailable: '+output[:1000])
                records.append(dict(argv=argv,code=code,output=output))
    return records


def passed(records):
    return bool(records) and all(r['code']==0 for r in records)
