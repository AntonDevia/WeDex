"""Install the explicitly selected Medeo static fonts for the current Windows user."""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import winreg

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / '.local/design-python'))
from fontTools.ttLib import TTFont

source = ROOT / 'assets/fonts/medeo'
destination = Path(os.environ['LOCALAPPDATA']) / 'Microsoft/Windows/Fonts'
destination.mkdir(parents=True, exist_ok=True)
records = []
with winreg.CreateKey(winreg.HKEY_CURRENT_USER, r'Software\Microsoft\Windows NT\CurrentVersion\Fonts') as registry:
    for file in sorted(source.glob('*.ttf')):
        font = TTFont(file)
        full_name = font['name'].getDebugName(4)
        family = font['name'].getDebugName(16) or font['name'].getDebugName(1)
        style = font['name'].getDebugName(17) or font['name'].getDebugName(2)
        cmap = font.getBestCmap()
        missing = [c for c in 'wedexПоискПонравившиесяЁёЙйЩщЫы0123456789' if ord(c) not in cmap]
        if missing:
            raise RuntimeError(f'{file.name}: missing required glyphs {missing}')
        target = destination / file.name
        if not target.exists() or target.read_bytes() != file.read_bytes():
            shutil.copyfile(file, target)
        winreg.SetValueEx(registry, full_name + ' (TrueType)', 0, winreg.REG_SZ, str(target))
        loaded = ctypes.windll.gdi32.AddFontResourceExW(str(target), 0, None)
        records.append({'file': file.name, 'family': family, 'style': style, 'fullName': full_name, 'sha256': hashlib.sha256(file.read_bytes()).hexdigest(), 'loadedResources': loaded})
result = ctypes.c_size_t()
ctypes.windll.user32.SendMessageTimeoutW(0xFFFF, 0x001D, 0, 0, 2, 1000, ctypes.byref(result))
(ROOT / '.local/medeo-installation.json').write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps([{'family': r['family'], 'style': r['style'], 'loaded': r['loadedResources']} for r in records], ensure_ascii=False))
