import {appendFirmwareNotices,firmwareNoticePayload,productInputs} from './artifact-inputs.mjs';
// Maintainer-only packaging. Users run the resulting binary, never this script.
import {readFileSync,writeFileSync,mkdirSync,readdirSync,cpSync,copyFileSync,existsSync,rmSync,statSync} from 'node:fs';
import {resolve,join,sep,isAbsolute,basename} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync,spawnSync} from 'node:child_process';
import {embeddedFrontendDist,validateGpl2Notice,prependWindowsSearchPath,sha256,inspectPE,inspectConsolePE,mergePackageMetadata,collectNotices,zipDirectory,extractZip,executableMetadata} from './distribution-lib.mjs';

const APP_VERSION='0.2.0-preview.1';
const FIRMWARE_APPLICATION='selfrionette-loadcell-7ch';
const FIRMWARE_BOARD='sparkfun_promicro16';
const FIRMWARE_MCU='atmega32u4';
const FIRMWARE_CLOCK_HZ=16_000_000;
const FIRMWARE_PROTOCOL_SCHEMA=3;
const FIRMWARE_SETTINGS_SCHEMA=1;
const AVRDUDE_VERSION='8.3';
const AVRDUDE_SOURCE_COMMIT='f139b390a6821039f5e4a2693f179803a85117b0';
const AVRDUDE_ARCHIVE_URL='https://github.com/avrdudes/avrdude/releases/download/v8.3/avrdude-v8.3-windows-x64.zip';
const AVRDUDE_ARCHIVE_SHA256='8630c7d8bb9682a1685aaa9ad42550ae1131c9d13528f0d20975368ba615e9b0';
const AVRDUDE_COPYING_URL=`https://raw.githubusercontent.com/avrdudes/avrdude/${AVRDUDE_SOURCE_COMMIT}/COPYING`;
const AVRDUDE_COPYING_MAX_BYTES=64*1024;
const AVRDUDE_SOURCE_ARCHIVE_URL='https://codeload.github.com/avrdudes/avrdude/zip/'+AVRDUDE_SOURCE_COMMIT;
const AVRDUDE_SOURCE_ARCHIVE_SHA256='aa255d1750e168226d31dcb44be723c4023503b3a0805447032567da0febe455';
const CLI_PACKAGE='selfrionettectl';
const CLI_EXECUTABLE='selfrionettectl.exe';

if(process.platform!=='win32'||process.arch!=='x64')throw Error('Package on Windows x64 with the MSVC toolchain.');
const root=resolve(fileURLToPath(new URL('..',import.meta.url))),repo=resolve(root,'../..');
process.chdir(root);
const taskTempRoot=process.env.SELFRIONETTE_TASK_TEMP_ROOT;
if(!taskTempRoot||!isAbsolute(taskTempRoot))throw Error('Set SELFRIONETTE_TASK_TEMP_ROOT to the absolute task-scoped system temporary root.');
const absoluteTempRoot=resolve(taskTempRoot);
const packageWork=resolve(absoluteTempRoot,'device-manager-package');
if(!packageWork.startsWith(absoluteTempRoot+sep))throw Error('Package staging must remain below the task temporary root.');
if(existsSync(packageWork))throw Error('Task package staging already exists; preserve it and use a fresh task temporary root.');
mkdirSync(packageWork,{recursive:true});

