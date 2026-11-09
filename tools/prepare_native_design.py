"""Compile the measured Figma scene into the native Rust renderer's assets.

No screen captures are used as UI assets. Icons and cover images are the original
files imported into the Figma document; geometry and typography are live reads.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
data = json.loads((ROOT / 'design/native-scene.json').read_text(encoding='utf-8'))
foundation = json.loads((ROOT / 'design/figma-state.json').read_text(encoding='utf-8'))['foundation']
icons = sorted((int(v.split(':')[1]), k) for k, v in foundation['icons'].items())
text_layout = json.loads((ROOT/'design/native-text-layout.json').read_text(encoding='utf8'))
asset_manifest=json.loads((ROOT/'design/native-assets.json').read_text(encoding='utf8'))
hashes={v['imageHash']:key for key,v in asset_manifest.items()}

def prepare(n):
    kids = n.get('children', [])
    if n['type']=='TEXT':
        layout=text_layout.get(n['id'],{})
        n['originalCharacters']=n.get('characters','')
        n['naturalWidth']=layout.get('width',n.get('width',0)) if layout.get('resize')=='WIDTH_AND_HEIGHT' else 0
    if n['type'] == 'INSTANCE' and kids and all(c['type'] == 'VECTOR' for c in kids):
        base = kids[0]['id'].split(';')[-1]
        if base.startswith('3:'):
            n['asset'] = next(name for number, name in icons if number >= int(base.split(':')[1]))
            paint = (kids[0].get('strokes') or kids[0].get('fills') or [{}])[0]
            n['tint'] = paint.get('color', {'r': 1, 'g': 1, 'b': 1})
    if 'Wordmark' in n['name'] or n['name'] == 'WeDex / Flow':
        n['asset'] = 'wordmark'
    for f in n.get('fills', []):
        if f.get('type') == 'IMAGE':
            f['source'] = hashes[f['imageHash']]
    for c in kids:
        prepare(c)

for root in data['roots']:
    prepare(root)
target = ROOT / 'assets/ui'
target.mkdir(parents=True, exist_ok=True)
(target / 'scene.json').write_text(json.dumps(data, ensure_ascii=False, separators=(',', ':')), encoding='utf-8')
print(f'Compiled {len(data["roots"])} measured Figma states.')

rust = ['// Generated from the measured Figma scene and current asset library.', 'pub fn bytes(name: &str) -> Option<&\'static [u8]> {', '    Some(match name {']
for file in sorted((ROOT / 'assets/fontawesome').glob('*.svg')):
    rust.append(f'        "{file.stem}" => include_bytes!("../../assets/ui/{file.stem}.png"),')
for file in sorted((ROOT / 'design/assets').glob('*.jpg')):
    rust.append(f'        "{file.stem}" => include_bytes!("../../design/assets/{file.name}"),')
rust += ['        "provider-hitmo" => include_bytes!("../../assets/providers/hitmo.png"),', '        "provider-youtube" => include_bytes!("../../assets/providers/youtube.png"),', '        "provider-yandex" => include_bytes!("../../assets/providers/yandex.png"),', '        "provider-lastfm" => include_bytes!("../../assets/providers/lastdotfm.png"),', '        "provider-soundcloud" => include_bytes!("../../assets/providers/soundcloud.png"),', '        "wordmark-light" => include_bytes!("../../assets/brand/logo-primary.png"),', '        "app-symbol" => include_bytes!("../../assets/brand/app-icon.png"),', '        "wordmark" => include_bytes!("../../assets/brand/logo-inverse.png"),', '        _ => return None,', '    })', '}']
(ROOT / 'src/glass/assets.rs').write_text('\n'.join(rust)+'\n', encoding='utf-8')
