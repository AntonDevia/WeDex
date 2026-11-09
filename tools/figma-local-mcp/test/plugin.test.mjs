import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../plugin/operations.js',import.meta.url),'utf8');
const plan=JSON.parse(fs.readFileSync(new URL('../../../design/finish-prototype/plan.json',import.meta.url),'utf8'));
function model(){
  const page={id:plan.pageId,name:'01 · Prototype',type:'PAGE',children:[],selection:[],flowStartingPoints:[],prototypeBackgrounds:[]};
  const nodes=new Map([[page.id,page]]);let writes=0,failId=null;
  const node=id=>{if(nodes.has(id))return nodes.get(id);const n={id,name:id,type:'FRAME',parent:page,children:[],reactions:[],async setReactionsAsync(rs){writes++;if(n.id===failId){failId=null;throw new Error('Simulated write failure');}n.reactions=structuredClone(rs);}};nodes.set(id,n);return n;};
  for(const f of plan.expectedFrames){const n=node(f.id);n.name=f.name;page.children.push(n);}
  node(plan.startingFrame);
  for(const item of plan.reactions){node(item.nodeId);for(const r of item.reactions)for(const a of r.actions||[])if(a.type==='NODE'&&a.destinationId)node(a.destinationId);}
  const figma={fileKey:plan.fileKey,root:{name:'Sonora test',children:[page]},editorType:'figma',currentPage:page,getNodeByIdAsync:async id=>nodes.get(id)||null,setCurrentPageAsync:async p=>{figma.currentPage=p;},viewport:{scrollAndZoomIntoView(){}},commitUndo(){}};
  const run=vm.runInNewContext(source+'\nrunOperation',{figma,console,Uint8Array});
  return {run,nodes,page,figma,writes:()=>writes,fail:id=>{failId=id;}};
}
test('real Sonora plan preflights every expected screen without modifying reactions',async()=>{
  const m=model();const result=await m.run('prototype',{plan,dryRun:true});assert.equal(result.validated,true);assert.equal(result.controls,plan.reactions.length);assert.ok(result.controls>400);assert.equal(m.writes(),0);
  m.nodes.delete(plan.expectedFrames[2].id);await assert.rejects(m.run('prototype',{plan,dryRun:false}),/not found/);assert.equal(m.writes(),0);
});
test('prototype writes and verifies all planned reactions; a second write failure restores earlier changes',async()=>{
  const m=model();const applied=await m.run('prototype',{plan,dryRun:false});assert.equal(applied.verified,true);assert.equal(m.writes(),plan.reactions.length);assert.equal(m.page.selection[0].id,plan.startingFrame);
  const n=model();n.fail(plan.reactions[1].nodeId);await assert.rejects(n.run('prototype',{plan,dryRun:false}),/all reactions rolled back/);for(const item of plan.reactions)assert.equal(n.nodes.get(item.nodeId).reactions.length,0);
});
test('wrong file rejected; inspection is bounded; standard execute returns data',async()=>{
  const m=model();m.figma.fileKey='another-file';await assert.rejects(m.run('prototype',{plan,dryRun:false}),/another Figma file/);assert.equal(m.writes(),0);
  const inspection=await m.run('inspect',{nodeIds:[],depth:2,maxNodes:3});assert.equal(inspection.truncated,true);assert.equal(inspection.nodes[0].children.length,2);
  const result=await m.run('execute',{code:'return {fileName:figma.root.name, helper:typeof helpers.autoLayout};'});assert.equal(result.fileName,'Sonora test');assert.equal(result.helper,'function');
});
