import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
export function isProductInput(path){
 return path==='docs/operations/first-use-and-sharing.md' || path==='THIRD_PARTY_NOTICES.md' || path==='firmware/editions.json' || path.startsWith('firmware/compatible_7ch_package/')
 || /^apps\/device-manager\/licenses\/[^/]+\.(txt|md)$/.test(path)
 || /^(crates\/.*\.(rs|toml)|Cargo\.(toml|lock)|rust-toolchain.*|\.cargo\/.*)$/.test(path)
 || /^firmware\/(loadcell_7ch\/.*\.(cpp|h|ini|py)|shared\/.*\.(h|cpp))$/.test(path)
 || /^firmware\/(legacy\/.*|legacy_7ch_package\/.*)$/.test(path)
 || path==='apps/device-manager/tools/firmware-packages.mjs'
 || /^docs\/contracts\/(firmware-protocol-v3|firmware-update|firmware-memory-budget)\.md$/.test(path)
 || /^tools\/check_firmware_size\.py$/.test(path)
 || /^(apps\/device-manager\/src-tauri\/.*|apps\/device-manager\/(package(-lock)?\.json|frontend\/(?!.*\.test\.mjs$).*|tools\/(package-windows|distribution-lib|artifact-inputs)\.mjs|tools\/platformio-requirements\.txt)|docs\/operations\/device-manager-distribution\.md)$/.test(path);
}
export function firmwareNoticePayload(repo){
 return readFileSync(join(repo,'THIRD_PARTY_NOTICES.md'));
}
export function appendFirmwareNotices(generatedNotices,firmwareNoticeBytes){
 return generatedNotices+'\n\nFirmware package third-party notices:\n'+Buffer.from(firmwareNoticeBytes).toString('utf8');
}
export function productInputs(repo){
 const paths=execFileSync('git',['-C',repo,'ls-files','--cached','--others','--exclude-standard','-z'],{encoding:'utf8'}).split('\0').filter(isProductInput).sort();
 return Object.fromEntries(paths.map(path=>[path,createHash('sha256').update(readFileSync(join(repo,path))).digest('hex')]));
}
export function verifyProductInputs(record,current){
 if(!record||JSON.stringify(record)!==JSON.stringify(current))throw Error('Product inputs differ from the built artifact; rebuild instead of relabelling an old binary.');
}
