const kit=await figma.getNodeByIdAsync(F.pages.kit);await figma.setCurrentPageAsync(kit);
if(kit.children.some(n=>n.name==='WeDex/Play'))throw new Error('Components already exist. Recover from ledger.');
const C={},props={};
function base(name,w,h,direction='HORIZONTAL'){
  const n=remember(figma.createComponent());n.name=name;n.resize(w,h);n.fills=[];n.layoutMode=direction;n.primaryAxisSizingMode='FIXED';n.counterAxisSizingMode='FIXED';n.primaryAxisAlignItems='CENTER';n.counterAxisAlignItems='CENTER';kit.appendChild(n);return n;
}
function combine(name,nodes,x,y,columns=2){
  const set=remember(figma.combineAsVariants(nodes,kit));set.name=name;set.x=x;set.y=y;
  const w=Math.max(...nodes.map(n=>n.width)),h=Math.max(...nodes.map(n=>n.height));
  nodes.forEach((n,i)=>{n.x=24+(i%columns)*(w+20);n.y=24+Math.floor(i/columns)*(h+20);});
  set.resize(columns*(w+20)+28,Math.ceil(nodes.length/columns)*(h+20)+28);return set;
}
const play=[];
for(const state of ['Playing','Paused']){
 const n=base('State='+state,48,48);n.cornerRadius=999;color(n,'accent/action');icon(n,state==='Playing'?'pause':'play',0,0,20,'text/primary');play.push(n);
}
const playSet=combine('WeDex/Play',play,160,700);playSet.description='Play/pause. Prototype state changes on click. Lime accent is reserved for the primary playback action.';
for(let i=0;i<play.length;i++)await play[i].setReactionsAsync([{trigger:{type:'ON_CLICK'},actions:[navAction(play[1-i].id,'CHANGE_TO',0.18)]}]);C.play=play.map(n=>n.id);C.playSet=playSet.id;
const toggle=[];
for(const tone of ['Dark','Light'])for(const state of ['Off','On']){
 const n=base('Tone='+tone+', State='+state,40,40);n.cornerRadius=999;
 if(state==='On')color(n,tone==='Dark'?'surface/glass-strong':'surface/tint');
 const g=icon(n,'heart',0,0,20,state==='On'&&tone==='Dark'?'accent/action':tone==='Dark'?'text/inverse':'text/primary');
 const p=n.addComponentProperty('Glyph','INSTANCE_SWAP',F.icons.heart);g.componentPropertyReferences={mainComponent:p};toggle.push(n);
}
const toggleSet=combine('WeDex/Toggle',toggle,440,700);toggleSet.description='Like and shuffle use an icon swap, with selected/unselected states. Tone adapts to light or glass surfaces.';
for(let i=0;i<toggle.length;i++)await toggle[i].setReactionsAsync([{trigger:{type:'ON_CLICK'},actions:[navAction(toggle[i%2===0?i+1:i-1].id,'CHANGE_TO',0.18)]}]);
C.toggle=toggle.map(n=>n.id);C.toggleSet=toggleSet.id;
const volume=[];for(const state of ['Sound','Muted']){
 const n=base('State='+state,40,40);n.cornerRadius=999;icon(n,state==='Sound'?'volume-2':'volume-x');volume.push(n);
}
const volumeSet=combine('WeDex/Volume',volume,720,700);for(let i=0;i<2;i++)await volume[i].setReactionsAsync([{trigger:{type:'ON_CLICK'},actions:[navAction(volume[1-i].id,'CHANGE_TO',0.18)]}]);C.volume=volume.map(n=>n.id);
const buttons=[];
for(const tone of ['Glass','Primary','Quiet','Paper']){
 const n=base('Tone='+tone,168,44);n.cornerRadius=999;n.setBoundVariable('cornerRadius',vars['radius/999']);padding(n,12,8);
 if(tone==='Glass')await glass(n);else if(tone==='Primary')color(n,'accent/action');else if(tone==='Paper')color(n,'surface/tint');
 const label=await text(n,'Продолжить','Body',tone==='Glass'?'text/inverse':'text/primary');
 props['buttonLabel']=await componentText(n,'Label',label,'Продолжить');buttons.push(n);
}
const buttonSet=combine('WeDex/Button',buttons,1000,700);C.button=Object.fromEntries(buttons.map((n,i)=>[['Glass','Primary','Quiet','Paper'][i],n.id]));
const navs=[];for(const state of ['Default','Active']){
 const n=base('State='+state,156,48);n.primaryAxisAlignItems='MIN';n.cornerRadius=16;padding(n,12,12);
 if(state==='Active')color(n,'surface/glass-strong');
 const g=icon(n,'audio-lines',0,0,20,state==='Active'?'accent/action':'text/inverse');
 const gp=n.addComponentProperty('Glyph','INSTANCE_SWAP',F.icons['audio-lines']);g.componentPropertyReferences={mainComponent:gp};
 const label=await text(n,'Sonora','Body','text/inverse');await componentText(n,'Label',label,'Sonora');navs.push(n);
}
const navSet=combine('WeDex/Navigation',navs,160,980);C.nav=navs.map(n=>n.id);
const card=base('WeDex/Album card',312,390,'VERTICAL');card.x=560;card.y=980;card.primaryAxisAlignItems='MIN';padding(card,12,16);card.cornerRadius=28;await glass(card);
const art=image(card,'Cover','charlie',288,0,0,20);
const captions=box(card,'Caption',288,60,0,0,null,0,'VERTICAL');captions.counterAxisAlignItems='CENTER';captions.itemSpacing=2;
const song=await text(captions,'How Long','Title','text/inverse');const artist=await text(captions,'Charlie Puth','Body','text/inverse');artist.opacity=0.68;
await componentText(card,'Title',song,'How Long');await componentText(card,'Artist',artist,'Charlie Puth');C.card=card.id;
const segment=[];for(const state of ['Default','Selected']){
 const n=base('State='+state,120,38);n.cornerRadius=999;if(state==='Selected')color(n,'surface/tint');const label=await text(n,'Все треки','Small',state==='Selected'?'text/primary':'text/secondary');await componentText(n,'Label',label,'Все треки');segment.push(n);
}combine('WeDex/Segment',segment,960,1020);C.segment=segment.map(n=>n.id);
const ambient=[];
for(const phase of [0,1]){
 const n=base('Phase='+(phase===0?'Ember':'Bloom'),1440,960);n.layoutMode='NONE';color(n,'surface/canvas');n.clipsContent=true;
 const specs=phase===0?[[260,190,790,650,'#C91823'],[840,320,580,620,'#DA8B16'],[80,590,840,520,'#912E41'],[700,-170,800,550,'#4A1B24']]:[[440,70,880,690,'#D72F23'],[850,130,650,700,'#E8AB25'],[200,650,1000,520,'#932D44'],[330,-120,760,550,'#582331']];
 for(let i=0;i<specs.length;i++){
  const [x,y,w,h,hex]=specs[i];const b=remember(figma.createEllipse());n.appendChild(b);b.name='Aurora/'+i;b.resize(w,h);b.x=x;b.y=y;
  b.fills=[{type:'GRADIENT_RADIAL',gradientTransform:[[1,0,0],[0,1,0]],gradientStops:[{position:0,color:{...rgb(hex),a:0.9}},{position:1,color:{...rgb(hex),a:0}}]}];
  await b.setEffectStyleIdAsync(F.effects.Ambient);
 }
 ambient.push(n);
}
const ambientSet=combine('WeDex/Ambient',ambient,160,1580,2);ambientSet.description='Two looping Smart Animate phases. Warm diffuse light moves slowly behind glass; no content moves with it. 7 seconds per phase.';
for(let i=0;i<2;i++)await ambient[i].setReactionsAsync([{trigger:{type:'AFTER_TIMEOUT',timeout:0.05},actions:[navAction(ambient[1-i].id,'CHANGE_TO',7)]}]);C.ambient=ambient.map(n=>n.id);
// A compact sample of the component family, kept separate from the prototype screens.
const specimen=box(kit,'WeDex · Core controls',960,360,160,2720,'surface/canvas',24,'VERTICAL');padding(specimen,32,24);await text(specimen,'Стекло. Воздух. Музыка.','Title','text/inverse');
const sample=box(specimen,'Interactive controls',840,80,0,0,null,0,'HORIZONTAL');padding(sample,8,24);
for(const id of [C.play[0],C.toggle[0],C.volume[0],C.button.Glass])place(await figma.getNodeByIdAsync(id),sample,'Interactive sample',0,0);
return {createdNodeIds:made,mutatedNodeIds:changed,components:C,properties:props,counts:{components:kit.findAllWithCriteria({types:['COMPONENT']}).length,componentSets:kit.findAllWithCriteria({types:['COMPONENT_SET']}).length},ambientLoopSeconds:14,selectedLibrary:{baseIconButton:'2:46',usage:'transport and section actions'},pageId:kit.id};
