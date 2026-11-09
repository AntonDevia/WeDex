figma.showUI(__html__,{width:380,height:440,themeColors:true});
let busy=false;
figma.ui.onmessage=async message=>{
  if(!message||typeof message.type!=='string')return;
  if(message.type==='ready'){
    const saved=await figma.clientStorage.getAsync('wedex-local-bridge-v1');
    figma.ui.postMessage({type:'init',saved:saved||null,context:await runOperation('context',{})});return;
  }
  if(message.type==='save'){await figma.clientStorage.setAsync('wedex-local-bridge-v1',{key:message.key,port:message.port});return;}
  if(message.type==='context'){figma.ui.postMessage({type:'context',context:await runOperation('context',{})});return;}
  if(message.type!=='job')return;
  if(busy){figma.ui.postMessage({type:'result',jobId:message.job.id,ok:false,error:'Plugin is busy. Command was not executed.',outcome:'not-executed'});return;}
  busy=true;
  try{const result=await runOperation(message.job.operation,message.job.params);figma.ui.postMessage({type:'result',jobId:message.job.id,ok:true,result});}
  catch(error){figma.ui.postMessage({type:'result',jobId:message.job.id,ok:false,error:error&&error.message||String(error),outcome:error&&error.outcome||(['context','inspect','export'].includes(message.job.operation)?'not-executed':'unknown')});}
  finally{busy=false;}
};
