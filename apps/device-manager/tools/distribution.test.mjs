import {appendFirmwareNotices,firmwareNoticePayload,isProductInput,verifyProductInputs} from './artifact-inputs.mjs';
import {waitForAppPage} from './native-session.mjs';
import {ciPolicyEnabled} from './ci-webview-policy.mjs';
import {tmpdir} from 'node:os';import {join,dirname} from 'node:path';
import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync,mkdirSync,mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {embeddedFrontendDist,validateGpl2Notice,prependWindowsSearchPath,inspectPE,inspectConsolePE,mergePackageMetadata,runtimeEnvironment,sha256,verifyBundledUpdaterResources,zipDirectory,extractZip,executableMetadata,supplementalCrateLicenses} from './distribution-lib.mjs';
function image(dll='KERNEL32.dll') {
 const b=Buffer.alloc(1024);b.writeUInt16LE(0x5a4d,0);b.writeUInt32LE(0x80,0x3c);b.writeUInt32LE(0x4550,0x80);
 b.writeUInt16LE(0x8664,0x84);b.writeUInt16LE(1,0x86);b.writeUInt16LE(240,0x94);b.writeUInt16LE(0x20b,0x98);
 b.writeUInt16LE(2,0x98+68);b.writeUInt32LE(512,0x98+60);b.writeUInt32LE(0x1000,0x98+120);
 const s=0x98+240;b.writeUInt32LE(0x1000,s+12);b.writeUInt32LE(512,s+16);b.writeUInt32LE(512,s+20);
 b.writeUInt32LE(1,512);b.writeUInt32LE(0x1080,524);b.write(dll,640,'ascii');return b;
}
test('release PE accepts x64 GUI image with OS imports',()=>assert.deepEqual(inspectPE(image()).imports,['KERNEL32.dll']));
test('release PE rejects debug CRT and external loader dependencies',()=>{for(const name of ['VCRUNTIME140.dll','MSVCP140.dll','ucrtbased.dll','WebView2Loader.dll'])assert.throws(()=>inspectPE(image(name)),/dependency/);});
test('release PE rejects console, wrong architecture and malformed headers',()=>{let b=image();b.writeUInt16LE(3,0x98+68);assert.throws(()=>inspectPE(b),/GUI/);b=image();b.writeUInt16LE(0x14c,0x84);assert.throws(()=>inspectPE(b),/x64/);for(const n of [0,1,64,300,520])assert.throws(()=>inspectPE(image().subarray(0,n)));});
test('packaged Rust CLI requires x64 console PE and no dynamic C runtime',()=>{const b=image();b.writeUInt16LE(3,0x98+68);assert.equal(inspectConsolePE(b).subsystem,'windows_console');assert.throws(()=>inspectConsolePE(image()),/console/);const crt=image('VCRUNTIME140.dll');crt.writeUInt16LE(3,0x98+68);assert.throws(()=>inspectConsolePE(crt),/dependency/);});
test('Cargo license inventories merge by exact package id and reject conflicts',()=>{const shared={id:'registry+https://github.com/rust-lang/crates.io-index#crossterm@0.29.0',name:'crossterm',version:'0.29.0',source:'registry+https://github.com/rust-lang/crates.io-index',license:'MIT OR Apache-2.0'};const cli={id:'path+file:///repo/crates/selfrionettectl#0.1.0',name:'selfrionettectl',version:'0.1.0',source:null,license:null};const merged=mergePackageMetadata({packages:[shared]},{packages:[shared,cli]});assert.deepEqual(merged.packages.map(pkg=>pkg.id),[shared.id,cli.id]);assert.throws(()=>mergePackageMetadata({packages:[shared]},{packages:[{...shared,license:'GPL-3.0-only'}]}),/Conflicting Cargo metadata/);});
test('runtime test excludes developer paths, credentials and runtime overrides',()=>{const e=runtimeEnvironment({SystemRoot:'C:\\Windows',TEMP:'C:\\Temp',PATH:'D:\\VS',GH_TOKEN:'secret',CARGO_HOME:'D:\\Rust',WEBVIEW2_BROWSER_EXECUTABLE_FOLDER:'D:\\Browser'});assert.equal(e.GH_TOKEN,undefined);assert.equal(e.CARGO_HOME,undefined);assert.equal(e.WEBVIEW2_BROWSER_EXECUTABLE_FOLDER,undefined);assert(!e.PATH.includes('VS'));assert(e.PATH.includes('System32'));});
test('distribution config keeps developer path separate and embeds offline WebView2',()=>{const c=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url)));const r=JSON.parse(readFileSync(new URL('../src-tauri/tauri.release.conf.json',import.meta.url)));assert.equal(c.bundle.active,false);assert.equal(c.build.windows.staticVCRuntime,true);assert.equal(r.bundle.active,true);assert.deepEqual(r.bundle.targets,['nsis']);assert.equal(r.bundle.windows.webviewInstallMode.type,'offlineInstaller');assert.equal(r.bundle.windows.nsis.installMode,'currentUser');const launcher=readFileSync(new URL('../run-native.cmd',import.meta.url),'utf8');assert(!launcher.includes('D:\\Xpotato-apps'));assert(launcher.includes('vswhere.exe'));assert(launcher.includes('run build -- --debug'));});
test('release config leaves resources to the isolated package staging path',()=>{const r=JSON.parse(readFileSync(new URL('../src-tauri/tauri.release.conf.json',import.meta.url)));assert.equal(r.bundle.resources,undefined);const packaging=readFileSync(new URL('./package-windows.mjs',import.meta.url),'utf8');assert(packaging.includes('SELFRIONETTE_TASK_TEMP_ROOT'));assert(packaging.includes('CARGO_TARGET_DIR'));assert(packaging.includes('PLATFORMIO_BUILD_DIR'));assert(packaging.includes("'--config',buildFlavor"));});
function matchesAppLockVersion(text,version) {
 return text.replace(/\r\n/g,'\n').includes('name = "selfrionette-device-manager"\nversion = "'+version+'"');
}
test('Cargo version comparison accepts LF and CRLF without accepting another package or version',()=>{
 const version='0.2.0-preview.1',text='[[package]]\nname = "selfrionette-device-manager"\nversion = "'+version+'"\n';
 for(const newline of ['\n','\r\n']) {
  const lock=text.replace(/\n/g,newline);
  assert(matchesAppLockVersion(lock,version));
  assert.equal(matchesAppLockVersion(lock,'0.1.0-preview.5'),false);
  assert.equal(matchesAppLockVersion(lock.replace('selfrionette-device-manager','another-package'),version),false);
 }
});
test('application, Cargo, and npm package versions remain identical',()=>{const c=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url)));const cargo=readFileSync(new URL('../src-tauri/Cargo.toml',import.meta.url),'utf8');const app=JSON.parse(readFileSync(new URL('../package.json',import.meta.url)));const npmLock=JSON.parse(readFileSync(new URL('../package-lock.json',import.meta.url)));const cargoLock=readFileSync(new URL('../src-tauri/Cargo.lock',import.meta.url),'utf8');assert.equal(c.version,'0.2.0-preview.1');assert(cargo.includes('version = "'+c.version+'"'));assert.equal(app.version,c.version);assert.equal(npmLock.version,c.version);assert.equal(npmLock.packages[''].version,c.version);assert(matchesAppLockVersion(cargoLock,c.version));});
test('release packaging locks firmware, updater runtime, CLI and merged license provenance',()=>{const packaging=readFileSync(new URL('./package-windows.mjs',import.meta.url),'utf8');const pins=readFileSync(new URL('./platformio-requirements.txt',import.meta.url),'utf8');const license=readFileSync(new URL('../licenses/avrdude-UPSTREAM.md',import.meta.url),'utf8');const ini=readFileSync(new URL('../../../firmware/loadcell_7ch/platformio.ini',import.meta.url),'utf8');assert.match(pins,/^platformio==6\.1\.19\s*$/m);assert.match(ini,/^\s*platform\s*=\s*atmelavr@\d+\.\d+\.\d+\s*$/m);assert(packaging.includes('SELFRIONETTE_SOURCE_SHA=commit'));assert(packaging.includes('AVRDUDE_ARCHIVE_SHA256'));assert(packaging.includes('AVRDUDE_COPYING_URL'));assert(packaging.includes("'target-feature=+crt-static'"));assert(packaging.includes('inspectConsolePE(cliBytes)'));assert(packaging.includes('mergePackageMetadata(guiMetadata,cliPackageMetadata)'));assert(packaging.includes("'distribution/'+CLI_EXECUTABLE"));assert(packaging.includes('firmwareNoticePayload(repo)'));assert(packaging.includes('firmwarePackageFile'));assert(packaging.includes('package_sha256'));assert(packaging.includes('appendFirmwareNotices'));assert(license.includes('8630c7d8bb9682a1685aaa9ad42550ae1131c9d13528f0d20975368ba615e9b0'));});
test('checksums are SHA-256 over actual bytes',()=>assert.equal(sha256(Buffer.from('abc')),'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'));
test('installed payload follows the NSIS destination and no-port runtime smoke requires success',()=>{
 const verifier=readFileSync(new URL('./test-distribution.mjs',import.meta.url),'utf8');
 const installer=readFileSync(new URL('../src-tauri/windows/installer.nsi',import.meta.url),'utf8');
 assert(installer.includes(String.raw`CreateDirectory "$INSTDIR\\{{this}}"`));
 assert(verifier.includes('verifyBundledUpdaterResources(dirname(installed),info)'));
 assert(verifier.includes('assert.equal(avrdude.status,0'));
});
test('firmware package contains the full root notice and the notice is a product input',()=>{
 const repo=fileURLToPath(new URL('../../../',import.meta.url));
 const payload=firmwareNoticePayload(repo);
 assert(isProductInput('THIRD_PARTY_NOTICES.md'));
 assert(payload.equals(readFileSync(new URL('../../../THIRD_PARTY_NOTICES.md',import.meta.url))));
 assert(payload.toString('utf8').includes('MIT License'));
 assert(appendFirmwareNotices('Cargo notices',payload).endsWith(payload.toString('utf8')));
});
test('bundled updater files and both notice payloads match BUILD-INFO hashes',()=>{
 const run=mkdtempSync(join(tmpdir(),'srn-updater-resources-'));
 try {
  const applicationVersion='0.2.0-preview.1',sourceCommit='1234567890abcdef1234567890abcdef12345678';
  const firmware=Buffer.from('firmware hex bytes'),firmwareNotice=Buffer.from('complete firmware notice'),combinedNotice=Buffer.from('combined notices');
  const manifest=Buffer.from(JSON.stringify({image_file:'firmware.hex',image_sha256:sha256(firmware),image_size:firmware.length,source_sha:sourceCommit,release_version:applicationVersion,firmware_version:applicationVersion}));
  const put=(relative,bytes)=>{const path=join(run,...relative.split('/'));mkdirSync(dirname(path),{recursive:true});writeFileSync(path,bytes);return {file:relative,sha256:sha256(bytes)};};
  const manifestRecord=put('distribution/firmware/manifest.json',manifest);
  const imageRecord=put('distribution/firmware/firmware.hex',firmware);
  const firmwareNoticeRecord=put('distribution/firmware/THIRD-PARTY-NOTICES.txt',firmwareNotice);
  const executableRecord=put('distribution/firmware-tools/avrdude.exe',Buffer.from('avrdude'));
  const configRecord=put('distribution/firmware-tools/avrdude.conf',Buffer.from('config'));
  const licenseRecord=put('distribution/firmware-tools/AVRDUDE-COPYING.txt',Buffer.from('GPL'));
  const sourceRecord=put('distribution/firmware-tools/AVRDUDE-SOURCE.zip',Buffer.from('pinned source archive'));
  const combinedNoticeRecord=put('distribution/THIRD-PARTY-NOTICES.txt',combinedNotice);
  const info={application_version:applicationVersion,source_commit:sourceCommit,
   firmware:{...manifestRecord,manifest_file:manifestRecord.file,manifest_sha256:manifestRecord.sha256,
    image_file:imageRecord.file,image_sha256:imageRecord.sha256,source_sha:sourceCommit,
    notice_file:firmwareNoticeRecord.file,notice_sha256:firmwareNoticeRecord.sha256},
   updater_runtime:{executable_file:executableRecord.file,executable_sha256:executableRecord.sha256,
    configuration_file:configRecord.file,configuration_sha256:configRecord.sha256,
    license_file:licenseRecord.file,license_sha256:licenseRecord.sha256,source_archive_file:sourceRecord.file,source_archive_sha256:sourceRecord.sha256},
   third_party_notices:combinedNoticeRecord};
  const verified=verifyBundledUpdaterResources(run,info);
  assert.deepEqual(verified.manifest,JSON.parse(manifest.toString('utf8')));
  assert(verified.imageBytes.equals(firmware));
  writeFileSync(join(run,sourceRecord.file),'changed source');
  assert.throws(()=>verifyBundledUpdaterResources(run,info),/AVRDUDE source archive/);
  writeFileSync(join(run,sourceRecord.file),'pinned source archive');
  writeFileSync(join(run,'distribution/firmware-tools/avrdude.conf'),'changed');
  assert.throws(()=>verifyBundledUpdaterResources(run,info),/AVRDUDE configuration/);
 } finally {rmSync(run,{recursive:true,force:true});}
});

