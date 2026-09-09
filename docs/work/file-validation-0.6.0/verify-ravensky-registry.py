"""Fetch release records and archives; call with repository path and crate name."""
from pathlib import Path
import datetime
import hashlib
import json
import sys
import urllib.request

root = Path(sys.argv[1])
name = sys.argv[2]
out = root / 'target' / 'registry-060'
out.mkdir(exist_ok=True)

def fetch(url):
    request = urllib.request.Request(url, headers={'User-Agent': 'ravensky-release-verification/0.6.0'})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()

record = json.loads(fetch(f'https://crates.io/api/v1/crates/{name}/0.6.0'))
version = record['version']
assert version['num'] == '0.6.0' and not version['yanked']
archive = fetch(f'https://static.crates.io/crates/{name}/{name}-0.6.0.crate')
assert hashlib.sha256(archive).hexdigest() == version['checksum']
(out / f'{name}-0.6.0.crate').write_bytes(archive)
dependencies = json.loads(fetch(f'https://crates.io/api/v1/crates/{name}/0.6.0/dependencies'))
for dep in dependencies['dependencies']:
    if dep['crate_id'] in ['astro-io', 'astro-metadata', 'astro-metrics']:
        assert dep['req'] == '^0.6.0', dep
result = {'checked_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'version': version, 'dependencies': dependencies['dependencies'],
          'download_checksum_verified': True}
(out / f'{name}.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'crate': name, 'version': version['num'], 'created_at': version['created_at'],
                  'checksum': version['checksum'], 'download_checksum_verified': True}))
