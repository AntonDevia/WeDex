import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..');
const read=name=>JSON.parse(fs.readFileSync(path.join(root,name),'utf8'));
const foundation=read('figma-state.json').foundation;
const S={...read('figma-sonora.json').screens,...read('figma-browse.json').screens,...read('figma-extra.json').screens};
const O=read('figma-overlays.json').overlays,completed=read('figma-complete-states.json');
const refinements=fs.existsSync(path.join(root,'figma-control-refinements.json'))?read('figma-control-refinements.json'):{};
const EX={...completed,extra:{...completed.extra,...refinements.extra}};
const allOverlays={...O,...EX.extra};
const ledgerFiles=['figma-state.json','figma-components.json','figma-list-components.json','figma-sonora.json','figma-browse.json','figma-extra.json','figma-overlays.json','figma-complete-states.json',...(refinements.createdNodeIds?['figma-control-refinements.json']:[])];
const known=new Set(Object.values(foundation.pages));
function collect(value){if(!value||typeof value!=='object')return;for(const [key,v]of Object.entries(value)){if(key==='createdNodeIds')for(const id of v)known.add(id);else if(v&&typeof v==='object')collect(v);}}
for(const name of ledgerFiles)collect(read(name));
const planned=new Map();
const nodes=new Map([...known].map(id=>[id,{id,reactions:[],async setReactionsAsync(reactions){this.reactions=structuredClone(reactions);planned.set(id,this.reactions);}}]));
const page=nodes.get(foundation.pages.prototype);
const figma={getNodeByIdAsync:async id=>nodes.get(id)||null,viewport:{scrollAndZoomIntoView(){}}};
const navAction=(destination,navigation='NAVIGATE',duration=0.35)=>({type:'NODE',destinationId:destination,navigation,transition:{type:'SMART_ANIMATE',duration,easing:{type:'EASE_IN_AND_OUT'}},resetScrollPosition:false,resetInteractiveComponents:false});
const rgb=hex=>({r:parseInt(hex.slice(1,3),16)/255,g:parseInt(hex.slice(3,5),16)/255,b:parseInt(hex.slice(5,7),16)/255});
const code=fs.readFileSync(path.join(root,'scripts/wire-prototype.js'),'utf8');
const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
const compile=new AsyncFunction('figma','page','node','S','O','EX','changed','made','navAction','rgb',code);
const result=await compile(figma,page,figma.getNodeByIdAsync,S,O,EX,[],[],navAction,rgb);
assert.equal(result.validation.missingDestinations.length,0);
for(const [id,reactions]of planned){assert(known.has(id));for(const reaction of reactions)for(const action of reaction.actions||[])if(action.type==='NODE')assert(known.has(action.destinationId),'Unknown destination '+action.destinationId);}
// Prove every requested screen/panel is reachable from the initial Sonora state.
const owner=new Map();function refs(value,id){if(typeof value==='string')owner.set(value,id);else if(Array.isArray(value))for(const x of value)refs(x,id);else if(value&&typeof value==='object')for(const x of Object.values(value))refs(x,id);}
for(const screen of Object.values(S)){owner.set(screen.id,screen.id);refs(screen.refs,screen.id);}
for(const screen of Object.values(allOverlays)){owner.set(screen.id,screen.id);refs(screen.refs,screen.id);}
for(const [screenId,menu]of Object.entries(EX.menus))owner.set(menu,screenId);
const edges=[];for(const [from,reactions]of planned)for(const reaction of reactions)for(const action of reaction.actions||[])if(action.type==='NODE')edges.push({from,to:action.destinationId,navigation:action.navigation});
// Native Figma rejects self-navigation and same-overlay swaps, and this file's
// plan supports one action per reaction. Catch those errors before any writes.
for(const reactions of planned.values())for(const reaction of reactions)assert.equal((reaction.actions||[]).length,1,'Expected one action per prototype trigger');
for(const link of edges)if(['NAVIGATE','SWAP'].includes(link.navigation))assert.notEqual(owner.get(link.from),link.to,'Self-navigation is invalid in native Figma: '+link.from);
const adjacency=new Map();for(const link of edges){const source=owner.get(link.from);if(source){if(!adjacency.has(source))adjacency.set(source,new Set());adjacency.get(source).add(link.to);}}
const reached=new Set([S.sonora.id]),pending=[S.sonora.id];while(pending.length){for(const next of adjacency.get(pending.shift())||[]){if(!reached.has(next)){reached.add(next);pending.push(next);}}}
const expected=Object.values({...S,...allOverlays}).map(s=>s.id);
const unreachable=expected.filter(id=>!reached.has(id));assert.deepEqual(unreachable,[]);
const plan={fileKey:'hh9WPK6OnPYis1tL7S0jDY',pageId:foundation.pages.prototype,startingFrame:S.sonora.id,expectedFrames:Object.values({...S,...allOverlays}).map(s=>({id:s.id,name:s.name})),reactions:[...planned].map(([nodeId,reactions])=>({nodeId,reactions})),flowStartingPoints:page.flowStartingPoints,prototypeBackgrounds:page.prototypeBackgrounds,validation:{...result.validation,linkedActions:edges.length,controls:planned.size,reachable:reached.size,unreachable}};
const folder=path.join(root,'finish-prototype');fs.mkdirSync(folder,{recursive:true});
fs.writeFileSync(path.join(folder,'plan.json'),JSON.stringify(plan,null,2));
const runtime=fs.readFileSync(path.join(root,'scripts/native-plugin-runtime.js'),'utf8');
fs.writeFileSync(path.join(folder,'code.js'),'const PLAN='+JSON.stringify(plan)+';\n'+runtime);
fs.writeFileSync(path.join(root,'prototype-plan-validation.json'),JSON.stringify(plan.validation,null,2));
console.log(JSON.stringify(plan.validation,null,2));
