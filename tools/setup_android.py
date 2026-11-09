"""Install a reproducible Android toolchain from official upstream distributions."""
from pathlib import Path
import concurrent.futures
import hashlib
import json
import os
import subprocess
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TOOLS = Path(os.environ.get('WEDEX_ANDROID_TOOLS', str(ROOT / '.local/android-toolchain')))
SDK = TOOLS / 'sdk'

def download(url, path, algorithm, expected):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists() and hashlib.new(algorithm, path.read_bytes()).hexdigest() == expected:
        return path
    print(f'Downloading {path.name}', flush=True)
    temporary = path.with_suffix(path.suffix + '.partial')
    with urllib.request.urlopen(url, timeout=90) as response, temporary.open('wb') as output:
        while chunk := response.read(1024 * 1024):
            output.write(chunk)
    actual = hashlib.new(algorithm, temporary.read_bytes()).hexdigest()
    if actual != expected:
        raise RuntimeError(f'Checksum mismatch: {path.name}')
    temporary.replace(path)
    return path

def unpack(archive, destination):
    destination.mkdir(parents=True, exist_ok=True)
    target = destination.resolve()
    with zipfile.ZipFile(archive) as bundle:
        for name in bundle.namelist():
            if not (target / name).resolve().is_relative_to(target):
                raise RuntimeError('Unsafe archive path')
        bundle.extractall(target)

def install_sdk():
    executable = SDK / 'cmdline-tools/19.0/bin/sdkmanager.bat'
    if executable.exists():
        return
    repository = ET.fromstring(urllib.request.urlopen('https://dl.google.com/android/repository/repository2-1.xml', timeout=60).read())
    package = next(p for p in repository.findall('remotePackage') if p.attrib['path'] == 'cmdline-tools;19.0')
    archive = next(a for a in package.findall('./archives/archive') if a.findtext('host-os') == 'windows').find('complete')
    path = download('https://dl.google.com/android/repository/' + archive.findtext('url'), TOOLS / 'downloads/android-commandline.zip', 'sha1', archive.findtext('checksum'))
    unpack(path, TOOLS / 'sdk-commandline-unpack')
    source = TOOLS / 'sdk-commandline-unpack/cmdline-tools'
    executable.parent.parent.parent.mkdir(parents=True, exist_ok=True)
    source.rename(SDK / 'cmdline-tools/19.0')

def install_jdk():
    folder = TOOLS / 'jdk'
    installed = list(folder.glob('*/bin/javac.exe')) if folder.exists() else []
    if installed:
        return installed[0].parent.parent
    base = 'https://aka.ms/download-jdk/microsoft-jdk-21-windows-x64.zip'
    checksum = urllib.request.urlopen(base + '.sha256sum.txt', timeout=60).read().decode().split()[0]
    archive = download(base, TOOLS / 'downloads/jdk-21.zip', 'sha256', checksum)
    unpack(archive, folder)
    return next(folder.glob('*/bin/javac.exe')).parent.parent

def install_gradle():
    folder = TOOLS / 'gradle-8.11.1'
    if not (folder / 'bin/gradle.bat').exists():
        url = 'https://downloads.gradle.org/distributions/gradle-8.11.1-bin.zip'
        checksum = urllib.request.urlopen(url + '.sha256', timeout=60).read().decode().strip()
        try:
            archive = download(url, TOOLS / 'downloads/gradle-8.11.1.zip', 'sha256', checksum)
        except Exception:
            # Verify the mirror bytes against the checksum from the official Gradle distribution service.
            archive = download('https://repo.huaweicloud.com/gradle/gradle-8.11.1-bin.zip', TOOLS / 'downloads/gradle-8.11.1.zip', 'sha256', checksum)
        unpack(archive, TOOLS)
    return folder

def main():
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        sdk_future = pool.submit(install_sdk)
        jdk_future = pool.submit(install_jdk)
        gradle_future = pool.submit(install_gradle)
        sdk_future.result()
        jdk = jdk_future.result()
        gradle = gradle_future.result()
    environment = os.environ.copy()
    environment.update(JAVA_HOME=str(jdk), ANDROID_HOME=str(SDK), ANDROID_SDK_ROOT=str(SDK))
    environment['PATH'] = str(jdk / 'bin') + os.pathsep + environment.get('PATH', '')
    manager = SDK / 'cmdline-tools/19.0/bin/sdkmanager.bat'
    subprocess.run(['cmd', '/d', '/c', str(manager), '--sdk_root=' + str(SDK), '--licenses'], input='y\n' * 100, text=True, env=environment, check=True)
    packages = ['platform-tools', 'platforms;android-35', 'build-tools;35.0.0', 'ndk;28.2.13676358', 'cmake;3.22.1', 'emulator', 'system-images;android-35;google_apis;x86_64']
    subprocess.run(['cmd', '/d', '/c', str(manager), '--sdk_root=' + str(SDK), *packages], input='y\n' * 100, text=True, env=environment, check=True)
    cargo_bin = Path.home() / '.cargo/bin'
    subprocess.run([str(cargo_bin / 'rustup.exe'), 'target', 'add', 'aarch64-linux-android', 'x86_64-linux-android'], check=True)
    if not (cargo_bin / 'cargo-ndk.exe').exists():
        subprocess.run([str(cargo_bin / 'cargo.exe'), 'install', 'cargo-ndk', '--version', '4.1.2', '--locked'], check=True)
    configuration = {'sdk': str(SDK), 'jdk': str(jdk), 'gradle': str(gradle), 'ndk': str(SDK / 'ndk/28.2.13676358'), 'api': 35}
    (ROOT / '.local').mkdir(exist_ok=True)
    (ROOT / '.local/android-tools.json').write_text(json.dumps(configuration, indent=2), encoding='utf-8')
    print(json.dumps(configuration, indent=2), flush=True)

if __name__ == '__main__':
    main()
