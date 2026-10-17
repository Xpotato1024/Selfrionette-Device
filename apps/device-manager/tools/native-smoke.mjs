import {nativeMenu,enableSynthetic} from './native-menu.mjs';
import {waitForAppPage} from './native-session.mjs';
import {installCIPolicy,removeCIPolicy} from './ci-webview-policy.mjs';
import {runtimeEnvironment} from './distribution-lib.mjs';
// Windows-only native Tauri IPC smoke test. Synthetic records; no device I/O.
import {chromium} from 'playwright-core';
import {checkUI} from './ui-qa.mjs';
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
async function launch(startupArgs=[],onlyMode=false){
 const port=await freePort();
 const policy=installCIPolicy(port,join(run,'webview'));
 const runtimeOnly=process.env.SELFRIONETTE_GUI_RUNTIME_ONLY==='1';
 const child=spawn(exe,startupArgs,{cwd:run,windowsHide:true,env:{...(runtimeOnly?runtimeEnvironment(process.env):process.env),SELFRIONETTE_GUI_FIXTURE_DIR:store,WEBVIEW2_USER_DATA_FOLDER:join(run,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port='+port+' --remote-debugging-address=127.0.0.1'}});
 let diagnostic='';child.stderr.on('data',v=>{diagnostic=(diagnostic+v).slice(-8000)});child.stdout.on('data',v=>{diagnostic=(diagnostic+v).slice(-8000)});
 active={child,port,policy};
 let ready=false;
 for(let i=0;i<100;i++){
  if(child.exitCode!==null)throw Error('Native exit '+child.exitCode+': '+diagnostic);
  try{const r=await fetch('http://127.0.0.1:'+port+'/json/version');if(r.ok){ready=true;break;}}catch{}
  await sleep(200);
 }
 if(!ready)throw Error('WebView2 CDP did not become ready within 20 seconds. '+diagnostic);
 const browser=await chromium.connectOverCDP('http://127.0.0.1:'+port);
 active.browser=browser;
 const page=await waitForAppPage(browser,{check:()=>{if(child.exitCode!==null)throw Error('Native exit '+child.exitCode+': '+diagnostic);}});
 await page.waitForFunction(()=>typeof window.__TAURI__?.core?.invoke==='function');
 await page.waitForFunction(source=>document.getElementById('workspace').dataset.source===source,startupArgs.includes('--synthetic')?'synthetic':'hardware');
 assert.equal(await page.locator('#source-mode').count(),0);
 const mode=await page.evaluate(()=>window.__TAURI__.core.invoke('mode_command',{request:{expected:null,command:{action:'snapshot'}}}));
 assert.equal(mode.developer,startupArgs.includes('--developer-mode'));assert.equal(mode.source,startupArgs.includes('--synthetic')?'synthetic':'hardware');
 if(mode.source==='hardware')await assert.rejects(()=>page.evaluate(()=>window.__TAURI__.core.invoke('device_command',{request:{expected:null,command:{action:'status'}}})));
 else assert((await page.evaluate(()=>window.__TAURI__.core.invoke('device_command',{request:{expected:null,command:{action:'status'}}}))).view);
 const menu=action=>nativeMenu(child.pid,action);report.native_menu=menu();
 assert.equal(report.native_menu.some(item=>item.Text==='合成試験'),mode.developer,'Native developer menu visibility differs from Rust gate');
 if(onlyMode)return {page,mode,menu};
 nativeMenu(child.pid,undefined,join(run,'native-window-normal.png'));
 await menu('choose-device');await page.waitForFunction(()=>document.getElementById('hardware-connect-dialog').open);
 await page.screenshot({path:join(run,'native-port-picker.png')});await page.locator('#hardware-connect-cancel').click();
 const hardwareStatus=await page.evaluate(()=>window.__TAURI__.core.invoke('hardware_command',{request:{expected:null,command:{action:'snapshot'}}}));
 assert.equal(hardwareStatus.view.connected,false);assert.equal(hardwareStatus.view.received_frames,0);assert.equal(hardwareStatus.view.sample,null);
 await page.screenshot({path:join(run,'native-hardware.png'),fullPage:true});
 const hardwareSize=await page.evaluate(()=>({width:innerWidth,height:innerHeight}));
 for(const width of [800,1100,1600]) {
   await page.setViewportSize({width,height:680});
   for(const name of ['device','adjust','diagnostics']) {
     await page.locator('#tab-'+name).click();
     assert.equal(await page.locator('#tab-'+name).getAttribute('aria-controls'),'hardware-page-'+name);
     assert(await page.locator('#hardware-page-'+name).isVisible());
     assert.equal(await page.locator('[data-command]:visible').count(),0,'Real mode must not offer synthetic calibration commands');
     const layout=await page.evaluate(()=>({width:innerWidth,scrollWidth:document.documentElement.scrollWidth,overflow:[...document.querySelectorAll('body *')].filter(e=>e.getClientRects().length&&e.getBoundingClientRect().right>innerWidth+1).slice(0,15).map(e=>({id:e.id,tag:e.tagName,right:e.getBoundingClientRect().right,width:e.getBoundingClientRect().width}))}));
     if(layout.scrollWidth>layout.width+1)await page.screenshot({path:join(run,'hardware-overflow.png'),fullPage:true});
     assert(layout.scrollWidth<=layout.width+1,'Hardware page overflow '+name+': '+JSON.stringify(layout));
   }
 }
 await page.setViewportSize(hardwareSize);await page.locator('#tab-device').click();
 await enableSynthetic(page,menu);
 nativeMenu(child.pid,undefined,join(run,'native-window-developer.png'));
 assert.match(page.url(),/tauri.localhost/);
 const invoke=request=>page.evaluate(request=>window.__TAURI__.core.invoke('device_command',{request}),request);
 const raw=async(action, fields={})=>{
   const status=await invoke({expected:null,command:{action:'status'}});
   return invoke({expected:status.view.token,command:{action,...fields}});
 };
 const call=async(action,fields={})=>{
   const reply=await raw(action,fields);
   if(reply.error)throw Error(JSON.stringify(reply.error));
   return {...reply.view,export:reply.export};
 };
 return {page,call,raw,invoke,menu};
}
async function close(){
 if(!active)return;
 const {child,browser,policy}=active;
 try {
 if(child.exitCode===null){
  const r=spawnSync('powershell.exe',['-NoProfile','-Command','(Get-Process -Id '+child.pid+').CloseMainWindow()'],{windowsHide:true,encoding:'utf8',timeout:5000});
  assert.match(r.stdout,/True/,'Owned native window did not accept graceful close');
  for(let i=0;i<50&&child.exitCode===null;i++)await sleep(100);
  assert.notEqual(child.exitCode,null,'Owned native process did not exit; no forced termination');
  assert.equal(child.exitCode,0,'Native app crashed during close');
 }
 await browser?.close().catch(()=>{});
 } finally { if(policy)removeCIPolicy(); active=null; }
}

async function rejected(call,action,fields={}){await assert.rejects(()=>call(action,fields));}
async function qualified(call){await call('select',{role:'left'});await call('begin');await call('baseline');await call('assess',{scenario:'good'});return call('preview',{level:'0.5'});}
async function approved(call){await qualified(call);await call('confirm');}
async function noOldApproval(call){const v=await call('status');assert.equal(v.rows.length,0);assert.notEqual(v.phase,'record');await rejected(call,'export');await rejected(call,'save');}
async function syncUI(page){await page.reload();await page.waitForFunction(()=>document.getElementById('workspace').dataset.source==='synthetic'&&!document.getElementById('role').disabled);}
try {
 let {page,call,raw,invoke,menu}=await launch();
 assert.equal((await call('status')).phase,'not_started');
 assert.equal(await page.locator('#baseline-help').textContent(),'','A hidden baseline action must not repeat the begin instruction');
 assert.match(await page.locator('#environment').innerText(),/合成試験.*実機未接続/);
 await page.locator('nav [data-page="adjust"]').click();
 for(const id of ['begin','baseline','assess','preview'])await page.locator('#'+id).click();
 await page.waitForFunction(()=>document.querySelectorAll('#rows tr').length===7);
 let v=await call('status');assert.equal(v.hardware_verified,false);assert.equal(v.control_ready,false);
 assert.deepEqual(v.rows.map(r=>r.normalized),Array(7).fill(0.5));
 assert.deepEqual(v.rows.map(r=>r.wire),[0,1,2,3,4,5,6]);assert(v.rows.every(r=>r.physical===null));
 const original=v.rows.map(r=>r.firmware);
 await page.screenshot({path:join(run,'native-adjust.png'),fullPage:true});
 await page.locator('nav [data-page="device"]').click();await page.screenshot({path:join(run,'native-device.png'),fullPage:true});
 await page.locator('nav [data-page="adjust"]').click();await page.locator('#confirm').click();
 await page.waitForFunction(()=>!document.getElementById('save').disabled);
 const exported=JSON.parse((await call('export')).export);
 assert.deepEqual(exported.historical_observation.firmware_sample,original);
 assert.deepEqual(exported.historical_observation.normalized,Array(7).fill(0.5));
 assert.equal(exported.hardware_verified,false);assert.equal(exported.control_ready,false);
 // The direct export advanced the request revision: deliberately synchronize the client.
 await syncUI(page);await page.locator('nav [data-page="adjust"]').click();await page.locator('#save').click();
 await page.waitForFunction(()=>document.getElementById('record-state').textContent.includes('保存済み'));
 const before=await call('status');
 await page.locator('nav [data-page="diagnostics"]').click();await page.locator('#record-controls > summary').click();await page.locator('#read-history').click();
 await page.waitForFunction(()=>document.getElementById('history-summary').textContent.includes('表示中'));
 v=await call('status');assert.equal(v.epoch,before.epoch);assert.equal(v.core_revision,before.core_revision);assert.equal(v.phase,'record');assert(v.saved_current);
 assert.equal(v.rows.length,7);assert.deepEqual(v.history.historical_observation.firmware_sample,original);
 assert.equal(v.history.operation.active_baseline_restored,false);
 await page.screenshot({path:join(run,'native-history.png'),fullPage:true});
 await page.locator('#resume').click();
 assert(await page.locator('#confirm-dialog').isVisible());await page.locator('#dialog-cancel').click();
 assert.equal((await call('status')).epoch,before.epoch);
 await page.locator('#resume').click();await page.locator('#dialog-apply').click();
 await page.waitForFunction(()=>!document.getElementById('baseline').disabled&&document.querySelectorAll('#rows tr').length===0);
 v=await call('status');assert.equal(v.epoch,before.epoch+1);assert.equal(v.phase,'baseline');
 await rejected(call,'preview',{level:'0.5'});await rejected(call,'export');
 report.cases.push('native GUI/IPC, original/corrected export, read-only history, explicit resume/cancel and fresh-baseline gate');
 for(const [action,fields] of [['preview',{level:'NaN'}],['preview',{level:'bad'}],['preview',{level:'3'}],['assess',{scenario:'unknown'}],['flip',{channel:0,reason:''}],['flip',{channel:99,reason:'reason'}]]){
  await approved(call);await rejected(call,action,fields);await noOldApproval(call);
 }
 // A failed reassessment specifically clears previously qualified candidates.
 await approved(call);await rejected(call,'assess',{scenario:'unknown'});
 await rejected(call,'flip',{channel:0,reason:'synthetic reason'});await rejected(call,'preview',{level:'0.5'});await noOldApproval(call);
 report.cases.push('invalid mutation values revoke old confirmation; failed assessment cannot recover old candidates');
 for(const scenario of ['noise','disagreement','unstable','release','crosstalk','timeout','direction']){
  await approved(call);await rejected(call,'assess',{scenario});await noOldApproval(call);
 }
 report.cases.push('all 7 inconclusive quality/direction scenarios revoke old approval');
 await qualified(call);v=await call('preview',{level:'1.5'});
 assert.deepEqual(v.rows.map(r=>r.normalized),Array(7).fill(1.5));assert.deepEqual(v.rows.map(r=>r.displayed),Array(7).fill(1));assert(v.rows.every(r=>r.saturated));
 assert(!v.operations.find(o=>o.id==='confirm').enabled);
 await syncUI(page);await page.locator('nav [data-page="adjust"]').click();
 assert(await page.locator('#back-to-comparison').isVisible());assert(await page.locator('#confirm').isDisabled());
 assert.equal(await page.locator('#page-adjust button.primary:visible').getAttribute('id'),'back-to-comparison');
 assert.equal(await page.locator('.step-panel:visible').count(),1);
 await page.screenshot({path:join(run,'native-saturation.png'),fullPage:true});
 await rejected(call,'confirm');await rejected(call,'save');
 v=await call('preview',{level:'-0.5'});assert(v.rows.every(r=>r.normalized===null&&r.displayed===null));
 report.cases.push('unclamped display saturation and explicitly unmeasured reverse span');
 await approved(call);const left=await call('status');
 await call('select',{role:'right'});assert.equal((await call('status')).phase,'not_started');
 let result=await invoke({expected:left.token,command:{action:'invalidate'}});
 assert.equal(result.error.code,'stale_target');assert.equal(result.view.phase,'not_started');
 await call('begin');await call('baseline');await call('assess',{scenario:'good'});await call('preview',{level:'0.5'});await call('confirm');await call('save');
 const right=await call('status');await call('select',{role:'left'});v=await call('status');
 assert.equal(v.phase,'record');assert.equal(v.epoch,left.epoch);assert.equal(v.core_revision,left.core_revision);
 await call('select',{role:'right'});assert.equal((await call('status')).epoch,right.epoch);
 await call('select',{role:'left'});const token=(await call('status')).token;
 result=await invoke({expected:token,command:{action:'invalidate'}});assert.equal(result.error,null);
 result=await invoke({expected:token,command:{action:'begin'}});assert.equal(result.error.code,'stale_target');
 report.cases.push('independent left/right sessions, selection without reset, stale-target and repeated-request rejection');
 await qualified(call);await syncUI(page);await page.locator('nav [data-page="adjust"]').click();
 await page.locator('#previous-step').click();
 await page.locator('#level').evaluate(el=>el.add(new Option('invalid fixture','NaN')));await page.locator('#level').selectOption('NaN');await page.locator('#preview').click();
 await page.waitForFunction(()=>!document.getElementById('error-panel').hidden);
 assert.equal(await page.locator('#rows tr').count(),0);assert(await page.locator('#save').isDisabled());
 report.cases.push('frontend failure renders current Rust state and clears stale values');
 await approved(call);
 await call('select',{role:'right'});await rejected(call,'resume'); // No viewed record: explicit mutation still invalidates.
 await call('select',{role:'left'});await call('read_history');
 const dir=join(store,'srn-22222222222222222222222222222222');
 const damaged=join(dir,readdirSync(dir).find(n=>n.endsWith('.srnp')));writeFileSync(damaged,'corrupt synthetic test record');
 await call('select',{role:'right'});await call('begin');await call('baseline');await call('assess',{scenario:'good'});await call('preview',{level:'0.5'});await call('confirm');
 const beforeFailure=await call('status');await rejected(call,'read_history');v=await call('status');
 assert.equal(v.phase,'record');assert.equal(v.epoch,beforeFailure.epoch);assert.equal(v.core_revision,beforeFailure.core_revision);assert.equal(v.history,null);
 report.cases.push('corrupt history does not modify active preparation or restore an older history view');
 await checkUI(page,run,report,call,menu);
 report.records=readdirSync(store).map(id=>({id,files:readdirSync(join(store,id))}));assert.equal(report.records.length,2);
 assert(report.records.every(r=>r.files.length===1));
 await close();({page,call,raw,invoke,menu}=await launch());
 assert.equal((await call('status')).phase,'not_started');v=await call('read_history');
 assert.deepEqual(v.history.historical_observation.firmware_sample,original);assert.equal(v.phase,'not_started');assert.equal(v.epoch,0);assert.equal(v.rows.length,0);
 await rejected(call,'preview',{level:'0.5'});await rejected(call,'export');
 await assert.rejects(()=>invoke({expected:null,command:{action:'status',arbitrary:true}}));
 report.cases.push('new process reads evidence before session start without restoring readiness; strict request shape');
 await menu('disable-developer');await page.waitForFunction(()=>document.getElementById('workspace').dataset.developer==='false');
 await assert.rejects(()=>invoke({expected:null,command:{action:'status'}}));
 await close();report.startup_modes=[];
 for(const args of [['--developer-mode'],['--developer-mode','--synthetic'],[]]){
   const initial=await launch(args,true);report.startup_modes.push({args,mode:initial.mode});await close();
 }
 const invalid=spawnSync(exe,['--synthetic'],{cwd:run,env:runtimeEnvironment(process.env),timeout:10000,encoding:'utf8'});
 assert.equal(invalid.status,2,'Synthetic CLI flag without developer-mode must be rejected');
 report.cases.push('native menus; ordinary mode rejects synthetic IPC; explicit GUI/CLI gates; disable and restart reset; invalid CLI rejected');
 report.result='PASS';writeFileSync(join(run,'report.json'),JSON.stringify(report,null,2));
 console.log('Native Tauri smoke PASS: '+report.cases.length+' groups; report '+join(run,'report.json'));
}catch(e){report.result='FAIL';report.error=String(e);writeFileSync(join(run,'report.json'),JSON.stringify(report,null,2));throw e;}
finally{await close();}
