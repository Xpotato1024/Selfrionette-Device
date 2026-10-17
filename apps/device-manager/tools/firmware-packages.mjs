// Exactly two supported frozen source families; no network or serial operations.
import {readFileSync,readdirSync} from 'node:fs';
import {join,relative} from 'node:path';
import {createHash} from 'node:crypto';
export const legacySource='firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_legacy';
export const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
export function sourceTreeHash(root){
 const files=[];function walk(at){for(const e of readdirSync(at,{withFileTypes:true})){const p=join(at,e.name);if(e.isDirectory())walk(p);else if(e.isFile())files.push(p);else throw Error('Source tree contains a special entry');}}walk(root);
 const h=createHash('sha256');for(const p of files.sort((a,b)=>relative(root,a).replaceAll('\\','/').localeCompare(relative(root,b).replaceAll('\\','/'),'en'))){h.update(relative(root,p).replaceAll('\\','/'));h.update('\0');h.update(readFileSync(p));h.update('\0');}return h.digest('hex');
}
export function legacyManifest({version,commit,repo,image}){
 return {schema:2,family:'legacy_7ch',source_path:legacySource,capabilities:['legacy_vector7'],source_tree_sha256:sourceTreeHash(join(repo,legacySource)),release_version:version,application:'selfrionette-loadcell-7ch',board:'sparkfun_promicro16',mcu:'atmega32u4',clock_hz:16000000,protocol_schema:0,settings_schema:0,firmware_version:'',source_sha:commit,image_file:'firmware.hex',image_sha256:digest(image),image_size:image.length,flash_start:0,flash_end_exclusive:0x7000};
}
export function packageCatalog(modern,legacy){return {schema:1,families:[{family:'modern',label:'新版（個体設定・個人校正対応）',manifest:'manifest.json',manifest_sha256:digest(modern)},{family:'legacy_7ch',label:'旧版（初期保管ソース）',manifest:'legacy-7ch/manifest.json',manifest_sha256:digest(legacy)}]};}
export function verifyFamilyCatalog(root){
 const bytes=readFileSync(join(root,'catalog.json'));if(bytes.length>8192)throw Error('Firmware catalog too large');
 const catalog=JSON.parse(bytes);if(catalog.schema!==1||catalog.families?.length!==2)throw Error('Unsupported firmware catalog');
 for(const [i,family]of ['modern','legacy_7ch'].entries()){
  const record=catalog.families[i],path=i?'legacy-7ch/manifest.json':'manifest.json';if(record.family!==family||record.manifest!==path)throw Error('Unexpected firmware family/path');
  const manifestBytes=readFileSync(join(root,path));if(manifestBytes.length>8192||digest(manifestBytes)!==record.manifest_sha256)throw Error('Firmware catalog hash mismatch');
  const m=JSON.parse(manifestBytes),image=readFileSync(join(root,i?'legacy-7ch/firmware.hex':'firmware.hex'));
  if((m.family??'modern')!==family||digest(image)!==m.image_sha256||!image.length||image.length>512*1024||image.length!==m.image_size||m.board!=='sparkfun_promicro16'||m.clock_hz!==16000000||m.settings_schema!==(i?0:1)||m.protocol_schema!==(i?0:3))throw Error('Firmware family payload mismatch');
  if(m.schema===2&&m.source_path!==(i?legacySource:'firmware/loadcell_7ch'))throw Error('Firmware source path mismatch');
  if(i&&(m.schema!==2||m.firmware_version!==''||JSON.stringify(m.capabilities)!==JSON.stringify(['legacy_vector7'])||!/^[a-f0-9]{64}$/.test(m.source_tree_sha256??'')))throw Error('Legacy family metadata is invalid');
 }
 return catalog;
}
