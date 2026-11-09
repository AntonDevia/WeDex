const page=await figma.getNodeByIdAsync(F.pages.prototype);await figma.setCurrentPageAsync(page);
const components=Object.fromEntries(await Promise.all(Object.entries({...K,...L}).filter(([,v])=>typeof v==='string').map(async([key,id])=>[key,await figma.getNodeByIdAsync(id)])));
const node=async id=>await figma.getNodeByIdAsync(id);
const TRACKS=[
 {key:'charlie',title:'How Long',artist:'Charlie Puth',time:'3:20',source:'Яндекс'},
 {key:'weeknd',title:'Blinding Lights',artist:'The Weeknd',time:'3:20',source:'Яндекс'},
 {key:'rihanna',title:'Needed Me',artist:'Rihanna',time:'3:11',source:'SoundCloud'},
 {key:'massive',title:'Teardrop',artist:'Massive Attack',time:'5:30',source:'Hitmo'},
 {key:'arctic',title:'Do I Wanna Know?',artist:'Arctic Monkeys',time:'4:32',source:'Яндекс'},
 {key:'oneheart',title:'snowfall',artist:'Øneheart · reidenshi',time:'2:04',source:'Яндекс'}
];
async function cloneRefs(root,originalRefs){
 const byName=name=>root.findOne(n=>n.name===name);
 const refs={nav:{},rows:[],cards:[],pills:[]};for(const label of ['Sonora','Поиск','Медиатека'])refs.nav[label]=byName('nav/'+label)?.id;
 for(const [key,name]of Object.entries({settings:'button/Настройки',profile:'button/Мой профиль',tune:'button/sliders-horizontal',queue:'button/list-music',currentTrack:'Now playing labels',previous:'button/skip-back',play:'Play / pause',next:'button/skip-forward',queueDock:'button/list-music',eq:'button/sliders-horizontal',back:'button/chevron-left',more:'button/ellipsis'})){const n=byName(name);if(n)refs[key]=n.id;}
 const dock=byName('Floating player');refs.queueDock=dock?.findOne(n=>n.name==='button/list-music')?.id;refs.eq=dock?.findOne(n=>n.name==='button/sliders-horizontal')?.id;
 const labels=['Слушать всё','Слушать','Перемешать'];for(const [i,key]of ['listen','listen','shuffle'].entries()){const n=byName('button/'+labels[i]);if(n)refs[key]=n.id;}
 refs.rows=root.findAllWithCriteria({types:['INSTANCE']}).filter(n=>n.name.startsWith('track/')).map(n=>({id:n.id,track:TRACKS.findIndex(t=>'track/'+t.key===n.name)}));
 const filter=byName('Library filters');if(filter)refs.pills=filter.children.map(n=>n.id);
 return refs;
}
function override(instance,name,value){const entry=Object.entries(instance.componentProperties).find(([key,p])=>key.split('#')[0]===name);if(entry)instance.setProperties({[entry[0]]:value});}
function artwork(instance,key){const cover=instance.findOne(n=>n.name==='Cover'&&n.type==='RECTANGLE');if(cover)cover.fills=[{type:'IMAGE',imageHash:ART[key],scaleMode:'FILL'}];}
async function button(parent,label,x,y,width=168,tone='Paper'){
 const n=place(await node(K.button[tone]),parent,'button/'+label,x,y,width,44);override(n,'Label',label);return n;
}
async function trackRow(parent,track,x,y,width=786,compact=false){
 const n=place(components.row,parent,'track/'+track.key,x,y,width,68);artwork(n,track.key);
 for(const [key,val]of Object.entries({Title:track.title,Artist:track.artist,Source:track.source,Time:track.time}))override(n,key,val);
 if(compact){for(const c of n.children){if(c.type==='TEXT'||c.name==='Save track')c.visible=false;}n.resize(width,56);n.paddingLeft=0;n.paddingRight=0;n.paddingTop=4;n.paddingBottom=4;}
 return n;
}
async function playlistCard(parent,title,subtitle,key,x,y){const n=place(components.playlistCard,parent,'playlist/'+title,x,y);override(n,'Title',title);override(n,'Subtitle',subtitle);artwork(n,key);return n;}
async function scaffold(name,col,row,active='Sonora',light=false,trackIndex=0){
 if(page.children.some(n=>n.name===name))throw new Error('Screen already exists: '+name);
 const root=box(page,name,1440,960,160+col*1560,160+row*1080,light?'surface/tint':'surface/canvas');root.clipsContent=true;
 const refs={nav:{},rows:[],cards:[],pills:[]};
 const bg=place(await node(K.ambient[0]),root,'Ambient light',0,0);if(light)bg.opacity=0.18;
 const sidebar=box(root,'Sidebar',184,800,24,24,null,24,'VERTICAL');padding(sidebar,16,24);await glass(sidebar);if(light)color(sidebar,'surface/dark-glass');
 const brand=box(sidebar,'Brand',152,82,0,0,null,0,'VERTICAL');padding(brand,8,4);
 const logo=await text(brand,'wedex','Title','text/inverse');logo.fontSize=26;logo.letterSpacing={unit:'PIXELS',value:-1.4};await text(brand,'МУЗЫКА БЛИЖЕ','Micro','text/inverse');
 const navigation=box(sidebar,'Navigation',152,176,0,0,null,0,'VERTICAL');navigation.itemSpacing=8;
 for(const [label,glyph]of [['Sonora','audio-lines'],['Поиск','search'],['Медиатека','library']]){
  const n=place(await node(K.nav[label===active?1:0]),navigation,'nav/'+label,0,0,152,48);override(n,'Label',label);override(n,'Glyph',F.icons[glyph]);refs.nav[label]=n.id;
 }
 const spacer=box(sidebar,'Flexible space',152,100);spacer.layoutSizingVertical='FILL';
 const footer=box(sidebar,'Account and settings',152,122,0,0,null,0,'VERTICAL');footer.itemSpacing=12;
 const source=box(footer,'Yandex connected',152,32,0,0,null,0,'HORIZONTAL');source.itemSpacing=8;source.counterAxisAlignItems='CENTER';
 const dot=remember(figma.createEllipse());dot.resize(5,5);source.appendChild(dot);color(dot,'accent/action');await text(source,'Яндекс подключён','Small','text/inverse');
 const settings=box(footer,'button/Настройки',152,36,0,0,null,0,'HORIZONTAL');settings.itemSpacing=12;settings.counterAxisAlignItems='CENTER';icon(settings,'settings-2',0,0,18);await text(settings,'Настройки','Small','text/inverse');refs.settings=settings.id;
 const profile=box(footer,'button/Мой профиль',152,32,0,0,null,0,'HORIZONTAL');profile.itemSpacing=10;profile.counterAxisAlignItems='CENTER';
 const avatar=box(profile,'Avatar',28,28,0,0,'surface/glass-strong',999,'HORIZONTAL');avatar.primaryAxisAlignItems='CENTER';avatar.counterAxisAlignItems='CENTER';await text(avatar,'А','Small','text/inverse');await text(profile,'Мой профиль','Small','text/inverse');refs.profile=profile.id;
 if(light)box(root,'Content surface',1192,800,224,24,'surface/content',32);
 const title=light?active:'МУЗЫКА БЕЗ ЛИШНЕГО';
 await text(root,title,light?'Small':'Micro',light?'text/secondary':'text/inverse',264,60);
 const topSettings=iconButton(root,'sliders-horizontal',1288,48,light?'text/primary':'text/inverse');refs.tune=topSettings.id;
 const topQueue=iconButton(root,'list-music',1340,48,light?'text/primary':'text/inverse');refs.queue=topQueue.id;
 const tr=TRACKS[trackIndex];
 const dock=box(root,'Floating player',1192,88,224,848,null,32,'HORIZONTAL');padding(dock,16,8);dock.primaryAxisAlignItems='SPACE_BETWEEN';dock.counterAxisAlignItems='CENTER';await glass(dock);color(dock,'surface/dark-glass');
 image(dock,'Current artwork',tr.key,44,0,0,12);
 const trackInfo=box(dock,'Now playing labels',156,44,0,0,null,0,'VERTICAL');await text(trackInfo,tr.title,'Body','text/inverse');const artist=await text(trackInfo,tr.artist,'Small','text/inverse');artist.opacity=0.66;refs.currentTrack=trackInfo.id;
 place(await node(K.toggle[0]),dock,'Save current track',0,0,32,32);
 refs.previous=iconButton(dock,'skip-back',0,0,'text/inverse',32).id;
 const play=place(await node(K.play[0]),dock,'Play / pause',0,0);refs.play=play.id;
 refs.next=iconButton(dock,'skip-forward',0,0,'text/inverse',32).id;
 const timeline=box(dock,'Timeline',320,44,0,0,null,0,'VERTICAL');timeline.itemSpacing=0;place(await node(L.sliders.Progress[0]),timeline,'Seek',0,0);
 const times=box(timeline,'Time labels',320,18,0,0,null,0,'HORIZONTAL');times.primaryAxisAlignItems='SPACE_BETWEEN';const at=await text(times,'0:48','Micro','text/inverse');at.opacity=0.6;const total=await text(times,tr.time,'Micro','text/inverse');total.opacity=0.6;
 const shuffle=place(await node(K.toggle[0]),dock,'Shuffle',0,0,32,32);override(shuffle,'Glyph',F.icons.shuffle);
 refs.queueDock=iconButton(dock,'list-music',0,0,'text/inverse',32).id;
 refs.eq=iconButton(dock,'sliders-horizontal',0,0,'text/inverse',32).id;
 place(await node(K.volume[0]),dock,'Mute',0,0,32,32);place(await node(L.sliders['Volume slider'][2]),dock,'Volume',0,0);
 return {root,refs,bg};
}
async function nowPlaying(root,refs,trackIndex=0){
 const track=TRACKS[trackIndex];const panel=box(root,'Now playing panel',268,610,1108,160,'surface/tint',24,'VERTICAL');padding(panel,20,8);
 const head=box(panel,'Panel heading',228,24,0,0,null,0,'HORIZONTAL');head.itemSpacing=8;head.counterAxisAlignItems='CENTER';icon(head,'audio-lines',0,0,16,'text/primary');await text(head,'Сейчас играет','Small');
 image(panel,'Panel artwork',track.key,228,0,0,20);
 const label=box(panel,'Track title',228,66,0,0,null,0,'VERTICAL');await text(label,track.title,'Section');await text(label,track.artist,'Small','text/secondary');
 const queue=box(panel,'Next heading',228,24,0,0,null,0,'HORIZONTAL');queue.primaryAxisAlignItems='SPACE_BETWEEN';await text(queue,'Далее','Small','text/secondary');refs.queuePanel=iconButton(queue,'list-music',0,0,'text/primary',24).id;
 for(const i of [1,2,5]){const n=await trackRow(panel,TRACKS[i],0,0,228,true);refs.rows.push({id:n.id,track:i});}
 return panel;
}
async function sora(name,col,trackIndex){
 const {root,refs}=await scaffold(name,col,0,'Sonora',false,trackIndex);
 const title=await text(root,'sonora','Display','text/inverse',264,112);title.letterSpacing={unit:'PIXELS',value:-5};
 const sub=await text(root,'Ваша музыка. Новые маршруты.','Body','text/inverse',269,213);sub.opacity=0.62;
 await pill(root,'ПОТОК ДЛЯ ВАС',1208,122,168);
 const stage=box(root,'Cover flow · swipe',1020,470,320,280);stage.clipsContent=false;refs.carousel=stage.id;
 const positions=[{offset:-2,x:8,y:104,scale:0.66,rotate:8},{offset:2,x:764,y:104,scale:0.66,rotate:-8},{offset:-1,x:156,y:58,scale:0.84,rotate:5},{offset:1,x:610,y:58,scale:0.84,rotate:-5},{offset:0,x:350,y:12,scale:1,rotate:0}];
 for(const spec of positions){
  const index=(trackIndex+spec.offset+TRACKS.length)%TRACKS.length;const track=TRACKS[index];
  const card=place(components.card,stage,'Album/'+track.key,spec.x,spec.y);card.rescale(spec.scale);card.rotation=spec.rotate;
  artwork(card,track.key);override(card,'Title',track.title);override(card,'Artist',track.artist);if(spec.offset!==0)card.opacity=Math.abs(spec.offset)===2?0.45:0.8;
  refs.cards.push({id:card.id,track:index});
 }
 const reason=box(root,'Recommendation reason',880,40,388,750,null,0,'HORIZONTAL');reason.primaryAxisAlignItems='CENTER';reason.counterAxisAlignItems='CENTER';reason.itemSpacing=12;
 const dot=remember(figma.createEllipse());dot.resize(5,5);reason.appendChild(dot);color(dot,'accent/action');
 const reasonText=await text(reason,'Похожее на вашу музыку · рекомендации Last.fm','Small','text/inverse');reasonText.opacity=0.6;
 refs.discover=(await pill(root,'Больше открытий',1040,750,168)).id;
 return {id:root.id,name,refs};
}
