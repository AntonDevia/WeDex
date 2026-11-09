from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'output/wedex'
font_path=ROOT/'assets/fonts/medeo/MedeoSans-Regular.ttf'
display=ROOT/'assets/fonts/medeo/MedeoSansDisplay-Medium.ttf'
font=lambda size: ImageFont.truetype(str(font_path),size)

def place(canvas,file,box,color=None):
    im=Image.open(OUT/file).convert('RGBA')
    im.thumbnail((box[2],box[3]),Image.Resampling.LANCZOS)
    if color:
        rgb=Image.new('RGBA',im.size,color);rgb.putalpha(im.getchannel('A'));im=rgb
    canvas.alpha_composite(im,(box[0]+(box[2]-im.width)//2,box[1]+(box[3]-im.height)//2))

board=Image.new('RGBA',(1800,1380),'#18171B');d=ImageDraw.Draw(board)
d.text((84,68),'wedex / identity',font=ImageFont.truetype(str(display),42),fill='#F7F6F2')
d.text((84,126),'Три направления · один характер музыки',font=font(22),fill='#9F9B9F')
for i,(name,subtitle) in enumerate([('01 / Flow','Плавная w · мягкие окончания · основной вариант'),('02 / Crossfade','Два канала в букве x · более технологичный характер'),('03 / Contour','Одна линия · геометрический ритм · лёгкая форма')]):
    top=213+i*365
    d.rounded_rectangle((64,top,1736,top+324),radius=30,fill='#222126',outline='#3C3A41',width=1)
    d.text((96,top+28),name,font=font(20),fill='#E7F7A3' if i==0 else '#A4A0A5')
    place(board,['concept-01-flow.png','concept-02-crossfade.png','concept-03-contour.png'][i],(260,top+66,1280,164))
    d.text((96,top+268),subtitle,font=font(20),fill='#B4B0B5')
board.convert('RGB').save(OUT/'concepts-preview.png')

preview=Image.new('RGBA',(1800,1200),'#F7F6F2');d=ImageDraw.Draw(preview)
d.rectangle((0,0,1800,680),fill='#18171B')
d.text((80,60),'wedex',font=font(22),fill='#AAA5AC')
d.text((80,99),'МУЗЫКА БЛИЖЕ',font=font(15),fill='#77727C')
place(preview,'logo-inverse.png',(260,205,1280,338))
d.text((80,615),'Medeo Sans / custom wordmark',font=font(19),fill='#AAA5AC')
d.line((80,749,1720,749),fill='#D7D3D8',width=1)
place(preview,'logo-primary.png',(120,810,650,175))
place(preview,'app-icon.png',(1060,815,180,180))
place(preview,'symbol.png',(1360,815,180,180))
d.text((120,1054),'Основной знак · SVG в кривых',font=font(21),fill='#615C65')
d.text((1060,1054),'Иконка приложения',font=font(21),fill='#615C65')
preview.convert('RGB').save(OUT/'logo-preview.png')

small=Image.new('RGBA',(1200,500),'#18171B');d=ImageDraw.Draw(small)
d.text((48,36),'Проверка малых размеров',font=font(24),fill='#F7F6F2')
for y,width in [(128,240),(248,152),(354,96)]:
    image=Image.open(OUT/'logo-inverse.png').convert('RGBA');image=image.resize((width,round(image.height*width/image.width)),Image.Resampling.LANCZOS)
    small.alpha_composite(image,(64,y));d.text((380,y),str(width)+' px',font=font(20),fill='#A4A0A5')
for i,size in enumerate([64,32,24]):
    image=Image.open(OUT/'app-icon.png').convert('RGBA').resize((size,size),Image.Resampling.LANCZOS)
    small.alpha_composite(image,(800+i*120,160));d.text((800+i*120,250),str(size)+' px',font=font(18),fill='#A4A0A5')
small.convert('RGB').save(OUT/'size-check.png')

Image.open(OUT/'app-icon.png').save(OUT/'app-icon.ico',sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])
print('Composed identity boards and small-size checks.')
