import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
const lock=await fs.readFile('src-tauri/Cargo.lock','utf8');
const packages=[...lock.matchAll(/\[\[package\]\]\r?\nname = "([^"]+)"\r?\nversion = "([^"]+)"/g)].map(match=>({name:match[1],version:match[2]}));
const registry=path.join(process.env.CARGO_HOME||path.join(os.homedir(),'.cargo'),'registry','src');
const roots=await fs.readdir(registry);
const sections=['Third-party notices for the locked Windows build of Darkroom.','Microsoft WebView2 Runtime is a separate Microsoft product and is not included in the portable archive.','This file includes available crate copyright and license notices from the build dependencies.',''];
let count=0;const groups=new Map();
for(const pkg of packages){
  for(const root of roots){
    const folder=path.join(registry,root,`${pkg.name}-${pkg.version}`);
    let names;try{names=await fs.readdir(folder);}catch{continue;}
    const licenses=names.filter(name=>/^(license|licence|copying|notice|copyright)([-_.]|$)/i.test(name));
    for(const name of licenses){try{const file=path.join(folder,name);if(!(await fs.stat(file)).isFile())continue;const text=(await fs.readFile(file,'utf8')).replace(/\r\n/g,'\n').trim();if(!groups.has(text))groups.set(text,[]);groups.get(text).push(`${pkg.name} ${pkg.version} / ${name}`);count++;}catch{}}
    break;
  }
}
if(count<20)throw Error('Dependency sources not available. Run cargo test first.');
for(const [text,names]of groups)sections.push(`\n${'='.repeat(72)}\n${names.join('\n')}\n${'='.repeat(72)}\n`,text);
await fs.writeFile('THIRD-PARTY-LICENSES.txt',sections.join('\n'));
console.log(`Collected ${count} dependency license files.`);
