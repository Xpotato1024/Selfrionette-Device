// Rebind a downloaded build to this checkout without compiling or changing its source provenance.
import {readFileSync,writeFileSync,mkdirSync,readdirSync,existsSync} from 'node:fs';
import {resolve,join} from 'node:path';import {fileURLToPath} from 'node:url';
import {productInputs,verifyProductInputs} from './artifact-inputs.mjs';import {extractZip,sha256} from './distribution-lib.mjs';
const app=resolve(fileURLToPath(new URL('..',import.meta.url))),repo=resolve(app,'../..');
let directory=resolve(process.argv[2]||join(app,'dist/build-artifact'));
if(!existsSync(join(directory,'BUILD-INFO.json'))){const children=readdirSync(directory,{withFileTypes:true}).filter(e=>e.isDirectory()&&existsSync(join(directory,e.name,'BUILD-INFO.json')));if(children.length!==1)throw Error('Expected one build artifact');directory=join(directory,children[0].name);}
const info=JSON.parse(readFileSync(join(directory,'BUILD-INFO.json'),'utf8'));
verifyProductInputs(info.product_inputs,productInputs(repo));
for(const line of readFileSync(join(directory,'SHA256SUMS.txt'),'utf8').trim().split(/\r?\n/)){
 const match=/^([0-9a-f]{64})  ([^\/\\]+)$/.exec(line);if(!match||sha256(readFileSync(join(directory,match[2])))!==match[1])throw Error('Artifact checksum mismatch');
}
const zips=readdirSync(directory).filter(n=>n.endsWith('-portable.zip'));if(zips.length!==1)throw Error('Expected one portable ZIP');
const portable=join(directory,'test-runtime');if(existsSync(portable))throw Error('Use a new extraction directory');extractZip(join(directory,zips[0]),portable);
const executable=join(portable,'selfrionette-device-manager.exe');if(sha256(readFileSync(executable))!==info.executable_sha256)throw Error('EXE mismatch');
mkdirSync(join(app,'dist'),{recursive:true});writeFileSync(join(app,'dist/latest-distribution.json'),JSON.stringify({directory,portable,executable,commit:info.source_commit},null,2)+'\n');
console.log('Reusing unchanged product artifact from '+info.source_commit+'; no build performed.');