let buildEnv={
 ...process.env,
 CARGO_HOME:join(packageWork,'cargo-home'),
 CARGO_TARGET_DIR:process.env.CARGO_TARGET_DIR??join(packageWork,'cargo-target'),
 PLATFORMIO_CORE_DIR:join(packageWork,'platformio-core'),
 PLATFORMIO_WORKSPACE_DIR:join(packageWork,'platformio-workspace'),
 PLATFORMIO_BUILD_DIR:join(packageWork,'platformio-build'),
 PIP_CACHE_DIR:join(packageWork,'pip-cache'),
 PYTHONPYCACHEPREFIX:join(packageWork,'python-cache'),
 PYTHONDONTWRITEBYTECODE:'1',
 TEMP:absoluteTempRoot,
 TMP:absoluteTempRoot,
 SELFRIONETTE_EXPECTED_FIRMWARE_MANIFEST_SHA256:'',
 SELFRIONETTE_EXPECTED_FIRMWARE_IMAGE_SHA256:'',
 SELFRIONETTE_EXPECTED_AVRDUDE_EXE_SHA256:'',
 SELFRIONETTE_EXPECTED_AVRDUDE_CONF_SHA256:'',
 SELFRIONETTE_SOURCE_SHA:'',
};
const run=(program,args,{cwd=root,env=buildEnv,timeout=30*60*1000,stdio='inherit'}={})=>{
 const result=spawnSync(program,args,{cwd,env,encoding:stdio==='inherit'?undefined:'utf8',stdio,timeout,maxBuffer:32*1024*1024});
 if(result.error)throw result.error;
 if(result.status!==0)throw Error(`${program} failed with exit code ${result.status}${result.stderr?`: ${result.stderr}`:''}`);
 return result.stdout?.trim()??'';
};
const output=(program,args,cwd=root)=>execFileSync(program,args,{cwd,env:buildEnv,encoding:'utf8',maxBuffer:24*1024*1024,timeout:180000}).trim();
const git=(...args)=>output('git',['-C',repo,...args]);
const hashFile=file=>sha256(readFileSync(file));
function staticCrtEnvironment(environment){
 const env={...environment};
 if(Object.hasOwn(env,'CARGO_ENCODED_RUSTFLAGS')){
  const flags=env.CARGO_ENCODED_RUSTFLAGS.split('\u001f').filter(Boolean);
  flags.push('-C','target-feature=+crt-static');
  env.CARGO_ENCODED_RUSTFLAGS=flags.join('\u001f');
 }else{
  env.RUSTFLAGS=[env.RUSTFLAGS,'-C target-feature=+crt-static'].filter(Boolean).join(' ');
 }
 return env;
}
const findUnique=(directory,name)=>{
 const matches=[];
 function walk(at){for(const entry of readdirSync(at,{withFileTypes:true})){const path=join(at,entry.name);if(entry.isDirectory())walk(path);else if(entry.isFile()&&entry.name===name)matches.push(path);}}
 walk(directory);
 if(matches.length!==1)throw Error(`Expected one ${name} in the verified AVRDUDE archive, found ${matches.length}.`);
 return matches[0];
};
async function fetchPinned(url,maximumBytes){
 const response=await fetch(url,{signal:AbortSignal.timeout(60_000),redirect:'follow'});
 if(!response.ok)throw Error(`Pinned upstream download failed (${response.status}): ${url}`);
 const bytes=Buffer.from(await response.arrayBuffer());
 if(bytes.length===0||bytes.length>maximumBytes)throw Error(`Pinned upstream file has an invalid size: ${url}`);
 return bytes;
}

