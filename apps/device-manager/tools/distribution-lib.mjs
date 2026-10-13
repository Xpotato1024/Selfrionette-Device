import {spawnSync} from 'node:child_process';
import { readFileSync, readdirSync, statSync, copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join, basename } from 'node:path';
import { createHash } from 'node:crypto';

export const sha256 = data => createHash('sha256').update(data).digest('hex');
export function runtimeEnvironment(env) {
  // Only OS/user-profile variables are passed into the tested executable.
  const allowed = new Set(['systemroot','windir','systemdrive','comspec','temp','tmp','userprofile','appdata','localappdata','programdata','programfiles','programfiles(x86)','os','username','userdomain']);
  const result = Object.fromEntries(Object.entries(env).filter(([k]) => allowed.has(k.toLowerCase())));
  const windows = env.SystemRoot || env.SYSTEMROOT || 'C:\\Windows';
  result.PATH = windows + '\\System32;' + windows;
  return result;
}
export function inspectPE(bytes) {
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
  if(subsystem!==2)throw Error('Distribution executable must use Windows GUI subsystem');
  return {machine:'x86_64',subsystem:'windows_gui',imports:[...new Set(imports)].sort()};
}
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
    if(!files.length || /MPL/.test(p.license??'')) {
      const archive=p.name+'-'+p.version+'.crate';
      const cache=join(dirname(dirname(dirname(dir))),'cache',basename(dirname(dir)),archive);
      copyFileSync(cache,join(output,'dependency-sources',archive));
      entry.archive='dependency-sources/'+archive;entry.archive_sha256=sha256(readFileSync(cache));
      if(!files.length)sections.push('The published crate has no top-level license text. Its original source archive is included; consult the pinned package/upstream license before public redistribution.');
    }
    inventory.push(entry);
  }
  return {text:sections.join('\n')+'\n',inventory,licenseReview:inventory.filter(p=>!p.notices.length).map(p=>p.name+'@'+p.version)};
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
