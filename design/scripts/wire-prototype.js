const overlays={...O,...EX.extra};
const overlayOwners=new Map();
function assignOverlayOwner(value,owner){if(typeof value==='string')overlayOwners.set(value,owner);else if(Array.isArray(value))for(const v of value)assignOverlayOwner(v,owner);else if(value&&typeof value==='object')for(const v of Object.values(value))assignOverlayOwner(v,owner);}
for(const overlay of Object.values(overlays))assignOverlayOwner(overlay.refs,overlay.id);
const destinations=[S.sonora,S.sonoraNext,S.sonoraThird,S.sonoraMassive,S.sonoraArctic,S.sonoraSnowfall];
const links=[];
const closeAction={type:'CLOSE'};
function overlayAction(id,swap=false){return {type:'NODE',destinationId:id,navigation:swap?'SWAP':'OVERLAY',transition:{type:swap?'SMART_ANIMATE':'DISSOLVE',duration:swap?0.28:0.22,easing:{type:'EASE_OUT'}},resetInteractiveComponents:swap&&['eq','eqWarm','eqReset'].some(key=>overlays[key]?.id===id)};}
async function react(id,actions,trigger={type:'ON_CLICK'}){if(!id)return;const n=await node(id);if(!n)throw new Error('Missing control '+id);await n.setReactionsAsync([{trigger,actions}]);changed.push(id);for(const a of actions)if(a.type==='NODE')links.push({from:id,to:a.destinationId,navigation:a.navigation});}
let currentScreenId=null;
async function navigate(id,dest,duration=0.35){if(dest===currentScreenId)return;await react(id,[navAction(dest,'NAVIGATE',duration)]);}
async function open(id,name){await react(id,[overlayAction(overlays[name].id)]);}
async function swap(id,name){if(overlayOwners.get(id)===overlays[name].id)return;await react(id,[overlayAction(overlays[name].id,true)]);}
async function dismiss(id){await react(id,[closeAction]);}
async function version(id,trackIndex=0){await react(id,[overlayAction(overlays.versions.id)]);}
const byPlaylist={'Ночной маршрут':S.playlist,'На повторе':S.playlist0,'Громче мыслей':S.playlist1,'Медленный вечер':S.playlist2};
for(const [key,screen]of Object.entries(S)){
 currentScreenId=screen.id;
 const r=screen.refs;const index=Math.max(0,destinations.findIndex(d=>d.id===screen.id));
 for(const [label,dest]of Object.entries({Sonora:S.sonora,Поиск:S.search,Медиатека:S.library}))await navigate(r.nav[label],dest.id);
 for(const id of [r.settings,r.profile])await open(id,'settings');
 await open(r.tune,key.startsWith('sonora')?'tune':'settings');
 for(const id of [r.queue,r.queueDock,r.queuePanel])await open(id,'queue');
 await open(r.eq,'eq');await version(r.currentTrack,key.startsWith('playlist')?5:index);
 await navigate(r.previous,destinations[(index+5)%6].id,0.5);await navigate(r.next,destinations[(index+1)%6].id,0.5);
 if(r.carousel){await react(r.carousel,[navAction(destinations[(index+1)%6].id,'NAVIGATE',0.5)],{type:'ON_DRAG'});}
 if(r.discover)await open(r.discover,'tuneExplore');
 for(const row of r.rows??[])await navigate(row.id,destinations[row.track]?.id??S.sonora.id,0.4);
 for(const card of r.cards??[]){if(card.playlist)await navigate(card.id,byPlaylist[card.playlist].id);else if(card.track!==undefined)await navigate(card.id,destinations[card.track].id,0.5);}
 if(r.searchField)await navigate(r.searchField,key==='results'?S.search.id:S.results.id);
 if(r.mic)await open(r.mic,'recognition');if(r.bestMatch)await navigate(r.bestMatch,S.sonoraNext.id);
 if(['library','favorites','playlists'].includes(key)){for(const [i,dest]of [S.library,S.favorites,S.playlists].entries())await navigate(r.pills[i],dest.id);}
 if(r.listen)await navigate(r.listen,key==='playlist'?S.sonoraSnowfall.id:key==='playlist0'?S.sonora.id:key==='playlist1'?S.sonoraArctic.id:key==='playlist2'?S.sonoraMassive.id:S.sonora.id);
 if(r.shuffle)await navigate(r.shuffle,S.sonoraArctic.id,0.5);
 if(r.import)await open(r.import,'toast');if(r.back)await navigate(r.back,S.playlists.id);
 if(r.more)await version(r.more,5);
 if(EX.menus[screen.id])await version(EX.menus[screen.id],key.startsWith('playlist')?5:0);
 const root=await node(screen.id);
 if(key.startsWith('sonora')){
  await root.setReactionsAsync([{trigger:{type:'ON_KEY_DOWN',device:'KEYBOARD',keyCodes:[39]},actions:[navAction(destinations[(index+1)%6].id,'NAVIGATE',0.5)]},{trigger:{type:'ON_KEY_DOWN',device:'KEYBOARD',keyCodes:[37]},actions:[navAction(destinations[(index+5)%6].id,'NAVIGATE',0.5)]}]);changed.push(root.id);
 }
}
for(const [key,overlay]of Object.entries(overlays)){
 const r=overlay.refs;
 for(const id of [r.close,r.scrim,r.done,r.cancel,r.apply])if(id)await dismiss(id);
 const frame=await node(overlay.id);await frame.setReactionsAsync([{trigger:{type:'ON_KEY_DOWN',device:'KEYBOARD',keyCodes:[27]},actions:[closeAction]}]);changed.push(frame.id);
 if(key==='queue')for(const row of r.rows)await react(row.id,[navAction(destinations[row.track].id)]);
}
for(const key of ['eq','eqWarm',...(overlays.eqReset?['eqReset']:[])]){
 const r=overlays[key].refs,flat=key==='eq'&&overlays.eqReset?'eqReset':'eq';
 await swap(r.flat,flat);await swap(r.warm,'eqWarm');await swap(r.reset,flat);
}
for(const key of ['tune','tuneExplore']){const r=overlays[key].refs;await swap(r.choices[0],'tune');await swap(r.choices[1],'tuneExplore');}
await swap(overlays.settings.refs.connect,'settingsConnected');await swap(overlays.settingsConnected.refs.connect,'settings');
const listen=await node(overlays.recognition.id);await listen.setReactionsAsync([{trigger:{type:'AFTER_TIMEOUT',timeout:2.6},actions:[overlayAction(overlays.recognized.id,true)]},{trigger:{type:'ON_KEY_DOWN',device:'KEYBOARD',keyCodes:[27]},actions:[closeAction]}]);changed.push(listen.id);
await react(overlays.recognized.refs.find,[navAction(S.results.id)]);
for(const key of ['versions','versionSelected','versionYandex','versionSoundcloud'])for(const [i,target]of ['versionYandex','versionSelected','versionSoundcloud'].entries())await swap(overlays[key].refs.choices[i],target);
const toast=await node(overlays.toast.id);await toast.setReactionsAsync([{trigger:{type:'AFTER_TIMEOUT',timeout:1.6},actions:[closeAction]},{trigger:{type:'ON_CLICK'},actions:[closeAction]}]);changed.push(toast.id);
page.flowStartingPoints=[{nodeId:S.sonora.id,name:'WeDex · Начать слушать'}];page.prototypeBackgrounds=[{type:'SOLID',color:rgb('#18171B')}];changed.push(page.id);
figma.viewport.scrollAndZoomIntoView([await node(S.sonora.id)]);
// Check all graph destinations now, including every overlay and every selected source state.
const unique=[...new Set(links.map(l=>l.to))];const missing=[];for(const id of unique)if(!await node(id))missing.push(id);
return {createdNodeIds:made,mutatedNodeIds:[...new Set(changed)],links,validation:{screens:Object.keys(S).length,overlays:Object.keys(overlays).length,linkedActions:links.length,distinctDestinations:unique.length,missingDestinations:missing},startingFrame:S.sonora.id};
