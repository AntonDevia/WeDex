import fs from 'node:fs';
import path from 'node:path';
import {Resvg} from '@resvg/resvg-js';
const folder=path.resolve(import.meta.dirname,'../../assets/providers');
for(const [name,color] of [['yandex',null],['lastdotfm','#D51007'],['soundcloud','#FF5500']]){
 let svg=fs.readFileSync(path.join(folder,name+'.svg'),'utf8');
 if(color)svg=svg.replace('<svg ',`<svg fill="${color}" `);
 fs.writeFileSync(path.join(folder,name+'-raw.png'),new Resvg(svg,{fitTo:{mode:'width',value:256}}).render().asPng());
}
