import test from 'node:test';
import assert from 'node:assert/strict';
import {wizardPlan,explainFailure} from './application-ui.mjs';
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
