import {tmpdir} from 'node:os';import {join} from 'node:path';
import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync,mkdirSync,mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {inspectPE,runtimeEnvironment,sha256,zipDirectory,extractZip} from './distribution-lib.mjs';
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

test('ZIP round trip keeps Japanese, spaces and quotes as path data',{skip:process.platform!=='win32'},()=>{
 const run=mkdtempSync(join(tmpdir(),'srn-zip-'));try{
 const source=join(run,"日本語 space's"),dest=join(run,'展開 先'),archive=join(run,'配布物.zip');
 mkdirSync(source);writeFileSync(join(source,'試験.txt'),'入力確認','utf8');zipDirectory(source,archive);extractZip(archive,dest);
 assert.equal(readFileSync(join(dest,'試験.txt'),'utf8'),'入力確認');
 }finally{rmSync(run,{recursive:true,force:true});}
});
