const screens={};
async function searchBox(root,refs,value){
 const field=box(root,'Search field',786,56,264,208,'surface/tint',16,'HORIZONTAL');padding(field,12,12);field.counterAxisAlignItems='CENTER';icon(field,'search',0,0,20,'text/primary');
 const label=await text(field,value,'Body',value==='Трек, исполнитель или альбом'?'text/secondary':'text/primary');label.resize(630,22);label.textAutoResize='HEIGHT';
 refs.mic=iconButton(field,'mic',0,0,'text/primary',32).id;refs.searchField=field.id;
}
{
 const {root,refs}=await scaffold('04 · Поиск — Обзор',0,1,'Поиск',true,0);
 await text(root,'Найдите свой звук','Heading','text/primary',264,112);
 await text(root,'По названию. По памяти. По звуку вокруг.','Body','text/secondary',264,164);
 await searchBox(root,refs,'Трек, исполнитель или альбом');
 await text(root,'С чего начнём?','Section','text/primary',264,304);
 const cards=box(root,'Album suggestions',786,246,264,348,null,0,'HORIZONTAL');cards.itemSpacing=24;
 for(const [i,track]of [TRACKS[1],TRACKS[0],TRACKS[3],TRACKS[5]].entries()){
  const card=await playlistCard(cards,track.artist,track.title,track.key,0,0);refs.cards.push({id:card.id,track:[1,0,3,5][i]});
 }
 await text(root,'Недавний поиск','Section','text/primary',264,632);
 for(const [i,track]of [TRACKS[1],TRACKS[4]].entries()){const n=await trackRow(root,track,264,670+i*68);refs.rows.push({id:n.id,track:i===0?1:4});}
 await nowPlaying(root,refs,0);screens.search={id:root.id,name:root.name,refs};
}
{
 const {root,refs}=await scaffold('05 · Поиск — Результаты',1,1,'Поиск',true,0);
 await text(root,'Нашлось то самое','Heading','text/primary',264,112);
 await text(root,'Один трек — несколько источников. Выбирайте свою версию.','Body','text/secondary',264,164);
 await searchBox(root,refs,'Blinding Lights');
 const filters=box(root,'Search filters',786,38,264,284,null,0,'HORIZONTAL');filters.itemSpacing=8;
 for(const [i,label]of ['Все','Треки','Артисты','Альбомы'].entries()){const c=place(await node(K.segment[i===0?1:0]),filters,'filter/'+label,0,0,96,38);override(c,'Label',label);refs.pills.push(c.id);}
 const match=box(root,'Best match',786,164,264,346,'surface/tint',24,'HORIZONTAL');padding(match,20,24);match.counterAxisAlignItems='CENTER';image(match,'Best match artwork','weeknd',124,0,0,16);
 const description=box(match,'Best match title',380,116,0,0,null,0,'VERTICAL');description.itemSpacing=6;await text(description,'ЛУЧШЕЕ СОВПАДЕНИЕ','Micro','text/secondary');await text(description,'Blinding Lights','Title');await text(description,'The Weeknd · After Hours','Small','text/secondary');await text(description,'Яндекс Музыка · 3:20','Small','text/secondary');
 const play=place(await node(K.play[1]),match,'Play best match',0,0);refs.bestMatch=play.id;refs.cards.push({id:match.id,track:1});
 await text(root,'Версии в каталогах','Section','text/primary',264,544);
 for(const [i,source]of ['Яндекс','SoundCloud','YouTube'].entries()){const tr={...TRACKS[1],source};const row=await trackRow(root,tr,264,580+i*68);refs.rows.push({id:row.id,track:1});}
 await nowPlaying(root,refs,0);screens.results={id:root.id,name:root.name,refs};
}
{
 const {root,refs}=await scaffold('06 · Медиатека — Главная',2,1,'Медиатека',true,0);
 await text(root,'Ваша медиатека','Heading','text/primary',264,112);
 await text(root,'Всё, к чему хочется вернуться.','Body','text/secondary',264,164);
 const tabs=box(root,'Library filters',786,38,264,208,null,0,'HORIZONTAL');tabs.itemSpacing=8;
 for(const [i,label]of ['Обзор','Понравившиеся','Плейлисты'].entries()){const n=place(await node(K.segment[i===0?1:0]),tabs,'tab/'+label,0,0,i===1?156:120,38);override(n,'Label',label);refs.pills.push(n.id);}
 const hero=box(root,'Favorite collection',786,164,264,272,'surface/tint',24);hero.clipsContent=true;
 await text(hero,'Любимое — всегда рядом.','Title','text/primary',24,24);await text(hero,'6 треков из разных музыкальных миров','Small','text/secondary',24,66);
 refs.listen=(await button(hero,'Слушать',24,100,128,'Primary')).id;
 image(hero,'Collection cover','weeknd',184,590,-12,24).rotation=-8;
 await text(root,'Ваши плейлисты','Section','text/primary',264,468);
 const cards=box(root,'Playlist grid',786,242,264,510,null,0,'HORIZONTAL');cards.itemSpacing=24;
 for(const [title,subtitle,key]of [['Ночной маршрут','6 треков · Яндекс','oneheart'],['На повторе','6 треков · все источники','charlie'],['Громче мыслей','6 треков · Яндекс','arctic'],['Медленный вечер','6 треков · все источники','massive']]){const c=await playlistCard(cards,title,subtitle,key,0,0);refs.cards.push({id:c.id,playlist:title});}
 await nowPlaying(root,refs,0);screens.library={id:root.id,name:root.name,refs};
}
{
 const {root,refs}=await scaffold('07 · Медиатека — Понравившиеся',0,2,'Медиатека',true,0);
 await text(root,'Понравившиеся','Heading','text/primary',264,112);await text(root,'6 треков · ваша общая коллекция','Body','text/secondary',264,164);
 const tabs=box(root,'Library filters',786,38,264,208,null,0,'HORIZONTAL');tabs.itemSpacing=8;
 for(const [i,label]of ['Обзор','Понравившиеся','Плейлисты'].entries()){const n=place(await node(K.segment[i===1?1:0]),tabs,'tab/'+label,0,0,i===1?156:120,38);override(n,'Label',label);refs.pills.push(n.id);}
 refs.listen=(await button(root,'Слушать всё',264,266,156,'Primary')).id;
 refs.shuffle=(await button(root,'Перемешать',432,266,156,'Paper')).id;
 refs.import=iconButton(root,'refresh-cw',1006,266,'text/primary').id;
 for(let i=0;i<TRACKS.length;i++){const row=await trackRow(root,TRACKS[i],264,334+i*68);refs.rows.push({id:row.id,track:i});}
 await text(root,'Яндекс Музыка · SoundCloud · Hitmo · YouTube Music','Small','text/secondary',276,768);
 await nowPlaying(root,refs,0);screens.favorites={id:root.id,name:root.name,refs};
}
{
 const {root,refs}=await scaffold('08 · Медиатека — Ночной маршрут',1,2,'Медиатека',true,5);
 refs.back=iconButton(root,'chevron-left',256,108,'text/primary',32).id;await text(root,'Плейлисты','Small','text/secondary',300,114);
 image(root,'Playlist artwork','oneheart',176,264,176,24);
 await text(root,'ВАШ ПЛЕЙЛИСТ','Micro','text/secondary',472,184);await text(root,'Ночной маршрут','Heading','text/primary',472,216);
 await text(root,'6 треков · из Яндекс Музыки','Small','text/secondary',475,270);
 refs.listen=(await button(root,'Слушать',472,310,148,'Primary')).id;
 refs.shuffle=(await button(root,'Перемешать',632,310,156,'Paper')).id;
 refs.more=iconButton(root,'ellipsis',1010,310,'text/primary').id;
 await text(root,'ТРЕК','Micro','text/secondary',276,398);await text(root,'ИСТОЧНИК','Micro','text/secondary',884,398);
 const scroll=box(root,'Scrollable playlist',786,368,264,430,null,0,'VERTICAL');scroll.clipsContent=true;scroll.overflowDirection='VERTICAL';scroll.itemSpacing=0;
 for(const i of [5,3,4,0,1,2]){const row=await trackRow(scroll,TRACKS[i],0,0);refs.rows.push({id:row.id,track:i});}
 await nowPlaying(root,refs,5);screens.playlist={id:root.id,name:root.name,refs};
}
return {createdNodeIds:made,mutatedNodeIds:changed,screens};
