import {verifyFamilyCatalog} from './firmware-packages.mjs';
import {productInputs,verifyProductInputs} from './artifact-inputs.mjs';
// Test the Release executable from an extracted ZIP, not from a clone/build folder.
import {readFileSync,mkdirSync,mkdtempSync,readdirSync,writeFileSync,rmSync} from 'node:fs';
import {resolve,join,dirname,basename,sep,isAbsolute} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync,execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';
import {inspectPE,inspectConsolePE,sha256,extractZip,executableMetadata,runtimeEnvironment,verifyBundledUpdaterResources} from './distribution-lib.mjs';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));process.chdir(root);
const distribution=JSON.parse(readFileSync('dist/latest-distribution.json','utf8'));
const zip=readdirSync(distribution.directory).find(n=>n.endsWith('-portable.zip'));assert(zip);
const taskTempRoot=process.env.SELFRIONETTE_TASK_TEMP_ROOT||process.env.TEMP||process.env.TMP;
assert(taskTempRoot&&isAbsolute(taskTempRoot),'Set an absolute task-scoped temporary root for distribution QA.');
const absoluteTempRoot=resolve(taskTempRoot),parent=resolve(absoluteTempRoot,'distribution-qa');
assert(parent.startsWith(absoluteTempRoot+sep),'Distribution QA staging must stay below the task temporary root.');
mkdirSync(parent,{recursive:true});const run=mkdtempSync(join(parent,'run-'));
try {
const extracted=join(run,'日本語 空白のあるフォルダー');mkdirSync(extracted);
extractZip(join(distribution.directory,zip),extracted);
const exe=join(extracted,'selfrionette-device-manager.exe'),info=JSON.parse(readFileSync(join(extracted,'BUILD-INFO.json'),'utf8'));
verifyProductInputs(info.product_inputs,productInputs(resolve(root,'../..')));
assert.equal(sha256(readFileSync(exe)),info.executable_sha256);const pe=inspectPE(readFileSync(exe));
const bundledResources=verifyBundledUpdaterResources(extracted,info);
const cli=join(extracted,'distribution/selfrionettectl.exe');
assert.equal(info.cli?.file,'distribution/selfrionettectl.exe');
assert.equal(sha256(readFileSync(cli)),info.cli.executable_sha256);
const cliPe=inspectConsolePE(readFileSync(cli));
assert.deepEqual(cliPe,info.cli.pe);
const executables=[exe];
if(process.env.SELFRIONETTE_INSTALLED_EXE) {
  const installed=resolve(process.env.SELFRIONETTE_INSTALLED_EXE);
  assert.equal(sha256(readFileSync(installed)),info.executable_sha256);
  // NSIS installs resource destinations relative to $INSTDIR, including distribution/.
  verifyBundledUpdaterResources(dirname(installed),info);
  executables.push(installed);
}
if(process.env.SELFRIONETTE_INSTALLED_CLI){
  const installedCli=resolve(process.env.SELFRIONETTE_INSTALLED_CLI);
  assert.equal(sha256(readFileSync(installedCli)),info.cli.executable_sha256);
  assert.deepEqual(inspectConsolePE(readFileSync(installedCli)),info.cli.pe);
}
const firmwarePackageFile=info.firmware?.package_file;
assert.equal(typeof firmwarePackageFile,'string');
assert.equal(basename(firmwarePackageFile),firmwarePackageFile);
assert(firmwarePackageFile.endsWith('-firmware.zip'));
const firmwarePackagePath=join(distribution.directory,firmwarePackageFile);
const firmwarePackageBytes=readFileSync(firmwarePackagePath);
assert.equal(sha256(firmwarePackageBytes),info.firmware.package_sha256);
const standaloneFirmware=join(run,'standalone-firmware');mkdirSync(standaloneFirmware);
extractZip(firmwarePackagePath,standaloneFirmware);
assert.deepEqual(readdirSync(standaloneFirmware).sort(),['SOURCE-INFO.json','THIRD-PARTY-NOTICES.txt','catalog.json','firmware.hex','legacy-7ch','manifest.json','sources']);
verifyFamilyCatalog(standaloneFirmware);
assert.equal(sha256(readFileSync(join(standaloneFirmware,'manifest.json'))),info.firmware.manifest_sha256);
assert.equal(sha256(readFileSync(join(standaloneFirmware,'firmware.hex'))),info.firmware.image_sha256);
assert.equal(sha256(readFileSync(join(standaloneFirmware,'THIRD-PARTY-NOTICES.txt'))),info.firmware.notice_sha256);
assert(readFileSync(join(standaloneFirmware,'manifest.json')).equals(readFileSync(join(extracted,'distribution/firmware/manifest.json'))));
assert(readFileSync(join(standaloneFirmware,'firmware.hex')).equals(bundledResources.imageBytes));
assert(readFileSync(join(standaloneFirmware,'THIRD-PARTY-NOTICES.txt')).equals(readFileSync(join(extracted,'distribution/firmware/THIRD-PARTY-NOTICES.txt'))));
const metadata=[];
for(const binary of executables) {
  const value=executableMetadata(binary);
  assert.equal(value.company_name,info.publisher);
  assert.equal(value.product_name,'Selfrionette Device Manager');
  assert.equal(value.signature_status,'NotSigned');
  metadata.push(value);
  const test=spawnSync(process.execPath,['tools/native-smoke.mjs'],{cwd:root,stdio:'inherit',timeout:180000,
    env:{...process.env,SELFRIONETTE_GUI_EXE:binary,SELFRIONETTE_GUI_RUNTIME_ONLY:'1'}});
  if(test.error)throw test.error;assert.equal(test.status,0);
}
const avrdudePath=join(extracted,'distribution/firmware-tools/avrdude.exe');
const avrdudeConfig=join(extracted,'distribution/firmware-tools/avrdude.conf');
const avrdude=spawnSync(avrdudePath,['-N','-C',avrdudeConfig,'-?'],{
  cwd:run,env:runtimeEnvironment(process.env),encoding:'utf8',timeout:30_000,maxBuffer:1024*1024,
});
if(avrdude.error)throw avrdude.error;
assert.equal(avrdude.signal,null,'Packaged AVRDUDE help smoke must exit without a signal.');
// v8.3 src/main.c case '?' exits 0 for an explicit -?; no target is opened.
// https://github.com/avrdudes/avrdude/blob/v8.3/src/main.c
assert.equal(avrdude.status,0,'Packaged AVRDUDE help must complete successfully.');
assert.match((avrdude.stdout||'')+'\n'+(avrdude.stderr||''),/Usage:/i);
const report={result:'PASS',tester_commit:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),windows_metadata:metadata,source_commit:info.source_commit,executable_sha256:info.executable_sha256,pe,cli_executable_sha256:info.cli.executable_sha256,cli_pe:cliPe,
  firmware_package_sha256:info.firmware.package_sha256,packaged_payload_verified:true,
  updater_runtime_executed:true,avrdude_no_port_help_smoke:true,
  tested_binaries:executables.length,installer_tested:executables.length===2,
  scope:'Release executable; system-only child PATH, isolated working directory; synthetic Rust IPC + GUI regression tests',
  limitations:['Build tools still installed on host disk','Local mode uses an existing WebView2 Runtime; CI installs through the packaged setup','No clean VM or missing-runtime install test','No physical device access']};
writeFileSync(join(distribution.directory,'VALIDATION.json'),JSON.stringify(report,null,2)+'\n');
const sums=join(distribution.directory,'SHA256SUMS.txt');
const old=readFileSync(sums,'utf8').split('\n').filter(line=>line && !line.endsWith('  VALIDATION.json'));
old.push(sha256(readFileSync(join(distribution.directory,'VALIDATION.json')))+'  VALIDATION.json');
writeFileSync(sums,old.join('\n')+'\n');
console.log('Extracted Release distribution PASS; temporary QA tree removed after validation.');
} finally {rmSync(run,{recursive:true,force:true});}
