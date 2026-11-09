const kit=await figma.getNodeByIdAsync(F.pages.kit);await figma.setCurrentPageAsync(kit);
const row=remember(figma.createComponent());row.name='WeDex/Track row';row.resize(744,68);row.layoutMode='HORIZONTAL';row.primaryAxisSizingMode='FIXED';row.counterAxisSizingMode='FIXED';row.counterAxisAlignItems='CENTER';row.fills=[];row.cornerRadius=12;padding(row,12,12);kit.appendChild(row);row.x=1680;row.y=700;
image(row,'Cover','weeknd',44,0,0,8);
const labels=box(row,'Track labels',350,44,0,0,null,0,'VERTICAL');labels.itemSpacing=0;labels.layoutSizingHorizontal='FILL';
const name=await text(labels,'Blinding Lights','Body');await componentText(row,'Title',name,'Blinding Lights');
const artist=await text(labels,'The Weeknd','Small','text/secondary');await componentText(row,'Artist',artist,'The Weeknd');
const source=await text(row,'Яндекс','Small','text/secondary');await componentText(row,'Source',source,'Яндекс');
const dur=await text(row,'3:20','Small','text/secondary');await componentText(row,'Time',dur,'3:20');
const like=place(await figma.getNodeByIdAsync(K.toggle[2]),row,'Save track',0,0);
const card=remember(figma.createComponent());card.name='WeDex/Playlist card';card.resize(176,242);card.layoutMode='VERTICAL';card.primaryAxisSizingMode='FIXED';card.counterAxisSizingMode='FIXED';card.itemSpacing=12;card.fills=[];kit.appendChild(card);card.x=1680;card.y=880;
image(card,'Cover','massive',176,0,0,20);
const caption=box(card,'Caption',176,50,0,0,null,0,'VERTICAL');caption.itemSpacing=2;
const title=await text(caption,'Ночной маршрут','Body');await componentText(card,'Title',title,'Ночной маршрут');
const subtitle=await text(caption,'24 трека','Small','text/secondary');await componentText(card,'Subtitle',subtitle,'24 трека');
const sliderSets={},sliders={};
for(const [kind,width,y]of [['Progress',320,1200],['Volume slider',96,1340]]){
 const variants=[];
 for(const [i,value]of [0.25,0.5,0.75].entries()){
  const c=remember(figma.createComponent());c.name='Value='+[25,50,75][i];c.resize(width,20);c.fills=[];kit.appendChild(c);
  const rail=box(c,'Rail',width,3,0,8,'surface/glass-strong',999);const progress=box(c,'Progress',width*value,3,0,8,'text/inverse',999);
  const thumb=remember(figma.createEllipse());c.appendChild(thumb);thumb.name='Thumb';thumb.resize(8,8);thumb.x=width*value-4;thumb.y=5.5;color(thumb,'text/inverse');variants.push(c);
 }
 const set=remember(figma.combineAsVariants(variants,kit));set.name='WeDex/'+kind;set.x=1680;set.y=y;variants.forEach((n,i)=>{n.x=24+i*(width+24);n.y=24;});set.resize(3*(width+24)+24,68);
 for(let i=0;i<3;i++)await variants[i].setReactionsAsync([{trigger:{type:'ON_CLICK'},actions:[navAction(variants[(i+1)%3].id,'CHANGE_TO',0.18)]},{trigger:{type:'ON_DRAG'},actions:[navAction(variants[(i+1)%3].id,'CHANGE_TO',0.18)]}]);
 sliderSets[kind]=set.id;sliders[kind]=variants.map(n=>n.id);
}
return {createdNodeIds:made,mutatedNodeIds:changed,components:{row:row.id,playlistCard:card.id,sliders},sets:sliderSets};
