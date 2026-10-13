import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdtempSync,mkdirSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {randomUUID} from 'node:crypto';
const app=resolve(import.meta.dirname,'..');
const template=readFileSync(join(app,'src-tauri/windows/installer.nsi'),'utf8');
const source=template.match(/Function SRNReadRegisteredDirectory\r?\n[\s\S]*?FunctionEnd/)[0];
const nsis=process.env.SRN_MAKENSIS || join(process.env.LOCALAPPDATA,'tauri/NSIS/makensis.exe');
assert(existsSync(nsis),'Build the package once so the locked NSIS toolchain is available, or set SRN_MAKENSIS.');
const run=mkdtempSync(join(tmpdir(),'srn-installer-state-'));
const target=join(run,'space 日本語'),fixtureKey='Software\\SelfrionetteInstallerTests\\'+randomUUID();mkdirSync(target);
writeFileSync(join(target,'sample.exe'),'test-owned');writeFileSync(join(target,'uninstall.exe'),'test-owned');
const q=s=>s.replaceAll('$','$$').replaceAll('"','$\\"');
const lines=['Unicode true','RequestExecutionLevel user','SilentInstall silent','!include LogicLib.nsh','!include FileFunc.nsh','!include StrFunc.nsh','${StrLoc}',
 '!define PRODUCTNAME "Selfrionette Installer Test"','!define MAINBINARYNAME "sample"','!define UNINSTKEY "'+fixtureKey+'"','Var SRNRegisteredDirectory','Var SRNRegistrationState',
 'OutFile "'+q(join(run,'probe.exe'))+'"',source,'Section','SetShellVarContext current','SetRegView 64','FileOpen $9 "'+q(join(run,'results.txt'))+'" w'];
const expected=[];
function entry(name,changes={},remove=[]){
 lines.push('DeleteRegKey HKCU "${UNINSTKEY}"');
 const attrs={DisplayName:'Selfrionette Installer Test',MainBinaryName:'sample.exe',InstallLocation:'"'+target+'"',UninstallString:'"'+join(target,'uninstall.exe')+'"',...changes};
 for(const [k,v] of Object.entries(attrs))if(!remove.includes(k))lines.push('WriteRegStr HKCU "${UNINSTKEY}" "'+k+'" "'+q(v)+'"');
 lines.push('Call SRNReadRegisteredDirectory','FileWriteUTF16LE $9 "'+name+'|$SRNRegistrationState|$SRNRegisteredDirectory$\\r$\\n"');
 expected.push([name,changes.expectedState??'invalid',changes.expectedState==='valid'?target:'']);
}
lines.push('Call SRNReadRegisteredDirectory','FileWriteUTF16LE $9 "absent|$SRNRegistrationState|$SRNRegisteredDirectory$\\r$\\n"');expected.push(['absent','absent','']);
entry('quoted-unicode-space',{expectedState:'valid'});
entry('unquoted-path',{InstallLocation:target,expectedState:'valid'});
entry('empty-directory',{InstallLocation:''});
entry('missing-directory',{},['InstallLocation']);
entry('wrong-product',{DisplayName:'Other app'});
entry('wrong-binary',{MainBinaryName:'different.exe'});
entry('different-uninstaller',{UninstallString:'"'+join(run,'other.exe')+'"'});
entry('extra-uninstall-arguments',{UninstallString:'"'+join(target,'uninstall.exe')+'" /OTHER'});
entry('relative-directory',{InstallLocation:'relative\\directory'});
entry('drive-root',{InstallLocation:'D:\\'});
entry('missing-files',{InstallLocation:'"'+join(run,'absent')+'"',UninstallString:'"'+join(run,'absent','uninstall.exe')+'"'});
entry('broken-quotes',{InstallLocation:'"'+target});
lines.push('DeleteRegKey HKCU "${UNINSTKEY}"','FileClose $9','SectionEnd');
const nsi=join(run,'probe.nsi');writeFileSync(nsi,lines.join('\n')+'\n');
let result=spawnSync(nsis,['/INPUTCHARSET','UTF8','/WX','/V2',nsi],{encoding:'utf8',timeout:30000});
assert.equal(result.status,0,result.stdout+result.stderr);
result=spawnSync(join(run,'probe.exe'),['/S'],{timeout:10000});assert.equal(result.status,0);
const actual=readFileSync(join(run,'results.txt'),'utf16le').replace(/^\uFEFF/,'').trim().split(/\r?\n/);
for(let i=0;i<expected.length;i++){
 const [name,state]=expected[i],fields=actual[i]?.split('|');assert.equal(fields?.[0],name);assert.equal(fields?.[1],state,actual[i]);assert.equal(fields?.[2],expected[i][2]);
}
console.log('PASS: '+expected.length+' NSIS registry/path cases; isolated key '+fixtureKey);
console.log('Evidence: '+run);
