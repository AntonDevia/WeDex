import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import http from 'node:http';
import {createBroker} from '../broker.mjs';

export async function harness(t){
  const config={port:0,secret:crypto.randomBytes(32).toString('hex')};const broker=createBroker(config,{pollMs:100});config.port=await broker.listen();t.after(()=>broker.close());
  async function call(route,body,extra={}){const r=await fetch(`http://127.0.0.1:${config.port}${route}`,{method:body===undefined?'GET':'POST',headers:{Authorization:`Bearer ${config.secret}`,'Content-Type':'application/json',...extra},body:body===undefined?undefined:JSON.stringify(body)});return {status:r.status,data:r.status===204?null:await r.json()};}
  const attach=async()=>{const r=await call('/plugin/attach',{fileName:'Test design',pageId:'0:1',pageName:'Test page'});assert.equal(r.status,200);return r.data.clientId;};
  return {config,broker,call,attach};
}
test('loopback authentication, CORS and Host checks reject unrelated requests',async t=>{
  const h=await harness(t);
  assert.equal((await h.call('/status',undefined,{Authorization:'Bearer wrong'})).status,401);
  assert.equal((await h.call('/status',undefined,{Origin:'https://example.com'})).status,403);
  assert.equal((await h.call('/status',undefined,{Origin:'null'})).status,200);
  const hostile=await new Promise((resolve,reject)=>{const req=http.request({host:'127.0.0.1',port:h.config.port,path:'/status',headers:{Host:'attacker.example',Authorization:`Bearer ${h.config.secret}`}},r=>{r.resume();r.on('end',()=>resolve(r.statusCode));});req.on('error',reject);req.end();});assert.equal(hostile,403);
  assert.equal(JSON.stringify((await h.call('/status')).data).includes(h.config.secret),false);
});
test('round-trip, request deduplication and result acknowledgement never execute twice',async t=>{
  const h=await harness(t),clientId=await h.attach(),requestId=crypto.randomUUID();
  const payload={clientId,requestId,operation:'execute',params:{code:'return 7;'},timeoutMs:1000};
  const response=h.call('/rpc',payload),job=(await h.call('/plugin/poll',{clientId})).data.job;
  assert.equal(job.operation,'execute');
  assert.equal((await h.call('/plugin/result',{clientId,jobId:job.id,ok:true,result:7})).status,200);
  assert.deepEqual((await response).data,{ok:true,result:7});
  assert.deepEqual((await h.call('/rpc',payload)).data,{ok:true,result:7});
  assert.equal((await h.call('/plugin/result',{clientId,jobId:job.id,ok:true,result:7})).data.replayed,true);
  assert.equal((await h.call('/plugin/poll',{clientId})).status,204);
  const other=await h.attach();assert.equal((await h.call('/rpc',{...payload,clientId:other})).status,409);
});
test('timeouts distinguish undelivered commands and hold the queue until a delivered job actually finishes',async t=>{
  const h=await harness(t),clientId=await h.attach();
  const queued=await h.call('/rpc',{clientId,requestId:crypto.randomUUID(),operation:'execute',timeoutMs:100});assert.equal(queued.data.outcome,'not-executed');
  const first=h.call('/rpc',{clientId,requestId:crypto.randomUUID(),operation:'execute',timeoutMs:100});
  const job=(await h.call('/plugin/poll',{clientId})).data.job;assert.equal((await first).data.outcome,'unknown');
  assert.equal(h.broker.status().clients[0].busy,true);
  const next=h.call('/rpc',{clientId,requestId:crypto.randomUUID(),operation:'context',timeoutMs:2000});
  assert.equal((await h.call('/plugin/poll',{clientId})).status,204);
  assert.equal((await h.call('/plugin/result',{clientId,jobId:job.id,ok:true,result:'late'})).data.late,true);
  const job2=(await h.call('/plugin/poll',{clientId})).data.job;
  await h.call('/plugin/result',{clientId,jobId:job2.id,ok:true,result:{done:true}});assert.equal((await next).data.ok,true);
});
test('disconnect cancels queued commands without dispatching them',async t=>{
  const h=await harness(t),clientId=await h.attach();
  const pending=h.call('/rpc',{clientId,requestId:crypto.randomUUID(),operation:'execute',timeoutMs:1000});
  await new Promise(r=>setTimeout(r,20));await h.call('/plugin/detach',{clientId});assert.equal((await pending).data.outcome,'not-executed');assert.equal(h.broker.status().pending,0);
});
