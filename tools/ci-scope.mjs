import {execFileSync} from 'node:child_process';
import {appendFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
export function classifyChanges(paths,full=false){
 const result={frontend:full,core:full,native:full};
 for(const path of paths){
  if(/^(docs\/|README\.md$)/.test(path)){if(path.startsWith('docs/contracts/'))result.core=result.native=true;continue;}
  if(path.startsWith('apps/device-manager/frontend/')||path.startsWith('apps/device-manager/tools/')||path==='apps/device-manager/preview-server.mjs'){result.frontend=true;continue;}
  if(path.startsWith('apps/device-manager/src-tauri/')){result.native=true;continue;}
  if(path.startsWith('apps/device-manager/')){result.frontend=result.native=true;continue;}
  if(path.startsWith('crates/')||/^Cargo\.|^rust-toolchain|^\.cargo\//.test(path)){result.core=result.native=true;continue;}
  if(path.startsWith('firmware/')||path.startsWith('hardware/'))continue;
  result.frontend=result.core=result.native=true; // Unknown impact never silently skips checks.
 }
 return result;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const base=process.argv[2],full=!base||base==='all';
 const paths=full?[]:execFileSync('git',['diff','--name-only','--no-renames',base+'...HEAD'],{encoding:'utf8'}).trim().split('\n').filter(Boolean);
 const result=classifyChanges(paths,full);console.log(JSON.stringify({paths,...result}));
 if(process.env.GITHUB_OUTPUT)for(const [key,value]of Object.entries(result))appendFileSync(process.env.GITHUB_OUTPUT,key+'='+value+'\n');
}
