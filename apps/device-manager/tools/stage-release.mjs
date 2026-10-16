// Prepare one verified GitHub Release from the successful tag-run distribution job.
// This script never builds an executable or talks to GitHub.
import assert from 'node:assert/strict';
import {readFileSync,readdirSync,existsSync,copyFileSync,mkdirSync,rmSync,writeFileSync,statSync} from 'node:fs';
import {resolve,join,basename} from 'node:path';
import {createHash} from 'node:crypto';
const repo=process.env.GITHUB_REPOSITORY,sha=process.env.GITHUB_SHA,tag=process.env.GITHUB_REF_NAME;
assert(repo&&/^[\w.-]+\/[\w.-]+$/.test(repo),'GitHub repository required');
assert(sha&&/^[0-9a-f]{40}$/.test(sha),'Tagged source SHA required');
assert(tag&&/^device-manager-v[0-9]+\.[0-9]+\.[0-9]+-[a-z0-9.]+$/.test(tag),'Preview release tag required');
const version=tag.slice('device-manager-v'.length);
const root=resolve('dist/release-assets'),qa=resolve('dist/release-qa');
const dirs=readdirSync(root,{withFileTypes:true}).filter(x=>x.isDirectory()&&x.name.startsWith('Selfrionette-Device-Manager_'));
assert.equal(dirs.length,1,'Exactly one output directory is required');
const dist=join(root,dirs[0].name);
const entries=readdirSync(dist);
const one=pattern=>{const matched=entries.filter(name=>pattern.test(name));assert.equal(matched.length,1,'Expected exactly one '+pattern);return matched[0];};
const setup=one(/-setup\.exe$/),portable=one(/-portable\.zip$/),firmware=one(/-firmware\.zip$/);
const expectedStem='Selfrionette-Device-Manager_'+version+'_'+sha.slice(0,8)+'_windows-x64';
assert.equal(setup,expectedStem+'-setup.exe');
assert.equal(portable,expectedStem+'-portable.zip');
assert.equal(firmware,expectedStem+'-firmware.zip');
const readJson=name=>JSON.parse(readFileSync(join(dist,name),'utf8'));
const build=readJson('BUILD-INFO.json'),validation=readJson('VALIDATION.json'),upgrade=readJson('UPGRADE-VALIDATION.json');
assert.equal(build.source_commit,sha);
assert.equal(build.application_version,version);
assert.equal(build.target,'x86_64-pc-windows-msvc');
assert.deepEqual(build.license_review_required,[],'Resolve unreviewed dependency licenses before publication');
assert.equal(build.source_dirty,false);
assert.equal(validation.result,'PASS');
assert.equal(validation.source_commit,sha);
assert.equal(validation.packaged_payload_verified,true,'Firmware, updater and license payload validation is required');
assert.equal(validation.updater_runtime_executed,true,'The pinned updater must execute without developer PATH');
assert.equal(upgrade.result,'PASS');
assert.equal(upgrade.source_commit,sha);
const hash=file=>createHash('sha256').update(readFileSync(file)).digest('hex');
const sums=readFileSync(join(dist,'SHA256SUMS.txt'),'utf8').trim().split(/\r?\n/);
const declared=new Set();
for(const line of sums){
 const m=/^([a-f0-9]{64})  ([^\r\n]+)$/.exec(line);
 assert(m,'Invalid checksum entry: '+line);
 assert(!declared.has(m[2]),'Duplicate checksum name');
 declared.add(m[2]);
 assert(existsSync(join(dist,m[2])),'Missing checksum file '+m[2]);
 assert.equal(hash(join(dist,m[2])),m[1],'Checksum mismatch '+m[2]);
}
assert(declared.has(setup)&&declared.has(portable)&&declared.has(firmware));
const screenshotCandidates=[];
function walk(directory){
 if(!existsSync(directory))return;
 for(const d of readdirSync(directory,{withFileTypes:true})){
  const file=join(directory,d.name);
  if(d.isDirectory())walk(file);
  else if(d.isFile()&&d.name==='REPORT.json'){
   const evidence=JSON.parse(readFileSync(file,'utf8'));
   if(evidence.result!=='PASS'||evidence.executable_sha256!==build.executable_sha256||evidence.application?.version!==version)continue;
   const required=['01-device-direction-tare.png','02-personal-calibration-verified.png','03-saved-runtime-input.png'];
   if(!required.every(name=>evidence.screenshots?.includes(name)&&existsSync(join(directory,name))))continue;
   screenshotCandidates.push(join(directory,required[2]));
  }
 }
}
walk(qa);
assert(screenshotCandidates.length>0,'Verified startup IPC, personal calibration and runtime GUI evidence from this executable is required');
const stage=resolve('dist/release-upload');
rmSync(stage,{recursive:true,force:true});mkdirSync(stage,{recursive:true});
const assetNames=[setup,portable,firmware,'SHA256SUMS.txt','README.md','BUILD-INFO.json','VALIDATION.json','UPGRADE-VALIDATION.json'];
for(const name of assetNames){assert(existsSync(join(dist,name)));copyFileSync(join(dist,name),join(stage,name));}
const screenshot='selfrionette-device-manager-'+version+'.png';
copyFileSync(screenshotCandidates[0],join(stage,screenshot));
const assetUrl=name=>'https://github.com/'+repo+'/releases/download/'+tag+'/'+encodeURIComponent(name);
const notesPath=resolve('docs/releases/'+tag+'.md');
assert(existsSync(notesPath),'Reviewed Japanese release notes are required');
let body=readFileSync(notesPath,'utf8');
const replacements={
 '{{SETUP_URL}}':assetUrl(setup),
 '{{PORTABLE_URL}}':assetUrl(portable),
 '{{FIRMWARE_URL}}':assetUrl(firmware),
 '{{CHECKSUM_URL}}':assetUrl('SHA256SUMS.txt'),
 '{{SCREENSHOT_URL}}':assetUrl(screenshot),
 '{{SOURCE_SHA}}':sha,
};
for(const [marker,value] of Object.entries(replacements)){
 assert(body.includes(marker),'Missing notes marker: '+marker);
 body=body.replaceAll(marker,value);
}
assert(!/\{\{[^{}]+\}\}/.test(body),'Unexpanded notes markers');
writeFileSync('dist/release-notes.md',body);
const manifest={tag,source_commit:sha,version,assets:assetNames.concat(screenshot).map(name=>({name,size:statSync(join(stage,name)).size,sha256:hash(join(stage,name))}))};
writeFileSync('dist/release-manifest.json',JSON.stringify(manifest,null,2)+'\n');
console.log('Release staged and validated: '+JSON.stringify({tag,sha,files:manifest.assets.map(a=>a.name)}));