test('exact fallback crates include pinned upstream license texts in product inputs',()=>{
 const expected={
  'alloc-stdlib-BSD-3-Clause.txt':'cd496ba72eb37e83a44958358d1f89a8a28cbc15',
  'defmt-MIT.txt':'e68a7ab37c8accca023db5f1505d48e4375d4324',
  'servo-MPL-2.0.txt':'d8b475b6e17e390d702c9be4d3a735c7622b9735',
  'webview2-rs-MIT.txt':'de9a7e660e173136b351982a34f715af424ff812'
 };
 assert.equal(Object.keys(supplementalCrateLicenses).length,6);
 for(const {file} of Object.values(supplementalCrateLicenses)){
  const bytes=readFileSync(new URL('../licenses/'+file,import.meta.url));
  const gitBlob=createHash('sha1').update('blob '+bytes.length).update(Buffer.from([0])).update(bytes).digest('hex');
  assert.equal(gitBlob,expected[file],'Upstream text was changed: '+file);
  assert(isProductInput('apps/device-manager/licenses/'+file));
 }
});

test('ZIP round trip keeps Japanese, spaces and quotes as path data',{skip:process.platform!=='win32'},()=>{
 const run=mkdtempSync(join(tmpdir(),'srn-zip-'));try{
 const source=join(run,"日本語 space's"),dest=join(run,'展開 先'),archive=join(run,'配布物.zip');
 mkdirSync(source);writeFileSync(join(source,'試験.txt'),'入力確認','utf8');zipDirectory(source,archive);extractZip(archive,dest);
 assert.equal(readFileSync(join(dest,'試験.txt'),'utf8'),'入力確認');
 }finally{rmSync(run,{recursive:true,force:true});}
});

