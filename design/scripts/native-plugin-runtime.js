// Runs in the regular Figma Desktop development-plugin environment.
// No MCP calls, HTTP calls, imported libraries, new layers or account credentials.
async function connectPrototype() {
  const page=await figma.getNodeByIdAsync(PLAN.pageId);
  if(!page||page.type!=='PAGE'||page.name!=='01 · Prototype')throw new Error('Откройте файл WeDex — Sonora · Glass Music.');
  await figma.setCurrentPageAsync(page);
  // Validate the exact frames and every control/destination before changing anything.
  for(const frame of PLAN.expectedFrames){const n=await figma.getNodeByIdAsync(frame.id);if(!n||n.name!==frame.name)throw new Error('Макет изменён: не найден экран '+frame.name);}
  const ids=new Set(PLAN.reactions.map(item=>item.nodeId));
  for(const item of PLAN.reactions)for(const reaction of item.reactions)for(const action of reaction.actions||[])if(action.type==='NODE')ids.add(action.destinationId);
  const nodes=new Map();for(const id of ids){const n=await figma.getNodeByIdAsync(id);if(!n)throw new Error('Отсутствует слой '+id+'. Изменения не внесены.');nodes.set(id,n);}
  const originals=new Map();for(const item of PLAN.reactions){const n=nodes.get(item.nodeId);if(typeof n.setReactionsAsync!=='function')throw new Error('Слой не поддерживает переходы: '+item.nodeId);originals.set(n.id,JSON.parse(JSON.stringify(n.reactions)));}
  const applied=[];
  try {
    for(const item of PLAN.reactions){const n=nodes.get(item.nodeId);await n.setReactionsAsync(item.reactions);applied.push(n.id);}
    page.flowStartingPoints=PLAN.flowStartingPoints;
    page.prototypeBackgrounds=PLAN.prototypeBackgrounds;
  } catch(error) {
    for(const id of applied.reverse()){try{await nodes.get(id).setReactionsAsync(originals.get(id));}catch(rollback){console.error('Rollback',id,rollback);}}
    throw error;
  }
  // Read-back verifies that every planned reaction was actually stored by Figma.
  const signature=reactions=>reactions.map(r=>({trigger:r.trigger?.type,actions:(r.actions||[]).map(a=>({type:a.type,to:a.type==='NODE'?a.destinationId:null,navigation:a.type==='NODE'?a.navigation:null}))}));
  const failed=[];for(const item of PLAN.reactions){const n=nodes.get(item.nodeId);if(JSON.stringify(signature(n.reactions))!==JSON.stringify(signature(item.reactions)))failed.push(item.nodeId);}
  if(failed.length)throw new Error('Переходы записаны, но проверка требует внимания: '+failed.join(', '));
  figma.currentPage.selection=[nodes.get(PLAN.startingFrame)];
  figma.viewport.scrollAndZoomIntoView([nodes.get(PLAN.startingFrame)]);
  return PLAN.validation.controls;
}
connectPrototype().then(count=>{
  figma.notify('WeDex: связаны '+count+' элементов. Нажмите Present для просмотра.',{timeout:7000});
  figma.closePlugin();
}).catch(error=>{
  console.error(error);
  figma.notify('WeDex: '+error.message,{error:true,timeout:12000});
  figma.closePlugin();
});
