import {verifyFamilyCatalog,digest,sourceTreeHash,legacySource} from './firmware-packages.mjs';
import {spawnSync} from 'node:child_process';
import { readFileSync, readdirSync, statSync, copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join, basename, isAbsolute } from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

// FrontendDist is an untagged URL/directory/files union. A Windows D:\ path
// parses as a URL; an explicit file array always selects embedded assets.
export function embeddedFrontendDist(directory, names) {
  if (!isAbsolute(directory) || !Array.isArray(names) || !names.includes('index.html') || new Set(names).size !== names.length) throw Error('Expected an absolute staging directory and unique frontend files including index.html');
  if (names.some(name => typeof name !== 'string' || !name || name === '.' || name === '..' || /[\\/:]/.test(name))) throw Error('Frontend files must be flat filenames');
  return names.map(name => join(directory, name));
}

export const sha256 = data => createHash('sha256').update(data).digest('hex');
export function validateGpl2Notice(bytes) {
  const text = bytes.toString('utf8').trimStart();
  if (!text.startsWith('GNU GENERAL PUBLIC LICENSE') || !text.includes('Version 2, June 1991')) throw Error('Expected the upstream GPL version 2 notice');
}
export function verifyBundledUpdaterResources(applicationRoot, buildInfo) {
  const check = (record, fileField, hashField, expectedFile, label) => {
    if (!record || record[fileField] !== expectedFile) throw Error(`Build metadata has an invalid ${label} path.`);
    const expected = record[hashField];
    if (!/^[a-f0-9]{64}$/.test(expected ?? '')) throw Error(`Build metadata has no valid ${label} SHA-256.`);
    const bytes = readFileSync(join(applicationRoot, ...expectedFile.split('/')));
    if (sha256(bytes) !== expected) throw Error(`Bundled ${label} does not match BUILD-INFO.json.`);
    return bytes;
  };
  const firmware = buildInfo.firmware;
  const manifestBytes = check(firmware, 'manifest_file', 'manifest_sha256', 'distribution/firmware/manifest.json', 'firmware manifest');
  const imageBytes = check(firmware, 'image_file', 'image_sha256', 'distribution/firmware/firmware.hex', 'firmware image');
  check(firmware, 'notice_file', 'notice_sha256', 'distribution/firmware/THIRD-PARTY-NOTICES.txt', 'firmware notice');
  const runtime = buildInfo.updater_runtime;
  check(runtime, 'executable_file', 'executable_sha256', 'distribution/firmware-tools/avrdude.exe', 'AVRDUDE executable');
  check(runtime, 'configuration_file', 'configuration_sha256', 'distribution/firmware-tools/avrdude.conf', 'AVRDUDE configuration');
  check(runtime, 'license_file', 'license_sha256', 'distribution/firmware-tools/AVRDUDE-COPYING.txt', 'AVRDUDE COPYING');
  check(runtime, 'source_archive_file', 'source_archive_sha256', 'distribution/firmware-tools/AVRDUDE-SOURCE.zip', 'AVRDUDE source archive');
  check(buildInfo.third_party_notices, 'file', 'sha256', 'distribution/THIRD-PARTY-NOTICES.txt', 'combined third-party notice');
  let manifest;
  try { manifest = JSON.parse(manifestBytes.toString('utf8')); }
  catch (error) { throw Error(`Bundled firmware manifest is invalid JSON: ${error.message}`); }
  if (manifest.image_file !== 'firmware.hex' || manifest.image_sha256 !== firmware.image_sha256 ||
      manifest.image_size !== imageBytes.length || manifest.source_sha !== buildInfo.source_commit ||
      manifest.source_sha !== firmware.source_sha || manifest.release_version !== buildInfo.application_version ||
      manifest.firmware_version !== buildInfo.application_version) {
    throw Error('Bundled firmware manifest fields disagree with BUILD-INFO.json or the firmware image.');
  }
  if(buildInfo.application_version==='0.2.0-preview.2'){
   check(firmware,'catalog_file','catalog_sha256','distribution/firmware/catalog.json','firmware family catalog');
   verifyFamilyCatalog(join(applicationRoot,'distribution/firmware'));
   const sourceInfo=JSON.parse(check(firmware,'source_info_file','source_info_sha256','distribution/firmware/SOURCE-INFO.json','firmware source info'));
   const sources=join(applicationRoot,'distribution/firmware/sources');
   const legacy=JSON.parse(readFileSync(join(applicationRoot,'distribution/firmware/legacy-7ch/manifest.json')));
   if(sourceInfo.schema!==1||sourceInfo.source_commit!==buildInfo.source_commit||sourceInfo.modern?.source_tree_sha256!==manifest.source_tree_sha256||sourceInfo.legacy?.source_tree_sha256!==legacy.source_tree_sha256||sourceTreeHash(join(sources,'loadcell_7ch'))!==manifest.source_tree_sha256||sourceTreeHash(join(sources,legacySource.slice('firmware/'.length)))!==legacy.source_tree_sha256||sourceTreeHash(join(sources,'shared'))!==sourceInfo.modern.shared_tree_sha256)throw Error('Packaged firmware source fingerprint mismatch');
   for(const [name,path]of [['legacy_manifest_sha256','manifest.json'],['legacy_image_sha256','firmware.hex']])if(digest(readFileSync(join(applicationRoot,'distribution/firmware/legacy-7ch',path)))!==firmware[name])throw Error('Legacy firmware build binding mismatch');
  }
  return {manifest, imageBytes};
}
// Windows child environments must have a single case-insensitive search-path key.
export function prependWindowsSearchPath(environment, directory) {
  const env = {...environment};
  const keys = Object.keys(env).filter(key => key.toUpperCase() === 'PATH');
  const values = [...new Set(keys.map(key => env[key]).filter(value => value !== undefined))];
  if (values.length > 1) throw Error('Conflicting Windows PATH values');
  for (const key of keys) delete env[key];
  env.PATH = [directory, values[0]].filter(Boolean).join(';');
  return env;
}
export function runtimeEnvironment(env) {
  // Only OS/user-profile variables are passed into the tested executable.
  const allowed = new Set(['systemroot','windir','systemdrive','comspec','temp','tmp','userprofile','appdata','localappdata','programdata','programfiles','programfiles(x86)','os','username','userdomain']);
  const result = Object.fromEntries(Object.entries(env).filter(([k]) => allowed.has(k.toLowerCase())));
  const windows = env.SystemRoot || env.SYSTEMROOT || 'C:\\Windows';
  result.PATH = windows + '\\System32;' + windows;
  return result;
}
export function inspectPE(bytes) { return inspectPESubsystem(bytes, 2, 'GUI', 'windows_gui'); }
export function inspectConsolePE(bytes) { return inspectPESubsystem(bytes, 3, 'console', 'windows_console'); }
function inspectPESubsystem(bytes, expectedSubsystem, expectedKind, subsystemName) {
  const ensure = (offset, size) => { if (!Number.isInteger(offset) || offset < 0 || offset + size > bytes.length) throw Error('Truncated PE image'); };
  const u16 = offset => { ensure(offset,2); return bytes.readUInt16LE(offset); };
  const u32 = offset => { ensure(offset,4); return bytes.readUInt32LE(offset); };
  if (u16(0) !== 0x5a4d) throw Error('Not an executable');
  const pe = u32(0x3c); if (u32(pe) !== 0x4550) throw Error('Invalid PE signature');
  const machine=u16(pe+4), sections=u16(pe+6), optional=pe+24, optionalSize=u16(pe+20);
  if (machine !== 0x8664 || u16(optional)!==0x20b) throw Error('Expected Windows x64 PE32+');
  ensure(optional,optionalSize);
  const subsystem=u16(optional+68), sectionTable=optional+optionalSize;
  if (sections>96 || optionalSize<240) throw Error('Invalid PE header');
  const rva = value => {
    if (value < u32(optional+60)) { ensure(value,1); return value; }
    for(let i=0;i<sections;i++) {
      const s=sectionTable+i*40; ensure(s,40);
      const start=u32(s+12), rawSize=u32(s+16);
      if(value>=start && value<start+rawSize) { const offset=u32(s+20)+value-start; ensure(offset,1); return offset; }
    }
    throw Error('Unmapped PE address');
  };
  const text = offset => {
    for(let i=0;i<256;i++) { ensure(offset+i,1); if(bytes[offset+i]===0) return bytes.toString('ascii',offset,offset+i); }
    throw Error('Unterminated import name');
  };
  const imports=[];
  for(const [directory,stride,nameOffset] of [[1,20,12],[13,32,4]]) {
    const base=u32(optional+112+directory*8); if(!base)continue;
    const offset=rva(base);let ended=false;
    for(let i=0;i<256;i++) {
      const d=offset+i*stride;ensure(d,stride);
      if(bytes.subarray(d,d+stride).every(v=>v===0)){ended=true;break;}
      if(directory===13 && u32(d)!==1)throw Error('Unsupported delay-import addressing');
      imports.push(text(rva(u32(d+nameOffset))));
    }
    if(!ended)throw Error('Unterminated PE imports');
  }
  const forbidden=imports.filter(n=>/^(vcruntime|msvcp|msvcr|ucrtbased)|^WebView2Loader\.dll$/i.test(n));
  if(forbidden.length)throw Error('Non-system runtime DLL dependency: '+forbidden.join(', '));
  if(subsystem!==expectedSubsystem)throw Error('Distribution executable must use Windows '+expectedKind+' subsystem');
  return {machine:'x86_64',subsystem:subsystemName,imports:[...new Set(imports)].sort()};
}
export function mergePackageMetadata(...metadata) {
  const packages = new Map();
  for (const source of metadata) for (const pkg of source.packages) {
    if (!pkg.id) throw Error('Cargo metadata package id is missing.');
    const existing = packages.get(pkg.id);
    if (existing) {
      if (existing.name !== pkg.name || existing.version !== pkg.version ||
          existing.source !== pkg.source || existing.license !== pkg.license) {
        throw Error('Conflicting Cargo metadata for exact package id '+pkg.id);
      }
      continue;
    }
    packages.set(pkg.id, pkg);
  }
  return {...metadata[0], packages:[...packages.values()]};
}
// These exact published crates omit a top-level license file. The upstream license
// texts are pinned in ../licenses/ and are bundled as additional notices.
export const supplementalCrateLicenses = Object.freeze({
  'alloc-stdlib@0.3.0': {declared:'BSD-3-Clause',file:'alloc-stdlib-BSD-3-Clause.txt'},
  'defmt-parser@1.0.0': {declared:'MIT OR Apache-2.0',file:'defmt-MIT.txt'},
  'selectors@0.38.0': {declared:'MPL-2.0',file:'servo-MPL-2.0.txt'},
  'webview2-com-macros@0.8.1': {declared:'MIT',file:'webview2-rs-MIT.txt'},
  'webview2-com-sys@0.39.1': {declared:'MIT',file:'webview2-rs-MIT.txt'},
  'webview2-com@0.39.1': {declared:'MIT',file:'webview2-rs-MIT.txt'},
});
export function collectNotices(metadata, output) {
  mkdirSync(join(output,'dependency-sources'),{recursive:true});
  const sections=['Third-party notices / exact dependency sources',
    'The inventory includes build-time dependencies as well as runtime dependencies. Dependency license declarations are not a license for Selfrionette-Device.',
    'Upstream source archives are supplied for crates with MPL declarations or missing top-level license files. No dependency source has been modified.'];
  const inventory=[];
  for(const p of metadata.packages.filter(p=>p.source).sort((a,b)=>(a.name+'@'+a.version).localeCompare(b.name+'@'+b.version))) {
    const dir=dirname(p.manifest_path);
    const files=readdirSync(dir).filter(n=>/^(licen[cs]e|copying|copyright|notice)([._-]|$)/i.test(n)&&statSync(join(dir,n)).isFile());
    const entry={name:p.name,version:p.version,license:p.license,source:'https://crates.io/crates/'+p.name+'/'+p.version,repository:p.repository,notices:files};
    sections.push('\n============================================================\n'+p.name+' '+p.version+'\nLicense: '+p.license+'\nSource: '+entry.source+'\nRepository: '+(p.repository||'See source archive'));
    for(const name of files)sections.push('\n--- '+name+' ---\n'+readFileSync(join(dir,name),'utf8'));
    const supplemental = !files.length && supplementalCrateLicenses[p.name+'@'+p.version];
    if (supplemental) {
      if (p.license !== supplemental.declared) throw Error('Re-review changed license declaration for '+p.name+'@'+p.version);
      const licenseText = readFileSync(fileURLToPath(new URL('../licenses/'+supplemental.file,import.meta.url)));
      entry.supplemental_license = supplemental.file;
      entry.supplemental_sha256 = sha256(licenseText);
      sections.push('\n--- Upstream license: '+supplemental.file+' (pinned copy) ---\n'+licenseText.toString('utf8'));
    }
    if(!files.length || /MPL/.test(p.license??'')) {
      const archive=p.name+'-'+p.version+'.crate';
      const cache=join(dirname(dirname(dirname(dir))),'cache',basename(dirname(dir)),archive);
      copyFileSync(cache,join(output,'dependency-sources',archive));
      entry.archive='dependency-sources/'+archive;entry.archive_sha256=sha256(readFileSync(cache));
      if(!files.length && !supplemental)sections.push('The published crate has no top-level license text. Its original source archive is included; review the upstream license before public redistribution.');
    }
    inventory.push(entry);
  }
  return {text:sections.join('\n')+'\n',inventory,licenseReview:inventory.filter(p=>!p.notices.length&&!p.supplemental_license).map(p=>p.name+'@'+p.version)};
}

