import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawn} from 'node:child_process';

export const workspaceRoot=path.resolve(import.meta.dirname,'../..');
export const privateDir=path.join(workspaceRoot,'.local','figma-local-mcp');
export const configPath=process.env.WEDEX_FIGMA_CONFIG||path.join(privateDir,'config.json');
export function loadConfig(){
  const config=JSON.parse(fs.readFileSync(configPath,'utf8'));
  if(!Number.isInteger(config.port)||config.port<1024||config.port>65535||!/^[a-f0-9]{64}$/.test(config.secret))throw new Error('Invalid local bridge configuration. Run setup.mjs.');
  return {...config,host:'127.0.0.1',workspaceRoot:path.resolve(config.workspaceRoot)};
}
export function createConfig(){
  fs.mkdirSync(privateDir,{recursive:true});
  if(!fs.existsSync(configPath))fs.writeFileSync(configPath,JSON.stringify({version:1,port:38473,secret:crypto.randomBytes(32).toString('hex'),workspaceRoot},null,2),{mode:0o600,flag:'wx'});
  return loadConfig();
}
export async function bridgeFetch(config,route,body,timeout=10000){
  const res=await fetch(`http://127.0.0.1:${config.port}${route}`,{method:body===undefined?'GET':'POST',headers:{Authorization:`Bearer ${config.secret}`,'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(timeout)});
  const value=res.status===204?null:await res.json();
  if(!res.ok){const error=new Error(value?.error||`Bridge HTTP ${res.status}`);error.outcome=value?.outcome;throw error;}
  return value;
}
export async function ensureBroker(config){
  try{const res=await fetch(`http://127.0.0.1:${config.port}/health`,{signal:AbortSignal.timeout(1000)});const value=await res.json();if(value.service!=='wedex-figma-bridge')throw new Error('Port belongs to another service');return;}catch(error){if(error.message==='Port belongs to another service')throw error;}
  const child=spawn(process.execPath,[path.join(import.meta.dirname,'broker.mjs')],{detached:true,windowsHide:true,stdio:'ignore',env:{...process.env,WEDEX_FIGMA_CONFIG:configPath}});child.unref();
  for(let attempt=0;attempt<35;attempt++){
    await new Promise(resolve=>setTimeout(resolve,100));
    try{const health=await bridgeFetch(config,'/status');if(health.service==='wedex-figma-bridge')return;}catch{}
  }
  throw new Error('Local Figma broker did not start. Run node tools/figma-local-mcp/broker.mjs to inspect the error.');
}
export function localFile(config,input){
  const file=path.resolve(config.workspaceRoot,input),root=fs.realpathSync(config.workspaceRoot);
  const real=fs.realpathSync(file),relative=path.relative(root,real);
  if(relative==='..'||relative.startsWith('..'+path.sep)||path.isAbsolute(relative))throw new Error('The file must be inside the configured workspace.');
  return real;
}
