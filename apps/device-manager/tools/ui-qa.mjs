// Focused native UI checks. Synthetic fixture only; no hardware or screen-reader claim.
import assert from 'node:assert/strict';
import {join} from 'node:path';
export async function checkUI(page,run,report) {
  const metrics=await page.evaluate(()=>({
    body:getComputedStyle(document.body).fontSize,
    label:getComputedStyle(document.querySelector('label')).fontSize,
    heading:getComputedStyle(document.querySelector('th')).fontSize,
    missingLabels:[...document.querySelectorAll('input,select')].filter(e=>!e.labels?.length).map(e=>e.id),
    errorsLive:[...document.querySelectorAll('#message,#reason-error')].some(e=>e.hasAttribute('aria-live')||e.getAttribute('role')==='alert'),
    tableCaption:document.querySelector('caption').textContent,
    columns:[...document.querySelectorAll('th')].every(e=>e.scope==='col')
  }));
  assert.equal(metrics.body,'16px');assert.equal(metrics.label,'16px');assert.equal(metrics.heading,'16px');
  assert.deepEqual(metrics.missingLabels,[]);assert.equal(metrics.errorsLive,false);assert(metrics.columns);
  assert.match(metrics.tableCaption,/合成7ch/);
  await page.locator('#level').selectOption('0.5');
  await page.locator('#role').selectOption('left');
  await page.waitForFunction(()=>document.getElementById('device-id').textContent.includes('1111'));
  await page.locator('#role').focus();await page.keyboard.press('ArrowDown');
  await page.waitForFunction(()=>document.getElementById('device-id').textContent.includes('2222'));
  assert.match(await page.locator('#role-summary').innerText(),/右（Right）.*合成/);
  await page.keyboard.press('Tab');
  assert.equal(await page.evaluate(()=>document.activeElement.id),'begin');
  assert.equal(await page.locator('#begin').evaluate(e=>getComputedStyle(e).outlineWidth),'3px');
  await page.keyboard.press('Enter');
  await page.waitForFunction(()=>!document.getElementById('baseline').disabled);
  for(const [id,key,stage] of [['baseline','Space',2],['assess','Enter',3],['preview','Enter',4],['confirm','Enter',5]]) {
    await page.locator('#'+id).focus();await page.keyboard.press(key);
    try { await page.waitForFunction(s=>document.querySelector('[aria-current="step"]')?.textContent.startsWith(String(s)),stage); }
    catch(error) { report.ui_failure={id,key,stage,observed:await page.evaluate(()=>({current:document.querySelector('[aria-current="step"]')?.textContent,message:document.getElementById('message').textContent,focus:document.activeElement.id}))};throw error; }
  }
  assert.equal(await page.locator('#rows tr').count(),7);
  assert.match(await page.locator('#step-guidance').innerText(),/現在5\/5/);
  assert.equal(await page.locator('#steps .step-state').filter({hasText:'完了'}).count(),4);
  await page.screenshot({path:join(run,'ui-right-preview.png'),fullPage:true});
  await page.locator('.table-scroll').focus();await page.keyboard.press('ArrowRight');
  await page.waitForFunction(()=>document.querySelector('.table-scroll').scrollLeft>0);
  await page.locator('.table-scroll').evaluate(e=>{e.scrollLeft=e.scrollWidth;});
  await page.locator('.table-scroll').screenshot({path:join(run,'ui-table-scroll.png')});
  await page.locator('#manual-controls summary').click();
  await page.locator('#reason').fill('');
  await page.locator('#flip').focus();await page.keyboard.press('Space');
  await page.waitForFunction(()=>document.getElementById('reason').getAttribute('aria-invalid')==='true');
  assert.equal(await page.evaluate(()=>document.activeElement.id),'reason');
  assert.match(await page.locator('#reason-error').innerText(),/根拠を入力/);
  assert.match(await page.locator('#reason').getAttribute('aria-describedby'),/reason-error/);
  assert.equal(await page.locator('#rows tr').count(),0);assert(await page.locator('#save').isDisabled());
  assert(await page.locator('#export').isDisabled());assert(await page.locator('#confirm').isDisabled());
  await page.screenshot({path:join(run,'ui-input-error.png'),fullPage:true});
  const size=await page.evaluate(()=>({width:innerWidth,height:innerHeight}));
  await page.setViewportSize({width:320,height:850});
  const narrow=await page.evaluate(()=>({
    width:innerWidth,overflow:document.documentElement.scrollWidth-innerWidth,
    tableScroll:document.querySelector('.table-scroll').scrollWidth>document.querySelector('.table-scroll').clientWidth,
    horizontalRegion:document.querySelector('.table-scroll').tabIndex,
    reasonWidth:document.getElementById('reason').getBoundingClientRect().width
  }));
  assert(narrow.overflow<=1,'Unexpected page-wide horizontal overflow');assert.equal(narrow.horizontalRegion,0);assert(narrow.reasonWidth>0);
  await page.screenshot({path:join(run,'ui-narrow.png'),fullPage:true});
  await page.setViewportSize({width:640,height:850});
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'640px reflow failed');
  await page.screenshot({path:join(run,'ui-640.png'),fullPage:true});
  await page.setViewportSize(size);
  const contrast=await page.evaluate(()=>{
    const luminance=c=>{const a=c.match(/[\d.]+/g).slice(0,3).map(Number).map(x=>{x/=255;return x<=.04045?x/12.92:((x+.055)/1.055)**2.4});return .2126*a[0]+.7152*a[1]+.0722*a[2]};
    const background=e=>{for(let p=e;p;p=p.parentElement){const c=getComputedStyle(p).backgroundColor;if(c!=='rgba(0, 0, 0, 0)'&&c!=='transparent')return c;}return 'rgb(255,255,255)';};
    return ['body','p','th','.field-error'].map(selector=>{const e=document.querySelector(selector),s=getComputedStyle(e),a=luminance(s.color),b=luminance(background(e));return {selector,ratio:(Math.max(a,b)+.05)/(Math.min(a,b)+.05)}});
  });
  assert(contrast.every(v=>v.ratio>=4.5),'Sample text contrast below 4.5:1');
  report.ui={metrics,narrow,contrast,limitations:['No screen-reader test','320/640 CSS pixel reflow; not native 200% zoom certification']};
  report.cases.push('native keyboard flow, left/right identity, step semantics, associated Japanese error and approval invalidation, 16px text, narrow reflow, sampled contrast');
}
