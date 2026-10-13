import test from 'node:test';
import assert from 'node:assert/strict';
import {canVisit, currentStep, createStepNavigation, stepKeys} from './navigation.mjs';
const view = (phase='confirmation', id='left') => ({id,phase,saved_current:false,steps:stepKeys.map((key,i)=>({state:i<stepKeys.indexOf(phase)?'complete':key===phase?'current':'pending'}))});
test('future steps cannot be opened; initial baseline is the only start position',()=>{
  const initial=view('not_started');
  assert.equal(currentStep(initial),'baseline');
  assert.equal(canVisit(initial,'baseline'),true);
  for(const key of stepKeys.slice(1))assert.equal(canVisit(initial,key),false);
  assert.equal(canVisit(view(),'unknown'),false);
  assert.equal(canVisit({...view(),steps:[]},'baseline'),false);
  assert.equal(canVisit(null,'baseline'),false);
});
test('reviewing completed steps cannot mutate domain state or advance completion',()=>{
  const model=view(); const before=structuredClone(model); const nav=createStepNavigation();
  assert(nav.visit(model,'baseline')); assert.equal(nav.position(model),'baseline');
  assert(!nav.visit(model,'record')); assert.equal(nav.position(model),'baseline');
  assert.deepEqual(model,before); nav.follow(model); assert.equal(nav.position(model),'confirmation');
});
test('navigation is retained by identity while invalidated future positions are rejected',()=>{
  const nav=createStepNavigation(),left=view('record'),right=view('assessment','right');
  nav.visit(left,'confirmation'); nav.visit(right,'baseline');
  assert.equal(nav.position(left),'confirmation'); assert.equal(nav.position(right),'baseline');
  assert.equal(nav.position(view('assessment')),'assessment');
  const saved={...left,saved_current:true,steps:left.steps.map(()=>({state:'complete'}))}; nav.follow(saved); assert.equal(nav.position(saved),'record');
});