test('ZIP and NSIS use identical unstamped application bytes',()=>{const packaging=readFileSync(new URL('./package-windows.mjs',import.meta.url),'utf8');assert(packaging.includes("'--no-binary-patching'"));});

test('CI-only debugger policy is off locally and refuses ordinary machines',()=>{assert.equal(ciPolicyEnabled({}),false);assert.throws(()=>ciPolicyEnabled({SELFRIONETTE_CI_WEBVIEW_POLICY:'1'}),/restricted/);assert.throws(()=>ciPolicyEnabled({SELFRIONETTE_CI_WEBVIEW_POLICY:'1',GITHUB_ACTIONS:'true',RUNNER_ENVIRONMENT:'self-hosted'}),/restricted/);assert.equal(ciPolicyEnabled({SELFRIONETTE_CI_WEBVIEW_POLICY:'1',GITHUB_ACTIONS:'true',RUNNER_ENVIRONMENT:'github-hosted'}),true);});

test('publisher is configured without replacing the data/installation identity or pretending to sign',()=>{
 const c=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url)));
 const r=JSON.parse(readFileSync(new URL('../src-tauri/tauri.release.conf.json',import.meta.url)));
 assert.equal(c.bundle.publisher,'Xpotato.net'); assert.equal(c.bundle.homepage,'https://xpotato.net');
 assert.equal(c.identifier,'dev.selfrionette.manager'); assert.equal(c.productName,'Selfrionette Device Manager');
 assert.equal(c.app.windows[0].decorations,true); assert.match(c.app.windows[0].title,/Selfrionette Device Manager/);
 assert.equal(r.bundle.windows.certificateThumbprint,undefined); assert.equal(r.bundle.windows.signCommand,undefined);
});


