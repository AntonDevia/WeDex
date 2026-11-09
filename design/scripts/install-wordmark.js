const page=await figma.getNodeByIdAsync('0:1');await figma.setCurrentPageAsync(page);
const master=await figma.getNodeByIdAsync(WORDMARK_ID);
if(!master||master.type!=='COMPONENT')throw new Error('Wordmark master missing');
const slots=[];
for(const root of page.children.filter(n=>n.type==='FRAME')){
 const sidebar=root.findOne(n=>n.type==='FRAME'&&n.name==='Sidebar');if(!sidebar)continue;
 const brand=sidebar.findOne(n=>n.type==='FRAME'&&n.name==='Brand');if(!brand)throw new Error('Brand slot missing: '+root.id);
 if(brand.children.some(n=>n.type==='INSTANCE'&&n.name==='wedex / Wordmark'))throw new Error('Wordmark already installed: '+root.id);
 const old=brand.children.find(n=>n.type==='TEXT'&&n.characters==='wedex');if(!old)throw new Error('Old text wordmark missing: '+root.id);
 slots.push({root,brand,old,index:brand.children.indexOf(old)});
}
if(slots.length!==15)throw new Error('Expected fifteen application screens, found '+slots.length);
const createdNodeIds=[],mutatedNodeIds=[],removedNodeIds=[];
function collect(n){createdNodeIds.push(n.id);if('children'in n)for(const c of n.children)collect(c);}
for(const {root,brand,old,index}of slots){
 const instance=master.createInstance();brand.insertChild(index,instance);instance.name='wedex / Wordmark';collect(instance);
 removedNodeIds.push(old.id);old.remove();mutatedNodeIds.push(brand.id);
}
return {createdNodeIds,mutatedNodeIds,removedNodeIds,screenIds:slots.map(s=>s.root.id),installed:slots.length,componentId:master.id};
