import fs from 'node:fs';import path from 'node:path';import {Resvg} from '@resvg/resvg-js';
const folder=path.resolve(import.meta.dirname,'../../assets/fontawesome');
for(const name of fs.readdirSync(folder).filter(n=>n.endsWith('.svg'))){let svg=fs.readFileSync(path.join(folder,name),'utf8').replaceAll('currentColor','#FFFFFF').replace('<svg ','<svg fill="#FFFFFF" ');fs.writeFileSync(path.join(folder,name.replace('.svg','-raw.png')),new Resvg(svg,{fitTo:{mode:'width',value:256}}).render().asPng());}
