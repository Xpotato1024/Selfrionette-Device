// Verify every uploaded release asset before changing a draft into a public prerelease.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const expected=JSON.parse(readFileSync('dist/release-manifest.json','utf8'));
const actual=JSON.parse(readFileSync('dist/release-response.json','utf8'));
assert.equal(actual.tag_name,expected.tag);
assert.equal(actual.draft,true,'Existing published releases must not be overwritten');
assert.equal(actual.prerelease,true);
assert.equal(actual.assets.length,expected.assets.length,'Unexpected asset count');
for(const asset of expected.assets){
 const remote=actual.assets.find(item=>item.name===asset.name);
 assert(remote,'Missing uploaded asset: '+asset.name);
 assert.equal(remote.size,asset.size,'Uploaded size mismatch: '+asset.name);
 if(remote.digest)assert.equal(remote.digest,'sha256:'+asset.sha256,'Uploaded digest mismatch: '+asset.name);
}
console.log('GitHub draft release assets verified: '+expected.assets.length);
