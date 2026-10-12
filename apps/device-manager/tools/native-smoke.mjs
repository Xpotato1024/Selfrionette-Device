// Windows-only native Tauri IPC smoke test. Synthetic records; no device I/O.
import {chromium} from 'playwright-core';
import {spawn,spawnSync} from 'node:child_process';
import {mkdirSync,mkdtempSync,writeFileSync,readdirSync,readFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import net from 'node:net';
import assert from 'node:assert/strict';
if(process.platform!=='win32')throw Error('Native smoke requires Windows');
const appRoot=resolve(fileURLToPath(new URL('..',import.meta.url)));
const output=join(appRoot,'src-tauri','target','native-qa');mkdirSync(output,{recursive:true});
const run=mkdtempSync(join(output,'run-')),store=join(run,'synthetic-store');
const exe=process.env.SELFRIONETTE_GUI_EXE||join(appRoot,'src-tauri','target','debug','selfrionette-device-manager.exe');
const report={provenance:'synthetic native Tauri IPC',hardware_verified:false,control_ready:false,run,exe,cases:[]};
let active;
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function launch(){
 const port=await freePort();
 const child=spawn(exe,[],{windowsHide:true,env:{...process.env,SELFRIONETTE_GUI_FIXTURE_DIR:store,WEBVIEW2_USER_DATA_FOLDER:join(run,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port='+port+' --remote-debugging-address=127.0.0.1'}});
 let diagnostic='';child.stderr.on('data',v=>{diagnostic=(diagnostic+v).slice(-8000)});child.stdout.on('data',v=>{diagnostic=(diagnostic+v).slice(-8000)});
 active={child,port};
 for(let i=0;i<100;i++){
  if(child.exitCode!==null)throw Error('Native exit '+child.exitCode+': '+diagnostic);
  try{const r=await fetch('http://127.0.0.1:'+port+'/json/version');if(r.ok)break;}catch{}
  await sleep(200);
 }
 const browser=await chromium.connectOverCDP('http://127.0.0.1:'+port);
 active.browser=browser;
 const page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost'))||browser.contexts()[0].pages()[0];
 await page.waitForFunction(()=>typeof window.__TAURI__?.core?.invoke==='function');
 assert.match(page.url(),/tauri.localhost/);
 const call=(action,argument='')=>page.evaluate(({action,argument})=>window.__TAURI__.core.invoke('fixture_action',{action,argument}),{action,argument});
 return {page,call};
}
async function close(){
 if(!active)return;
 const {child,browser}=active;
 if(child.exitCode===null){
  const r=spawnSync('powershell.exe',['-NoProfile','-Command','(Get-Process -Id '+child.pid+').CloseMainWindow()'],{windowsHide:true,encoding:'utf8',timeout:5000});
  assert.match(r.stdout,/True/,'Owned native window did not accept graceful close');
  for(let i=0;i<50&&child.exitCode===null;i++)await sleep(100);
  assert.notEqual(child.exitCode,null,'Owned native process did not exit; no forced termination');
  assert.equal(child.exitCode,0,'Native app crashed during close');
 }
 await browser?.close().catch(()=>{});
 active=null;
}
async function rejected(call,action,arg=''){await assert.rejects(()=>call(action,arg));}
async function qualified(call){await call('begin','left');await call('baseline');await call('assess','good');return call('preview','0.5');}
async function approved(call){await qualified(call);await call('confirm');}
async function noOldApproval(call){const v=await call('status');assert.equal(v.rows.length,0);assert(v.stage<4);await rejected(call,'export');await rejected(call,'save');}
try{
 let {page,call}=await launch();
 assert.equal((await call('status')).stage,0);
 assert.match(await page.locator('#environment').innerText(),/Rust core/);
 for(const id of ['begin','baseline','assess','preview'])await page.locator('#'+id).click();
 await page.waitForFunction(()=>document.querySelectorAll('#rows tr').length===7);
 assert.equal(await page.locator('#rows tr').count(),7);
 let v=await call('status');assert.equal(v.hardware_verified,false);assert.equal(v.control_ready,false);
 assert.deepEqual(v.rows.map(r=>r.normalized),Array(7).fill(0.5));
 const original=v.rows.map(r=>r.firmware);
 assert.deepEqual(v.rows.map(r=>r.wire),[0,1,2,3,4,5,6]);
 await page.screenshot({path:join(run,'native-preview.png'),fullPage:true});
 await page.locator('#confirm').click();
 await page.waitForFunction(()=>!document.getElementById('save').disabled);
 const exported=JSON.parse((await call('export')).export);
 assert.deepEqual(exported.historical_observation.firmware_sample,original);
 assert.deepEqual(exported.historical_observation.normalized,Array(7).fill(0.5));
 assert.equal(exported.hardware_verified,false);assert.equal(exported.control_ready,false);
 await page.locator('#save').click();await page.locator('#load').click();
 await page.waitForFunction(()=>document.querySelectorAll('#rows tr').length===0);
 assert.equal(await page.locator('#rows tr').count(),0);
 assert.equal(await page.locator('#preview').isDisabled(),true);
 const saved=JSON.parse(await page.locator('#saved-profile').innerText());
 assert.deepEqual(saved.historical_observation.firmware_sample,original);
 assert.equal(saved.operation.active_baseline_restored,false);
 await rejected(call,'preview','0.5');await rejected(call,'export');
 report.cases.push('native GUI/IPC 7ch, original vs corrected export, disk save/load and fresh-baseline gate');
 for(const [action,arg] of [['preview','NaN'],['preview','bad'],['preview','3'],['assess','unknown'],['flip','bad'],['flip','0:'],['flip','99:reason'],['begin','other']]){
  await approved(call);await rejected(call,action,arg);await noOldApproval(call);
 }
 await approved(call);await rejected(call,'assess','unknown');
 await rejected(call,'flip','0:synthetic reason');await rejected(call,'preview','0.5');await noOldApproval(call);
 report.cases.push('invalid mutation requests revoke old confirmation/save/export and cannot reuse old candidates');
 for(const scenario of ['noise','disagreement','unstable','release','crosstalk','timeout','direction']){
  await approved(call);await rejected(call,'assess',scenario);await noOldApproval(call);
 }
 report.cases.push('7 inconclusive quality/direction scenarios revoke old confirmation');
 await qualified(call);v=await call('preview','1.5');
 assert.deepEqual(v.rows.map(r=>r.normalized),Array(7).fill(1.5));
 assert.deepEqual(v.rows.map(r=>r.displayed),Array(7).fill(1));assert(v.rows.every(r=>r.saturated));
 await rejected(call,'confirm');await rejected(call,'save');
 v=await call('preview','-0.5');assert(v.rows.every(r=>r.normalized===null));
 report.cases.push('unclamped saturation and unknown reverse span');
 await approved(call);await call('invalidate');await noOldApproval(call);await rejected(call,'preview','0.5');
 await call('begin','right');await rejected(call,'load');await noOldApproval(call);
 await call('baseline');await call('assess','good');await call('preview','0.5');await call('confirm');await call('save');
 await call('begin','left');v=await call('load');
 assert.equal(v.id,'srn-11111111111111111111111111111111');assert.deepEqual(JSON.parse(v.saved_profile).historical_observation.firmware_sample,original);
 report.cases.push('local reset/tare invalidation, separate left/right synthetic IDs');
 for(const id of ['begin','baseline','assess','preview','confirm'])await page.locator('#'+id).click();
 await page.waitForFunction(()=>!document.getElementById('save').disabled);
 await page.locator('#level').evaluate(el=>el.add(new Option('invalid fixture','NaN')));
 await page.locator('#level').selectOption('NaN');await page.locator('#preview').click();
 await page.waitForFunction(()=>document.getElementById('message').textContent.includes('判定不可'));
 assert.equal(await page.locator('#rows tr').count(),0);assert(await page.locator('#save').isDisabled());
 report.cases.push('frontend error renders current Rust state and clears stale rows');
 report.records=readdirSync(store).map(id=>({id,files:readdirSync(join(store,id))}));
 assert.equal(report.records.length,2);assert(report.records.every(r=>r.files.length===1));
 for(const record of report.records)assert.match(readFileSync(join(store,record.id,record.files[0]),'utf8'),/hardware_verified,0\ncontrol_ready,0/);
 await close();
 ({page,call}=await launch());
 assert.equal((await call('status')).stage,0);await call('begin','left');v=await call('load');
 assert.deepEqual(JSON.parse(v.saved_profile).historical_observation.firmware_sample,original);
 assert.equal(v.stage,1);assert.equal(v.rows.length,0);await rejected(call,'preview','0.5');await rejected(call,'export');
 report.cases.push('new native process reload preserves evidence but restores no readiness');
 await close();
 report.result='PASS';writeFileSync(join(run,'report.json'),JSON.stringify(report,null,2));
 console.log('Native Tauri smoke PASS: '+report.cases.length+' groups; report '+join(run,'report.json'));
}catch(e){report.result='FAIL';report.error=String(e);writeFileSync(join(run,'report.json'),JSON.stringify(report,null,2));throw e;}
finally{await close();}
