import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {Client} from '@modelcontextprotocol/sdk/client/index.js';
import {StdioClientTransport} from '@modelcontextprotocol/sdk/client/stdio.js';
import {createBroker} from '../broker.mjs';

test('MCP handshake, tool discovery, live status and a plugin round-trip over actual stdio',async t=>{
  const secret=crypto.randomBytes(32).toString('hex'),broker=createBroker({port:0,secret},{pollMs:100});const port=await broker.listen();
  const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'wedex-figma-mcp-'));const config=path.join(tmp,'config.json');fs.writeFileSync(config,JSON.stringify({port,secret,workspaceRoot:path.resolve(import.meta.dirname,'../../..')}));
  const transport=new StdioClientTransport({command:process.execPath,args:[path.resolve(import.meta.dirname,'../server.mjs')],env:{...process.env,WEDEX_FIGMA_CONFIG:config},stderr:'pipe'});
  let stderr='';transport.stderr?.on('data',chunk=>{stderr+=chunk.toString();});
  const client=new Client({name:'integration-test',version:'1.0.0'});
  t.after(async()=>{await client.close();await broker.close();fs.rmSync(tmp,{recursive:true,force:true});});
  await client.connect(transport);
  const list=await client.listTools();assert.equal(list.tools.length,8);assert.ok(list.tools.some(t=>t.name==='figma_apply_prototype'));
  let result=await client.callTool({name:'figma_status',arguments:{}});assert.equal(JSON.parse(result.content[0].text).clients.length,0);
  const api=async(route,body)=>{const r=await fetch(`http://127.0.0.1:${port}${route}`,{method:'POST',headers:{Authorization:`Bearer ${secret}`,'Content-Type':'application/json'},body:JSON.stringify(body)});return r.status===204?null:r.json();};
  const {clientId}=await api('/plugin/attach',{fileName:'Actual transport mock',pageId:'0:1'});
  result=await client.callTool({name:'figma_status',arguments:{}});assert.equal(JSON.parse(result.content[0].text).clients[0].clientId,clientId);
  const getFile=client.callTool({name:'figma_get_file',arguments:{client_id:clientId}});let packet=null;for(let i=0;i<10&&!packet;i++)packet=await api('/plugin/poll',{clientId});assert.equal(packet.job.operation,'context');
  await api('/plugin/result',{clientId,jobId:packet.job.id,ok:true,result:{fileName:'Actual transport mock'}});
  assert.equal(JSON.parse((await getFile).content[0].text).fileName,'Actual transport mock');
  const invalid=await client.callTool({name:'figma_execute_code',arguments:{client_id:'not-a-uuid',code:'return 1;'}});assert.equal(invalid.isError,true);
  assert.equal(stderr.includes(secret),false);
});
