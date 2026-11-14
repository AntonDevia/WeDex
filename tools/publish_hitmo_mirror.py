"""Publish a validated, non-secret mirror record on an isolated data branch."""
import base64
import json
import os
from pathlib import Path
import subprocess

repo = os.environ['GITHUB_REPOSITORY']
folder = Path('.local/mirror')
data = (folder / 'result.json').read_bytes()
value = json.loads(data)
assert value['service'] == 'wedex-hitmo-mirror' and 'error' not in value


def gh(*args, **kwargs):
    return subprocess.run(['gh', 'api', *args], capture_output=True, text=True, **kwargs)


ref = gh(f'repos/{repo}/git/ref/heads/hitmo-mirror')
if ref.returncode:
    base = json.loads(gh(f'repos/{repo}/git/ref/heads/main', check=True).stdout)['object']['sha']
    gh(f'repos/{repo}/git/refs', '-f', 'ref=refs/heads/hitmo-mirror', '-f', f'sha={base}', check=True)
old = gh(f'repos/{repo}/contents/mirror.json?ref=hitmo-mirror')
payload = {'message': 'Refresh verified Hitmo mirror', 'branch': 'hitmo-mirror',
           'content': base64.b64encode(data).decode()}
if not old.returncode:
    payload['sha'] = json.loads(old.stdout)['sha']
request = folder / 'publish.json'
request.write_text(json.dumps(payload))
gh(f'repos/{repo}/contents/mirror.json', '--method', 'PUT', '--input', str(request), check=True)
print('Published verified mirror:', value['origin'], 'discovery:', value['discovery'])
