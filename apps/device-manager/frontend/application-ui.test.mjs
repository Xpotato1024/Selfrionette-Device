import {readFileSync} from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import {wizardPlan,explainFailure,calibrationRecovery,qualityObservationRows} from './application-ui.mjs';

test('quality diagnostics keep unknown transport latency separate from measured frame rates',()=>{
 assert(qualityObservationRows({}).every(([,value])=>value==='未測定'));
 const rows=new Map(qualityObservationRows({sample_age_ms:12,diagnostics:{receive_frame_hz:80,observed_source_frame_hz:80,fresh_frame_hz:[80,80,40,80,80,80,80],usable_ratio:0.5,normalized_frames:99,transport_latency_ms:null,relative_backlog_ms:0}}));
 assert.equal(rows.get('受信frame/s'),'80');assert.equal(rows.get('全ch usable率'),'50 %');
 assert.equal(rows.get('絶対的な輸送遅延'),'未測定');assert.equal(rows.get('相対的なホスト滞留'),'0 ms');
 assert.match(rows.get('各ch fresh frame/s'),/80 \/ 80 \/ 40/);
});

test('recovery requires current verified evidence, fresh neutral and explicit apply',()=>{
 const verified={connected:true,fit:{},heldout:{passed:true,trials:10,method:'diagonal'},session_neutral_valid:false,ready:false};
 let next=calibrationRecovery(verified);assert.equal(next.neutralAllowed,true);assert.equal(next.applyAllowed,false);assert.match(next.message,/証拠は保持.*新しい中立値.*明示的/);
 next=calibrationRecovery({...verified,session_neutral_valid:true});assert.equal(next.applyAllowed,true);assert.match(next.message,/明示的に適用/);
 for(const change of [{fit:null},{heldout:{passed:true}},{heldout:{passed:false,trials:10,method:'diagonal'}},{heldout:{passed:true,trials:10,method:'coupled'}},{capture:{active:true}},{connected:false}]){
  next=calibrationRecovery({...verified,...change,session_neutral_valid:true});assert.equal(next.neutralAllowed,false);assert.equal(next.applyAllowed,false);assert.equal(next.saveAllowed,false);
 }
 assert.match(calibrationRecovery({...verified,heldout:{passed:false}}).message,/不足・中断.*別試行.*検証/);
 assert.match(calibrationRecovery({...verified,fit:null}).message,/個人校正.*再検証/);
});
test('personal instructions use the confirmed per-device mapping and preserve both directions and repeats',()=>{
 const channels=Array.from({length:7},(_,wire)=>({wire,logical_axis:6-wire,location:'装着部位'+wire,direction:'既知方向'+wire}));
 const plan=wizardPlan(channels);assert.equal(plan.length,113);assert.equal(plan[0].phase,'neutral');
 for(let axis=0;axis<7;axis++)for(const direction of [1,-1])for(let repeat=0;repeat<2;repeat++){
  const trial=plan.filter(s=>s.axis===axis&&s.direction===direction&&s.repeat===repeat);
  assert.deepEqual(trial.map(s=>s.phase),['trial_neutral','ramp','hold','release']);assert(trial.every(s=>s.instruction.includes('装着部位'+(6-axis))));
 }
 assert(plan.some(s=>s.instruction.includes('痛みのない')));
});
test('missing physical labels remain explicitly unknown instead of a thumb/index default',()=>{
 const plan=wizardPlan([]);assert(plan.slice(1).every(s=>s.instruction.includes('未設定の部位')));assert(!plan.some(s=>/親指|人差指|小指/.test(s.instruction)));
});
test('quality and context errors provide a concise next action in Japanese',()=>{assert.match(explainFailure('QualityUnusable'),/再測定/);assert.match(explainFailure('profile context mismatch'),/使用条件/);assert.match(explainFailure('IncompleteRepeats(3)'),/不足/);assert.match(explainFailure('unknown backend error'),/操作を完了できません/);});


test('live setup controls retain associated labels and scoped table headers',()=>{
 const ui=readFileSync(new URL('./application-ui.mjs',import.meta.url),'utf8');
 assert(ui.includes("element('label',cell.getAttribute('aria-label'),{for:cell.id,class:'sr-only'})"));
 assert(ui.includes('<label class="sr-only" for="live-method">採用する補正方法</label>'));
 assert(!/<th>/.test(ui));assert((ui.match(/<th scope="col">/g)||[]).length>=13);
});

test('screen-reader labels stay inside their horizontally scrolled table cells',()=>{
 const css=readFileSync(new URL('./style.css',import.meta.url),'utf8');
 assert.match(css,/\.live-setup-table td\s*\{[^}]*position:relative/);
});

test('fixed application DOM references exist in the rendered templates',()=>{
 const ui=readFileSync(new URL('./application-ui.mjs',import.meta.url),'utf8');
 const html=readFileSync(new URL('./index.html',import.meta.url),'utf8');
 const ids=new Set([...`${ui}\n${html}`.matchAll(/id=["']([-\w]+)["']/g)].map(m=>m[1]));
 const references=[...ui.matchAll(/\$\('([-\w]+)'\)/g)].map(m=>m[1]);
 assert.deepEqual([...new Set(references.filter(id=>!ids.has(id)))],[]);
});
