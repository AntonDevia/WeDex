const screens={};
for(const [name,col,index,key]of [['09 · Sonora — Teardrop',3,3,'sonoraMassive'],['10 · Sonora — Do I Wanna Know',4,4,'sonoraArctic'],['11 · Sonora — snowfall',5,5,'sonoraSnowfall']]){
 const scene=await sora(name,col,index);screens[key]=scene;
 const frame=await node(scene.id);const ambient=frame.findOne(n=>n.name==='Ambient light');
 const palette=index===3?['#486B5F','#A1BA78','#273932','#354E52']:index===4?['#375A67','#989E9E','#243238','#58646B']:['#174E83','#487AAA','#342859','#3D376A'];
 for(const [i,ellipse]of ambient.findAllWithCriteria({types:['ELLIPSE']}).entries()){
  ellipse.fills=ellipse.fills.map(p=>p.type==='GRADIENT_RADIAL'?{...p,gradientStops:p.gradientStops.map(s=>({...s,color:{...rgb(palette[i%4]),a:s.color.a}}))}:p);changed.push(ellipse.id);
 }
}
{
 const base=await node(B.library.id);const root=rememberTree(base.clone());page.appendChild(root);root.name='12 · Медиатека — Плейлисты';root.x=160+2*1560;root.y=160+2*1080;
 const title=root.findOne(n=>n.type==='TEXT'&&n.characters==='Ваша медиатека');title.characters='Плейлисты';
 root.findOne(n=>n.name==='Favorite collection').remove();
 const heading=root.findOne(n=>n.type==='TEXT'&&n.characters==='Ваши плейлисты');heading.y=286;
 const grid=root.findOne(n=>n.name==='Playlist grid');grid.y=328;
 const filters=root.findOne(n=>n.name==='Library filters');for(const [i,c]of filters.children.entries()){if(c.type==='INSTANCE'){c.swapComponent(await node(K.segment[i===2?1:0]));override(c,'Label',['Обзор','Понравившиеся','Плейлисты'][i]);}}
 const refs=await cloneRefs(root,B.library.refs);refs.pills=filters.children.map(n=>n.id);refs.cards=grid.children.map((n,i)=>({id:n.id,playlist:['Ночной маршрут','На повторе','Громче мыслей','Медленный вечер'][i]}));delete refs.listen;
 await text(root,'Ваши коллекции синхронизируются из Яндекс Музыки.','Small','text/secondary',270,638);
 screens.playlists={id:root.id,name:root.name,refs};
}
for(const [i,title,key]of [[0,'На повторе','charlie'],[1,'Громче мыслей','arctic'],[2,'Медленный вечер','massive']]){
 const base=await node(B.playlist.id);const root=rememberTree(base.clone());page.appendChild(root);root.name='13.'+(i+1)+' · Плейлист — '+title;root.x=160+i*1560;root.y=160+3*1080;
 root.findOne(n=>n.type==='TEXT'&&n.characters==='Ночной маршрут').characters=title;
 const art=root.findOne(n=>n.name==='Playlist artwork');art.fills=[{type:'IMAGE',imageHash:ART[key],scaleMode:'FILL'}];
 const refs=await cloneRefs(root,B.playlist.refs);screens['playlist'+i]={id:root.id,name:root.name,refs};
}
return {createdNodeIds:made,mutatedNodeIds:changed,screens};
