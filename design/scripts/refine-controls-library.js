// Runs through our local MCP in the standard native Figma Plugin API.
const kit=await figma.getNodeByIdAsync('3:2');
await figma.setCurrentPageAsync(kit);
if(kit.children.some(n=>n.name==='WeDex/EQ gain'))throw new Error('EQ gain component already exists; inspect the refinement ledger before retrying.');
const createdNodeIds=[],mutatedNodeIds=[];
const remember=n=>{createdNodeIds.push(n.id);return n;};
const source=await figma.getNodeByIdAsync('13:2232');
const gainSource=source.children.find(n=>n.type==='TEXT'&&n.y===0);
for(const s of gainSource.getStyledTextSegments(['fontName']))await figma.loadFontAsync(s.fontName);
const railSource=source.children.find(n=>n.name==='Rail'),thumbSource=source.children.find(n=>n.type==='ELLIPSE');
const right=Math.max(...kit.children.map(n=>n.x+n.width));
const variants=[];
for(let gain=-6;gain<=6;gain++){
 const c=remember(figma.createComponent());kit.appendChild(c);c.name='Gain='+gain;c.resize(52,184);c.fills=[];
 const label=remember(gainSource.clone());c.appendChild(label);label.name='Gain label';label.characters=(gain>0?'+':'')+gain;label.x=(52-label.width)/2;label.y=0;
 const rail=remember(railSource.clone());c.appendChild(rail);
 const thumb=remember(thumbSource.clone());c.appendChild(thumb);thumb.name='Thumb';thumb.y=96-gain*8;
 variants.push(c);
}
const set=remember(figma.combineAsVariants(variants,kit));set.name='WeDex/EQ gain';set.x=right+160;set.y=700;
set.description='Independent EQ gain, −6 to +6 dB in 1 dB prototype steps. Click a rail position to choose gain. Frequency labels remain on each host lane. Uses the existing Manrope Small style and glass/action variables.';
for(let i=0;i<variants.length;i++){variants[i].x=24+i*68;variants[i].y=24;}set.resize(13*68+32,232);
const action=to=>({type:'NODE',destinationId:to,navigation:'CHANGE_TO',transition:{type:'SMART_ANIMATE',duration:0.14,easing:{type:'EASE_OUT'}},resetInteractiveComponents:false});
for(const c of variants){
 for(let gain=6;gain>=-6;gain--){
  const center=102-gain*8,top=gain===6?28:center-4,bottom=gain===-6?176:center+4;
  const hit=remember(figma.createFrame());c.appendChild(hit);hit.name='Set gain/'+gain;hit.resize(52,bottom-top);hit.x=0;hit.y=top;hit.fills=[{type:'SOLID',color:{r:0,g:0,b:0},opacity:0}];
  if(c!==variants[gain+6])await hit.setReactionsAsync([{trigger:{type:'ON_CLICK'},actions:[action(variants[gain+6].id)]}]);
 }
}
const sliderFamilies=[['7:52','7:56','7:60'],['7:71','7:75','7:79']];
for(const ids of sliderFamilies)for(let i=0;i<ids.length;i++){
 const c=await figma.getNodeByIdAsync(ids[i]);
 if(c.children.some(n=>n.name.startsWith('Set position/')))throw new Error('Slider refinement already present: '+c.id);
 await c.setReactionsAsync([{trigger:{type:'ON_DRAG'},actions:[action(ids[(i+1)%3])]}]);mutatedNodeIds.push(c.id);
 for(let j=0;j<3;j++){
  const hit=remember(figma.createFrame());c.appendChild(hit);hit.name='Set position/'+[25,50,75][j];hit.resize(c.width/3,c.height);hit.x=j*c.width/3;hit.y=0;hit.fills=[{type:'SOLID',color:{r:0,g:0,b:0},opacity:0}];
  if(i!==j)await hit.setReactionsAsync([{trigger:{type:'ON_CLICK'},actions:[action(ids[j])]}]);
 }
}
return {createdNodeIds,mutatedNodeIds,eqSet:set.id,eqVariants:Object.fromEntries(variants.map((n,i)=>[i-6,n.id])),variantCount:variants.length,sliderFamilies:2,tokenBindingsPreserved:true};
