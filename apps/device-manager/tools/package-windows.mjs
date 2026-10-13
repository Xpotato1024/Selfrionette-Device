// Maintainer-only packaging. Users run the resulting binary, never this script.
import {readFileSync,writeFileSync,mkdirSync,readdirSync,cpSync,copyFileSync,existsSync,rmSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync,spawnSync} from 'node:child_process';
import {sha256,inspectPE,collectNotices,zipDirectory,executableMetadata} from './distribution-lib.mjs';
if(process.platform!=='win32'||process.arch!=='x64')throw Error('Package on Windows x64 with the MSVC toolchain.');
const root=resolve(fileURLToPath(new URL('..',import.meta.url))),repo=resolve(root,'../..');
process.chdir(root);
const output=(program,args,cwd=root)=>execFileSync(program,args,{cwd,encoding:'utf8',maxBuffer:24*1024*1024,timeout:180000}).trim();
const git=(...args)=>output('git',['-C',repo,...args]);
if(git('status','--porcelain','--untracked-files=normal'))throw Error('Commit or stash changes before creating a traceable distribution. Developer build/dev accepts an ordinary working tree.');
const config=JSON.parse(readFileSync('src-tauri/tauri.conf.json','utf8'));
const commit=git('rev-parse','HEAD'),version=config.version;
const reference=process.env.GITHUB_REF??'';
if(reference.startsWith('refs/tags/device-manager-v') && reference.slice('refs/tags/device-manager-v'.length)!==version)throw Error('Release tag and application version differ.');
const stem='Selfrionette-Device-Manager_'+version+'_'+commit.slice(0,8)+'_windows-x64';
const destination=resolve('dist',stem);
if(existsSync(destination))throw Error('Distribution already exists; use a fresh output directory or remove only the old generated distribution explicitly.');

const resources=resolve('dist/bundle-resources');
// These are build-owned staging directories only; never reuse stale notices/assets.
rmSync(resources,{recursive:true,force:true});mkdirSync(resources,{recursive:true});
const metadata=JSON.parse(output('cargo',['metadata','--locked','--format-version','1','--manifest-path','src-tauri/Cargo.toml','--filter-platform','x86_64-pc-windows-msvc']));
const notices=collectNotices(metadata,resources);
const installerUpstream=JSON.parse(readFileSync('src-tauri/windows/UPSTREAM.json','utf8'));
const cliVersion=JSON.parse(readFileSync('node_modules/@tauri-apps/cli/package.json','utf8')).version;
if(cliVersion!==installerUpstream.cli_version)throw Error('Review the custom NSIS template before changing Tauri CLI.');
notices.text+='\n\nTauri NSIS installer template (local path-resolution changes):\n'+readFileSync('src-tauri/windows/LICENSE-MIT.tauri','utf8');
writeFileSync(join(resources,'THIRD-PARTY-NOTICES.txt'),notices.text);
writeFileSync(join(resources,'DEPENDENCIES.json'),JSON.stringify(notices.inventory,null,2)+'\n');
copyFileSync(join(repo,'docs/operations/device-manager-distribution.md'),join(resources,'README.md'));
// Explicit file allowlist: development tests are not embedded into the Release executable.
const frontend=resolve('dist/frontend');rmSync(frontend,{recursive:true,force:true});mkdirSync(frontend,{recursive:true});
for(const name of ['index.html','style.css','app.mjs','controller.mjs','design-preview.mjs','navigation.mjs','hardware-ui.mjs','hardware-controller.mjs'])copyFileSync(join(root,'frontend',name),join(frontend,name));
const info={schema:1,application_version:version,publisher:config.bundle.publisher,homepage:config.bundle.homepage,application_identifier:config.identifier,source_commit:commit,source_tree:git('rev-parse','HEAD^{tree}'),source_dirty:false,
  built_at_utc:new Date().toISOString(),target:'x86_64-pc-windows-msvc',profile:'release',installer_template_sha256:sha256(readFileSync('src-tauri/windows/installer.nsi')),installer_bookkeeping:'Software\\dev.selfrionette.manager\\Installer',bundle_binary_patching:false,mode:'serial-monitor-and-synthetic',hardware_verified:false,
  runtime:'Microsoft Edge WebView2 Evergreen',installer_runtime_mode:'offlineInstaller',code_signing:'unsigned',
  toolchain:{rust:output('rustc',['-Vv']),cargo:output('cargo',['-V']),node:process.version},
  lockfiles:{cargo:sha256(readFileSync('src-tauri/Cargo.lock')),npm:sha256(readFileSync('package-lock.json'))},
  license_review_required:notices.licenseReview,validation:'Not a hardware acceptance or clean-VM certification'};
writeFileSync(join(resources,'BUILD-INFO.json'),JSON.stringify(info,null,2)+'\n');
const cli=join(root,'node_modules/@tauri-apps/cli/tauri.js');
// No updater is used: ship exactly the same executable in ZIP and NSIS, without a bundle-type stamp.
const result=spawnSync(process.execPath,[cli,'build','--config','src-tauri/tauri.release.conf.json','--bundles','nsis','--ci','--no-binary-patching','--','--locked'],{cwd:root,stdio:'inherit',timeout:30*60*1000});
if(result.error)throw result.error;if(result.status!==0)throw Error('Tauri packaging failed: '+result.status);
const exe=resolve('src-tauri/target/release/selfrionette-device-manager.exe');
info.windows_metadata=executableMetadata(exe);
if(info.windows_metadata.company_name!==config.bundle.publisher || info.windows_metadata.product_name!==config.productName)throw Error('Executable publisher/product metadata mismatch');
if(info.windows_metadata.signature_status!=='NotSigned')throw Error('Update signing metadata/policy before distributing a signed executable');
info.pe=inspectPE(readFileSync(exe));info.executable_sha256=sha256(readFileSync(exe));
mkdirSync(destination,{recursive:true});
const portable=join(destination,'portable');mkdirSync(portable);copyFileSync(exe,join(portable,'selfrionette-device-manager.exe'));
cpSync(resources,portable,{recursive:true});
writeFileSync(join(portable,'BUILD-INFO.json'),JSON.stringify(info,null,2)+'\n');
const bundle=resolve('src-tauri/target/release/bundle/nsis');
const setups=readdirSync(bundle).filter(n=>n.includes(version)&&n.endsWith('-setup.exe'));
if(setups.length!==1)throw Error('Expected exactly one matching NSIS installer, found '+setups.length);
copyFileSync(join(bundle,setups[0]),join(destination,stem+'-setup.exe'));
const zip=join(destination,stem+'-portable.zip');
zipDirectory(portable,zip);
writeFileSync(join(destination,'BUILD-INFO.json'),JSON.stringify(info,null,2)+'\n');
copyFileSync(join(resources,'README.md'),join(destination,'README.md'));
const files=readdirSync(destination).filter(n=>/\.(exe|zip|json|md)$/.test(n));
writeFileSync(join(destination,'SHA256SUMS.txt'),files.map(n=>sha256(readFileSync(join(destination,n)))+'  '+n).join('\n')+'\n');
writeFileSync(resolve('dist/latest-distribution.json'),JSON.stringify({directory:destination,portable,executable:join(portable,'selfrionette-device-manager.exe'),commit},null,2)+'\n');
if(git('rev-parse','HEAD')!==commit || git('status','--porcelain','--untracked-files=normal'))throw Error('Source changed during build; do not distribute these files.');
console.log('Distribution created: '+destination);
console.log('Runtime dependencies:',info.pe.imports.join(', '));
console.log('Public-release license follow-up:',notices.licenseReview.join(', ')||'none');