export function zipDirectory(source, destination) {
  zipOperation(source,destination,'[IO.Compression.ZipFile]::CreateFromDirectory($env:SRN_ZIP_SOURCE,$env:SRN_ZIP_DESTINATION)');
}
export function extractZip(source,destination) {
  zipOperation(source,destination,'[IO.Compression.ZipFile]::ExtractToDirectory($env:SRN_ZIP_SOURCE,$env:SRN_ZIP_DESTINATION)');
}
function zipOperation(source,destination,operation) {
  // .NET and UTF-16 environment variables preserve Unicode on non-Japanese Windows.
  // Paths are data, never interpolated into PowerShell source.
  const command="$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.IO.Compression.FileSystem; "+operation;
  const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',command],{
    env:{...process.env,SRN_ZIP_SOURCE:source,SRN_ZIP_DESTINATION:destination},encoding:'utf8',timeout:180000,maxBuffer:1024*1024});
  if(result.error)throw result.error;
  if(result.status!==0)throw Error('ZIP operation failed: '+result.stderr);
}

// Read Windows version resources and the actual signature, not the publisher label alone.
export function executableMetadata(file, environment = process.env) {
  // pwsh -> node -> powershell.exe otherwise inherits incompatible PS7 module paths.
  // Windows PowerShell reconstructs its own defaults; only the child environment changes.
  const env = Object.fromEntries(Object.entries(environment).filter(([key]) => key.toUpperCase() !== 'PSMODULEPATH'));
  const command = "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); $v=[Diagnostics.FileVersionInfo]::GetVersionInfo($env:SRN_METADATA_EXE); $s=Get-AuthenticodeSignature -LiteralPath $env:SRN_METADATA_EXE; @{company_name=$v.CompanyName; product_name=$v.ProductName; file_version=$v.FileVersion; signature_status=[string]$s.Status} | ConvertTo-Json -Compress";
  const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',command],{
    env:{...env,SRN_METADATA_EXE:file},encoding:'utf8',timeout:30000,maxBuffer:1024*1024});
  if(result.error)throw result.error;
  if(result.status!==0)throw Error('Windows metadata inspection failed: '+result.stderr);
  return JSON.parse(result.stdout.replace(/^\uFEFF/,''));
}
