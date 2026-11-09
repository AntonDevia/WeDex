import fs from 'node:fs';
import path from 'node:path';
import {Resvg} from '@resvg/resvg-js';
const root=path.resolve(import.meta.dirname,'../..');
const folder=path.join(root,'output/wedex');
for(const file of fs.readdirSync(folder).filter(n=>n.endsWith('.svg'))){
 const source=fs.readFileSync(path.join(folder,file),'utf8');
 const width=file.startsWith('symbol')?256:1600;
 const png=new Resvg(source,{fitTo:{mode:'width',value:width}}).render().asPng();
 fs.writeFileSync(path.join(folder,file.replace(/\.svg$/,'.png')),png);
}
console.log('Rendered vector wordmarks and symbol previews.');
