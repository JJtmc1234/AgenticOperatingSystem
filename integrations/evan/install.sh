#!/usr/bin/env bash
# Install a fixed snapshot without enabling publication or changing existing policy.
set -euo pipefail
[[ $EUID != 0 ]] || { echo 'Run as your normal user.' >&2; exit 1; }
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
for program in python3 git gh claude bwrap; do
  command -v "$program" >/dev/null || { echo "Missing $program" >&2; exit 1; }
done
python3 - "$here" <<'PY'
import json,os,shutil,sys,tempfile
from pathlib import Path
source=Path(sys.argv[1])
home=Path.home()
base=home/'.local/share/aos-evan'
base.mkdir(parents=True,exist_ok=True)
release=Path(tempfile.mkdtemp(prefix='release-',dir=base))
for package,folder in [('evan_workflow',source),('iris_workflow',source.parent/'iris')]:
    shutil.copytree(folder/package,release/package,ignore=shutil.ignore_patterns('__pycache__'))
launcher=home/'.local/bin/aos-evan'
launcher.parent.mkdir(parents=True,exist_ok=True)
if launcher.exists():shutil.copy2(launcher,release/'previous-launcher')
fd,name=tempfile.mkstemp(prefix='.aos-evan-',dir=launcher.parent)
with os.fdopen(fd,'w') as out:
    out.write('#!/usr/bin/env python3\nimport sys\nsys.path.insert(0, '+repr(str(release))+')\nfrom evan_workflow.__main__ import main\nraise SystemExit(main())\n')
os.chmod(name,0o755)
os.replace(name,launcher)
state=home/'.carl/evan'
state.mkdir(parents=True,exist_ok=True,mode=0o700)
config=state/'config.json'
if not config.exists():
    config.write_text(json.dumps({'owner':'JJtmc1234','repositories':{},'publish':False},indent=2)+'\n')
    config.chmod(0o600)
units=home/'.config/systemd/user'
units.mkdir(parents=True,exist_ok=True)
for name in ['aos-evan.service','aos-evan.timer']:
    dest=units/name
    if dest.exists():shutil.copy2(dest,release/(name+'.previous'))
    shutil.copy2(source/'systemd'/name,dest)
print('Installed:',release)
print('Configure repository and test authority in:',config)
PY
systemctl --user daemon-reload
"$HOME/.local/bin/aos-evan" doctor
printf '%s\n' 'After configuring repositories: systemctl --user enable --now aos-evan.timer'
