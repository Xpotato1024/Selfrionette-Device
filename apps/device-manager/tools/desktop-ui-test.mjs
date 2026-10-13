// Production HTML/CSS/JS with a test-only in-memory IPC fixture. Never opens a device.
import {chromium} from 'playwright-core';import {createServer} from 'node:http';import {readFileSync,existsSync,mkdirSync,mkdtempSync,writeFileSync} from 'node:fs';import {resolve,join,extname} from 'node:path';import {fileURLToPath} from 'node:url';import assert from 'node:assert/strict';
const app=resolve(fileURLToPath(new URL('..',import.meta.url))),dir=join(app,'frontend');
const output=join(app,'src-tauri/target/desktop-qa');mkdirSync(output,{recursive:true});const run=mkdtempSync(join(output,'run-'));
const server=createServer((req,res)=>{try{const name=new URL(req.url,'http://localhost').pathname.slice(1)||'index.html';if(!/^[a-zA-Z0-9_.-]+$/.test(name))throw Error('path');res.setHeader('Content-Type',({'.html':'text/html','.mjs':'text/javascript','.css':'text/css'})[extname(name)]||'text/plain');res.end(readFileSync(join(dir,name)));}catch{res.writeHead(404);res.end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const candidates=[process.env.SELFRIONETTE_UI_BROWSER_EXE,((process.env.SystemDrive||'C:')+'/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'),((process.env.SystemDrive||'C:')+'/Program Files/Microsoft/Edge/Application/msedge.exe')].filter(Boolean);const browserPath=candidates.find(p=>existsSync(p));if(!browserPath){server.close();throw Error('No installed Edge. Set SELFRIONETTE_UI_BROWSER_EXE to a verified Chromium browser.');}const browser=await chromium.launch({executablePath:browserPath,headless:true});const report={scope:'production UI / in-memory fixture, no device I/O',cases:[],limitations:['deviceScaleFactor is not a real Windows DPI transition']};
try {
 for(const scale of [1,1.5,2]){
  const context=await browser.newContext({viewport:{width:1100,height:720},deviceScaleFactor:scale});const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await page.addInitScript(()=>{
   window.testFeed={revision:1,connected:true,port:'COM9',phase:'receiving',ports:[{name:'COM9',product:'表示試験用の長い機器名 · 複数の語を含む接続対象'}],received_frames:1,sample:{received_age_ms:0,values:[2207,61.4,1246.33,2496.67,-2716.6,306.27,49.27]},diagnostics:[]};
   window.__TAURI__={core:{invoke:async(name)=>{if(name!=='hardware_command')throw Error('Unexpected command in display-only test');window.testFeed.received_frames++;return {view:structuredClone(window.testFeed),error:null};}}};
  });
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.querySelectorAll('.readout-integer')[0]?.textContent==='2,207');
  await page.evaluate(()=>{window.originalRows=[...document.querySelectorAll('#hardware-value-rows tr')];});
  for(const width of [800,1100,1600]){
   await page.setViewportSize({width,height:720});
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
 report.result='PASS';console.log('Desktop UI PASS: '+report.cases.length+' size/pixel-density combinations; stable rows and decimal anchors.');
} finally {writeFileSync(join(run,'report.json'),JSON.stringify(report,null,2));await browser.close();await new Promise(r=>server.close(r));}