test('Windows metadata uses its own modules even when a parent process supplies an incompatible Security module',{skip:process.platform!=='win32'},()=>{
  const run=mkdtempSync(join(tmpdir(),'srn-metadata-'));
  try {
    const module=join(run,'Microsoft.PowerShell.Security');mkdirSync(module);
    writeFileSync(join(module,'Microsoft.PowerShell.Security.psm1'),"throw 'Incompatible parent PowerShell module must not load'\nfunction Get-AuthenticodeSignature {}\n");
    const environment={...process.env,PSModulePath:run};
    const result=executableMetadata(process.execPath,environment);
    assert.equal(environment.PSModulePath,run,'Parent environment must remain unchanged');
    assert.equal(typeof result.signature_status,'string');
    assert(result.signature_status.length>0);
    assert.equal(typeof result.product_name,'string');
  } finally {rmSync(run,{recursive:true,force:true});}
});


test('native harness waits for a delayed app page instead of treating a live CDP port as page readiness',async()=>{
  let polls=0;const page={url:()=> 'http://tauri.localhost/'};
  const browser={contexts:()=>polls<2?[]:[{pages:()=>polls<4?[{url:()=> 'about:blank'}]:[page]}]};
  assert.equal(await waitForAppPage(browser,{attempts:5,wait:async()=>{polls++;}}),page);assert.equal(polls,4);
});
test('native harness rejects missing or unrelated pages after a finite wait',async()=>{
  let polls=0;const browser={contexts:()=>[{pages:()=>[{url:()=> 'https://example.org/'}]}]};
  await assert.rejects(waitForAppPage(browser,{attempts:3,wait:async()=>{polls++;}}),/bounded startup/);assert.equal(polls,3);
});
test('native harness preserves early process failure during page discovery',async()=>{
  await assert.rejects(waitForAppPage({contexts:()=>[]},{check:()=>{throw Error('Process exited');}}),/Process exited/);
});


