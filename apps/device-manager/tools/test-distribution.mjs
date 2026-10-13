// Test the Release executable from an extracted ZIP, not from a clone/build folder.
import {readFileSync,mkdirSync,mkdtempSync,readdirSync,writeFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';
import {inspectPE,sha256,extractZip} from './distribution-lib.mjs';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));process.chdir(root);
const distribution=JSON.parse(readFileSync('dist/latest-distribution.json','utf8'));
const zip=readdirSync(distribution.directory).find(n=>n.endsWith('-portable.zip'));assert(zip);
const parent=resolve('src-tauri/target/distribution-qa');mkdirSync(parent,{recursive:true});const run=mkdtempSync(join(parent,'run-'));
const extracted=join(run,'日本語 空白のあるフォルダー');mkdirSync(extracted);
extractZip(join(distribution.directory,zip),extracted);
const exe=join(extracted,'selfrionette-device-manager.exe'),info=JSON.parse(readFileSync(join(extracted,'BUILD-INFO.json'),'utf8'));
assert.equal(sha256(readFileSync(exe)),info.executable_sha256);const pe=inspectPE(readFileSync(exe));
const executables=[exe];
if(process.env.SELFRIONETTE_INSTALLED_EXE) {
  const installed=resolve(process.env.SELFRIONETTE_INSTALLED_EXE);
  assert.equal(sha256(readFileSync(installed)),info.executable_sha256);
  executables.push(installed);
}
for(const binary of executables) {
  const test=spawnSync(process.execPath,['tools/native-smoke.mjs'],{cwd:root,stdio:'inherit',timeout:180000,
    env:{...process.env,SELFRIONETTE_GUI_EXE:binary,SELFRIONETTE_GUI_RUNTIME_ONLY:'1'}});
  if(test.error)throw test.error;assert.equal(test.status,0);
}
const report={result:'PASS',source_commit:info.source_commit,executable_sha256:info.executable_sha256,pe,
  tested_binaries:executables.length,installer_tested:executables.length===2,
  scope:'Release executable; system-only child PATH, isolated working directory; synthetic Rust IPC + GUI regression tests',
  limitations:['Build tools still installed on host disk','Local mode uses an existing WebView2 Runtime; CI installs through the packaged setup','No clean VM or missing-runtime install test','No physical device access']};
writeFileSync(join(distribution.directory,'VALIDATION.json'),JSON.stringify(report,null,2)+'\n');
const sums=join(distribution.directory,'SHA256SUMS.txt');
const old=readFileSync(sums,'utf8').split('\n').filter(line=>line && !line.endsWith('  VALIDATION.json'));
old.push(sha256(readFileSync(join(distribution.directory,'VALIDATION.json')))+'  VALIDATION.json');
writeFileSync(sums,old.join('\n')+'\n');
writeFileSync(join(run,'report.json'),JSON.stringify(report,null,2)+'\n');
console.log('Extracted Release distribution PASS:',run);
