"""Build the shared Rust app and package an installable Android APK."""
from pathlib import Path
import argparse
import json
import os
import re
import shutil
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]

def licenses(cargo, env):
    destination = ROOT/'android/app/src/main/assets/licenses'
    destination.mkdir(parents=True, exist_ok=True)
    metadata = json.loads(subprocess.check_output([str(cargo),'metadata','--format-version','1','--locked','--filter-platform','aarch64-linux-android'],cwd=ROOT,env=env))
    inventory = ['# Dependency licenses', '', 'Includes target-specific and build dependencies.', '']
    for package in sorted(metadata['packages'], key=lambda p:(p['name'],p['version'])):
        inventory.append(f"- {package['name']} {package['version']}: {package.get('license') or 'see license file'}")
        source = Path(package['manifest_path']).parent
        target = destination/f"{package['name']}-{package['version']}"
        target.mkdir(exist_ok=True)
        files = [p for p in source.iterdir() if p.is_file() and re.match(r'^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)',p.name,re.I)]
        if package.get('license_file'): files.append(source/package['license_file'])
        for path in files:
            if path.is_file(): shutil.copy2(path,target/path.name)
    for source, name in [('LICENSE','WeDex-MIT.txt'),('THIRD_PARTY.md','THIRD_PARTY.md'),('src/fingerprinting/LICENSE-MIT','ShazamIO-Core-MIT.txt'),('assets/fonts/medeo/OFL.txt','Medeo-OFL.txt'),('assets/fonts/medeo/AUTHORS.txt','Medeo-AUTHORS.txt'),('assets/fontawesome/LICENSE.txt','FontAwesome-Free.txt')]:
        path=ROOT/source
        if path.is_file(): shutil.copy2(path,destination/name)
    inventory += ['', 'AndroidX GameActivity 4.4.0 and AppCompat 1.7.1: Apache-2.0.', 'AndroidX transitive libraries and Kotlin runtime retain their upstream Apache-2.0 notices.']
    (destination/'INVENTORY.md').write_text('\n'.join(inventory)+'\n',encoding='utf-8')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--abi', choices=['arm64-v8a', 'x86_64', 'all'], default='all')
    parser.add_argument('--rust-only', action='store_true')
    args = parser.parse_args()
    tools = json.loads((ROOT / '.local/android-tools.json').read_text(encoding='utf-8'))
    sdk, jdk, ndk = (Path(tools[name]) for name in ['sdk','jdk','ndk'])
    cargo = Path.home() / '.cargo/bin'/('cargo.exe' if os.name=='nt' else 'cargo')
    env = os.environ.copy()
    env.update(JAVA_HOME=str(jdk), ANDROID_HOME=str(sdk), ANDROID_SDK_ROOT=str(sdk), ANDROID_NDK_HOME=str(ndk))
    env['PATH'] = os.pathsep.join([str(cargo.parent), str(jdk/'bin'), str(sdk/'cmake/3.22.1/bin'), env.get('PATH','')])
    env['CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS'] = '-C link-arg=-Wl,-z,max-page-size=16384'
    env['CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS'] = '-C link-arg=-Wl,-z,max-page-size=16384'
    abis = ['arm64-v8a','x86_64'] if args.abi == 'all' else [args.abi]
    command = [str(cargo), 'ndk', '-o', str(ROOT/'android/app/src/main/jniLibs'), '--platform', '26']
    for abi in abis: command += ['-t', abi]
    subprocess.run(command + ['build','--lib','--release','--locked'], cwd=ROOT, env=env, check=True)
    if args.rust_only: return
    licenses(cargo, env)
    android = ROOT / 'android'
    (android/'local.properties').write_text('sdk.dir='+sdk.as_posix()+'\n',encoding='utf-8')
    resources = android/'app/src/main/res'
    (resources/'mipmap-xxxhdpi').mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT/'assets/brand/app-icon.png', resources/'mipmap-xxxhdpi/ic_launcher.png')
    (resources/'drawable').mkdir(exist_ok=True)
    for target, name in {'ic_play':'play','ic_pause':'pause','ic_previous':'skip-back','ic_next':'skip-forward','ic_music':'audio-lines'}.items():
        svg=ET.fromstring((ROOT/f'assets/fontawesome/{name}.svg').read_text(encoding='utf-8'))
        _,_,width,height=svg.attrib['viewBox'].split()
        paths=''.join('<path android:fillColor="#FFFFFFFF" android:pathData="'+p.attrib['d']+'"/>' for p in svg if p.tag.endswith('path'))
        (resources/f'drawable/{target}.xml').write_text('<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="24dp" android:height="24dp" android:viewportWidth="'+width+'" android:viewportHeight="'+height+'">'+paths+'</vector>',encoding='utf-8')
    gradle = Path(tools['gradle'])/'bin'/('gradle.bat' if os.name=='nt' else 'gradle')
    # Incremental APK replacement may leave unused aligned native-library
    # blocks behind. Recreate this generated file for a compact deliverable.
    apk=android/'app/build/outputs/apk/debug/app-debug.apk'
    apk.unlink(missing_ok=True)
    subprocess.run((['cmd','/d','/c'] if os.name=='nt' else [])+[str(gradle),'--no-daemon',':app:assembleDebug'],cwd=android,env=env,check=True)
    version=re.search(r'^version = "([^"]+)"',(ROOT/'Cargo.toml').read_text(encoding='utf-8'),re.M).group(1)
    destination=ROOT/f'dist/WeDex-Android-{version}'
    destination.mkdir(parents=True,exist_ok=True)
    shutil.copy2(apk,destination/f'WeDex-{version}-Android.apk')
    for name in ['LICENSE','THIRD_PARTY.md']:
        shutil.copy2(ROOT/name,destination/name)
    shutil.copy2(ROOT/'docs/ANDROID.md',destination/'README-Android.md')
    shutil.copy2(ROOT/'docs/ANDROID_VALIDATION.md',destination/'ANDROID_VALIDATION.md')
    print(destination,flush=True)

if __name__ == '__main__': main()