test('installer paths are not derived from the displayed publisher',()=>{
 const c=JSON.parse(readFileSync(new URL('../src-tauri/tauri.release.conf.json',import.meta.url)));
 const t=readFileSync(new URL('../src-tauri/windows/installer.nsi',import.meta.url),'utf8');
 assert.equal(c.bundle.windows.nsis.template,'windows/installer.nsi');
 assert(t.includes('!define MANUKEY "Software\\${BUNDLEID}"'));
 assert(t.includes('ReadRegStr $1 SHCTX "${UNINSTKEY}" "InstallLocation"'));
 assert(!t.includes('ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""'));
 assert(t.includes('StrCpy $R1 "$R1 _?=$4"'));
 assert(t.includes('StrCpy $4 $SRNRegisteredDirectory'));
 const body=t.match(/Function SRNReadRegisteredDirectory[\s\S]*?FunctionEnd/)[0];
 assert(!/WriteReg|DeleteReg|Exec|RMDir/.test(body),'Path resolution must have no side effects');
 assert(body.includes('GetFullPathName'));
});
test('vendored installer template follows the locked Tauri CLI',()=>{
 const lock=JSON.parse(readFileSync(new URL('../package-lock.json',import.meta.url)));
 const origin=JSON.parse(readFileSync(new URL('../src-tauri/windows/UPSTREAM.json',import.meta.url)));
 assert.equal(origin.cli_version,lock.packages['node_modules/@tauri-apps/cli'].version);
 assert.match(origin.upstream_sha256,/^[a-f0-9]{64}$/);
});

test('artifact reuse separates production inputs from test-only scripts',()=>{assert(isProductInput('apps/device-manager/frontend/style.css'));assert(isProductInput('apps/device-manager/frontend/readout.mjs'));assert(!isProductInput('apps/device-manager/tools/ui-qa.mjs'));assert(!isProductInput('apps/device-manager/frontend/readout.test.mjs'));assert(isProductInput('apps/device-manager/src-tauri/windows/installer.nsi'));assert(isProductInput('firmware/loadcell_7ch/src/main.cpp'));assert(isProductInput('firmware/loadcell_7ch/platformio.ini'));assert(isProductInput('firmware/shared/firmware_protocol_v3.h'));assert(isProductInput('tools/check_firmware_size.py'));assert(isProductInput('docs/contracts/firmware-update.md'));assert(isProductInput('apps/device-manager/licenses/avrdude-UPSTREAM.md'));assert(isProductInput('THIRD_PARTY_NOTICES.md'));assert(!isProductInput('apps/device-manager/tools/native-smoke.mjs'));});
test('missing, changed, added and removed product inputs reject reuse',()=>{verifyProductInputs({a:'same'},{a:'same'});for(const record of [null,{}, {a:'other'},{a:'same',b:'added'}])assert.throws(()=>verifyProductInputs(record,{a:'same'}),/rebuild/);});

