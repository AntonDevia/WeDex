import fs from 'node:fs';
import path from 'node:path';
import {Resvg} from '@resvg/resvg-js';
const root=path.resolve(import.meta.dirname,'../..');
const out=path.join(root,'assets/ui');fs.mkdirSync(out,{recursive:true});
for(const name of fs.readdirSync(path.join(root,'design/assets')).filter(n=>n.endsWith('.svg'))){
 const svg=fs.readFileSync(path.join(root,'design/assets',name),'utf8');
 // Lucide currentColor assets are tinted with each Figma instance's paint at runtime.
 const png=new Resvg(svg.replaceAll('currentColor','#FFFFFF'),{fitTo:{mode:'width',value:96}}).render().asPng();
 fs.writeFileSync(path.join(out,name.replace('.svg','.png')),png);
}
console.log('Rendered the 24 original Figma icon assets at 4x resolution.');
