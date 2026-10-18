// 固定3系統の配布識別。旧2系統catalogも読み取り互換を維持する。
import {readFileSync,readdirSync} from 'node:fs';
import {join,relative} from 'node:path';
import {createHash} from 'node:crypto';
export const compatibleSource='firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_pro_micro';
export function firmwareEditions(repo){const c=JSON.parse(readFileSync(join(repo,'firmware/editions.json'),'utf8'));if(c.schema!==1||c.editions?.length!==3)throw Error('Invalid edition registry');return c.editions;}
export function editionFields(repo,family){const e=firmwareEditions(repo).find(e=>e.family===family);if(!e)throw Error('Unknown firmware edition');return {schema:3,firmware_name:e.name,distribution_version:e.distribution_version,firmware_version:e.firmware_version??'',supported_protocols:e.protocols};}
export const legacySource='firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_legacy';
export const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
export function sourceTreeHash(root){
 const files=[];function walk(at){for(const e of readdirSync(at,{withFileTypes:true})){const p=join(at,e.name);if(e.isDirectory())walk(p);else if(e.isFile())files.push(p);else throw Error('Source tree contains a special entry');}}walk(root);
 const h=createHash('sha256');for(const p of files.sort((a,b)=>relative(root,a).replaceAll('\\','/').localeCompare(relative(root,b).replaceAll('\\','/'),'en'))){h.update(relative(root,p).replaceAll('\\','/'));h.update('\0');h.update(readFileSync(p));h.update('\0');}return h.digest('hex');
}
export function legacyManifest({version,commit,repo,image}){
 return {schema:2,family:'legacy_7ch',source_path:legacySource,capabilities:['legacy_vector7'],source_tree_sha256:sourceTreeHash(join(repo,legacySource)),release_version:version,application:'selfrionette-loadcell-7ch',board:'sparkfun_promicro16',mcu:'atmega32u4',clock_hz:16000000,protocol_schema:0,settings_schema:0,firmware_version:'',source_sha:commit,image_file:'firmware.hex',image_sha256:digest(image),image_size:image.length,flash_start:0,flash_end_exclusive:0x7000};
}
export function compatibleManifest(args){return {...legacyManifest(args),...editionFields(args.repo,'compatible_7ch'),family:'compatible_7ch',source_path:compatibleSource,source_tree_sha256:sourceTreeHash(join(args.repo,compatibleSource))};}
export function packageCatalog(modern,legacy,compatible){if(compatible){const records=[['modern','manifest.json',modern],['legacy_7ch','legacy-7ch/manifest.json',legacy],['compatible_7ch','compatible-7ch/manifest.json',compatible]];return {schema:2,families:records.map(([family,manifest,bytes])=>{const m=JSON.parse(bytes);return {family,label:m.firmware_name+' '+m.distribution_version,manifest,manifest_sha256:digest(bytes)};})};}return {schema:1,families:[{family:'modern',label:'新版（個体設定・個人校正対応）',manifest:'manifest.json',manifest_sha256:digest(modern)},{family:'legacy_7ch',label:'旧版（初期保管ソース）',manifest:'legacy-7ch/manifest.json',manifest_sha256:digest(legacy)}]};}
export function verifyFamilyCatalog(root){
 const bytes=readFileSync(join(root,'catalog.json'));if(bytes.length>8192)throw Error('Firmware catalog too large');
 const catalog=JSON.parse(bytes),families=catalog.schema===1?['modern','legacy_7ch']:catalog.schema===2?['modern','legacy_7ch','compatible_7ch']:[];
 if(!families.length||catalog.families?.length!==families.length)throw Error('Unsupported firmware catalog');
 for(const [i,family]of families.entries()){
  const folder=['','legacy-7ch/','compatible-7ch/'][i],record=catalog.families[i],path=folder+'manifest.json';
  if(record.family!==family||record.manifest!==path)throw Error('Unexpected firmware family/path');
  const manifestBytes=readFileSync(join(root,path));if(manifestBytes.length>8192||digest(manifestBytes)!==record.manifest_sha256)throw Error('Firmware catalog hash mismatch');
  const m=JSON.parse(manifestBytes),image=readFileSync(join(root,folder+'firmware.hex'));
  if((m.family??'modern')!==family||digest(image)!==m.image_sha256||!image.length||image.length>512*1024||image.length!==m.image_size||m.board!=='sparkfun_promicro16'||m.clock_hz!==16000000||m.settings_schema!==(i?0:1)||m.protocol_schema!==(i?0:3))throw Error('Firmware family payload mismatch');
  const source=['firmware/loadcell_7ch',legacySource,compatibleSource][i];
  if([2,3].includes(m.schema)&&m.source_path!==source)throw Error('Firmware source path mismatch');
  if(i&&(![2,3].includes(m.schema)||m.firmware_version!==''||JSON.stringify(m.capabilities)!==JSON.stringify(['legacy_vector7'])||!/^[a-f0-9]{64}$/.test(m.source_tree_sha256??'')))throw Error('Legacy family metadata is invalid');
  if(catalog.schema===2){
   const names=['Selfrionette Managed','Selfrionette Original','Selfrionette Compatible'];
   if(m.schema!==3||m.firmware_name!==names[i]||!/^[0-9]+\.[0-9]+\.[0-9]+(?:-[a-z0-9.]+)?$/.test(m.distribution_version??'')||JSON.stringify(m.supported_protocols)!==JSON.stringify(i?[]:[2,3])||(!i&&m.firmware_version!==m.distribution_version))throw Error('Firmware edition metadata mismatch');
  }
 }
 return catalog;
}
