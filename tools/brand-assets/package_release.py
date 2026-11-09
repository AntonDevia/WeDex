"""Package the current branded Rust release without touching a running old build."""
from pathlib import Path
import hashlib
import json
import shutil
import zipfile

ROOT=Path(__file__).resolve().parents[2]
source=ROOT/'target/release/sonora.exe'
bundle=ROOT/'dist/WeDex-Medeo'
bundle.mkdir(parents=True,exist_ok=True)
shutil.copy2(source,bundle/'WeDex-Sonora.exe')
for name in ['README.md','LICENSE','THIRD_PARTY.md']:
    shutil.copy2(ROOT/name,bundle/name)
for name in ['docs']:
    shutil.copytree(ROOT/name,bundle/name,dirs_exist_ok=True)
licenses=bundle/'licenses';licenses.mkdir(exist_ok=True)
base_licenses=ROOT/'dist/WeDex-0.1.0/licenses'
if base_licenses.is_dir():
    shutil.copytree(base_licenses,licenses,dirs_exist_ok=True)
if not (licenses/'INVENTORY.md').exists():
    raise RuntimeError('Prepare the Rust dependency license inventory using the base package workflow first.')
for name,target in [('OFL.txt','Medeo-OFL-1.1.txt'),('AUTHORS.txt','Medeo-AUTHORS.txt'),('sources.json','Medeo-sources.json')]:
    shutil.copy2(ROOT/'assets/fonts/medeo'/name,licenses/target)
brand=bundle/'brand';brand.mkdir(exist_ok=True)
for name in ['logo-primary.svg','logo-inverse.svg','app-icon.png','app-icon.ico','brand-notes.md']:
    shutil.copy2(ROOT/'output/wedex'/name,brand/name)
checksum=hashlib.sha256(source.read_bytes()).hexdigest()
if checksum!=hashlib.sha256((bundle/'WeDex-Sonora.exe').read_bytes()).hexdigest():
    raise RuntimeError('Packaged executable differs from release build')
archive=ROOT/'dist/WeDex-Medeo.zip'
# Cargo license files may have reproducible pre-1980 timestamps. Clamp only ZIP
# metadata, preserving the actual files and the source package timestamps.
with zipfile.ZipFile(archive,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=6,strict_timestamps=False) as z:
    for file in sorted(bundle.rglob('*')):
        if file.is_file():
            z.write(file,file.relative_to(bundle.parent))
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None
    assert hashlib.sha256(z.read('WeDex-Medeo/WeDex-Sonora.exe')).hexdigest()==checksum
manifest={'executable':'dist/WeDex-Medeo/WeDex-Sonora.exe','sha256':checksum,'archive':'dist/WeDex-Medeo.zip','fontsEmbedded':True,'fontLicenseIncluded':True}
(ROOT/'output/wedex/release.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(json.dumps({**manifest,'archiveBytes':archive.stat().st_size}))
