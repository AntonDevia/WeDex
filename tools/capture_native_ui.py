"""Capture isolated, deterministic native UI fixtures without account/network IO."""
import os
from pathlib import Path
import subprocess
import sys

root=Path(__file__).resolve().parents[1]
folder=root/'.local/ui-captures'
folder.mkdir(parents=True,exist_ok=True)
for name in sys.argv[1:] or ['sonora']:
    env=os.environ.copy()
    suffix='-'+env['WEDEX_PREVIEW_SIZE'] if env.get('WEDEX_PREVIEW_SIZE') else ''
    path=folder/f'{name}{suffix}.png'
    env.update(WEDEX_PREVIEW=name,WEDEX_REDUCED_MOTION='1',EFRAME_SCREENSHOT_TO=str(path))
    startup=subprocess.STARTUPINFO()
    startup.dwFlags|=subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow=0
    result=subprocess.run([str(root/'target/release/sonora.exe')],cwd=root,env=env,startupinfo=startup,timeout=30,capture_output=True)
    print(f'{path.name}: exit={result.returncode}, png={path.exists()}')
    if result.returncode:
        print(result.stderr.decode('utf8',errors='replace')[-3000:])
        raise SystemExit(result.returncode)
