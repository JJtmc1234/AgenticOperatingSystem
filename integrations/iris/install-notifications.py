#!/usr/bin/env python3
"""Install the local completion notifier and preserve Codex settings."""
import json
import os
from pathlib import Path
import shutil
import tempfile
import tomllib

home=Path.home()
source=Path(__file__).resolve().parent/'iris_workflow/notifications.py'
launcher=home/'.local/bin/aos-notify'
launcher.parent.mkdir(parents=True,exist_ok=True)
fd,name=tempfile.mkstemp(prefix='.aos-notify-',dir=launcher.parent)
os.close(fd)
shutil.copyfile(source,name)
os.chmod(name,0o755)
os.replace(name,launcher)
config=home/'.codex/config.toml'
config.parent.mkdir(parents=True,exist_ok=True)
original=config.read_text() if config.exists() else ''
settings=tomllib.loads(original)
command=[str(launcher)]
if settings.get('notify') != command:
    if 'notify' in settings:
        raise SystemExit('An existing Codex notification handler needs to be preserved before adding this one.')
    updated='notify = '+json.dumps(command)+'\n'+original
    assert tomllib.loads(updated)==dict(settings,notify=command)
    fd,backup=tempfile.mkstemp(prefix='config.before-notifications.',dir=config.parent)
    with os.fdopen(fd,'w') as out:out.write(original)
    fd,name=tempfile.mkstemp(prefix='.config-notifications-',dir=config.parent)
    with os.fdopen(fd,'w') as out:out.write(updated)
    os.replace(name,config)
    print('Previous Codex config:',backup)
print('Installed:',launcher)
print('Codex completion hook configured. New sessions load the setting.')
