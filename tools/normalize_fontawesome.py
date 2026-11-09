from pathlib import Path
from PIL import Image
root=Path(__file__).resolve().parents[1];folder=root/'assets/fontawesome'
for raw in folder.glob('*-raw.png'):
    im=Image.open(raw).convert('RGBA');im=im.crop(im.getbbox());scale=80/max(im.size);im=im.resize((max(1,round(im.width*scale)),max(1,round(im.height*scale))),Image.Resampling.LANCZOS);canvas=Image.new('RGBA',(96,96));canvas.alpha_composite(im,((96-im.width)//2,(96-im.height)//2));canvas.save(root/'assets/ui'/(raw.name.removesuffix('-raw.png')+'.png'))
