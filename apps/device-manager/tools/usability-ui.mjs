import assert from 'node:assert/strict';
import {join} from 'node:path';
import {installDesktopFixture} from './desktop-fixture.mjs';

// Called by desktop-ui-test. CSS viewport reduction models 125/150% usable
// window area; it does not claim a real Windows per-monitor DPI transition.
export async function verifyUsability(browser,url,output){
 const cases=[];
 for(const width of [800,1100,1600])for(const scale of [1,1.25,1.5]){
  const context=await browser.newContext({viewport:{width:Math.floor(width/scale),height:Math.floor(900/scale)},deviceScaleFactor:scale}),page=await context.newPage();
  const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await page.addInitScript(installDesktopFixture);await page.goto(url);
  await page.waitForFunction(()=>document.querySelector('.readout-integer')?.textContent==='2,207');
  await page.locator('#tab-adjust').click();await page.locator('[data-live-page="update"]').click();
  await page.waitForFunction(()=>document.getElementById('update-port').selectedOptions[0]?.textContent.startsWith('COM9'));
  assert(await page.locator('#update-apply').isDisabled());assert(await page.locator('#update-prepare').isDisabled());
  await page.locator('#update-scan').click();assert.match(await page.locator('#update-port option:checked').innerText(),/^COM9/);
  const geometry=await page.evaluate(()=>{
   const overlaps=[];for(const label of document.querySelectorAll('[data-live-panel="update"] label:not(.check-line)')){
    const control=label.querySelector('select,input');if(!control||!control.getClientRects().length)continue;
    const caption=[...label.childNodes].find(n=>n.nodeType===Node.TEXT_NODE&&n.textContent.trim());if(!caption)continue;
    const range=document.createRange();range.selectNodeContents(caption);const text=range.getBoundingClientRect(),box=control.getBoundingClientRect();if(text.bottom>box.top+1)overlaps.push(label.textContent);
   }
   const content=document.querySelector('.live-content'),work=document.querySelector('.work-area');return {overlaps,bodyOverflow:document.documentElement.scrollWidth-innerWidth,contentOverflow:content.scrollWidth-content.clientWidth,workOverflow:work.scrollWidth-work.clientWidth};
  });assert.deepEqual(geometry.overlaps,[]);assert(geometry.bodyOverflow<=1);assert(geometry.contentOverflow<=1);assert(geometry.workOverflow<=1);
  await page.screenshot({path:join(output,`updater-${width}-${scale}.png`)});
  await page.locator('#update-handoff').click();await page.waitForFunction(()=>!window.testFeed.connected);
  assert(await page.locator('#update-prepare').isEnabled());assert.match(await page.locator('#update-port option:checked').innerText(),/^COM9/);
  await page.locator('#update-write-confirm').check();assert(await page.locator('#update-apply').isDisabled(),'confirmation alone cannot enable writing');
  await page.locator('#update-prepare').click();await page.waitForFunction(()=>document.getElementById('update-result').textContent.includes('検査結果'));
  assert(await page.locator('#update-apply').isDisabled());await page.locator('#update-write-confirm').check();assert(await page.locator('#update-apply').isEnabled());
  await page.locator('#update-family').selectOption('legacy_7ch');assert(await page.locator('#update-apply').isDisabled());assert(!await page.locator('#update-write-confirm').isChecked());
  await page.locator('#tab-device').click();assert.equal(await page.locator('.readout-integer').first().textContent(),'—');
  page.once('dialog',d=>d.accept());await page.locator('#hardware-reconnect').click();await page.waitForFunction(()=>window.testFeed.connected);assert.equal(await page.locator('.readout-integer').first().textContent(),'—');
  assert.deepEqual(errors,[]);cases.push({width,scale,geometry});await context.close();
 }
 // Ownership, last-used source and persisted candidate use a separate fixture
 // case; native service/neutral acceptance remains in live-native-qa.mjs.
 const context=await browser.newContext({viewport:{width:1100,height:720}}),page=await context.newPage();
 await page.addInitScript(installDesktopFixture,{connected:false});await page.goto(url);
 await page.locator('#tab-adjust').click();await page.locator('#live-port option').filter({hasText:'COM8'}).waitFor({state:'attached'});
 await page.locator('#live-port').selectOption({label:await page.locator('#live-port option').filter({hasText:'COM8'}).innerText()});
 await page.locator('#live-connect').click();await page.waitForFunction(()=>window.testModern.connected);
 await page.locator('#tab-device').click();await page.locator('#hardware-disconnect').click();
 await page.waitForFunction(()=>!window.testModern.owns_handle);
 page.once('dialog',d=>d.accept());await page.locator('#hardware-reconnect').click();
 await page.waitForFunction(()=>window.testModern.connected);assert.equal(await page.evaluate(()=>window.testModern.ready),false);
 await page.evaluate(()=>window.fixtureMenu('pause'));await page.waitForFunction(()=>!window.testModern.owns_handle);
 await page.locator('#hardware-choose').click();await page.locator('#port-rows tr[data-name="COM9"] input').check();await page.locator('#hardware-connect-confirm').click();
 await page.waitForFunction(()=>window.testFeed.connected&&window.testFeed.port==='COM9');
 await page.evaluate(()=>window.fixtureMenu('pause'));await page.waitForFunction(()=>!window.testFeed.connected);
 page.once('dialog',d=>d.accept());await page.evaluate(()=>window.fixtureMenu('reconnect'));
 await page.waitForFunction(()=>window.testFeed.connected);
 assert.equal(await page.evaluate(()=>window.testModern.connected),false,'old modern candidate cannot redirect monitor resume');
 await page.reload();await page.waitForFunction(()=>document.getElementById('workspace').dataset.source==='hardware');
 assert.equal(await page.evaluate(()=>window.testFeed.connected),false,'preference never auto-connects');
 await page.locator('#tab-adjust').click();await page.locator('[data-live-page="update"]').click();
 await page.waitForFunction(()=>document.querySelector('#update-port option:checked')?.textContent.startsWith('COM9'));
 await page.evaluate(()=>{window.testFeed.ports=[];});await page.locator('#update-scan').click();
 assert(await page.locator('#update-prepare').isDisabled());assert(await page.locator('#update-apply').isDisabled());
 assert.equal(await page.locator('#update-port').inputValue(),'');
 await page.locator('#tab-device').click();await page.locator('#hardware-choose').click();await page.locator('#hardware-scan').click();
 assert(await page.locator('#port-empty').isVisible());assert(await page.locator('#hardware-connect-confirm').isDisabled());
 cases.push({ownership:'modern pause/resume, latest monitor candidate, restart preference, empty/disconnected'});
 await context.close();return cases;
}
