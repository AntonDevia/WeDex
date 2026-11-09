const overlays={};
async function overlay(name,col,row,w,h,x=(1440-w)/2,y=(960-h)/2){
 const root=box(page,name,1440,960,160+col*1560,160+row*1080);root.clipsContent=true;
 const scrim=box(root,'Click outside to close',1440,960,0,0,'surface/canvas');scrim.opacity=0.46;
 const panel=box(root,'Glass dialog',w,h,x,y,null,32);await glass(panel);color(panel,'surface/canvas','fills',0.82);
 const close=iconButton(panel,'x',w-56,16,'text/inverse',36);
 const refs={close:close.id,scrim:scrim.id,choices:[],rows:[]};return {root,panel,refs};
}
{
 const {root,panel,refs}=await overlay('14 · Overlay — Очередь',3,1,372,760,1044,44);
 await text(panel,'Очередь','Title','text/inverse',28,28);await text(panel,'Ваш поток, трек за треком','Small','text/inverse',28,68);
 await text(panel,'УЖЕ ПРОЗВУЧАЛО','Micro','text/inverse',28,116);
 const old=await trackRow(panel,TRACKS[3],28,146,316,true);old.opacity=0.45;refs.rows.push({id:old.id,track:3});
 await text(panel,'СЕЙЧАС','Micro','accent/action',28,226);
 const current=await trackRow(panel,TRACKS[0],28,258,316,true);color(current,'surface/glass');refs.rows.push({id:current.id,track:0});
 await text(panel,'ДАЛЕЕ','Micro','text/inverse',28,340);
 for(const [i,index]of [1,2,5,4].entries()){const n=await trackRow(panel,TRACKS[index],28,374+i*66,316,true);refs.rows.push({id:n.id,track:index});}
 for(const row of panel.findAllWithCriteria({types:['INSTANCE']}).filter(n=>n.name.startsWith('track/'))){for(const label of row.findAllWithCriteria({types:['TEXT']}))color(label,'text/inverse');}
 await text(panel,'Подобрано по вашей музыке в Last.fm','Small','text/inverse',28,708);
 overlays.queue={id:root.id,name:root.name,refs,panel:panel.id};
}
for(const [warm,col]of [[false,0],[true,1]]){
 const {root,panel,refs}=await overlay('15 · Overlay — EQ '+(warm?'Теплее':'Ровно'),col,4,720,484);
 await text(panel,'Эквалайзер','Title','text/inverse',32,28);await text(panel,'Чуть больше вашего звучания.','Small','text/inverse',32,70);
 const bands=box(panel,'Frequency bands',656,228,32,116,null,0,'HORIZONTAL');bands.primaryAxisAlignItems='SPACE_BETWEEN';
 const gains=warm?[4,3,2,1,0,-1,-1,0,1,2]:[0,0,0,0,0,0,0,0,0,0];
 for(const [i,hz]of ['31','62','125','250','500','1k','2k','4k','8k','16k'].entries()){
  const lane=box(bands,'EQ/'+hz,52,228);await text(lane,(gains[i]>0?'+':'')+gains[i],'Small','text/inverse',18,0);
  box(lane,'Rail',3,140,25,32,'surface/glass-strong',999);
  const knob=remember(figma.createEllipse());lane.appendChild(knob);knob.resize(12,12);knob.x=20;knob.y=96-gains[i]*8;color(knob,'accent/action');
  await text(lane,hz,'Small','text/inverse',hz.length===2?20:15,196);refs.choices.push(lane.id);
 }
 refs.flat=(await button(panel,'Ровно',32,376,152,warm?'Glass':'Primary')).id;
 refs.warm=(await button(panel,'Теплее',196,376,152,warm?'Primary':'Glass')).id;
 refs.reset=(await button(panel,'Сбросить',360,376,152,'Glass')).id;
 refs.done=(await button(panel,'Готово',524,376,164,'Paper')).id;
 overlays[warm?'eqWarm':'eq']={id:root.id,name:root.name,refs,panel:panel.id};
}
for(const [explore,col]of [[false,2],[true,3]]){
 const {root,panel,refs}=await overlay('16 · Overlay — Характер '+(explore?'Открытия':'Знакомое'),col,4,560,366);
 await text(panel,'Характер потока','Title','text/inverse',28,28);await text(panel,'Куда пойдём сегодня?','Small','text/inverse',28,70);
 for(const [i,label]of ['Ближе к любимому','Больше открытий'].entries()){
  const choice=box(panel,'Choice/'+label,240,116,28+i*264,112,i===(explore?1:0)?'surface/glass-strong':'surface/glass',20,'VERTICAL');padding(choice,16,12);
  icon(choice,i===0?'heart':'shuffle',0,0,22,i===(explore?1:0)?'accent/action':'text/inverse');await text(choice,label,'Body','text/inverse');refs.choices.push(choice.id);
 }
 refs.apply=(await button(panel,'Применить',28,270,504,'Primary')).id;
 overlays[explore?'tuneExplore':'tune']={id:root.id,name:root.name,refs,panel:panel.id};
}
for(const [connected,col]of [[false,0],[true,1]]){
 const {root,panel,refs}=await overlay('17 · Overlay — Источники '+(connected?'Подключено':'Обзор'),col,5,620,626);
 await text(panel,'Источники музыки','Title','text/inverse',28,28);await text(panel,'Любимая музыка собирается здесь.','Small','text/inverse',28,70);
 const sources=[['Яндекс Музыка','Подключён · подписка Плюс'],['Last.fm','Подборки активны'],['Hitmo','Доступен'],['SoundCloud',connected?'Подключён · демо':'Не подключён'],['YouTube Music','Открывать в сервисе']];
 for(const [i,[title,status]]of sources.entries()){
  const row=box(panel,'Source/'+title,564,70,28,116+i*76,'surface/glass',16,'HORIZONTAL');padding(row,12,12);row.counterAxisAlignItems='CENTER';
  const badge=box(row,'Source mark',40,40,0,0,'surface/glass-strong',12,'HORIZONTAL');badge.primaryAxisAlignItems='CENTER';badge.counterAxisAlignItems='CENTER';await text(badge,['Я','fm','H','S','Y'][i],'Body','text/inverse');
  const labels=box(row,'Source labels',282,44,0,0,null,0,'VERTICAL');await text(labels,title,'Body','text/inverse');const sub=await text(labels,status,'Small','text/inverse');sub.opacity=0.58;
  if(i===3)refs.connect=(await button(row,connected?'Подключён':'Подключить',0,0,180,connected?'Glass':'Primary')).id;
  else icon(row,'check',0,0,18,'accent/action');
 }
 await text(panel,'В прототипе подключение аккаунтов демонстрационное.','Small','text/inverse',32,520);
 refs.done=(await button(panel,'Готово',28,560,564,'Glass')).id;
 overlays[connected?'settingsConnected':'settings']={id:root.id,name:root.name,refs,panel:panel.id};
}
for(const [found,col]of [[false,2],[true,3]]){
 const {root,panel,refs}=await overlay('18 · Overlay — Распознавание '+(found?'Найдено':'Слушаем'),col,5,520,486);
 await text(panel,found?'Нашли ваш трек':'Слушаем музыку','Title','text/inverse',28,28);
 await text(panel,found?'Осталось выбрать, где слушать.':'Поднесите микрофон ближе к источнику звука.','Small','text/inverse',28,70);
 if(found){image(panel,'Recognized artwork','weeknd',144,188,120,24);const title=await text(panel,'Blinding Lights','Title','text/inverse',0,284);title.x=(520-title.width)/2;const artist=await text(panel,'The Weeknd','Small','text/inverse',0,324);artist.x=(520-artist.width)/2;refs.find=(await button(panel,'Найти в каталогах',28,380,464,'Primary')).id;}
 else {
  for(const [i,size]of [164,128,88].entries()){const circle=remember(figma.createEllipse());panel.appendChild(circle);circle.resize(size,size);circle.x=(520-size)/2;circle.y=122+(164-size)/2;color(circle,'surface/glass','fills',1-i*0.1);}
  icon(panel,'mic',238,182,44,'accent/action');await text(panel,'Улавливаем мелодию…','Body','text/inverse',181,318);refs.cancel=(await button(panel,'Отменить',28,380,464,'Glass')).id;
 }
 overlays[found?'recognized':'recognition']={id:root.id,name:root.name,refs,panel:panel.id};
}
for(const [selected,col]of [[false,0],[true,1]]){
 const {root,panel,refs}=await overlay('19 · Overlay — Версии '+(selected?'Выбрана':'Выбор'),col,6,620,536);
 await text(panel,'Версии трека','Title','text/inverse',28,28);const label=await text(panel,'Текущий трек','Body','text/inverse',28,76);refs.trackLabel=label.id;
 await text(panel,'Выберите запись после сравнения звучания.','Small','text/inverse',28,108);
 for(const [i,[source,status]]of [['Яндекс Музыка','Explicit · отметка каталога'],['Hitmo',selected?'Оригинал · выбрано вами':'Версия не проверена'],['SoundCloud','Версия не проверена']].entries()){
  const row=box(panel,'Version/'+source,564,82,28,154+i*94,selected&&i===1?'surface/glass-strong':'surface/glass',16,'HORIZONTAL');padding(row,16,12);row.counterAxisAlignItems='CENTER';
  const labels=box(row,'Version labels',350,48,0,0,null,0,'VERTICAL');await text(labels,source,'Body','text/inverse');const sub=await text(labels,status,'Small','text/inverse');sub.opacity=0.62;
  place(await node(K.play[1]),row,'Preview version',0,0,40,40);
  const choice=iconButton(row,selected&&i===1?'check':'plus',0,0,selected&&i===1?'accent/action':'text/inverse',32);refs.choices.push(choice.id);
 }
 refs.apply=(await button(panel,selected?'Сохранить выбор':'Оставить текущую',28,454,564,selected?'Primary':'Glass')).id;
 overlays[selected?'versionSelected':'versions']={id:root.id,name:root.name,refs,panel:panel.id};
}
return {createdNodeIds:made,mutatedNodeIds:changed,overlays};
