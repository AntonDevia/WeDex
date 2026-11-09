// F is the foundation ledger injected by the MCP orchestration code.
const made = [], changed = [];
const remember = n => { made.push(n.id); return n; };
const rememberTree = n => { made.push(n.id,...n.findAll().map(c=>c.id)); return n; };
const vars = Object.fromEntries(await Promise.all(Object.entries(F.variables).map(async ([name,id])=>[name,await figma.variables.getVariableByIdAsync(id)])));
const localTextStyles=await Promise.all(Object.values(F.styles).map(id=>figma.getStyleByIdAsync(id)));
await Promise.all(localTextStyles.filter(Boolean).map(style=>figma.loadFontAsync(style.fontName)));
const rgb = hex => ({r:parseInt(hex.slice(1,3),16)/255,g:parseInt(hex.slice(3,5),16)/255,b:parseInt(hex.slice(5,7),16)/255});
function color(n,token,property='fills',opacity=1) {
  const p=figma.variables.setBoundVariableForPaint({type:'SOLID',color:{r:0,g:0,b:0},opacity},'color',vars[token]);
  n[property]=[p];
}
function box(parent,name,w,h,x=0,y=0,fill=null,radius=0,direction=null) {
  const n=remember(direction?figma.createAutoLayout(direction):figma.createFrame());
  n.name=name;n.resize(w,h);n.fills=[];parent.appendChild(n);n.x=x;n.y=y;
  if(direction){n.primaryAxisSizingMode='FIXED';n.counterAxisSizingMode='FIXED';}
  if(fill)color(n,fill);
  if(radius){n.cornerRadius=radius;if(vars['radius/'+radius])n.setBoundVariable('cornerRadius',vars['radius/'+radius]);}
  return n;
}
function padding(n,size,gap=0) {
  for(const prop of ['paddingTop','paddingBottom','paddingLeft','paddingRight']){n[prop]=size;if(vars['space/'+size])n.setBoundVariable(prop,vars['space/'+size]);}
  n.itemSpacing=gap;if(vars['space/'+gap])n.setBoundVariable('itemSpacing',vars['space/'+gap]);
}
async function text(parent,content,style='Body',token='text/primary',x=0,y=0,width=null) {
  const n=remember(figma.createText());n.name=content;parent.appendChild(n);
  n.fontName={family:F.font||'Medeo Sans',style:'Medium'};
  await n.setTextStyleIdAsync(F.styles[style]);n.characters=content;color(n,token);
  if(width!==null){n.textAutoResize='HEIGHT';n.resize(width,n.height);}else n.textAutoResize='WIDTH_AND_HEIGHT';
  n.x=x;n.y=y;return n;
}
async function glass(n,kind='Glass') {
  color(n,'surface/glass');color(n,'border/glass','strokes');n.strokeWeight=1;
  await n.setEffectStyleIdAsync(F.effects[kind]);return n;
}
function image(parent,name,key,size,x=0,y=0,radius=16) {
  const n=remember(figma.createRectangle());n.name=name;parent.appendChild(n);n.resize(size,size);n.x=x;n.y=y;n.cornerRadius=radius;
  n.fills=[{type:'IMAGE',imageHash:ART[key],scaleMode:'FILL'}];return n;
}
const iconNodes=Object.fromEntries(await Promise.all(Object.entries(F.icons).map(async([key,id])=>[key,await figma.getNodeByIdAsync(id)])));
function icon(parent,name,x=0,y=0,size=20,token='text/inverse') {
  const n=rememberTree(iconNodes[name].createInstance());parent.appendChild(n);n.name='icon/'+name;n.resize(size,size);n.x=x;n.y=y;
  for(const c of n.findAllWithCriteria({types:['VECTOR']})){if(c.strokes.length)color(c,token,'strokes');if(c.fills.length)color(c,token);}
  return n;
}
const iconButtonLibrary=await figma.importComponentSetByKeyAsync('e098805c9e6db6bfa6a87de61a8324a545d42501');
const iconButtonBase=iconButtonLibrary.children.find(n=>n.name==='Variant=Subtle, State=Default, Size=Medium');
function iconButton(parent,name,x,y,token='text/inverse',size=40) {
  const n=iconButtonBase.createInstance();parent.appendChild(n);n.name='button/'+name;n.resize(size,size);n.x=x;n.y=y;
  const iconKey=Object.keys(n.componentProperties).find(k=>k.split('#')[0]==='Icon');
  n.setProperties({[iconKey]:F.icons[name]});n.fills=[];n.strokes=[];n.cornerRadius=999;
  for(const c of n.findAllWithCriteria({types:['VECTOR']})){if(c.strokes.length)color(c,token,'strokes');if(c.fills.length)color(c,token);}
  return rememberTree(n);
}
function place(component,parent,name,x,y,w=null,h=null) {
  const n=rememberTree(component.createInstance());parent.appendChild(n);n.name=name;if(w&&h)n.resize(w,h);n.x=x;n.y=y;return n;
}
function pill(parent,label,x,y,w,light=false){
  const p=box(parent,'pill/'+label,w,32,x,y,light?'surface/tint':'surface/glass',999,'HORIZONTAL');padding(p,8,8);p.primaryAxisAlignItems='CENTER';p.counterAxisAlignItems='CENTER';
  return text(p,label,'Small',light?'text/secondary':'text/inverse').then(()=>p);
}
function navAction(destination,kind='NAVIGATE',duration=0.35) {return {type:'NODE',destinationId:destination,navigation:kind,transition:{type:'SMART_ANIMATE',duration,easing:{type:'EASE_IN_AND_OUT'}},resetScrollPosition:false,resetInteractiveComponents:false};}
async function click(node,destination,duration=0.35){await node.setReactionsAsync([{trigger:{type:'ON_CLICK'},actions:[navAction(destination,'NAVIGATE',duration)]}]);changed.push(node.id);}
async function back(node){await node.setReactionsAsync([{trigger:{type:'ON_CLICK'},actions:[{type:'BACK'}]}]);changed.push(node.id);}
async function componentText(component,label,node,defaultValue){const key=component.addComponentProperty(label,'TEXT',defaultValue);node.componentPropertyReferences={characters:key};return key;}
function allNodes(root){return [root,...root.findAll()];}
