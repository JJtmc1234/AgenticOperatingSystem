#!/usr/bin/env bash
# Install only the reviewed local browser suite and its pinned dependencies.
set -euo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$HOME/.local/bin:$PATH"
node --input-type=module -e 'import { DatabaseSync } from "node:sqlite"; new DatabaseSync(":memory:").close()'
base="$HOME/.local/share/aos-iris"
mkdir -p "$base"
release="$(mktemp -d "$base/browser-release-XXXXXX")"
cp "$here/browser-tests/"{package.json,package-lock.json,playwright.config.js,server.mjs} "$release/"
cp -R "$here/browser-tests/tests" "$release/tests"
mkdir -p "$release/portal"
portal="$here/../../carl/portal"
[[ -d "$portal" ]] || portal="$here/../carl/portal"
for file in worker.js page.js people.js schema.sql; do
  cp "$portal/$file" "$release/portal/$file"
done
npm --prefix "$release" ci --ignore-scripts --no-audit --no-fund
node "$release/node_modules/@playwright/test/cli.js" install chromium
python3 - "$release" "$base" <<'PY'
import hashlib,json,os,sys
from pathlib import Path
release,base=map(Path,sys.argv[1:])
files=[p for p in release.rglob('*') if p.is_file() and 'node_modules' not in p.parts and 'test-results' not in p.parts]
manifest={str(p.relative_to(release)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
(release/'source-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
link=base/(release.name+'.link')
link.symlink_to(release,target_is_directory=True)
os.replace(link,base/'browser-tests')
print('Installed reviewed browser suite:',release)
PY
