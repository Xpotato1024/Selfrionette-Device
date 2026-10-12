// Real WebView2 interaction checks; synthetic data, no hardware or certification claim.
import assert from 'node:assert/strict';
import {join} from 'node:path';
export async function checkUI(page, run, report, call) {
  await call('select', {role:'left'}); await call('begin');
  await page.reload(); await page.waitForFunction(()=>!document.getElementById('role').disabled);
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
  assert.deepEqual(metrics.missingLabels,[]);assert.equal(metrics.errorsLive,false);assert(metrics.columns);assert.match(metrics.tableCaption,/合成7ch/);
  await page.locator('nav [data-page="adjust"]').focus();await page.keyboard.press('Enter');
  assert.equal(await page.locator('nav [aria-current="page"]').getAttribute('data-page'),'adjust');
  const leftEpoch=(await call('status')).epoch;
  await page.locator('#role').focus();await page.keyboard.press('ArrowDown');
  await page.waitForFunction(()=>document.getElementById('device-id').textContent.includes('2222'));
  assert.match(await page.locator('#role-summary').innerText(),/右入力用/);
  await page.locator('#retry-controls > summary').click();await page.locator('#restart').focus();
  assert.equal(await page.locator('#restart').evaluate(e=>getComputedStyle(e).outlineWidth),'3px');
  await page.keyboard.press('Enter');assert(await page.locator('#confirm-dialog').isVisible());
  await page.keyboard.press('Escape');assert(!(await page.locator('#confirm-dialog').isVisible()));
  await page.locator('#restart').click();await page.locator('#dialog-apply').focus();await page.keyboard.press('Enter');
  await page.waitForFunction(()=>document.querySelector('[aria-current="step"]')?.textContent.startsWith('1'));
  for(const [id,key,step] of [['baseline','Space',2],['assess','Enter',3],['preview','Enter',4],['confirm','Enter',5]]) {
    await page.locator('#'+id).focus();await page.keyboard.press(key);
    await page.waitForFunction(s=>document.querySelector('[aria-current="step"]')?.textContent.startsWith(String(s)),step);
    assert.equal(await page.locator('#page-adjust button.primary:visible').count(),1,'Exactly one visible primary action at step '+step);
  }
  assert.equal(await page.locator('#rows tr').count(),7);
  assert.equal(await page.locator('#steps .step-state').filter({hasText:'完了'}).count(),4);
  assert.equal(await page.locator('#page-adjust button.primary:visible').count(),1);
  await page.screenshot({path:join(run,'ui-right-record.png'),fullPage:true});
  await page.locator('#role').selectOption('left');await page.waitForFunction(()=>document.getElementById('device-id').textContent.includes('1111'));
  assert.equal((await call('status')).epoch,leftEpoch);assert.equal((await call('status')).phase,'baseline');
  await page.locator('#role').selectOption('right');await page.waitForFunction(()=>document.getElementById('device-id').textContent.includes('2222'));
  assert.equal((await call('status')).phase,'record');
  await page.locator('#retry-controls > summary').click();await page.locator('#manual-controls summary').click();
  await page.locator('#reason').fill('');await page.locator('#flip').focus();await page.keyboard.press('Space');
  await page.waitForFunction(()=>document.getElementById('reason').getAttribute('aria-invalid')==='true');
  assert.equal(await page.evaluate(()=>document.activeElement.id),'reason');assert.match(await page.locator('#reason-error').innerText(),/根拠を入力/);
  assert.match(await page.locator('#reason').getAttribute('aria-describedby'),/reason-error/);
  assert.equal(await page.locator('#rows tr').count(),0);assert(await page.locator('#save').isDisabled());assert(await page.locator('#export').isDisabled());assert(await page.locator('#confirm').isDisabled());
  await page.screenshot({path:join(run,'ui-input-error.png'),fullPage:true});
  const size=await page.evaluate(()=>({width:innerWidth,height:innerHeight}));
  const reflow=[];
  for(const width of [320,640]) {
    await page.setViewportSize({width,height:850});
    for(const name of ['device','adjust','diagnostics']) {
      await page.locator('nav [data-page="'+name+'"]').click();
      const metric=await page.evaluate(()=>({width:innerWidth,overflow:document.documentElement.scrollWidth-innerWidth}));
      assert(metric.overflow<=1,'Unexpected page-wide horizontal overflow: '+name+' '+width);reflow.push({page:name,...metric});
      if(width===320)await page.screenshot({path:join(run,'ui-320-'+name+'.png'),fullPage:true});
    }
  }
  await page.locator('.table-scroll').focus();await page.keyboard.press('ArrowRight');
  await page.waitForFunction(()=>document.querySelector('.table-scroll').scrollLeft>0);
  assert.equal(await page.locator('.table-scroll').getAttribute('tabindex'),'0');
  await page.locator('.table-scroll').screenshot({path:join(run,'ui-table-scroll.png')});
  await page.setViewportSize(size);await page.locator('nav [data-page="adjust"]').click();
  const contrast=await page.evaluate(()=>{
    const luminance=c=>{const a=c.match(/[\d.]+/g).slice(0,3).map(Number).map(x=>{x/=255;return x<=.04045?x/12.92:((x+.055)/1.055)**2.4});return .2126*a[0]+.7152*a[1]+.0722*a[2]};
    const background=e=>{for(let p=e;p;p=p.parentElement){const c=getComputedStyle(p).backgroundColor;if(c!=='rgba(0, 0, 0, 0)'&&c!=='transparent')return c;}return 'rgb(255,255,255)';};
    return ['body','p','th','.field-error'].map(selector=>{const e=document.querySelector(selector),s=getComputedStyle(e),a=luminance(s.color),b=luminance(background(e));return {selector,ratio:(Math.max(a,b)+.05)/(Math.min(a,b)+.05)}});
  });
  assert(contrast.every(v=>v.ratio>=4.5),'Sample text contrast below 4.5:1');
  report.ui={metrics,reflow,contrast,limitations:['No screen-reader test','320/640 CSS pixel reflow; no Windows DPI/200% native zoom certification']};
  report.cases.push('native keyboard navigation/steps, independent selection, explicit restart/cancel, field error and revocation, single primary action, all 3 pages at 320/640px, sampled contrast');
}
