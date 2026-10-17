// Real native IPC + shared Rust service using an explicit scripted transport.
// This never selects, opens, resets, or programs an OS serial device.
import {chromium} from 'playwright-core';
import {spawn} from 'node:child_process';
import {mkdirSync,mkdtempSync,writeFileSync,readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import net from 'node:net';
import assert from 'node:assert/strict';
import {waitForAppPage} from './native-session.mjs';
import {wizardPlan} from '../frontend/application-ui.mjs';
import {installCIPolicy,removeCIPolicy} from './ci-webview-policy.mjs';
const app=resolve(fileURLToPath(new URL('..',import.meta.url)));
const output=join(app,'src-tauri/target/native-qa/live');mkdirSync(output,{recursive:true});
const run=mkdtempSync(join(output,'run-'));mkdirSync(join(run,'temp'));
const report={scope:'native startup IPC, scripted LineTransport, real common fitter/store/runtime',hardware_acceptance:'Not Run',cases:[],screenshots:[],run};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const server=net.createServer();await new Promise(r=>server.listen(0,'127.0.0.1',r));const port=server.address().port;await new Promise(r=>server.close(r));
const exe=process.env.SELFRIONETTE_GUI_EXE||join(app,'src-tauri/target/debug/selfrionette-device-manager.exe');
report.executable_sha256=createHash('sha256').update(readFileSync(exe)).digest('hex');
let child,browser,page,policy=false;
try{
 const external=process.env.SELFRIONETTE_QA_CDP_URL;
 if(!external)policy=installCIPolicy(port,join(run,'webview'));
 if(!external){child=spawn(exe,['--developer-mode','--scripted-hardware'],{cwd:run,windowsHide:true,env:{...process.env,TEMP:join(run,'temp'),TMP:join(run,'temp'),SELFRIONETTE_GUI_FIXTURE_DIR:join(run,'store'),WEBVIEW2_USER_DATA_FOLDER:join(run,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`}});let launchError;child.on('error',e=>launchError=e);for(let i=0;i<150;i++){if(launchError)throw launchError;if(child.exitCode!==null)throw Error('Native startup exit '+child.exitCode);try{if((await fetch(`http://127.0.0.1:${port}/json/version`)).ok)break;}catch{}await sleep(200);}}
 browser=await chromium.connectOverCDP(external||`http://127.0.0.1:${port}`);page=await waitForAppPage(browser);
 const errors=[];page.on('pageerror',e=>errors.push(String(e)));
 report.application=await page.evaluate(()=>window.__TAURI__.core.invoke('application_info'));
 await page.waitForFunction(()=>document.getElementById('workspace').dataset.source==='hardware');
 const ipc=command=>page.evaluate(command=>window.__TAURI__.core.invoke('application_command',{command}),command);
 const status=async()=> (await ipc({action:'snapshot'})).view;
 const mode=await page.evaluate(()=>window.__TAURI__.core.invoke('mode_command',{request:{expected:null,command:{action:'snapshot'}}}));assert(mode.developer&&mode.scripted_hardware);
 const ports=await ipc({action:'ports'});assert.equal(ports.ports.length,1);assert.equal(ports.ports[0].name,'SCRIPTED');
 // A single GUI click must acknowledge its result. Never retry a persistent
 // write or schedule a later ready-capture to compensate for polling races.
 const act=async(id,action)=>{
  const prior=Number(await page.locator('#application-workflow').getAttribute('data-command-id')??0);
  await page.locator('#'+id).click({timeout:5000});
  await page.waitForFunction(({prior,action})=>{const d=document.getElementById('application-workflow').dataset;return Number(d.commandId)>prior&&(d.commandStatus==='error'||(d.commandAction===action&&d.commandStatus==='success'));},{prior,action},{timeout:5000});
  const outcome=await page.locator('#application-workflow').getAttribute('data-command-status');
  assert.equal(outcome,'success',action+': '+await page.locator('#live-error-detail').textContent());
 };
 await page.locator('#tab-adjust').click();await page.locator('#live-port').selectOption('0');await act('live-connect','stream_start');
 await page.waitForFunction(()=>document.getElementById('live-info').textContent.includes('unprovisioned'));
 await page.locator('#live-provision-confirm').check();await act('live-provision','provision');await page.waitForFunction(()=>document.getElementById('live-info').textContent.includes('srn-'));
 assert.match((await status()).info.device_id,/^srn-[0-9a-f]{32}$/);
 const reconnectIdentity=(await status()).info.device_id;
 await act('live-disconnect','disconnect');assert.equal((await status()).connected,false);
 assert(await page.locator('#live-reconnect').isEnabled());
 page.once('dialog',dialog=>dialog.accept());await act('live-reconnect','stream_start');
 assert.equal((await status()).info.device_id,reconnectIdentity);assert.equal((await status()).ready,false);
 report.cases.push('pause releases the scripted owner; GUI resume performs a new handshake, checks known ID and never restores ready');
 const setRaw=async(raw,flags=Array(7).fill(1))=>{
  const prior=await page.locator('#application-workflow').getAttribute('data-sample-sequence');
  await ipc({action:'fixture_input',raw,flags});
  await page.waitForFunction(({raw,flags,prior})=>{
   if(document.getElementById('application-workflow').dataset.sampleSequence===prior)return false;
   return [...document.getElementById('live-sample-rows').children].every((row,ch)=>Number(row.children[1].textContent.replaceAll(',',''))===raw[ch]&&(flags[ch]===1?row.children[3].textContent==='fresh':row.children[3].textContent!=='fresh'));
  },{raw,flags,prior},{timeout:5000});
 };
 const baseline=Array(7).fill(1_000_000),channels=Array.from({length:7},(_,wire)=>({wire,logical_axis:wire,location:'試験部位'+wire,direction:'試験の正方向'+wire}));
 for(let wire=0;wire<7;wire++){
  await page.locator('#live-axis-'+wire).selectOption(String(wire));await page.locator('#live-location-'+wire).fill(channels[wire].location);await page.locator('#live-direction-'+wire).fill(channels[wire].direction);
  await page.locator('#live-observe-channel').selectOption(String(wire));await setRaw(baseline);await page.locator('#live-observe-rest').click();
  const directed=baseline.slice();directed[wire]+=wire===0?-10_000:10_000;await setRaw(directed);await page.locator('#live-observe-direction').click();
  await page.locator('#live-confirm-'+wire).check();
 }
 assert.equal(await page.locator('#live-polarity-0').inputValue(),'-1');await act('live-setup-save','save_device_settings');
 await page.waitForFunction(()=>[...document.querySelectorAll('#live-info dt')].some(label=>label.textContent==='設定の保存世代'&&label.nextElementSibling?.textContent==='1'));
 await setRaw(baseline);await page.locator('#live-tare-confirm').check();await act('live-tare','tare');assert((await status()).info.tare_valid);
 await page.screenshot({path:join(run,'01-device-direction-tare.png'),fullPage:true});report.screenshots.push('01-device-direction-tare.png');report.cases.push('native startup, OS-RNG provisioning, all7 mapping, reverse sensor, settings readback, explicit tare');
 await page.locator('[data-live-page="calibrate"]').click();await page.locator('#live-operator').fill('scripted-operator');await page.locator('#live-posture').fill('scripted-seated');await page.locator('#live-grip').fill('scripted-grip');await act('live-context-save','set_operator');await page.locator('#live-reverse-safe').check();
 const plan=wizardPlan(channels);
 async function measure(raw,id){
  await setRaw(raw);await act(id,'capture_start');
  assert.equal(await page.locator('#application-workflow').getAttribute('data-capture-active'),'true','new capture must be acknowledged active');
  await page.waitForFunction(()=>document.getElementById('application-workflow').dataset.captureActive==='false',{},{timeout:12000});
  const c=(await status()).capture;assert(c.completed&&c.samples>=16,JSON.stringify(c));
  report.captures=(report.captures??0)+1;
 }
 for(const step of plan){const raw=baseline.slice();if(step.axis!==null&&['ramp','hold'].includes(step.phase))raw[step.axis]+=step.direction*(step.axis===0?-10_000:10_000);await measure(raw,'live-capture-start');}
 await act('live-fit','fit');await page.waitForFunction(()=>document.getElementById('live-fit-summary').textContent.includes('正方向の操作範囲'));
 const fitted=(await status()).fit;assert.deepEqual(fitted.positive_span_counts,Array(7).fill(10_000));assert.deepEqual(fitted.negative_span_counts,Array(7).fill(10_000));
 const heldout=[];for(let axis=0;axis<7;axis++)for(const kind of ['positive','negative'])heldout.push({kind,axis});heldout.push({kind:'weak',axis:0},{kind:'combined',axis:0});
 for(const {kind,axis}of heldout){await page.locator('#live-heldout-kind').selectOption(kind);await page.locator('#live-heldout-axis').selectOption(String(axis));for(let phase=0;phase<4;phase++){const raw=baseline.slice();if(phase===1||phase===2){raw[axis]+=(axis===0?-1:1)*(kind==='negative'?-10_000:kind==='weak'?2500:kind==='combined'?5000:10_000);if(kind==='combined')raw[1]+=5000;}await measure(raw,'live-heldout-start');}}
 await act('live-heldout-verify','verify_held_out');assert((await status()).heldout.passed);
 await page.screenshot({path:join(run,'02-personal-calibration-verified.png'),fullPage:true});report.screenshots.push('02-personal-calibration-verified.png');
 await page.locator('[data-live-page="profiles"]').click();await page.locator('#live-profile-confirm').check();await act('live-profile-save','save_calibration');const saved=(await status()).saved_path;assert(saved);
 await act('live-profile-list','list_calibrations');const profiles=(await status()).profiles;assert.equal(profiles.length,1);await page.locator('#live-profiles').selectOption(profiles[0].path);await act('live-profile-load','load_calibration');assert.equal((await status()).ready,false,'reload never restores ready or neutral');
 await measure(baseline,'live-neutral-start');await act('live-profile-apply','apply_calibration');
 await page.waitForFunction(()=>document.getElementById('application-workflow').dataset.ready==='true',{},{timeout:5000});assert((await status()).ready);
 await page.waitForFunction(()=>[...document.getElementById('live-normalized-rows').children].every(row=>row.children[1].textContent==='0'),{},{timeout:5000});
 assert.deepEqual((await status()).output.normalized,Array(7).fill(0),'runtime must first observe fresh neutral after apply');
 const input=baseline.slice();input[0]-=5000;await setRaw(input);
 await page.waitForFunction(()=>Math.abs(Number(document.getElementById('live-normalized-rows').children[0].children[1].textContent.replaceAll(',',''))-0.5)<0.01,{},{timeout:5000});
 assert(Math.abs((await status()).output.normalized[0]-0.5)<0.01,'reverse polarity must be corrected exactly once');
 await page.screenshot({path:join(run,'03-saved-runtime-input.png'),fullPage:true});report.screenshots.push('03-saved-runtime-input.png');report.cases.push('GUI measured positive/negative repeated trials, heldout direction/weak/combined/release, immutable profile save, current neutral and runtime');
 const verifiedProof=(await status()).heldout;
 await setRaw(input,[2,1,1,1,1,1,1]);const invalid=await status();assert.equal(invalid.ready,false);assert.equal(invalid.output,null);assert.equal(invalid.session_neutral_valid,false);assert.deepEqual(invalid.heldout,verifiedProof,'ordinary bad sample must preserve completed independently validated evidence');
 assert(await page.locator('#live-profile-apply').isDisabled());assert(await page.locator('#live-neutral-start').isEnabled());
 await setRaw(baseline);const freshOnly=await status();assert.equal(freshOnly.ready,false,'good data alone must not resume');assert.equal(freshOnly.output,null);assert.deepEqual(freshOnly.heldout,verifiedProof);
 await measure(baseline,'live-neutral-start');assert.equal((await status()).ready,false,'fresh neutral still needs explicit apply');await act('live-profile-apply','apply_calibration');
 report.cases.push('nonfresh runtime revokes output and neutral, preserves verified proof, and requires fresh neutral plus explicit apply without profile reload');
 assert.equal((await status()).ready,true);
 await page.locator('#tab-device').click();await act('hardware-disconnect','disconnect');
 assert.equal((await status()).connected,false);assert.equal((await status()).ready,false);
 page.once('dialog',dialog=>dialog.accept());await act('hardware-reconnect','stream_start');
 assert.equal((await status()).info.device_id,reconnectIdentity);assert.equal((await status()).ready,false);
 assert.equal((await status()).output,null,'pause/resume must not restore previous neutral or output');
 report.cases.push('input-page pause uses native handle ownership; resume retains ID and invalidates previously ready neutral');
 await page.locator('#tab-adjust').click();await page.locator('[data-live-page="setup"]').click();await act('live-disconnect','disconnect');assert.equal((await status()).connected,false);
 assert.deepEqual(errors,[]);report.result='PASS';
}catch(error){report.result='FAIL';report.error=String(error.stack??error);if(page){await page.screenshot({path:join(run,'failure.png'),fullPage:true}).catch(()=>{});report.ui=await page.locator('body').innerText().catch(()=>null);}throw error;}
finally{writeFileSync(join(run,'REPORT.json'),JSON.stringify(report,null,2)+'\n');await browser?.close().catch(()=>{});if(child&&child.exitCode===null){child.kill();await new Promise(r=>child.once('exit',r));}if(policy)removeCIPolicy();console.log(JSON.stringify(report));}
