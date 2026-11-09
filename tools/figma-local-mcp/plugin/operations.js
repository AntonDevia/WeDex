/* Standard Figma Plugin API only. Bundled into plugin/code.js by setup.mjs. */
async function runOperation(operation,params){
  const p=params||{};
  const get=async id=>{const node=await figma.getNodeByIdAsync(id);if(!node)throw new Error('Layer not found: '+id);return node;};
  const context=()=>({fileName:figma.root.name,fileKey:figma.fileKey||null,fileKeySource:figma.fileKey?'plugin-api':'unavailable',editorType:figma.editorType,pageId:figma.currentPage.id,pageName:figma.currentPage.name,selection:figma.currentPage.selection.map(n=>({id:n.id,name:n.name,type:n.type})),pages:figma.root.children.map(n=>({id:n.id,name:n.name}))});
  if(operation==='context')return context();
  if(operation==='inspect'){
    let remaining=Math.min(2000,p.maxNodes||300),truncated=false;
    const visit=(n,depth)=>{if(remaining--<=0){truncated=true;return null;}const out={id:n.id,name:n.name,type:n.type};
      for(const key of ['x','y','width','height','visible','opacity','layoutMode','itemSpacing','paddingLeft','paddingRight','paddingTop','paddingBottom','cornerRadius','layoutSizingHorizontal','layoutSizingVertical']){const v=n[key];if(v!==undefined&&typeof v!=='symbol')out[key]=v;}
      if(n.type==='TEXT')out.characters=n.characters.slice(0,8000);
      if('reactions'in n)out.reactions=n.reactions;
      if(depth>0&&'children'in n)out.children=n.children.map(c=>visit(c,depth-1)).filter(Boolean);
      return out;};
    const nodes=[];for(const id of p.nodeIds||[])nodes.push(await get(id));if(!nodes.length)nodes.push(figma.currentPage);
    return {nodes:nodes.map(n=>visit(n,Math.min(6,p.depth??2))).filter(Boolean),truncated};
  }
  if(operation==='execute'){
    const helpers={
      async loadFonts(node){const fonts=node&&node.type==='TEXT'?node.getRangeAllFontNames(0,node.characters.length):[{family:'Inter',style:'Regular'}];for(const font of fonts)await figma.loadFontAsync(font);},
      autoLayout(options){const o=options||{},n=figma.createFrame();n.name=o.name||'Auto layout';n.layoutMode=o.direction==='horizontal'?'HORIZONTAL':'VERTICAL';n.primaryAxisSizingMode='AUTO';n.counterAxisSizingMode='AUTO';n.fills=[];if(o.gap!==undefined)n.itemSpacing=o.gap;if(o.padding!==undefined)for(const k of ['paddingLeft','paddingRight','paddingTop','paddingBottom'])n[k]=o.padding;return n;}
    };
    const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
    figma.commitUndo();
    try{const result=await new AsyncFunction('figma','helpers',p.code)(figma,helpers);const json=JSON.stringify(result===undefined?null:result);if(json.length>2000000)throw new Error('Return value too large. Return IDs and compact data.');return JSON.parse(json);}finally{figma.commitUndo();}
  }
  if(operation==='export'){
    const n=await get(p.nodeId);if(typeof n.exportAsync!=='function')throw new Error('Layer cannot be exported');
    const max=Math.min(2048,p.maxDimension||1440),dimension=Math.max(n.width||max,n.height||max),scale=Math.min(2,max/dimension);
    const bytes=await n.exportAsync({format:'PNG',constraint:{type:'SCALE',value:scale}});
    if(bytes.length>8*1024*1024)throw new Error('PNG too large. Reduce max_dimension.');
    return {nodeId:n.id,name:n.name,base64:figma.base64Encode(bytes)};
  }
  if(operation==='image'){
    const n=p.nodeId?await get(p.nodeId):null;if(n&&!('fills'in n))throw new Error('Target has no fills');
    const image=figma.createImage(figma.base64Decode(p.base64));if(n)n.fills=[{type:'IMAGE',imageHash:image.hash,scaleMode:p.scaleMode||'FILL'}];return {imageHash:image.hash,nodeId:n?n.id:null};
  }
  if(operation==='select'){
    const nodes=[];for(const id of p.nodeIds)nodes.push(await get(id));
    const pageOf=n=>{let v=n;while(v&&v.type!=='PAGE')v=v.parent;return v;};const page=pageOf(nodes[0]);if(!page||nodes.some(n=>pageOf(n)!==page)||nodes.some(n=>n.type==='PAGE'||n.type==='DOCUMENT'))throw new Error('Select scene nodes from one page');
    await figma.setCurrentPageAsync(page);figma.currentPage.selection=nodes;figma.viewport.scrollAndZoomIntoView(nodes);return {selected:nodes.map(n=>n.id)};
  }
  if(operation==='prototype'){
    const plan=p.plan;if(!plan||!Array.isArray(plan.expectedFrames)||!Array.isArray(plan.reactions)||plan.reactions.length>5000)throw new Error('Invalid prototype plan');
    if(figma.fileKey&&plan.fileKey&&figma.fileKey!==plan.fileKey)throw new Error('Plan belongs to another Figma file');
    const page=await get(plan.pageId);if(page.type!=='PAGE')throw new Error('Prototype page not found');
    for(const f of plan.expectedFrames){const n=await get(f.id);if(n.type!=='FRAME'||n.name!==f.name||n.parent!==page)throw new Error('Screen changed or wrong file: '+f.name);}
    const ids=new Set([plan.startingFrame]);for(const item of plan.reactions){ids.add(item.nodeId);for(const r of item.reactions)for(const a of r.actions||[])if(a.type==='NODE'&&a.destinationId)ids.add(a.destinationId);}
    for(const f of plan.flowStartingPoints||[])ids.add(f.nodeId);
    const nodes=new Map();for(const id of ids)nodes.set(id,await get(id));
    const originals=new Map();for(const item of plan.reactions){const n=nodes.get(item.nodeId);if(typeof n.setReactionsAsync!=='function')throw new Error('Layer cannot have reactions: '+n.id);originals.set(n.id,JSON.parse(JSON.stringify(n.reactions)));}
    const summary={screens:plan.expectedFrames.length,controls:plan.reactions.length,fileName:figma.root.name,pageId:page.id};
    if(p.dryRun!==false)return {...summary,dryRun:true,validated:true};
    const oldFlows=JSON.parse(JSON.stringify(page.flowStartingPoints)),oldBackgrounds=JSON.parse(JSON.stringify(page.prototypeBackgrounds)),applied=[];
    const signature=rs=>rs.map(r=>({trigger:r.trigger&&r.trigger.type,actions:(r.actions||[]).map(a=>({type:a.type,to:a.type==='NODE'?a.destinationId:null,navigation:a.type==='NODE'?a.navigation:null}))}));
    figma.commitUndo();let phase='reactions';
    try{
      for(const item of plan.reactions){phase='reaction '+item.nodeId;applied.push(item.nodeId);await nodes.get(item.nodeId).setReactionsAsync(item.reactions);}
      phase='page prototype settings';
      page.flowStartingPoints=plan.flowStartingPoints;page.prototypeBackgrounds=plan.prototypeBackgrounds;
      for(const item of plan.reactions)if(JSON.stringify(signature(nodes.get(item.nodeId).reactions))!==JSON.stringify(signature(item.reactions)))throw new Error('Reaction read-back failed: '+item.nodeId);
      await figma.setCurrentPageAsync(page);figma.currentPage.selection=[nodes.get(plan.startingFrame)];figma.viewport.scrollAndZoomIntoView(figma.currentPage.selection);
      figma.commitUndo();return {...summary,dryRun:false,verified:true};
    }catch(error){
      const failed=[];for(const id of applied.reverse())try{await nodes.get(id).setReactionsAsync(originals.get(id));}catch{failed.push(id);}
      try{page.flowStartingPoints=oldFlows;page.prototypeBackgrounds=oldBackgrounds;}catch{failed.push(page.id);}
      figma.commitUndo();const e=new Error(phase+': '+(error&&error.message||String(error))+(failed.length?'; rollback incomplete: '+failed.join(', '):'; all reactions rolled back'));e.outcome=failed.length?'unknown':'rolled-back';throw e;
    }
  }
  throw new Error('Unknown operation: '+operation);
}
