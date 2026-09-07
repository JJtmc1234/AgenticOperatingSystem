#!/usr/bin/env bash
# Install Iris's workflow without changing the other agents or Claude guard.
set -euo pipefail
[[ $EUID != 0 ]] || { echo 'Run as your normal user.' >&2; exit 1; }
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
for program in python3 git gh claude; do
  command -v "$program" >/dev/null || { echo "Missing $program" >&2; exit 1; }
done
python3 - "$here" <<'PY'
import json, os, shutil, sys, tempfile
from pathlib import Path
source=Path(sys.argv[1])
home=Path.home()
base=home/'.local/share/aos-iris'
base.mkdir(parents=True,exist_ok=True)
release=Path(tempfile.mkdtemp(prefix='release-',dir=base))
shutil.copytree(source/'iris_workflow',release/'iris_workflow',ignore=shutil.ignore_patterns('__pycache__'))
launcher=home/'.local/bin/aos-iris'
launcher.parent.mkdir(parents=True,exist_ok=True)
if launcher.exists():
    shutil.copy2(launcher,release/'previous-launcher')
fd,name=tempfile.mkstemp(prefix='.aos-iris-',dir=launcher.parent)
with os.fdopen(fd,'w') as out:
    out.write('#!/usr/bin/env python3\nimport sys\nsys.path.insert(0, '+repr(str(release))+')\nfrom iris_workflow.__main__ import main\nraise SystemExit(main())\n')
os.chmod(name,0o755)
os.replace(name,launcher)
state=home/'.carl/iris'
state.mkdir(parents=True,exist_ok=True,mode=0o700)
config=state/'config.json'
if not config.exists():
    config.write_text(json.dumps({'owner':'JJtmc1234','publish':True},indent=2)+'\n')
    config.chmod(0o600)
units=home/'.config/systemd/user'
units.mkdir(parents=True,exist_ok=True)
for name in ['aos-iris.service','aos-iris.timer']:
    dest=units/name
    if dest.exists():shutil.copy2(dest,release/(name+'.previous'))
    shutil.copy2(source/'systemd'/name,dest)
overview=home/'Projects/AOS/iris-status.md'
if overview.parent.is_dir() and not overview.exists() and not overview.is_symlink():
    overview.symlink_to(state/'overview.md')
print('Installed workflow:',release)
print('Configuration:',config)
PY
systemctl --user daemon-reload
"$HOME/.local/bin/aos-iris" doctor
"$HOME/.local/bin/aos-iris" status >/dev/null
printf '%s\n' 'Enable scheduled publishing after checks: systemctl --user enable --now aos-iris.timer'
