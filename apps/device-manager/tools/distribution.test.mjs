import {isProductInput,verifyProductInputs} from './artifact-inputs.mjs';
import {waitForAppPage} from './native-session.mjs';
import {ciPolicyEnabled} from './ci-webview-policy.mjs';
import {tmpdir} from 'node:os';import {join} from 'node:path';
import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync,mkdirSync,mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {inspectPE,runtimeEnvironment,sha256,zipDirectory,extractZip,executableMetadata,supplementalCrateLicenses} from './distribution-lib.mjs';
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
test('runtime test excludes developer paths, credentials and runtime overrides',()=>{const e=runtimeEnvironment({SystemRoot:'C:\\Windows',TEMP:'C:\\Temp',PATH:'D:\\VS',GH_TOKEN:'secret',CARGO_HOME:'D:\\Rust',WEBVIEW2_BROWSER_EXECUTABLE_FOLDER:'D:\\Browser'});assert.equal(e.GH_TOKEN,undefined);assert.equal(e.CARGO_HOME,undefined);assert.equal(e.WEBVIEW2_BROWSER_EXECUTABLE_FOLDER,undefined);assert(!e.PATH.includes('VS'));assert(e.PATH.includes('System32'));});
test('distribution config keeps developer path separate and embeds offline WebView2',()=>{const c=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url)));const r=JSON.parse(readFileSync(new URL('../src-tauri/tauri.release.conf.json',import.meta.url)));assert.equal(c.bundle.active,false);assert.equal(c.build.windows.staticVCRuntime,true);assert.equal(r.bundle.active,true);assert.deepEqual(r.bundle.targets,['nsis']);assert.equal(r.bundle.windows.webviewInstallMode.type,'offlineInstaller');assert.equal(r.bundle.windows.nsis.installMode,'currentUser');const launcher=readFileSync(new URL('../run-native.cmd',import.meta.url),'utf8');assert(!launcher.includes('D:\\Xpotato-apps'));assert(launcher.includes('vswhere.exe'));assert(launcher.includes('run build -- --debug'));});
test('application and bundle version remain identical',()=>{const c=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url)));const cargo=readFileSync(new URL('../src-tauri/Cargo.toml',import.meta.url),'utf8');assert(cargo.includes('version = "'+c.version+'"'));});
test('checksums are SHA-256 over actual bytes',()=>assert.equal(sha256(Buffer.from('abc')),'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'));

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

test('artifact reuse separates production inputs from test-only scripts',()=>{assert(isProductInput('apps/device-manager/frontend/style.css'));assert(isProductInput('apps/device-manager/frontend/readout.mjs'));assert(!isProductInput('apps/device-manager/tools/ui-qa.mjs'));assert(!isProductInput('apps/device-manager/frontend/readout.test.mjs'));assert(isProductInput('apps/device-manager/src-tauri/windows/installer.nsi'));});
test('missing, changed, added and removed product inputs reject reuse',()=>{verifyProductInputs({a:'same'},{a:'same'});for(const record of [null,{}, {a:'other'},{a:'same',b:'added'}])assert.throws(()=>verifyProductInputs(record,{a:'same'}),/rebuild/);});

test('release staging accepts only matching verified bytes and license-complete manifests',()=>{
 const run=mkdtempSync(join(tmpdir(),'srn-release-stage-'));
 const tag='device-manager-v0.1.0-preview.5',sha='a'.repeat(40),version='0.1.0-preview.5';
 const stem='Selfrionette-Device-Manager_'+version+'_'+sha.slice(0,8)+'_windows-x64';
 const dist=join(run,'dist','release-assets',stem),qa=join(run,'dist','release-qa','test');
 const notesDir=join(run,'docs','releases');mkdirSync(dist,{recursive:true});mkdirSync(qa,{recursive:true});mkdirSync(notesDir,{recursive:true});
 const setup=stem+'-setup.exe',portable=stem+'-portable.zip';
 const files={
  [setup]:'fixture setup',[portable]:'fixture zip',
  'README.md':'fixture',
  'BUILD-INFO.json':JSON.stringify({source_commit:sha,application_version:version,target:'x86_64-pc-windows-msvc',license_review_required:[],source_dirty:false}),
  'VALIDATION.json':JSON.stringify({result:'PASS',source_commit:sha}),
  'UPGRADE-VALIDATION.json':JSON.stringify({result:'PASS',source_commit:sha})
 };
 for(const [name,content] of Object.entries(files))writeFileSync(join(dist,name),content);
 const sums=Object.keys(files).map(name=>sha256(readFileSync(join(dist,name)))+'  '+name).join('\n')+'\n';
 writeFileSync(join(dist,'SHA256SUMS.txt'),sums);
 writeFileSync(join(qa,'native-window-normal.png'),Buffer.from('synthetic image bytes'));
 writeFileSync(join(notesDir,tag+'.md'),'{{SETUP_URL}} {{PORTABLE_URL}} {{CHECKSUM_URL}} {{SCREENSHOT_URL}} {{SOURCE_SHA}}');
 const env={...process.env,GITHUB_REPOSITORY:'Xpotato1024/Selfrionette-Device',GITHUB_REF_NAME:tag,GITHUB_SHA:sha};
 const script=name=>spawnSync(process.execPath,[fileURLToPath(new URL('./'+name,import.meta.url))],{cwd:run,env,encoding:'utf8',timeout:10000});
 try {
  let result=script('stage-release.mjs');assert.equal(result.status,0,result.stderr);
  const manifest=JSON.parse(readFileSync(join(run,'dist','release-manifest.json'),'utf8'));
  assert.equal(manifest.assets.length,8);
  const body=readFileSync(join(run,'dist','release-notes.md'),'utf8');
  assert(body.includes(setup)&&body.includes(sha)&&!body.includes('{{'));
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
