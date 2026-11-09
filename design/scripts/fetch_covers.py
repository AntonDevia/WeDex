import json
import pathlib
import urllib.parse
import urllib.request

root = pathlib.Path(__file__).resolve().parents[1] / 'assets'
root.mkdir(exist_ok=True)
requests = [('charlie', 'Charlie Puth Voicenotes'), ('weeknd', 'The Weeknd After Hours'), ('rihanna', 'Rihanna ANTI'), ('massive', 'Massive Attack Mezzanine'), ('arctic', 'Arctic Monkeys AM'), ('oneheart', 'Øneheart snowfall')]
manifest = []
for name, query in requests:
    url = 'https://itunes.apple.com/search?' + urllib.parse.urlencode({'term': query, 'entity': 'album', 'limit': 5})
    with urllib.request.urlopen(url, timeout=25) as response:
        found = json.load(response)['results']
    if not found:
        raise RuntimeError(query)
    album = found[0]
    image_url = album['artworkUrl100'].replace('100x100bb', '600x600bb')
    with urllib.request.urlopen(image_url, timeout=25) as response:
        (root / (name + '.jpg')).write_bytes(response.read())
    manifest.append({'name': name, 'artist': album.get('artistName'), 'album': album.get('collectionName'), 'url': image_url, 'source': album.get('collectionViewUrl')})
(root / 'sources.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps(manifest, ensure_ascii=True))
