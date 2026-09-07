"""Run the installed, reviewed portal suite without accepting arbitrary commands or URLs."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import signal
from .ledger import Ledger


def run(home):
    suite=Path.home()/'.local/share/aos-iris/browser-tests'
    manifest=suite/'source-manifest.json'
    if not manifest.is_file():
        raise RuntimeError('Iris browser suite is not installed. Run integrations/iris/install-browser-tests.sh')
    sources=json.loads(manifest.read_text())
    required={'server.mjs','playwright.config.js','package-lock.json','tests/portal.spec.js',
              'portal/page.js','portal/worker.js','portal/people.js','portal/schema.sql'}
    if not isinstance(sources,dict) or not required.issubset(sources):
        raise RuntimeError('Browser source manifest is incomplete')
    for name,digest in sources.items():
        path=Path(name)
        if path.is_absolute() or '..' in path.parts or hashlib.sha256((suite/path).read_bytes()).hexdigest()!=digest:
            raise RuntimeError('Installed browser suite changed. Reinstall the reviewed suite.')
    with Ledger(home) as ledger:
        stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
        output=Path(home).resolve()/'browser-tests'/stamp
        output.mkdir(parents=True,mode=0o700)
        ledger.append('browser_test_started',suite='portal',sources=sources,output=str(output))
        argv=['node',str(suite/'node_modules/@playwright/test/cli.js'),'test',
              '--config',str(suite/'playwright.config.js')]
        env=dict(os.environ,IRIS_TEST_OUTPUT=str(output))
        try:
            with (output/'run.log').open('w') as log:
                process=subprocess.Popen(argv,cwd=suite,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                try:
                    code=process.wait(timeout=180)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid,signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid,signal.SIGKILL)
                        process.wait()
                    code=124
        except OSError as error:
            (output/'run.log').write_text(str(error))
            code=127
        report=output/'results.json'
        try:
            stats=json.loads(report.read_text()).get('stats',{})
            if not isinstance(stats,dict): stats={}
        except (OSError,ValueError,AttributeError):
            stats={}
        passed=code==0 and stats.get('expected',0)>0 and stats.get('unexpected',0)==0 and stats.get('skipped',0)==0 and stats.get('flaky',0)==0
        event=ledger.append('browser_test_finished',suite='portal',passed=passed,exit_code=code,
                            stats=stats,output=str(output),sources=sources)
        (output/'summary.json').write_text(json.dumps(event,indent=2)+'\n')
        print(json.dumps(dict(passed=passed,stats=stats,report=str(output/'html/index.html'),log=str(output/'run.log')),indent=2))
        return 0 if passed else 1
