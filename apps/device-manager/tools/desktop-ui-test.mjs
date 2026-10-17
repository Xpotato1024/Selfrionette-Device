import {installDesktopFixture} from './desktop-fixture.mjs';
import {verifyUsability} from './usability-ui.mjs';
// Production HTML/CSS/JS with a test-only in-memory IPC fixture. Never opens a device.
import {chromium} from 'playwright-core';import {createServer} from 'node:http';import {readFileSync,existsSync,mkdirSync,mkdtempSync,writeFileSync} from 'node:fs';import {resolve,join,extname} from 'node:path';import {fileURLToPath} from 'node:url';import assert from 'node:assert/strict';
const app=resolve(fileURLToPath(new URL('..',import.meta.url))),dir=join(app,'frontend');
const output=join(app,'src-tauri/target/desktop-qa');mkdirSync(output,{recursive:true});const run=mkdtempSync(join(output,'run-'));
const server=createServer((req,res)=>{try{const name=new URL(req.url,'http://localhost').pathname.slice(1)||'index.html';if(!/^[a-zA-Z0-9_.-]+$/.test(name))throw Error('path');res.setHeader('Content-Type',({'.html':'text/html','.mjs':'text/javascript','.css':'text/css'})[extname(name)]||'text/plain');res.end(readFileSync(join(dir,name)));}catch{res.writeHead(404);res.end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const candidates=[process.env.SELFRIONETTE_UI_BROWSER_EXE,((process.env.SystemDrive||'C:')+'/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'),((process.env.SystemDrive||'C:')+'/Program Files/Microsoft/Edge/Application/msedge.exe')].filter(Boolean);const browserPath=candidates.find(p=>existsSync(p));if(!browserPath){server.close();throw Error('No installed Edge. Set SELFRIONETTE_UI_BROWSER_EXE to a verified Chromium browser.');}const browser=process.env.SELFRIONETTE_UI_CDP_URL?await chromium.connectOverCDP(process.env.SELFRIONETTE_UI_CDP_URL):await chromium.launch({executablePath:browserPath,headless:true});const report={scope:'production UI / in-memory fixture, no device I/O',cases:[],limitations:['deviceScaleFactor is not a real Windows DPI transition']};
try {
 for(const scale of [1,1.5,2]){
  const context=await browser.newContext({viewport:{width:1100,height:720},deviceScaleFactor:scale});const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await page.addInitScript(installDesktopFixture);
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.querySelectorAll('.readout-integer')[0]?.textContent==='2,207');
  await page.evaluate(()=>{window.originalRows=[...document.querySelectorAll('#hardware-value-rows tr')];});
  for(const width of [800,1100,1600]){
   await page.setViewportSize({width,height:width===800?520:720});
   const metric=await page.evaluate(()=>({bodyOverflow:document.documentElement.scrollWidth-innerWidth,table:document.getElementById('hardware-values').getBoundingClientRect().width,pane:document.querySelector('.readings-pane').getBoundingClientRect().width}));assert(metric.bodyOverflow<=1);assert.equal(Math.round(metric.pane),352);
   const anchor=await page.locator('.readout-fraction').first().evaluate(e=>e.getBoundingClientRect().x);
   for(const value of [-123456.78,0,99999999.99,1e100,-0.001,133411.67]){
    await page.evaluate(value=>{window.testFeed.sample.values.fill(value);},value);
    await page.waitForFunction(value=>document.querySelector('.readout').title===String(value),value);
    assert.equal(await page.locator('.readout-fraction').first().evaluate(e=>e.getBoundingClientRect().x),anchor,'Decimal anchor moved');
    assert(await page.evaluate(()=>window.originalRows.every((r,i)=>r===document.querySelectorAll('#hardware-value-rows tr')[i])),'Rows recreated');
   }
   report.cases.push({width,scale,...metric});
  }
  await page.setViewportSize({width:1100,height:720});await page.reload();await page.waitForFunction(()=>document.querySelector('.readout')?.title==='2207');await page.evaluate(()=>{window.testFeed.sample.values=[1679,178.4,1973.33,133411.67,-13945.6,42511.26,31.27];});await page.waitForFunction(()=>document.querySelector('.readout').title==='1679');
  await page.screenshot({path:join(run,'monitor-'+scale+'.png')});
  const before=await page.locator('#hardware-values').boundingBox();
  await page.evaluate(()=>{window.testFeed.sample=null;window.testFeed.phase='stale';});await page.waitForFunction(()=>document.querySelector('.readout-integer').textContent==='—');
  assert.deepEqual(await page.locator('#hardware-values').boundingBox(),before,'Stale state moved readout table');
  assert.deepEqual(errors,[]);await context.close();
 }

 const context=await browser.newContext({viewport:{width:1100,height:720}}),page=await context.newPage();
 const errors=[];page.on('pageerror',e=>errors.push(String(e)));await page.addInitScript(installDesktopFixture,{connected:false});
 await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.getElementById('workspace').dataset.source==='hardware');
 assert.equal(await page.locator('#source-mode').count(),0);assert(!((await page.locator('body').innerText()).includes('合成試験')));
 assert(await page.evaluate(()=>!window.fixtureRequests.some(r=>r.name==='device_command')),'Normal startup invoked synthetic commands');
 await page.locator('#hardware-choose').click();await page.waitForFunction(()=>document.querySelectorAll('#port-rows tr').length===2);
 assert.deepEqual(await page.locator('#port-rows tr').evaluateAll(rows=>rows.map(r=>r.dataset.name)),['COM8','COM9']);
 await page.locator('#port-option-0').check();assert(!await page.locator('#query-on-connect').isChecked());
 await page.screenshot({path:join(run,'candidate-selection.png')});
 await page.locator('#show-other-ports').check();assert.equal(await page.locator('#port-rows tr').count(),3);
 await page.locator('#hardware-connect-cancel').click();assert(await page.evaluate(()=>!window.fixtureRequests.some(r=>r.request?.command.action==='connect')));
 await page.locator('#hardware-choose').click();await page.locator('#port-option-0').check();
 await page.evaluate(()=>{window.testFeed.ports[1].serial_number='REPLACED';});await page.locator('#hardware-connect-confirm').click();
 await page.waitForFunction(()=>!document.getElementById('port-error').hidden);assert(await page.locator('#hardware-connect-dialog').isVisible());
 await page.locator('#hardware-connect-cancel').click();
 await page.evaluate(()=>window.fixtureMenu('enable-developer'));assert(await page.locator('#developer-dialog').isVisible());
 await page.screenshot({path:join(run,'developer-confirmation.png')});await page.locator('#developer-cancel').click();assert(await page.locator('#developer-badge').isHidden());
 await page.evaluate(()=>window.fixtureMenu('enable-developer'));await page.locator('#developer-enable').click();await page.waitForFunction(()=>document.getElementById('workspace').dataset.developer==='true');
 assert.equal(await page.locator('#workspace').getAttribute('data-source'),'hardware','Enabling must not change input source');
 await page.evaluate(()=>{window.testFeed.connected=true;window.testFeed.port='COM8';window.testFeed.phase='receiving';window.testFeed.sample={received_age_ms:0,values:[1,2,3,4,5,6,7]};});
 await page.waitForFunction(()=>!document.getElementById('hardware-disconnect').disabled);
 await page.evaluate(()=>window.fixtureMenu('source-synthetic'));await page.waitForFunction(()=>!document.getElementById('error-panel').hidden);
 assert.equal(await page.locator('#workspace').getAttribute('data-source'),'hardware');
 await page.evaluate(()=>{window.fixtureHoldRead=true;});await page.waitForFunction(()=>typeof window.fixtureReleaseRead==='function');
 assert(await page.locator('#hardware-disconnect').isEnabled(),'Polling disabled disconnect');await page.locator('#hardware-disconnect').click();
 await page.evaluate(()=>window.fixtureReleaseRead());await page.waitForFunction(()=>document.getElementById('environment').textContent==='一時停止（接続を解放）');
 assert(await page.evaluate(()=>window.fixtureRequests.some(r=>r.request?.command.action==='disconnect')),'Disconnect was dropped');
 await page.evaluate(()=>window.fixtureMenu('source-synthetic'));await page.waitForFunction(()=>document.getElementById('workspace').dataset.source==='synthetic');
 await page.evaluate(()=>window.fixtureMenu('disable-developer'));await page.waitForFunction(()=>document.getElementById('workspace').dataset.developer==='false');
 assert(!((await page.locator('body').innerText()).includes('合成試験')));assert.deepEqual(errors,[]);
 await page.locator('#hardware-ch-3').click();assert.equal(await page.locator('#trace-title').innerText(),'ch 3 の履歴');
 await page.keyboard.press('ArrowDown');assert.equal(await page.locator('#hardware-ch-4').getAttribute('aria-pressed'),'true');
 report.interactions=['normal gate','candidate/fallback list','descriptor replacement','explicit developer enable/cancel','connected source gate','queued disconnect during read','row-linked trace'];await context.close();
 report.usability=await verifyUsability(browser,'http://127.0.0.1:'+server.address().port,run);
 report.result='PASS';console.log('Desktop UI PASS: '+report.cases.length+' size/pixel-density combinations; stable rows and decimal anchors.');
} finally {writeFileSync(join(run,'report.json'),JSON.stringify(report,null,2));await browser.close();await new Promise(r=>server.close(r));}
