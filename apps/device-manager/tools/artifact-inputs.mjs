import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
export function isProductInput(path){
 return /^(crates\/.*\.(rs|toml)|Cargo\.(toml|lock)|rust-toolchain.*|\.cargo\/.*)$/.test(path)
 || /^(apps\/device-manager\/src-tauri\/.*|apps\/device-manager\/(package(-lock)?\.json|frontend\/(?!.*\.test\.mjs$).*|tools\/(package-windows|distribution-lib|artifact-inputs)\.mjs)|docs\/operations\/device-manager-distribution\.md)$/.test(path);
}
export function productInputs(repo){
 const paths=execFileSync('git',['-C',repo,'ls-files','--cached','--others','--exclude-standard','-z'],{encoding:'utf8'}).split('\0').filter(isProductInput).sort();
 return Object.fromEntries(paths.map(path=>[path,createHash('sha256').update(readFileSync(join(repo,path))).digest('hex')]));
}
export function verifyProductInputs(record,current){
 if(!record||JSON.stringify(record)!==JSON.stringify(current))throw Error('Product inputs differ from the built artifact; rebuild instead of relabelling an old binary.');
}
