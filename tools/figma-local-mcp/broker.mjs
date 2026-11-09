import http from 'node:http';
import crypto from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {loadConfig} from './config.mjs';

const SERVICE='wedex-figma-bridge';
const MAX_BODY=12*1024*1024;
function json(res,status,value){if(res.destroyed||res.writableEnded)return;res.writeHead(status,{'Content-Type':'application/json; charset=utf-8','Cache-Control':'no-store'});res.end(JSON.stringify(value));}
function auth(req,secret){const value=Buffer.from(req.headers.authorization||''),expected=Buffer.from(`Bearer ${secret}`);return value.length===expected.length&&crypto.timingSafeEqual(value,expected);}
async function body(req){
  if(!req.headers['content-type']?.toLowerCase().startsWith('application/json'))throw Object.assign(new Error('JSON content type required'),{status:415});
  const chunks=[];let size=0;for await(const chunk of req){size+=chunk.length;if(size>MAX_BODY)throw Object.assign(new Error('Request is too large'),{status:413});chunks.push(chunk);}
  try{return JSON.parse(Buffer.concat(chunks).toString('utf8'));}catch{throw Object.assign(new Error('Invalid JSON'),{status:400});}
}
export function createBroker(config,{pollMs=24000,ttlMs=120000}={}){
  const clients=new Map(),jobs=new Map(),requests=new Map(),completed=new Map(),acknowledged=new Map();
  const secret=config.secret;let server;
  function finish(job,status,value,keepActive=false){
    clearTimeout(job.timer);jobs.delete(job.id);requests.delete(job.requestId);
    const client=clients.get(job.clientId);if(client){client.queue=client.queue.filter(id=>id!==job.id);if(client.active===job.id&&!keepActive)client.active=null;}
    completed.set(job.requestId,{status,value,clientId:job.clientId});while(completed.size>128)completed.delete(completed.keys().next().value);
    for(const res of job.waiters)json(res,status,value);
    if(client)deliver(client);
  }
  function deliver(client){
    if(!client.poll||client.active)return;
    let job;while(client.queue.length&&!job)job=jobs.get(client.queue.shift());if(!job)return;
    const {res,timer}=client.poll;clearTimeout(timer);client.poll=null;
    if(res.destroyed){client.queue.unshift(job.id);return;}
    job.dispatched=true;client.active=job.id;
    json(res,200,{job:{id:job.id,operation:job.operation,params:job.params}});
  }
  function detach(client,reason){
    clients.delete(client.id);if(client.poll){clearTimeout(client.poll.timer);json(client.poll.res,410,{error:reason});client.poll=null;}
    for(const job of [...jobs.values()].filter(j=>j.clientId===client.id))finish(job,503,{error:reason,outcome:job.dispatched?'unknown':'not-executed',retrySafe:!job.dispatched});
  }
  function status(){return {service:SERVICE,version:'0.1.0',clients:[...clients.values()].map(c=>({clientId:c.id,fileName:c.fileName,fileKey:c.fileKey,fileKeySource:c.fileKeySource,pageId:c.pageId,pageName:c.pageName,editorType:c.editorType,busy:Boolean(c.active),queued:c.queue.length,lastSeen:new Date(c.lastSeen).toISOString()})),pending:jobs.size};}
  server=http.createServer(async(req,res)=>{
    const origin=req.headers.origin;
    const expectedPort=server.address()?.port||config.port;
    if(![`127.0.0.1:${expectedPort}`,`localhost:${expectedPort}`].includes(req.headers.host)){json(res,403,{error:'Invalid Host header'});return;}
    if(origin&&!['null','https://www.figma.com','https://figma.com','https://plugin.figma.com'].includes(origin)){json(res,403,{error:'Origin not allowed'});return;}
    if(origin){res.setHeader('Access-Control-Allow-Origin',origin);res.setHeader('Vary','Origin');}
    if(req.method==='OPTIONS'){
      res.writeHead(204,{'Access-Control-Allow-Methods':'GET, POST, OPTIONS','Access-Control-Allow-Headers':'Authorization, Content-Type','Access-Control-Max-Age':'600'});res.end();return;
    }
    if(req.method==='GET'&&req.url==='/health'){json(res,200,{service:SERVICE,version:'0.1.0'});return;}
    if(!auth(req,secret)){json(res,401,{error:'Invalid bridge key'});return;}
    try{
      if(req.method==='GET'&&req.url==='/status'){json(res,200,status());return;}
      if(req.method!=='POST'){json(res,405,{error:'Method not allowed'});return;}
      const data=await body(req);
      if(req.url==='/plugin/attach'){
        if(clients.size>=16){json(res,429,{error:'Too many plugin sessions. Close unused bridge plugins.'});return;}
        if(typeof data.fileName!=='string'||!data.fileName||data.fileName.length>500){json(res,400,{error:'File name is required'});return;}
        const client={id:crypto.randomUUID(),fileName:data.fileName,fileKey:typeof data.fileKey==='string'?data.fileKey:null,fileKeySource:data.fileKeySource==='plugin-api'?'plugin-api':'unavailable',pageId:String(data.pageId||''),pageName:String(data.pageName||''),editorType:String(data.editorType||'figma'),lastSeen:Date.now(),queue:[],active:null,poll:null};
        clients.set(client.id,client);json(res,200,{clientId:client.id,service:SERVICE});return;
      }
      if(req.url==='/rpc'){
        if(typeof data.clientId!=='string'||!clients.has(data.clientId)){json(res,409,{error:'Figma plugin is not connected. Open the bridge plugin in the target file first.',outcome:'not-executed'});return;}
        if(!['context','inspect','execute','export','image','prototype','select'].includes(data.operation)){json(res,400,{error:'Unknown operation'});return;}
        if(typeof data.requestId!=='string'||data.requestId.length>100){json(res,400,{error:'A request ID is required'});return;}
        const old=completed.get(data.requestId);if(old){if(old.clientId!==data.clientId){json(res,409,{error:'Request belongs to another file'});return;}json(res,old.status,old.value);return;}
        const running=requests.get(data.requestId);if(running){if(running.clientId!==data.clientId){json(res,409,{error:'Request belongs to another file'});return;}running.waiters.push(res);return;}
        const client=clients.get(data.clientId);if(client.queue.length>=20){json(res,429,{error:'File command queue is full',outcome:'not-executed'});return;}
        const timeout=Math.max(100,Math.min(90000,Number(data.timeoutMs)||30000));
        const job={id:crypto.randomUUID(),requestId:data.requestId,clientId:client.id,operation:data.operation,params:data.params||{},dispatched:false,waiters:[res],timer:null};
        job.timer=setTimeout(()=>finish(job,504,{error:job.dispatched?'Figma command timed out. Execution outcome is unknown; inspect the file before retrying.':'Figma did not receive this command before timeout.',outcome:job.dispatched?'unknown':'not-executed',retrySafe:!job.dispatched},job.dispatched),timeout);
        jobs.set(job.id,job);requests.set(job.requestId,job);client.queue.push(job.id);deliver(client);return;
      }
      const client=clients.get(data.clientId);if(!client){json(res,410,{error:'Plugin session expired. Reconnect the plugin.'});return;}client.lastSeen=Date.now();
      if(req.url==='/plugin/heartbeat'){if(data.pageId)client.pageId=String(data.pageId);if(data.pageName)client.pageName=String(data.pageName);json(res,200,{ok:true});return;}
      if(req.url==='/plugin/poll'){
        if(client.poll){json(res,409,{error:'A poll is already open'});return;}
        const timer=setTimeout(()=>{if(client.poll?.res===res){client.poll=null;res.writeHead(204);res.end();}},pollMs);
        client.poll={res,timer};res.on('close',()=>{if(client.poll?.res===res){clearTimeout(timer);client.poll=null;}});deliver(client);return;
      }
      if(req.url==='/plugin/result'){
        if(acknowledged.get(data.jobId)===client.id){json(res,200,{ok:true,replayed:true});return;}
        const job=jobs.get(data.jobId);
        const remember=()=>{acknowledged.set(data.jobId,client.id);while(acknowledged.size>256)acknowledged.delete(acknowledged.keys().next().value);};
        if(!job&&client.active===data.jobId){client.active=null;remember();json(res,200,{ok:true,late:true});deliver(client);return;}
        if(!job||job.clientId!==client.id||client.active!==job.id){json(res,409,{error:'Command is no longer pending. Do not execute it again.'});return;}
        const encoded=JSON.stringify(data.result??null);if(encoded.length>MAX_BODY-4096){finish(job,502,{error:'Plugin result exceeds the response limit',outcome:'completed'});json(res,413,{error:'Result too large'});return;}
        const value=data.ok?{ok:true,result:data.result??null}:{ok:false,error:String(data.error||'Figma operation failed').slice(0,2000),outcome:data.outcome||'unknown'};
        remember();finish(job,200,value);json(res,200,{ok:true});return;
      }
      if(req.url==='/plugin/detach'){detach(client,'Figma plugin disconnected');json(res,200,{ok:true});return;}
      json(res,404,{error:'Unknown route'});
    }catch(error){json(res,error.status||500,{error:error.status?error.message:'Bridge request failed'});}
  });
  server.requestTimeout=100000;
  const sweep=setInterval(()=>{for(const c of clients.values())if(Date.now()-c.lastSeen>ttlMs)detach(c,'Figma plugin heartbeat expired');},5000);sweep.unref();
  return {server,status,async listen(){await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(config.port,'127.0.0.1',resolve);});return server.address().port;},async close(){clearInterval(sweep);for(const c of [...clients.values()])detach(c,'Bridge server stopped');await new Promise(resolve=>{server.close(resolve);server.closeAllConnections();});}};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const broker=createBroker(loadConfig());await broker.listen();process.stderr.write('WeDex Figma bridge is listening on loopback.\n');
  for(const signal of ['SIGINT','SIGTERM'])process.once(signal,()=>broker.close().then(()=>process.exit(0)));
}