async function build(){
 if(git('status','--porcelain','--untracked-files=normal'))throw Error('Commit or stash changes before creating a traceable distribution. Developer build/dev accepts an ordinary working tree.');
 const config=JSON.parse(readFileSync('src-tauri/tauri.conf.json','utf8'));
 if(config.version!==APP_VERSION)throw Error('The application version does not match the reviewed updater package version.');
 const commit=git('rev-parse','HEAD');
 const reference=process.env.GITHUB_REF??'';
 if(reference.startsWith('refs/tags/device-manager-v') && reference.slice('refs/tags/device-manager-v'.length)!==config.version)throw Error('Release tag and application version differ.');
 const stem='Selfrionette-Device-Manager_'+config.version+'_'+commit.slice(0,8)+'_windows-x64';
 const destination=resolve('dist',stem);
 if(existsSync(destination))throw Error('Distribution already exists; use a fresh output directory or remove only the old generated distribution explicitly.');

 const firmwareProject=resolve(repo,'firmware/loadcell_7ch');
 const pioIni=readFileSync(join(firmwareProject,'platformio.ini'),'utf8');
 const platform=/^\s*platform\s*=\s*([^\s#]+)\s*$/m.exec(pioIni)?.[1];
 if(!platform||!/^atmelavr@\d+\.\d+\.\d+$/.test(platform))throw Error('The release firmware build requires an exact atmelavr platform pin in firmware/loadcell_7ch/platformio.ini.');
 const firmwareSource=readFileSync(join(firmwareProject,'src/main.cpp'),'utf8');
 if(!firmwareSource.includes(`#define SELFRIONETTE_FIRMWARE_VERSION "${config.version}"`))throw Error('Firmware version and Device Manager release version differ.');

 const python=process.env.PYTHON??'python';
 const pythonEnv=join(packageWork,'python-env');
 run(python,['-m','venv',pythonEnv],{cwd:root});
 const pythonExe=join(pythonEnv,'Scripts','python.exe');
 const pioExe=join(pythonEnv,'Scripts','pio.exe');
 buildEnv=prependWindowsSearchPath(buildEnv,join(pythonEnv,'Scripts'));
 run(pythonExe,['-m','pip','install','--disable-pip-version-check','--no-cache-dir','-r',join(root,'tools/platformio-requirements.txt')]);
 const pioVersion=output(pioExe,['--version']);
 if(!pioVersion.includes('6.1.19'))throw Error('The isolated firmware toolchain does not match the pinned PlatformIO Core version.');
 buildEnv.SELFRIONETTE_SOURCE_SHA=commit;
 run(pythonExe,[join(repo,'tools/check_firmware_size.py'),'--project-dir',firmwareProject,'--environment','pro_micro_7ch','--max-static-ram','1024','--max-flash','24576'],{cwd:repo});

 const firmwareHexPath=join(buildEnv.PLATFORMIO_BUILD_DIR,'pro_micro_7ch','firmware.hex');
 const firmwareHex=readFileSync(firmwareHexPath);
 if(firmwareHex.length===0||firmwareHex.length>512*1024)throw Error('The PlatformIO firmware image has an invalid size.');
 const firmwareHash=sha256(firmwareHex);
 const firmwareManifest={
  schema:1,
  release_version:config.version,
  application:FIRMWARE_APPLICATION,
  board:FIRMWARE_BOARD,
  mcu:FIRMWARE_MCU,
  clock_hz:FIRMWARE_CLOCK_HZ,
  protocol_schema:FIRMWARE_PROTOCOL_SCHEMA,
  settings_schema:FIRMWARE_SETTINGS_SCHEMA,
  firmware_version:config.version,
  source_sha:commit,
  image_file:'firmware.hex',
  image_sha256:firmwareHash,
  image_size:firmwareHex.length,
  flash_start:0,
  flash_end_exclusive:0x7000,
 };
 const firmwareManifestBytes=Buffer.from(JSON.stringify(firmwareManifest,null,2)+'\n','utf8');
 const firmwareManifestHash=sha256(firmwareManifestBytes);

 const firmwarePackage=join(packageWork,'resources','firmware');
 const firmwareTools=join(packageWork,'resources','firmware-tools');
 mkdirSync(firmwarePackage,{recursive:true});mkdirSync(firmwareTools,{recursive:true});
 writeFileSync(join(firmwarePackage,'manifest.json'),firmwareManifestBytes,{flag:'wx'});
 copyFileSync(firmwareHexPath,join(firmwarePackage,'firmware.hex'));
 const firmwareNoticeBytes=firmwareNoticePayload(repo);
 writeFileSync(join(firmwarePackage,'THIRD-PARTY-NOTICES.txt'),firmwareNoticeBytes,{flag:'wx'});
 const firmwarePackageFile=stem+'-firmware.zip';
 const firmwarePackageArchive=join(packageWork,firmwarePackageFile);
 zipDirectory(firmwarePackage,firmwarePackageArchive);
 const firmwarePackageHash=hashFile(firmwarePackageArchive);

 run('cargo',['build','--locked','--release','--package',CLI_PACKAGE,'--jobs','1'],{cwd:repo,env:staticCrtEnvironment(buildEnv)});
 const cliExecutable=resolve(buildEnv.CARGO_TARGET_DIR,'release',CLI_EXECUTABLE);
 const cliBytes=readFileSync(cliExecutable);
 const cliPe=inspectConsolePE(cliBytes);
 const cliSha256=sha256(cliBytes);
 const cliPackageMetadata=JSON.parse(output('cargo',[
  'metadata','--locked','--format-version','1','--manifest-path',join(repo,'crates/selfrionettectl/Cargo.toml'),
  '--filter-platform','x86_64-pc-windows-msvc',
 ],repo));
 const cliPackage=cliPackageMetadata.packages.find(pkg=>pkg.name===CLI_PACKAGE&&pkg.source===null);
 if(!cliPackage)throw Error('Cargo workspace metadata did not contain the packaged CLI crate.');

 const archivePath=join(packageWork,'avrdude-v8.3-windows-x64.zip');
 const archiveBytes=await fetchPinned(AVRDUDE_ARCHIVE_URL,16*1024*1024);
 if(sha256(archiveBytes)!==AVRDUDE_ARCHIVE_SHA256)throw Error('The official AVRDUDE v8.3 Windows archive checksum does not match the reviewed release.');
 writeFileSync(archivePath,archiveBytes,{flag:'wx'});
 const extracted=join(packageWork,'avrdude-extracted');mkdirSync(extracted);
 extractZip(archivePath,extracted);
 const avrdudeExe=findUnique(extracted,'avrdude.exe');
 const avrdudeConf=findUnique(extracted,'avrdude.conf');
 copyFileSync(avrdudeExe,join(firmwareTools,'avrdude.exe'));
 copyFileSync(avrdudeConf,join(firmwareTools,'avrdude.conf'));
 const avrdudeExeHash=hashFile(join(firmwareTools,'avrdude.exe'));
 const avrdudeConfHash=hashFile(join(firmwareTools,'avrdude.conf'));
 const copying=await fetchPinned(AVRDUDE_COPYING_URL,AVRDUDE_COPYING_MAX_BYTES);
 validateGpl2Notice(copying);
 const sourceArchive=await fetchPinned(AVRDUDE_SOURCE_ARCHIVE_URL,16*1024*1024);
 if(sha256(sourceArchive)!==AVRDUDE_SOURCE_ARCHIVE_SHA256)throw Error('The pinned AVRDUDE source archive checksum does not match.');
 writeFileSync(join(firmwareTools,'AVRDUDE-SOURCE.zip'),sourceArchive,{flag:'wx'});
 writeFileSync(join(firmwareTools,'AVRDUDE-COPYING.txt'),copying,{flag:'wx'});

 const resourceRoot=join(packageWork,'resources');
 copyFileSync(cliExecutable,join(resourceRoot,CLI_EXECUTABLE));
 const guiMetadata=JSON.parse(output('cargo',['metadata','--locked','--format-version','1','--manifest-path','src-tauri/Cargo.toml','--filter-platform','x86_64-pc-windows-msvc']));
 const metadata=mergePackageMetadata(guiMetadata,cliPackageMetadata);
 const notices=collectNotices(metadata,resourceRoot);
 const installerUpstream=JSON.parse(readFileSync('src-tauri/windows/UPSTREAM.json','utf8'));
 const cliVersion=JSON.parse(readFileSync('node_modules/@tauri-apps/cli/package.json','utf8')).version;
 if(cliVersion!==installerUpstream.cli_version)throw Error('Review the custom NSIS template before changing Tauri CLI.');
 notices.text=appendFirmwareNotices(notices.text,firmwareNoticeBytes);
 notices.text+='\n\nAVRDUDE v8.3 Windows x64 runtime (unmodified upstream binary and configuration):\n'+
  'Source commit: https://github.com/avrdudes/avrdude/commit/'+AVRDUDE_SOURCE_COMMIT+'\n'+
  'Release archive: '+AVRDUDE_ARCHIVE_URL+'\n'+
  'Archive SHA-256: '+AVRDUDE_ARCHIVE_SHA256+'\n'+
  'License: GNU General Public License version 2; full upstream COPYING and corresponding source are included as AVRDUDE-COPYING.txt and AVRDUDE-SOURCE.zip.\n';
 writeFileSync(join(resourceRoot,'THIRD-PARTY-NOTICES.txt'),notices.text,{flag:'wx'});
 writeFileSync(join(resourceRoot,'DEPENDENCIES.json'),JSON.stringify(notices.inventory,null,2)+'\n',{flag:'wx'});
 copyFileSync(join(repo,'docs/operations/device-manager-distribution.md'),join(resourceRoot,'README.md'));
 const info={
  schema:3,product_inputs:productInputs(repo),application_version:config.version,publisher:config.bundle.publisher,homepage:config.bundle.homepage,
  application_identifier:config.identifier,source_commit:commit,source_tree:git('rev-parse','HEAD^{tree}'),source_dirty:false,
  built_at_utc:new Date().toISOString(),target:'x86_64-pc-windows-msvc',profile:'release',
  installer_template_sha256:sha256(readFileSync('src-tauri/windows/installer.nsi')),installer_bookkeeping:'Software\\dev.selfrionette.manager',
  bundle_binary_patching:false,mode:'serial-monitor-and-firmware-updater',hardware_verified:false,
  runtime:'Microsoft Edge WebView2 Evergreen',installer_runtime_mode:'offlineInstaller',code_signing:'unsigned',
  firmware:{manifest_file:'distribution/firmware/manifest.json',manifest_sha256:firmwareManifestHash,
   image_sha256:firmwareHash,source_sha:commit,platformio_platform:platform,platformio_core:pioVersion,
   platformio_config_sha256:sha256(Buffer.from(pioIni)),image_size:firmwareHex.length,flash_range:'0x0000..0x6fff',
   image_file:'distribution/firmware/firmware.hex',notice_file:'distribution/firmware/THIRD-PARTY-NOTICES.txt',
   notice_sha256:sha256(firmwareNoticeBytes),package_file:firmwarePackageFile,package_sha256:firmwarePackageHash},
  updater_runtime:{name:'avrdude',version:AVRDUDE_VERSION,source_commit:AVRDUDE_SOURCE_COMMIT,archive_url:AVRDUDE_ARCHIVE_URL,
   archive_sha256:AVRDUDE_ARCHIVE_SHA256,executable_file:'distribution/firmware-tools/avrdude.exe',
   executable_sha256:avrdudeExeHash,configuration_file:'distribution/firmware-tools/avrdude.conf',configuration_sha256:avrdudeConfHash,
   license:'GPL-2.0',license_file:'distribution/firmware-tools/AVRDUDE-COPYING.txt',license_sha256:sha256(copying),
   source_archive_file:'distribution/firmware-tools/AVRDUDE-SOURCE.zip',source_archive_sha256:AVRDUDE_SOURCE_ARCHIVE_SHA256,source_archive_url:AVRDUDE_SOURCE_ARCHIVE_URL},
  third_party_notices:{file:'distribution/THIRD-PARTY-NOTICES.txt',sha256:sha256(Buffer.from(notices.text,'utf8'))},
  cli:{package:CLI_PACKAGE,version:cliPackage.version,file:'distribution/'+CLI_EXECUTABLE,executable_sha256:cliSha256,
   target:'x86_64-pc-windows-msvc',profile:'release',runtime:'static C runtime',pe:cliPe},
  toolchain:{rust:output('rustc',['-Vv']),cargo:output('cargo',['-V']),node:process.version,python:output(pythonExe,['--version']),platformio:pioVersion},
  lockfiles:{cargo:sha256(readFileSync('src-tauri/Cargo.lock')),npm:sha256(readFileSync('package-lock.json'))},
  license_review_required:notices.licenseReview,validation:'Software package validation only; no physical device flashing or clean-VM certification',
 };
 writeFileSync(join(resourceRoot,'BUILD-INFO.json'),JSON.stringify(info,null,2)+'\n',{flag:'wx'});

 // Explicit frontend file allowlist: tests and maintainer tools are not embedded.
 const frontend=join(packageWork,'frontend');mkdirSync(frontend,{recursive:true});
 const frontendFiles=embeddedFrontendDist(frontend,['index.html','style.css','app.mjs','controller.mjs','design-preview.mjs','navigation.mjs','hardware-ui.mjs','hardware-controller.mjs','readout.mjs','trace.mjs','port-selection.mjs','application-controller.mjs','application-ui.mjs']);
 for(const file of frontendFiles){
  copyFileSync(join(root,'frontend',basename(file)),file);
 }
 buildEnv.SELFRIONETTE_EXPECTED_FIRMWARE_MANIFEST_SHA256=firmwareManifestHash;
 buildEnv.SELFRIONETTE_EXPECTED_FIRMWARE_IMAGE_SHA256=firmwareHash;
 buildEnv.SELFRIONETTE_EXPECTED_AVRDUDE_EXE_SHA256=avrdudeExeHash;
 buildEnv.SELFRIONETTE_EXPECTED_AVRDUDE_CONF_SHA256=avrdudeConfHash;
 const cli=join(root,'node_modules/@tauri-apps/cli/tauri.js');
 const buildFlavor=JSON.stringify({
  build:{frontendDist:frontendFiles},
  bundle:{resources:{[resourceRoot]:'distribution/'}},
 });
 const result=spawnSync(process.execPath,[cli,'build','--config','src-tauri/tauri.release.conf.json','--config',buildFlavor,'--bundles','nsis','--ci','--no-binary-patching','--','--locked'],{
  cwd:root,env:buildEnv,stdio:'inherit',timeout:30*60*1000,
 });
 if(result.error)throw result.error;if(result.status!==0)throw Error('Tauri packaging failed: '+result.status);
 const exe=resolve(buildEnv.CARGO_TARGET_DIR,'release/selfrionette-device-manager.exe');
 info.windows_metadata=executableMetadata(exe);
 if(info.windows_metadata.company_name!==config.bundle.publisher||info.windows_metadata.product_name!==config.productName)throw Error('Executable publisher/product metadata mismatch');
 if(info.windows_metadata.signature_status!=='NotSigned')throw Error('Update signing metadata/policy before distributing a signed executable');
 info.pe=inspectPE(readFileSync(exe));info.executable_sha256=hashFile(exe);
 mkdirSync(destination,{recursive:true});
 copyFileSync(firmwarePackageArchive,join(destination,firmwarePackageFile));
 const portable=join(destination,'portable');mkdirSync(portable);copyFileSync(exe,join(portable,'selfrionette-device-manager.exe'));
 const portableResources=join(portable,'distribution');mkdirSync(portableResources);
 cpSync(resourceRoot,portableResources,{recursive:true});
 writeFileSync(join(portable,'BUILD-INFO.json'),JSON.stringify(info,null,2)+'\n');
 const bundle=resolve(buildEnv.CARGO_TARGET_DIR,'release/bundle/nsis');
 const setups=readdirSync(bundle).filter(name=>name.includes(config.version)&&name.endsWith('-setup.exe'));
 if(setups.length!==1)throw Error('Expected exactly one matching NSIS installer, found '+setups.length);
 copyFileSync(join(bundle,setups[0]),join(destination,stem+'-setup.exe'));
 const zip=join(destination,stem+'-portable.zip');zipDirectory(portable,zip);
 writeFileSync(join(destination,'BUILD-INFO.json'),JSON.stringify(info,null,2)+'\n');
 copyFileSync(join(resourceRoot,'README.md'),join(destination,'README.md'));
 const files=readdirSync(destination).filter(name=>/\.(exe|zip|json|md|txt)$/.test(name));
 writeFileSync(join(destination,'SHA256SUMS.txt'),files.map(name=>hashFile(join(destination,name))+'  '+name).join('\n')+'\n');
 writeFileSync(resolve('dist/latest-distribution.json'),JSON.stringify({directory:destination,portable,executable:join(portable,'selfrionette-device-manager.exe'),commit},null,2)+'\n');
 if(git('rev-parse','HEAD')!==commit||git('status','--porcelain','--untracked-files=normal'))throw Error('Source changed during build; do not distribute these files.');
 console.log('Distribution created: '+destination);
 console.log('Firmware package SHA-256:',firmwareManifestHash,'image:',firmwareHash);
 console.log('Pinned AVRDUDE:',AVRDUDE_VERSION,'archive:',AVRDUDE_ARCHIVE_SHA256);
 console.log('Runtime dependencies:',info.pe.imports.join(', '));
 console.log('Public-release license follow-up:',notices.licenseReview.join(', ')||'none');
}

try{await build();}
finally{rmSync(packageWork,{recursive:true,force:true});}
