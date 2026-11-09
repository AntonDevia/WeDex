"""Build editable vector wordmark directions; font glyphs are converted to paths."""
from pathlib import Path
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / '.local/design-python'))
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

OUT = ROOT / 'output/wedex'
OUT.mkdir(parents=True, exist_ok=True)
FONTS = ROOT / 'assets/fonts/medeo'
FONT = TTFont(FONTS / 'MedeoSansDisplay-SemiBold.ttf')
GLYPHS = FONT.getGlyphSet()
CMAP = FONT.getBestCmap()
INK, PAPER, LIME = '#18171B', '#F7F6F2', '#E7F7A3'

WAVE = 'M70 490 L180 80 C194 12 257 -3 285 78 L384 389 C390 410 403 410 409 389 L508 78 C536 -3 599 12 613 80 L724 490'
CROSS = 'M58 490 L459 38 M459 490 L58 38'

def glyph(char, x=0, y=0):
    pen = SVGPathPen(GLYPHS)
    GLYPHS[CMAP[ord(char)]].draw(TransformPen(pen, (1, 0, 0, 1, x, y)))
    return pen.getCommands()

def doc(body, width, height, title='wedex', background=None):
    bg = f'<rect width="{width}" height="{height}" fill="{background}"/>' if background else ''
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" role="img" aria-label="{title}"><title>{title}</title>{bg}{body}</svg>\n'

def flow(color=INK, accent=None):
    x = 762
    letters = []
    for char, advance in [('e', 539), ('d', 551), ('e', 529)]:
        letters.append(glyph(char, x))
        x += advance
    body = f'<g transform="translate(36 772) scale(1 -1)" fill="{color}"><path d="{" ".join(letters)}"/>'
    body += f'<path d="{WAVE}" fill="none" stroke="{color}" stroke-width="110" stroke-linecap="round" stroke-linejoin="round"/>'
    body += f'<path transform="translate({x} 0)" d="{CROSS}" fill="none" stroke="{accent or color}" stroke-width="110" stroke-linecap="round"/></g>'
    return doc(body, x + 600, 838)

def crossfade():
    x = 10
    letters = []
    for char in 'wede':
        letters.append(glyph(char, x))
        x += FONT['hmtx'][CMAP[ord(char)]][0] - 30
    # Open diagonal crossover: recognisable x, with two separately shaped ribbons.
    ribbon = f'<path d="M{x+15} 534 L{x+148} 534 L{x+550} 0 L{x+417} 0 Z" fill="{LIME}"/><path d="M{x+409} 534 L{x+543} 534 L{x+322} 239 L{x+254} 329 Z M{x+217} 272 L{x+284} 182 L{x+147} 0 L{x+13} 0 Z" fill="{LIME}"/>'
    return doc(f'<g transform="translate(40 772) scale(1 -1)" fill="{PAPER}"><path d="{" ".join(letters)}"/>{ribbon}</g>', x+620, 838)

def contour():
    paths = [
        'M32 27 L45 73 Q49 83 54 73 L66 36 L79 73 Q83 83 87 73 L100 27',
        'M123 52 H171 C171 16 121 16 121 52 C121 83 151 91 170 70',
        'M236 8 V78 M236 53 C236 16 186 16 186 53 C186 90 236 90 236 53',
        'M259 52 H307 C307 16 257 16 257 52 C257 83 287 91 306 70',
        'M330 27 L369 78 M369 27 L330 78',
    ]
    return doc('<g fill="none" stroke="'+PAPER+'" stroke-width="9" stroke-linecap="round" stroke-linejoin="round">'+''.join(f'<path d="{p}"/>' for p in paths)+'</g>', 400, 104)

(OUT/'logo-primary.svg').write_text(flow(INK), encoding='utf-8')
(OUT/'logo-monochrome.svg').write_text(flow('#000000'), encoding='utf-8')
(OUT/'logo-inverse.svg').write_text(flow(PAPER), encoding='utf-8')
(OUT/'logo-accent.svg').write_text(flow(PAPER,LIME), encoding='utf-8')
(OUT/'concept-01-flow.svg').write_text(flow(PAPER), encoding='utf-8')
(OUT/'concept-02-crossfade.svg').write_text(crossfade(), encoding='utf-8')
(OUT/'concept-03-contour.svg').write_text(contour(), encoding='utf-8')
symbol = f'<g transform="translate(53 728) scale(1 -1)" fill="none" stroke="{INK}" stroke-width="110" stroke-linecap="round" stroke-linejoin="round"><path d="{WAVE}"/></g>'
(OUT/'symbol.svg').write_text(doc(symbol,900,900), encoding='utf-8')
(OUT/'symbol-inverse.svg').write_text(doc(symbol.replace(INK,PAPER),900,900), encoding='utf-8')
tile = '<rect x="0" y="0" width="900" height="900" rx="228" fill="'+INK+'"/>'+symbol.replace(INK,LIME)
(OUT/'app-icon.svg').write_text(doc(tile,900,900), encoding='utf-8')

manifest = {'repository':'https://github.com/DNFDEVS/medeo-font','revision':'72c9ca7cbd091f8b85ff2cee26e3dcc4e762f1a7','license':'SIL OFL 1.1','fonts':{f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(FONTS.glob('*.ttf'))}}
(FONTS/'sources.json').write_text(json.dumps(manifest,indent=2)+'\n', encoding='utf-8')
print(json.dumps({'svgFiles':len(list(OUT.glob('*.svg'))),'wordmark':'wedex','font':'Medeo Sans Display SemiBold','primary':'01 — Flow'},ensure_ascii=False))
