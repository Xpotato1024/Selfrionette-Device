import test from 'node:test';import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,readFileSync} from 'node:fs';import {tmpdir} from 'node:os';import {join} from 'node:path';import {fileURLToPath} from 'node:url';
import {sourceTreeHash,legacySource,legacyManifest,packageCatalog,verifyFamilyCatalog,digest,compatibleSource,compatibleManifest,editionFields,firmwareEditions} from './firmware-packages.mjs';
const repo=fileURLToPath(new URL('../../..',import.meta.url));
test('historical two-family catalog preserves the original archive identity',()=>{
 assert.equal(legacySource,'firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_legacy');
 assert.match(readFileSync(join(repo,'firmware/legacy_7ch_package/platformio.ini'),'utf8'),/src_dir = \.\.\/legacy\/xpotato-sim\/arduino\/legacy_selfrionette\/loadcell_7ch_legacy\/src/);
 assert.match(readFileSync(join(repo,legacySource,'src/main.cpp'),'utf8'),/bool\s+isLeader\s*=\s*true/);
 assert.equal(packageCatalog(Buffer.from('modern'),Buffer.from('legacy')).families[1].label,'旧版（初期保管ソース）');
 assert.equal(legacyManifest({version:'0.2.0-preview.2',commit:'a'.repeat(40),repo,image:Buffer.from('hex')}).source_path,legacySource);
});
function temporary(fn){const root=mkdtempSync(join(tmpdir(),'firmware-families-'));try{return fn(root);}finally{rmSync(root,{recursive:true,force:true});}}
test('source fingerprint binds names and exact frozen bytes independent of creation order',()=>temporary(root=>{
 for(const dir of ['a','b'])mkdirSync(join(root,dir));
 for(const [dir,names]of [['a',['x.cpp','y.h']],['b',['y.h','x.cpp']]])for(const name of names)writeFileSync(join(root,dir,name),name+'\r\n');
 assert.equal(sourceTreeHash(join(root,'a')),sourceTreeHash(join(root,'b')));writeFileSync(join(root,'b/x.cpp'),'x.cpp\n');assert.notEqual(sourceTreeHash(join(root,'a')),sourceTreeHash(join(root,'b')));
}));
test('legacy package declares missing identity/version/settings without relabelling a modern stream',()=>{
 const m=legacyManifest({version:'0.2.0-preview.2',commit:'a'.repeat(40),repo,image:Buffer.from('hex')});assert.equal(m.firmware_version,'');assert.equal(m.protocol_schema,0);assert.equal(m.settings_schema,0);assert.deepEqual(m.capabilities,['legacy_vector7']);assert.equal(m.family,'legacy_7ch');assert.match(m.source_tree_sha256,/^[a-f0-9]{64}$/);
});
test('family catalog binds both images and rejects changed data, family, count and paths',()=>temporary(root=>{
 const image=Buffer.from('hex'),legacy=legacyManifest({version:'0.2.0-preview.2',commit:'a'.repeat(40),repo,image});
 const modern={...legacy,family:'modern',source_path:'firmware/loadcell_7ch',protocol_schema:3,settings_schema:1,firmware_version:'0.2.0-preview.2',capabilities:['info3','device_settings','personal_calibration','safe_metadata_probe']};
 mkdirSync(join(root,'legacy-7ch'));const m=Buffer.from(JSON.stringify(modern)),l=Buffer.from(JSON.stringify(legacy));
 writeFileSync(join(root,'manifest.json'),m);writeFileSync(join(root,'firmware.hex'),image);writeFileSync(join(root,'legacy-7ch/manifest.json'),l);writeFileSync(join(root,'legacy-7ch/firmware.hex'),image);
 const catalog=packageCatalog(m,l),put=c=>writeFileSync(join(root,'catalog.json'),JSON.stringify(c));put(catalog);assert.equal(verifyFamilyCatalog(root).families.length,2);
 writeFileSync(join(root,'legacy-7ch/firmware.hex'),'changed');assert.throws(()=>verifyFamilyCatalog(root),/payload/);writeFileSync(join(root,'legacy-7ch/firmware.hex'),image);
 for(const changed of [{...catalog,families:[]},{...catalog,families:[catalog.families[0],{...catalog.families[1],manifest:'../manifest.json'}]},{...catalog,families:[catalog.families[0],{...catalog.families[1],family:'modern'}]}]){put(changed);assert.throws(()=>verifyFamilyCatalog(root));}
 put(catalog);const bad=Buffer.from(JSON.stringify({...legacy,capabilities:['info3']}));writeFileSync(join(root,'legacy-7ch/manifest.json'),bad);catalog.families[1].manifest_sha256=digest(bad);put(catalog);assert.throws(()=>verifyFamilyCatalog(root),/metadata/);
}));

test('release catalog contains three named independently versioned firmware editions',()=>temporary(root=>{
 const image=Buffer.from('firmware'),args={version:'2.4.0',commit:'a'.repeat(40),repo,image};
 const legacy={...legacyManifest(args),...editionFields(repo,'legacy_7ch')};
 const compatible=compatibleManifest(args);
 const modern={...legacy,...editionFields(repo,'modern'),family:'modern',source_path:'firmware/loadcell_7ch',protocol_schema:3,settings_schema:1,capabilities:['info3','device_settings','personal_calibration','safe_metadata_probe']};
 const entries=[['',modern],['legacy-7ch',legacy],['compatible-7ch',compatible]];
 const bytes=entries.map(([folder,m])=>{mkdirSync(join(root,folder),{recursive:true});const b=Buffer.from(JSON.stringify(m));writeFileSync(join(root,folder,'manifest.json'),b);writeFileSync(join(root,folder,'firmware.hex'),image);return b;});
 const catalog=packageCatalog(...bytes);writeFileSync(join(root,'catalog.json'),JSON.stringify(catalog));
 assert.equal(verifyFamilyCatalog(root).families.length,3);
 assert.equal(modern.release_version,'2.4.0'); assert.equal(modern.firmware_version,'1.0.0');
 assert.equal(legacy.firmware_version,''); assert.equal(compatible.firmware_version,'');
 assert.equal(compatible.source_path,compatibleSource);assert.equal(firmwareEditions(repo).length,3);
 const bad=Buffer.from(JSON.stringify({...compatible,firmware_version:'invented-original-version'}));
 writeFileSync(join(root,'compatible-7ch/manifest.json'),bad);catalog.families[2].manifest_sha256=digest(bad);writeFileSync(join(root,'catalog.json'),JSON.stringify(catalog));
 assert.throws(()=>verifyFamilyCatalog(root),/metadata/);
}));
test('Managed firmware version is injected from the registry, not the GUI version',()=>{
 const source=readFileSync(join(repo,'firmware/loadcell_7ch/src/main.cpp'),'utf8');
 const injection=readFileSync(join(repo,'firmware/loadcell_7ch/inject_source_sha.py'),'utf8');
 assert(!source.includes('#define SELFRIONETTE_FIRMWARE_VERSION'));
 assert(injection.includes('editions.json'));assert(injection.includes('SELFRIONETTE_FIRMWARE_VERSION'));
 assert.match(readFileSync(join(repo,'firmware/compatible_7ch_package/platformio.ini'),'utf8'),/loadcell_7ch_pro_micro\/src/);
});
