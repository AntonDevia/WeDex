import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..');
const folder=path.join(root,'finish-prototype');
const plan=JSON.parse(fs.readFileSync(path.join(folder,'plan.json'),'utf8'));
const code=fs.readFileSync(path.join(folder,'code.js'),'utf8');
async function run({missing=null,wrongFile=false,failAt=null}={}){
 const nodes=new Map();const original=[{trigger:{type:'ON_CLICK'},actions:[{type:'BACK'}]}];let writes=0,closed=false,notice='';
 function node(id){if(id===missing)return null;if(!nodes.has(id))nodes.set(id,{id,type:'FRAME',name:'control',reactions:structuredClone(original),async setReactionsAsync(next){writes++;if(writes===failAt)throw new Error('Injected API failure');this.reactions=structuredClone(next);}});return nodes.get(id);}
 for(const f of plan.expectedFrames){const n=node(f.id);if(n)n.name=f.name;}
 const page=node(plan.pageId);page.type='PAGE';page.name=wrongFile?'Different file':'01 · Prototype';
 const figma={getNodeByIdAsync:async id=>node(id),async setCurrentPageAsync(p){this.currentPage=p;},viewport:{scrollAndZoomIntoView(){}},notify(message){notice=message;},closePlugin(){closed=true;}};
 vm.runInNewContext(code,{figma,console:{error(){}},Map,Set,JSON,Error});
 for(let i=0;i<10000&&!closed;i++)await new Promise(resolve=>setImmediate(resolve));
 assert(closed,'Plugin did not terminate');return {nodes,writes,notice,original,page};
}
const success=await run();assert.equal(success.writes,plan.reactions.length);for(const item of plan.reactions)assert.deepEqual(success.nodes.get(item.nodeId).reactions,item.reactions);assert.equal(success.page.flowStartingPoints[0].nodeId,plan.startingFrame);
const wrong=await run({wrongFile:true});assert.equal(wrong.writes,0);assert.match(wrong.notice,/Откройте файл/);
const stale=await run({missing:plan.reactions[10].nodeId});assert.equal(stale.writes,0);assert.match(stale.notice,/Отсутствует слой|не найден экран/);
const partial=await run({failAt:53});assert.match(partial.notice,/Injected API failure/);for(const n of partial.nodes.values())assert.deepEqual(n.reactions,partial.original,'Partial changes were not rolled back');
console.log('Native plugin: success, wrong-file guard, stale-layer guard and rollback checks passed.');