test('release staging accepts only matching verified bytes and license-complete manifests',()=>{
 const run=mkdtempSync(join(tmpdir(),'srn-release-stage-'));
 const tag='device-manager-v0.2.0-preview.1',sha='a'.repeat(40),version='0.2.0-preview.1';
 const stem='Selfrionette-Device-Manager_'+version+'_'+sha.slice(0,8)+'_windows-x64';
 const dist=join(run,'dist','release-assets',stem),qa=join(run,'dist','release-qa','test');
 const notesDir=join(run,'docs','releases');mkdirSync(dist,{recursive:true});mkdirSync(qa,{recursive:true});mkdirSync(notesDir,{recursive:true});
 const setup=stem+'-setup.exe',portable=stem+'-portable.zip',firmware=stem+'-firmware.zip';
 const files={
  [setup]:'fixture setup',[portable]:'fixture zip',[firmware]:'fixture firmware package',
  'README.md':'fixture',
  'BUILD-INFO.json':JSON.stringify({source_commit:sha,application_version:version,target:'x86_64-pc-windows-msvc',license_review_required:[],source_dirty:false,executable_sha256:'e'.repeat(64)}),
  'VALIDATION.json':JSON.stringify({result:'PASS',source_commit:sha,packaged_payload_verified:true,updater_runtime_executed:true}),
  'UPGRADE-VALIDATION.json':JSON.stringify({result:'PASS',source_commit:sha})
 };
 for(const [name,content] of Object.entries(files))writeFileSync(join(dist,name),content);
 const sums=Object.keys(files).map(name=>sha256(readFileSync(join(dist,name)))+'  '+name).join('\n')+'\n';
 writeFileSync(join(dist,'SHA256SUMS.txt'),sums);
 const screenshots=['01-device-direction-tare.png','02-personal-calibration-verified.png','03-saved-runtime-input.png'];
 writeFileSync(join(qa,'REPORT.json'),JSON.stringify({result:'PASS',executable_sha256:'e'.repeat(64),application:{version},screenshots}));
 for(const screenshot of screenshots)writeFileSync(join(qa,screenshot),Buffer.from('synthetic image bytes'));
 writeFileSync(join(notesDir,tag+'.md'),'{{SETUP_URL}} {{PORTABLE_URL}} {{FIRMWARE_URL}} {{CHECKSUM_URL}} {{SCREENSHOT_URL}} {{SOURCE_SHA}}');
 const env={...process.env,GITHUB_REPOSITORY:'Xpotato1024/Selfrionette-Device',GITHUB_REF_NAME:tag,GITHUB_SHA:sha};
 const script=name=>spawnSync(process.execPath,[fileURLToPath(new URL('./'+name,import.meta.url))],{cwd:run,env,encoding:'utf8',timeout:10000});
 try {
  let result=script('stage-release.mjs');assert.equal(result.status,0,result.stderr);
  const manifest=JSON.parse(readFileSync(join(run,'dist','release-manifest.json'),'utf8'));
  assert.equal(manifest.assets.length,9);
  const body=readFileSync(join(run,'dist','release-notes.md'),'utf8');
  assert(body.includes(setup)&&body.includes(firmware)&&body.includes(sha)&&!body.includes('{{'));
  const release={tag_name:tag,draft:true,prerelease:true,assets:manifest.assets.map(a=>({name:a.name,size:a.size,digest:'sha256:'+a.sha256}))};
  const resultFile=join(run,'dist','release-response.json');
  writeFileSync(resultFile,JSON.stringify(release));
  assert.equal(script('verify-release-assets.mjs').status,0);
  release.assets[0].size++;
  writeFileSync(resultFile,JSON.stringify(release));
  assert.notEqual(script('verify-release-assets.mjs').status,0,'A wrong remote size must fail');
  const file=join(dist,'BUILD-INFO.json');
  const build=JSON.parse(readFileSync(file,'utf8'));build.license_review_required=['unreviewed@1.0'];
  writeFileSync(file,JSON.stringify(build));
  assert.notEqual(script('stage-release.mjs').status,0,'An unreviewed dependency must block publication');
 }finally{rmSync(run,{recursive:true,force:true});}
});

