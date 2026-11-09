if(figma.root.children.some(p=>p.name==='03 · Brand'))throw new Error('Brand page already exists. Inspect its ledger before retrying.');
const page=figma.createPage();page.name='03 · Brand';await figma.setCurrentPageAsync(page);
const createdNodeIds=[page.id];
const remember=n=>{createdNodeIds.push(n.id);return n;};
const variables={};for(const [key,id]of Object.entries({canvas:'VariableID:3:18',paper:'VariableID:3:19',glass:'VariableID:3:21',inverse:'VariableID:3:26',primary:'VariableID:3:24',secondary:'VariableID:3:25',accent:'VariableID:3:27'}))variables[key]=await figma.variables.getVariableByIdAsync(id);
const paint=(key)=>figma.variables.setBoundVariableForPaint({type:'SOLID',color:{r:1,g:1,b:1}},'color',variables[key]);
const frame=(parent,name,w,h,direction='VERTICAL',fill=null,padding=0)=>{const n=remember(figma.createFrame());parent.appendChild(n);n.name=name;n.resize(w,h);n.layoutMode=direction;n.primaryAxisSizingMode='FIXED';n.counterAxisSizingMode='FIXED';n.itemSpacing=16;n.fills=fill?[paint(fill)]:[];for(const k of ['paddingLeft','paddingRight','paddingTop','paddingBottom'])n[k]=padding;return n;};
await figma.loadFontAsync({family:'Medeo Sans',style:'Regular'});await figma.loadFontAsync({family:'Medeo Sans',style:'Medium'});await figma.loadFontAsync({family:'Medeo Sans Display',style:'SemiBold'});
async function label(parent,value,size=14,color='inverse',heading=false){const n=remember(figma.createText());parent.appendChild(n);n.fontName={family:heading?'Medeo Sans Display':'Medeo Sans',style:heading?'SemiBold':'Regular'};n.fontSize=size;n.characters=value;n.fills=[paint(color)];n.textAutoResize='WIDTH_AND_HEIGHT';return n;}
function tree(n){createdNodeIds.push(n.id);if('children'in n)for(const c of n.children)tree(c);}
const master=await figma.getNodeByIdAsync('23:3019');
function wordmark(parent,width,dark=false){const i=master.createInstance();parent.appendChild(i);i.name=dark?'wedex / dark':'wedex / light';i.rescale(width/master.width);tree(i);if(dark){const recolor=n=>{if('fills'in n&&Array.isArray(n.fills))n.fills=n.fills.map(p=>p.type==='SOLID'?figma.variables.setBoundVariableForPaint(p,'color',variables.primary):p);if('strokes'in n)n.strokes=n.strokes.map(p=>p.type==='SOLID'?figma.variables.setBoundVariableForPaint(p,'color',variables.primary):p);if('children'in n)for(const c of n.children)recolor(c);};recolor(i);}return i;}
function svg(parent,source,width){const n=figma.createNodeFromSvg(source);parent.appendChild(n);n.rescale(width/n.width);tree(n);return n;}
const board=frame(page,'wedex · Identity / Flow',1440,1240,'VERTICAL','canvas',72);board.x=160;board.y=160;board.itemSpacing=32;board.primaryAxisSizingMode='AUTO';board.cornerRadius=32;
const head=frame(board,'Identity header',1296,80);head.itemSpacing=4;await label(head,'wedex · Flow',36,'inverse',true);await label(head,'Словесный знак и типографика музыкального приложения',16,'inverse');
const hero=frame(board,'Primary wordmark',1296,300,'HORIZONTAL');hero.primaryAxisAlignItems='CENTER';hero.counterAxisAlignItems='CENTER';wordmark(hero,800);
const applications=frame(board,'Applications',1296,240,'HORIZONTAL');applications.itemSpacing=24;
const light=frame(applications,'Light surface',624,240,'VERTICAL','paper',32);light.cornerRadius=24;light.primaryAxisAlignItems='CENTER';wordmark(light,360,true);await label(light,'Основная версия · один цвет',14,'secondary');
const dark=frame(applications,'Application icon',648,240,'HORIZONTAL','glass',32);dark.cornerRadius=24;dark.counterAxisAlignItems='CENTER';svg(dark,ICON_SVG,132);const caption=frame(dark,'Icon caption',380,90);caption.itemSpacing=8;await label(caption,'Компактная w',24,'inverse',true);await label(caption,'Иконка приложения и небольшие размеры',14,'inverse');
const concepts=frame(board,'Directions',1296,250,'HORIZONTAL');concepts.itemSpacing=24;
for(const [i,source,title,subtitle]of [[0,FLOW_SVG,'01 · Flow','Выбран: плавность и близость'],[1,CROSSFADE_SVG,'02 · Crossfade','Более технологичный характер'],[2,CONTOUR_SVG,'03 · Contour','Лёгкий геометрический контур']]){
 const card=frame(concepts,'Direction/'+title,416,250,'VERTICAL','glass',24);card.cornerRadius=20;card.itemSpacing=16;
 await label(card,title,16,i===0?'accent':'inverse');
 const art=frame(card,'Wordmark specimen',368,112,'HORIZONTAL');art.primaryAxisAlignItems='CENTER';art.counterAxisAlignItems='CENTER';svg(art,source,336);
 await label(card,subtitle,13,'inverse');
}
const type=frame(board,'Typography',1296,110);type.itemSpacing=8;await label(type,'Medeo Sans / Medeo Sans Display',24,'inverse',true);await label(type,'Музыка ближе. Ваш поток, любимые треки и новые маршруты.',18,'inverse');await label(type,'Интерфейс 12–18 px · Заголовки 24–76 px · Кириллица и латиница',14,'inverse');
return {createdNodeIds,mutatedNodeIds:[],pageId:page.id,boardId:board.id,wordmarkId:master.id,width:board.width,height:board.height};
