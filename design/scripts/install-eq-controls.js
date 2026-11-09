const page=await figma.getNodeByIdAsync('0:1');await figma.setCurrentPageAsync(page);
const createdNodeIds=[],mutatedNodeIds=[],removedNodeIds=[];
const roots=[await figma.getNodeByIdAsync('13:2221'),await figma.getNodeByIdAsync('13:2290')];
if(page.children.some(n=>n.name==='22 · Overlay — EQ Сброшен'))throw new Error('EQ reset frame already exists; inspect ledger before retrying.');
const gains=[[0,0,0,0,0,0,0,0,0,0],[4,3,2,1,0,-1,-1,0,1,2]];
for(const [mode,root] of roots.entries()){
 const bands=root.findOne(n=>n.name==='Frequency bands');
 for(const [i,lane]of bands.children.entries()){
  if(lane.children.some(n=>n.name==='Independent gain'||n.name.startsWith('Gain/')))throw new Error('Lane already refined: '+lane.id);
  for(const t of lane.children.filter(n=>n.type==='TEXT'))for(const s of t.getStyledTextSegments(['fontName']))await figma.loadFontAsync(s.fontName);
  const old=lane.children.filter(n=>n.type!=='TEXT'||n.y<190);
  const master=await figma.getNodeByIdAsync(EQ_VARIANTS[String(gains[mode][i])]);
  const instance=master.createInstance();lane.appendChild(instance);instance.name='Gain/'+(mode?'warm':'flat')+'/'+lane.name.slice(3);instance.x=0;instance.y=0;createdNodeIds.push(instance.id);
  for(const n of old){removedNodeIds.push(n.id);n.remove();}
  await lane.setReactionsAsync([]);mutatedNodeIds.push(lane.id);
 }
}
const source=roots[0],reset=source.clone();page.appendChild(reset);reset.name='22 · Overlay — EQ Сброшен';reset.x=Math.max(...page.children.filter(n=>n!==reset).map(n=>n.x+n.width))+120;reset.y=4480;
const mapping=new Map();
function pair(a,b){mapping.set(a.id,b.id);createdNodeIds.push(b.id);if('children'in a)for(let i=0;i<a.children.length;i++)pair(a.children[i],b.children[i]);}pair(source,reset);
for(const lane of reset.findOne(n=>n.name==='Frequency bands').children)lane.children.find(n=>n.type==='INSTANCE').name='Gain/reset/'+lane.name.slice(3);
function remap(value){if(typeof value==='string')return mapping.get(value)||value;if(Array.isArray(value))return value.map(remap);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,remap(v)]));return value;}
const refs=remap(EQ_REFS);
return {createdNodeIds,mutatedNodeIds,removedNodeIds,installedBands:20,extra:{eqReset:{id:reset.id,name:reset.name,refs,panel:mapping.get('13:2223')}}};
