"""Package the Rust/Figma release without overwriting an older running build."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import tomllib

root=Path(__file__).resolve().parents[1]
source=root/'target/release/sonora.exe'
version=tomllib.loads((root/'Cargo.toml').read_text(encoding='utf8'))['package']['version']
bundle=root/f'dist/WeDex-{version}'
bundle.mkdir(parents=True,exist_ok=True)
shutil.copy2(source,bundle/'WeDex-Sonora.exe')
server_source=root/'target/release/wedex-server.exe'
if server_source.is_file():
    shutil.copy2(server_source,bundle/'WeDex-Server.exe')
for name in ['README.md','LICENSE','THIRD_PARTY.md']:
    shutil.copy2(root/name,bundle/name)
shutil.copytree(root/'docs',bundle/'docs',dirs_exist_ok=True)
licenses=bundle/'licenses'
base=root/'dist/WeDex-0.1.0/licenses'
if not base.is_dir():
    raise RuntimeError('Prepare the dependency license inventory with scripts/package.ps1 first.')
shutil.copytree(base,licenses,dirs_exist_ok=True)
metadata=json.loads(subprocess.check_output([str(Path.home()/'.cargo/bin/cargo.exe'),'metadata','--format-version','1','--locked','--offline'],cwd=root))
inventory=['# Rust dependency licenses','','Includes platform and build dependencies.','']
for package in sorted(metadata['packages'],key=lambda p:(p['name'],p['version'])):
    if package['name']=='wedex':continue
    inventory.append(f"- {package['name']} {package['version']} — {package.get('license') or 'See upstream license'}")
    upstream=Path(package['manifest_path']).parent
    destination=licenses/f"{package['name']}-{package['version']}"
    destination.mkdir(exist_ok=True)
    for file in upstream.iterdir():
        if file.is_file() and file.name.upper().startswith(('LICENSE','LICENCE','COPYING','COPYRIGHT','NOTICE')):
            shutil.copy2(file,destination/file.name)
    if package.get('license_file'):
        file=upstream/package['license_file']
        if file.is_file():shutil.copy2(file,destination/file.name)
(licenses/'INVENTORY.md').write_text('\n'.join(inventory)+'\n',encoding='utf8')
shutil.copy2(root/'assets/fontawesome/LICENSE.txt',licenses/'Font-Awesome-LICENSE.txt')
shutil.copy2(root/'assets/fontawesome/sources.json',licenses/'Font-Awesome-sources.json')
for name,target in [('OFL.txt','Medeo-OFL-1.1.txt'),('AUTHORS.txt','Medeo-AUTHORS.txt'),('sources.json','Medeo-sources.json')]:
    shutil.copy2(root/'assets/fonts/medeo'/name,licenses/target)
shutil.copy2(root/'design/assets/LUCIDE-LICENSE',licenses/'Lucide-ISC.txt')
shutil.copy2(root/'design/assets/sources.json',licenses/'Cover-art-sources.json')
shutil.copy2(root/'assets/providers/CC0.txt',licenses/'Provider-icons-CC0.txt')
shutil.copy2(root/'assets/providers/selfhst-LICENSE.txt',licenses/'Provider-Yandex-CC-BY-4.0.txt')
shutil.copy2(root/'assets/providers/sources.json',licenses/'Provider-icons-sources.json')
checksum=hashlib.sha256(source.read_bytes()).hexdigest()
assert checksum==hashlib.sha256((bundle/'WeDex-Sonora.exe').read_bytes()).hexdigest()
manifest={'executable':f'dist/WeDex-{version}/WeDex-Sonora.exe','sha256':checksum,'referenceFile':'hh9WPK6OnPYis1tL7S0jDY','fontsEmbedded':True,'responsive':True,'themes':['system','light','dark'],'audioPrefetch':True,'localCloud':True,'cloudProtocol':1}
(root/'design/native-release.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8')
print(json.dumps(manifest))
