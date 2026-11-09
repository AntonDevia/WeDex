// Command-line MCP client for diagnostics before Codex reloads its tool list.
import fs from 'node:fs';
import path from 'node:path';
import {Client} from '@modelcontextprotocol/sdk/client/index.js';
import {StdioClientTransport} from '@modelcontextprotocol/sdk/client/stdio.js';
const [name='figma_status',argFile]=process.argv.slice(2);
const args=argFile?JSON.parse(fs.readFileSync(argFile,'utf8')):{};
const client=new Client({name:'wedex-figma-diagnostic',version:'0.1.0'});
try{
  await client.connect(new StdioClientTransport({command:process.execPath,args:[path.join(import.meta.dirname,'server.mjs')],stderr:'inherit'}));
  const result=await client.callTool({name,arguments:args},undefined,{timeout:100000});
  for(const content of result.content||[])if(content.type==='text')process.stdout.write(content.text+'\n');
  if(result.isError)process.exitCode=1;
}finally{await client.close();}
