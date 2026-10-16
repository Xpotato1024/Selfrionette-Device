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
 await page.locator('#tab-adjust').click();await page.locator('#live-port').selectOption('0');await page.locator('#live-connect').click();
 await page.waitForFunction(()=>document.getElementById('live-info').textContent.includes('unprovisioned'));
 await page.locator('#live-provision-confirm').check();await page.locator('#live-provision').click();await page.waitForFunction(()=>document.getElementById('live-info').textContent.includes('srn-'));
 assert.match((await status()).info.device_id,/^srn-[0-9a-f]{32}$/);
 const setRaw=async(raw,flags=Array(7).fill(1))=>{await ipc({action:'fixture_input',raw,flags});await sleep(150);};
 const baseline=Array(7).fill(1_000_000),channels=Array.from({length:7},(_,wire)=>({wire,logical_axis:wire,location:'試験部位'+wire,direction:'試験の正方向'+wire}));
 for(let wire=0;wire<7;wire++){
  await page.locator('#live-axis-'+wire).selectOption(String(wire));await page.locator('#live-location-'+wire).fill(channels[wire].location);await page.locator('#live-direction-'+wire).fill(channels[wire].direction);
  await page.locator('#live-observe-channel').selectOption(String(wire));await setRaw(baseline);await page.locator('#live-observe-rest').click();
  const directed=baseline.slice();directed[wire]+=wire===0?-10_000:10_000;await setRaw(directed);await page.locator('#live-observe-direction').click();
  await page.locator('#live-confirm-'+wire).check();
 }
 assert.equal(await page.locator('#live-polarity-0').inputValue(),'-1');await page.locator('#live-setup-save').click();
 await page.waitForFunction(()=>document.getElementById('live-info').textContent.includes('settings generation1'));
 await setRaw(baseline);await page.locator('#live-tare-confirm').check();await page.locator('#live-tare').click();
 for(let n=0;n<50;n++){if((await status()).info.tare_valid)break;await sleep(100);}assert((await status()).info.tare_valid);
 await page.screenshot({path:join(run,'01-device-direction-tare.png'),fullPage:true});report.screenshots.push('01-device-direction-tare.png');report.cases.push('native startup, OS-RNG provisioning, all7 mapping, reverse sensor, settings readback, explicit tare');
 await page.locator('[data-live-page="calibrate"]').click();await page.locator('#live-operator').fill('scripted-operator');await page.locator('#live-posture').fill('scripted-seated');await page.locator('#live-grip').fill('scripted-grip');await page.locator('#live-context-save').click();await page.locator('#live-reverse-safe').check();
 const plan=wizardPlan(channels);
 async function measure(raw,click){
  await setRaw(raw);let accepted=false;
  for(let attempt=0;attempt<8&&!accepted;attempt++){
   await click();await sleep(70);
   const state=await status();accepted=!!state.capture?.active;
   if(!accepted){const detail=await page.locator('#live-error-detail').textContent();if(!detail?.includes('処理中'))throw Error('Capture did not start: '+JSON.stringify({detail,capture:state.capture,instruction:await page.locator('#live-instruction').textContent()}));await sleep(80);}
  }
  assert(accepted,'Capture never acknowledged after explicit busy retry');
  await page.waitForFunction(()=>document.getElementById('live-capture-progress').textContent.includes('取得終了'),{},{timeout:15000});
  const c=(await status()).capture;assert(c.completed&&c.samples>=16,JSON.stringify(c));
 }
 for(const step of plan){const raw=baseline.slice();if(step.axis!==null&&['ramp','hold'].includes(step.phase))raw[step.axis]+=step.direction*(step.axis===0?-10_000:10_000);await measure(raw,()=>page.locator('#live-capture-start').click());}
 await page.locator('#live-fit').click();await page.waitForFunction(()=>document.getElementById('live-fit-summary').textContent.includes('正方向span'));
 const fitted=(await status()).fit;assert.deepEqual(fitted.positive_span_counts,Array(7).fill(10_000));assert.deepEqual(fitted.negative_span_counts,Array(7).fill(10_000));
 const heldout=[];for(let axis=0;axis<7;axis++)for(const kind of ['positive','negative'])heldout.push({kind,axis});heldout.push({kind:'weak',axis:0},{kind:'combined',axis:0});
 for(const {kind,axis}of heldout){await page.locator('#live-heldout-kind').selectOption(kind);await page.locator('#live-heldout-axis').selectOption(String(axis));for(let phase=0;phase<4;phase++){const raw=baseline.slice();if(phase===1||phase===2){raw[axis]+=(axis===0?-1:1)*(kind==='negative'?-10_000:kind==='weak'?2500:kind==='combined'?5000:10_000);if(kind==='combined')raw[1]+=5000;}await measure(raw,()=>page.locator('#live-heldout-start').click());}}
 await page.locator('#live-heldout-verify').click();for(let n=0;n<20;n++){if((await status()).heldout.passed)break;await sleep(100);}assert((await status()).heldout.passed);
 await page.screenshot({path:join(run,'02-personal-calibration-verified.png'),fullPage:true});report.screenshots.push('02-personal-calibration-verified.png');
 await page.locator('[data-live-page="profiles"]').click();await page.locator('#live-profile-confirm').check();await page.locator('#live-profile-save').click();for(let n=0;n<20;n++){if((await status()).saved_path)break;await sleep(100);}assert((await status()).saved_path);
 await measure(baseline,()=>page.locator('#live-neutral-start').click());await page.locator('#live-profile-apply').click();for(let n=0;n<30;n++){if((await status()).ready)break;await sleep(100);}assert((await status()).ready);
 const input=baseline.slice();input[0]-=5000;await setRaw(input);assert(Math.abs((await status()).output.normalized[0]-0.5)<0.01,'reverse polarity must be corrected exactly once');
 await page.screenshot({path:join(run,'03-saved-runtime-input.png'),fullPage:true});report.screenshots.push('03-saved-runtime-input.png');report.cases.push('GUI measured positive/negative repeated trials, heldout direction/weak/combined/release, immutable profile save, current neutral and runtime');
 await setRaw(input,[2,1,1,1,1,1,1]);assert.equal((await status()).ready,false);assert.equal((await status()).output,null);report.cases.push('nonfresh quality invalidates current runtime');
 await page.locator('[data-live-page="setup"]').click();await page.locator('#live-disconnect').click();assert.equal((await status()).connected,false);
 assert.deepEqual(errors,[]);report.result='PASS';
}catch(error){report.result='FAIL';report.error=String(error.stack??error);if(page){await page.screenshot({path:join(run,'failure.png'),fullPage:true}).catch(()=>{});report.ui=await page.locator('body').innerText().catch(()=>null);}throw error;}
finally{writeFileSync(join(run,'REPORT.json'),JSON.stringify(report,null,2)+'\n');await browser?.close().catch(()=>{});if(child&&child.exitCode===null){child.kill();await new Promise(r=>child.once('exit',r));}if(policy)removeCIPolicy();console.log(JSON.stringify(report));}
