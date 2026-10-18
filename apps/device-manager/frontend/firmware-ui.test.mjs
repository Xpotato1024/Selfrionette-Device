import test from 'node:test';
import assert from 'node:assert/strict';
import {progressView,createProgressTracker,packageIdentityRows,observedIdentityRows} from './firmware-ui.mjs';
test('unknown percentages stay indeterminate and write 100 percent is not completion',()=>{
 assert.equal(progressView({stage:'resetting',percent:90}).percent,null);
 assert.equal(progressView({stage:'programming',percent:100,active:true}).active,true);
 assert.match(progressView({stage:'programming',percent:100}).title,/書込み中/);
 assert.equal(progressView({stage:'needs_recovery',percent:100}).percent,null);
 assert.equal(progressView({stage:'programming',percent:101}).percent,null);
});
test('old plans, duplicate progress and stale retries are ignored',()=>{
 const accept=createProgressTracker('new',4);
 assert.equal(accept({operation_id:'old',sequence:5}),null);
 assert.equal(accept({operation_id:'new',sequence:4}),null);
 assert(accept({operation_id:'new',sequence:5,stage:'verifying'}));
 assert.equal(accept({operation_id:'new',sequence:5,stage:'complete'}),null);
});
test('distribution, reported firmware and wire protocol versions are separate',()=>{
 const rows=Object.fromEntries(packageIdentityRows({family:'legacy_7ch',distribution_version:'1.0.0',firmware_version:'',release_version:'2.0.0',protocol_schema:0}));
 assert.equal(rows['Firmware配布版'],'1.0.0'); assert.equal(rows['同梱するアプリ版'],'2.0.0');
 assert.match(rows['本体Firmware版'],/取得機能なし/);
 assert.match(Object.fromEntries(observedIdentityRows({kind:'unidentified'}))['現在確認した通信'],/未識別/);
 const modern=Object.fromEntries(packageIdentityRows({family:'modern',firmware_version:'1.2.0',supported_protocols:[2,3]}));
 assert.equal(modern['本体Firmware版'],'1.2.0');assert.equal(modern['対応通信プロトコル'],'Protocol 2 / Protocol 3');
});
