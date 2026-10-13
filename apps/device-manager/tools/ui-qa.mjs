// Native WebView2 QA. Each assertion concerns real rendered UI; synthetic data only.
import assert from 'node:assert/strict';
import {join} from 'node:path';
import {readFileSync} from 'node:fs';
const appConfig=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url),'utf8'));
export async function checkUI(page,run,report,call) {
  await call('select',{role:'left'}); await call('begin');
  await page.reload(); await page.waitForFunction(()=>document.getElementById('source-mode')&&!document.getElementById('source-mode').disabled); await page.locator('#source-mode').selectOption('synthetic'); await page.waitForFunction(()=>!document.getElementById('role').disabled);
  const metrics=await page.evaluate(()=>({
    body:getComputedStyle(document.body).fontSize,
    label:getComputedStyle(document.querySelector('label')).fontSize,
    heading:getComputedStyle(document.querySelector('th')).fontSize,
    missingLabels:[...document.querySelectorAll('input,select')].filter(e=>!e.labels?.length).map(e=>e.id),
    errorsLive:document.getElementById('reason-error').hasAttribute('aria-live'),
    columns:[...document.querySelectorAll('th')].every(e=>e.scope==='col'),
    modeCount:(document.body.innerText.match(/実機未接続/g)||[]).length,
    taskTabs:[...document.querySelectorAll('[role="tab"]')].map(e=>({controls:e.getAttribute('aria-controls'),panel:document.getElementById(e.getAttribute('aria-controls'))?.getAttribute('role')}))
  }));
  assert.equal(metrics.body,'16px'); assert.equal(metrics.label,'16px'); assert.equal(metrics.heading,'16px');
  assert.deepEqual(metrics.missingLabels,[]); assert.equal(metrics.errorsLive,false); assert(metrics.columns);
  assert.equal(metrics.modeCount,1); assert(metrics.taskTabs.every(t=>t.panel==='tabpanel'));
  const beforeTabs=await call('status');
  assert.equal(await page.locator('.app-header').count(),0,'Native title must not be repeated as a page banner');
  assert.equal(await page.locator('body').innerText().then(t=>t.includes('Selfrionette')),false);
  await page.locator('#about').click();
  const appInfo=await page.evaluate(()=>window.__TAURI__.core.invoke('application_info'));
  report.application_info=appInfo; console.log('Application metadata: '+JSON.stringify(appInfo));
  assert.equal(appInfo.publisher,'Xpotato.net');
  await page.waitForFunction(()=>document.getElementById('about-publisher').textContent==='Xpotato.net');
  assert.equal(await page.locator('#about-version').innerText(),appConfig.version);
  assert.equal(await page.locator('#about-homepage').innerText(),'https://xpotato.net');
  assert.deepEqual((await call('status')).token,beforeTabs.token,'App information cannot mutate calibration state');
  await page.screenshot({path:join(run,'ui-about.png'),fullPage:true});
  await page.keyboard.press('Escape');
  assert.equal(await page.evaluate(()=>document.activeElement.id),'about');
  await page.locator('#tab-device').focus(); await page.keyboard.press('ArrowRight');
  assert.equal(await page.evaluate(()=>document.activeElement.id),'tab-adjust');
  assert.equal(await page.locator('#tab-device').getAttribute('aria-selected'),'true','Arrow key must only move focus');
  await page.keyboard.press('Enter'); assert(await page.locator('#page-adjust').isVisible());
  assert.deepEqual((await call('status')).token,beforeTabs.token,'Task navigation must not send a mutation');
  assert.equal(await page.locator('.step-panel:visible').count(),1);
  assert.equal(await page.locator('#steps [data-step-link="record"]').getAttribute('aria-disabled'),'true');
  await page.locator('#steps [data-step-link="record"]').focus(); await page.keyboard.press('Enter');
  assert.equal(await page.locator('.step-panel:visible').getAttribute('data-step'),'baseline','Future navigation must be blocked');
  assert.deepEqual((await call('status')).token,beforeTabs.token);
  const density=await page.evaluate(()=>({
    width:innerWidth,headingTop:document.querySelector('.step-panel:not([hidden]) h2').getBoundingClientRect().top,
    toolbarHeight:document.querySelector('.context-bar').getBoundingClientRect().height,
    controlHeight:document.getElementById('baseline').getBoundingClientRect().height
  }));
  if(density.width>=900) assert(density.headingTop<=120,'Toolbar and navigation consume too much work area');
  assert(density.controlHeight>=32&&density.controlHeight<=36,'Desktop control sizing changed unexpectedly');
  report.desktop_density=density;
  const sequence=[['baseline','assessment'],['assess','preview'],['preview','confirmation'],['confirm','record']];
  for(const [id,next] of sequence) {
    assert.equal(await page.locator('#page-adjust button.primary:visible').count(),1);
    assert.equal(await page.locator('#page-adjust button.primary:visible').getAttribute('id'),id);
    await page.locator('#'+id).focus();
    assert.equal(await page.locator('#'+id).evaluate(e=>getComputedStyle(e).outlineWidth),'2px');
    await page.screenshot({path:join(run,'ui-step-'+(await page.locator('.step-panel:visible').getAttribute('data-step'))+'.png'),fullPage:true});
    await page.keyboard.press(id==='baseline'?'Space':'Enter');
    await page.waitForFunction(step=>document.querySelector('.step-panel:not([hidden])')?.dataset.step===step,next);
    assert.equal(await page.locator('.step-panel:visible').count(),1);
  }
  assert.equal(await page.locator('#adjust-rows').isVisible(),false,'Save step must not repeat the result table');
  await page.screenshot({path:join(run,'ui-step-record.png'),fullPage:true});
  const approved=await call('status');
  await page.locator('#steps [data-step-link="baseline"]').click();
  assert.equal(await page.locator('.step-panel:visible').getAttribute('data-step'),'baseline');
  assert.deepEqual((await call('status')).token,approved.token,'Reviewing completed steps must not invalidate approval');
  await page.locator('#baseline').click(); assert(await page.locator('#confirm-dialog').isVisible());
  await page.keyboard.press('Escape'); assert.equal(await page.locator('#confirm-dialog').isVisible(),false);
  assert.deepEqual((await call('status')).token,approved.token,'Cancelling a repeat leaves all state untouched');
  await page.locator('#baseline').click(); await page.locator('#dialog-apply').click();
  await page.waitForFunction(()=>document.querySelector('.step-panel:not([hidden])')?.dataset.step==='assessment');
  assert.equal((await call('status')).phase,'assessment');
  assert.equal(await page.locator('#steps [data-step-link="record"]').getAttribute('aria-disabled'),'true');
  assert(await page.locator('#save').isDisabled());
  for(const [id,next] of sequence.slice(1)) { await page.locator('#'+id).click(); await page.waitForFunction(step=>document.querySelector('.step-panel:not([hidden])')?.dataset.step===step,next); }
  // Role selection is distinct from restarting the session.
  const left=await call('status');
  await page.locator('#role').focus(); await page.keyboard.press('ArrowDown');
  await page.waitForFunction(()=>document.getElementById('device-id').textContent.includes('2222'));
  assert.match(await page.locator('#role-summary').textContent(),/右/);
  await page.keyboard.press('ArrowUp');
  await page.waitForFunction(()=>document.getElementById('device-id').textContent.includes('1111'));
  const returned=await call('status'); assert.equal(returned.epoch,left.epoch); assert.equal(returned.core_revision,left.core_revision); assert.equal(returned.phase,'record');
  await page.locator('#steps [data-step-link="assessment"]').click();
  await page.locator('#manual-controls > summary').click(); await page.locator('#reason').fill('');
  await page.locator('#flip').focus(); await page.keyboard.press('Space');
  await page.waitForFunction(()=>document.getElementById('reason').getAttribute('aria-invalid')==='true');
  assert.equal(await page.evaluate(()=>document.activeElement.id),'reason');
  assert.match(await page.locator('#reason-error').innerText(),/根拠/);
  assert.match(await page.locator('#reason').getAttribute('aria-describedby'),/reason-error/);
  assert.equal(await page.locator('#rows tr').count(),0); assert(await page.locator('#save').isDisabled()); assert(await page.locator('#export').isDisabled()); assert(await page.locator('#confirm').isDisabled());
  await page.screenshot({path:join(run,'ui-input-error.png'),fullPage:true});
  const originalSize=await page.evaluate(()=>({width:innerWidth,height:innerHeight}));
  const reflow=[];
  for(const width of [320,640]) {
    await page.setViewportSize({width,height:850});
    for(const name of ['device','adjust','diagnostics']) {
      await page.locator('#tab-'+name).click();
      const metric=await page.evaluate(()=>({width:innerWidth,overflow:document.documentElement.scrollWidth-innerWidth}));
      assert(metric.overflow<=1,'Unexpected page-wide horizontal overflow: '+JSON.stringify({name,...metric}));
      if(name==='adjust') { assert.equal(await page.locator('.step-panel:visible').count(),1); assert.equal(await page.locator('#step-outline').getAttribute('open'),null); }
      reflow.push({name,...metric}); await page.screenshot({path:join(run,'ui-'+width+'-'+name+'.png'),fullPage:true});
    }
  }
  await page.locator('.table-scroll').focus(); await page.keyboard.press('ArrowRight');
  await page.waitForFunction(()=>document.querySelector('.table-scroll').scrollLeft>0);
  await page.setViewportSize(originalSize);
  const contrast=await page.evaluate(()=>{
    const luminance=c=>{const a=c.match(/[\d.]+/g).slice(0,3).map(Number).map(x=>{x/=255;return x<=.04045?x/12.92:((x+.055)/1.055)**2.4});return .2126*a[0]+.7152*a[1]+.0722*a[2]};
    const background=e=>{for(let p=e;p;p=p.parentElement){const c=getComputedStyle(p).backgroundColor;if(c!=='rgba(0, 0, 0, 0)'&&c!=='transparent')return c;}return 'rgb(255,255,255)';};
    return ['body','p','th','.field-error'].map(selector=>{const e=document.querySelector(selector),a=luminance(getComputedStyle(e).color),b=luminance(background(e));return {selector,ratio:(Math.max(a,b)+.05)/(Math.min(a,b)+.05)}});
  });
  assert(contrast.every(v=>v.ratio>=4.5),'Sample text contrast below 4.5:1');
  report.ui={metrics,reflow,contrast,limitations:['No screen-reader test','No native 200%/Windows DPI acceptance']};
  report.cases.push('horizontal manual-activation task tabs; one visible step; locked future steps; no side effects on back/tab/cancel; explicit repeat invalidation; keyboard/focus/errors; 320/640 reflow; sampled contrast');
}
