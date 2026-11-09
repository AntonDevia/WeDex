"""Normalize visible logo bounds, preserving proportions and transparency."""
from pathlib import Path
from PIL import Image

folder=Path(__file__).resolve().parents[1]/'assets/providers'
for name in ['yandex','lastdotfm','soundcloud']:
    raw=Image.open(folder/f'{name}-raw.png').convert('RGBA')
    logo=raw.crop(raw.getbbox())
    scale=112/max(logo.size)
    logo=logo.resize((round(logo.width*scale),round(logo.height*scale)),Image.Resampling.LANCZOS)
    canvas=Image.new('RGBA',(128,128))
    canvas.alpha_composite(logo,((128-logo.width)//2,(128-logo.height)//2))
    canvas.save(folder/f'{name}.png')