test('Windows packaging preserves PATH in any casing without duplicate keys',()=>{for(const key of ['PATH','Path','path']){const input={[key]:'C:/cargo/bin;C:/Windows',OTHER:'kept'};const env=prependWindowsSearchPath(input,'C:/venv/Scripts');assert.equal(env.PATH,'C:/venv/Scripts;C:/cargo/bin;C:/Windows');assert.deepEqual(Object.keys(env).filter(k=>k.toUpperCase()==='PATH'),['PATH']);assert.equal(input[key],'C:/cargo/bin;C:/Windows');assert.equal(env.OTHER,'kept');}assert.throws(()=>prependWindowsSearchPath({PATH:'a',Path:'b'},'first'),/Conflicting/);assert.equal(prependWindowsSearchPath({PATH:'same',Path:'same'},'first').PATH,'first;same');const source=readFileSync(new URL('./package-windows.mjs',import.meta.url),'utf8');assert(source.includes("buildEnv=prependWindowsSearchPath(buildEnv,join(pythonEnv,'Scripts'))"));});
test('Windows packaged child retains executables from inherited Path after venv insertion',()=>{if(process.platform!=='win32')return;const env=Object.fromEntries(Object.entries(process.env).filter(([key])=>key.toUpperCase()!=='PATH'));env.Path=process.env.PATH;const child=spawnSync('node.exe',['-e',"process.stdout.write('path-kept')"],{env:prependWindowsSearchPath(env,join(tmpdir(),'srn-nonexistent-venv')),encoding:'utf8',timeout:10000});assert.ifError(child.error);assert.equal(child.status,0,child.stderr);assert.equal(child.stdout,'path-kept');});

test('upstream GPL2 heading permits its original indentation without mutating bytes',()=>{for(const prefix of ['', '                    ','\ufeff\r\n  ']){const bytes=Buffer.from(prefix+'GNU GENERAL PUBLIC LICENSE\nVersion 2, June 1991\n');const copy=Buffer.from(bytes);assert.doesNotThrow(()=>validateGpl2Notice(bytes));assert(bytes.equals(copy));}for(const text of ['GNU GENERAL PUBLIC LICENSE\nVersion 3','not a license',''])assert.throws(()=>validateGpl2Notice(Buffer.from(text)),/GPL version 2/);});
test('updater package includes its pinned corresponding source and original license',()=>{const source=readFileSync(new URL('./package-windows.mjs',import.meta.url),'utf8');assert(source.includes('validateGpl2Notice(copying)'));assert(source.includes('aa255d1750e168226d31dcb44be723c4023503b3a0805447032567da0febe455'));assert(source.includes("'AVRDUDE-SOURCE.zip'"));assert(source.includes('source_archive_sha256:AVRDUDE_SOURCE_ARCHIVE_SHA256'));});


test('release frontend is an explicit asset list, never a drive-letter URL',()=>{
 const root=join(tmpdir(),'日本語 space frontend');const names=['index.html','app.mjs','style.css'];
 const files=embeddedFrontendDist(root,names);assert.deepEqual(JSON.parse(JSON.stringify({build:{frontendDist:files}})).build.frontendDist,names.map(n=>join(root,n)));
 assert.deepEqual(names,['index.html','app.mjs','style.css']);
 const packaging=readFileSync(new URL('./package-windows.mjs',import.meta.url),'utf8');
 assert(packaging.includes('const frontendFiles=embeddedFrontendDist(frontend,'));assert(packaging.includes('build:{frontendDist:frontendFiles}'));
 assert(!packaging.includes('build:{frontendDist:frontend}'));
});
test('embedded frontend rejects ambiguous or incomplete file lists',()=>{
 const root=join(tmpdir(),'frontend');
 for(const names of [[],['app.mjs'],['index.html','index.html'],['index.html','../escape'],['index.html','sub/file'],['index.html','sub\\file'],['index.html','d:external'],['index.html',null],['index.html','..']])assert.throws(()=>embeddedFrontendDist(root,names));
 assert.throws(()=>embeddedFrontendDist('relative',['index.html']));
});
