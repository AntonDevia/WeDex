const extra={};
const versionBase=await node(O.versionSelected.id);
for(const [index,col,key]of [[0,2,'versionYandex'],[2,3,'versionSoundcloud']]){
 const root=rememberTree(versionBase.clone());page.appendChild(root);root.name='20 · Overlay — Версия '+['Яндекс','Hitmo','SoundCloud'][index];root.x=160+col*1560;root.y=160+6*1080;
 const panel=root.findOne(n=>n.name==='Glass dialog');const rows=panel.children.filter(n=>n.name.startsWith('Version/'));
 for(const [i,row]of rows.entries()){
  color(row,i===index?'surface/glass-strong':'surface/glass');
  const labels=row.findAllWithCriteria({types:['TEXT']});const status=labels.find(n=>n.fontSize===12);
  if(status)status.characters=i===index?'Выбрано вами':i===0?'Explicit · отметка каталога':'Версия не проверена';
  const choice=row.children.find(n=>n.type==='INSTANCE'&&n.name.startsWith('button/'));
  if(choice){const iconKey=Object.keys(choice.componentProperties).find(k=>k.split('#')[0]==='Icon');choice.setProperties({[iconKey]:F.icons[i===index?'check':'plus']});}
 }
 const refs={close:root.findOne(n=>n.name==='button/x').id,scrim:root.findOne(n=>n.name==='Click outside to close').id,choices:rows.map(row=>row.children.find(n=>n.type==='INSTANCE'&&n.name.startsWith('button/')).id),rows:[],trackLabel:root.findOne(n=>n.type==='TEXT'&&n.characters==='Текущий трек').id,apply:root.findOne(n=>n.name==='button/Сохранить выбор').id};
 override(await node(refs.apply),'Label','Сохранить оригинал');extra[key]={id:root.id,name:root.name,refs,panel:panel.id};
}
const selected=await node(O.versionSelected.id);const status=selected.findOne(n=>n.type==='TEXT'&&n.characters==='Оригинал · выбрано вами');if(status){status.characters='Выбрано вами';changed.push(status.id);}
override(await node(O.versionSelected.refs.apply),'Label','Сохранить оригинал');changed.push(O.versionSelected.refs.apply);
for(const item of [O.versions,O.versionSelected,...Object.values(extra)]){const f=await node(item.id);const label=f.findOne(n=>n.type==='TEXT'&&n.characters==='Выберите запись после сравнения звучания.');if(label){label.characters='Сравните звучание и подтвердите оригинал.';changed.push(label.id);}}
const toast=box(page,'21 · Overlay — Медиатека обновлена',1440,960,160+4*1560,160+6*1080);const message=box(toast,'Toast',352,56,544,748,null,16,'HORIZONTAL');padding(message,16,12);message.counterAxisAlignItems='CENTER';await glass(message);color(message,'surface/canvas','fills',0.9);icon(message,'check',0,0,18,'accent/action');await text(message,'Медиатека обновлена','Body','text/inverse');extra.toast={id:toast.id,name:toast.name,refs:{},panel:message.id};
// Results are track-only in this app; remove controls for unsupported artist/album result modes.
const results=await node(B.results.id);const filters=results.findOne(n=>n.name==='Search filters');const keep=filters.children[0];override(keep,'Label','Треки');keep.resize(120,38);
for(const n of [...filters.children].slice(1))n.remove();changed.push(filters.id,keep.id);
await text(filters,'Совпадения во всех источниках','Small','text/secondary');
// Add a discoverable track menu beside each now-playing title.
const menus={};for(const s of Object.values(S)){const f=await node(s.id);if(f.findOne(n=>n.name==='Now playing panel'))menus[s.id]=iconButton(f,'ellipsis',1320,448,'text/primary',32).id;}
return {createdNodeIds:made,mutatedNodeIds:changed,extra,menus};
