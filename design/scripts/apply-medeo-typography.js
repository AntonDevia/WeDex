// BACKUP is the inspected typography ledger. PAGE_ID scopes one migration call.
const page=await figma.getNodeByIdAsync(PAGE_ID);await figma.setCurrentPageAsync(page);
const before=BACKUP.pages.find(p=>p.id===PAGE_ID);if(!before)throw new Error('Missing typography snapshot');
const target=(font,size)=>({family:size>=24?'Medeo Sans Display':'Medeo Sans',style:/bold/i.test(font.style)?(/semi/i.test(font.style)?'SemiBold':'Bold'):/medium/i.test(font.style)?'Medium':'Regular'});
// Display Bold isn't used by this design; retain a strong available Display weight.
const normalize=f=>f.family==='Medeo Sans Display'&&f.style==='Bold'?{...f,style:'SemiBold'}:f;
const required=new Map();const nodes=new Map();
for(const item of before.texts){
 const n=await figma.getNodeByIdAsync(item.id);if(!n||n.type!=='TEXT'||n.characters!==item.text)throw new Error('Text changed since snapshot: '+item.id);
 nodes.set(item.id,n);
 for(const run of item.fontRuns){required.set(JSON.stringify(run.fontName),run.fontName);const next=normalize(target(run.fontName,item.fontSize||14));required.set(JSON.stringify(next),next);}
}
for(const font of required.values())await figma.loadFontAsync(font);
const mutatedNodeIds=[],mutatedStyleIds=[];
if(UPDATE_STYLES)for(const old of BACKUP.styles){const style=await figma.getStyleByIdAsync(old.id);if(!style||style.type!=='TEXT')throw new Error('Text style missing: '+old.id);const next=normalize(target(old.font,old.size));await figma.loadFontAsync(next);style.fontName=next;mutatedStyleIds.push(style.id);}
for(const item of before.texts){
 const n=nodes.get(item.id);
 for(const run of item.fontRuns)n.setRangeFontName(run.start,run.end,normalize(target(run.fontName,item.fontSize||14)));
 if(n.name==='Gain label'&&n.parent.type==='COMPONENT')n.x=(n.parent.width-n.width)/2;
 if(n.parent.type==='FRAME'&&n.parent.name.startsWith('EQ/')&&n.y>=190)n.x=(n.parent.width-n.width)/2;
 mutatedNodeIds.push(n.id);
}
return {pageId:page.id,textsUpdated:mutatedNodeIds.length,mutatedNodeIds,mutatedStyleIds,families:['Medeo Sans','Medeo Sans Display']};
