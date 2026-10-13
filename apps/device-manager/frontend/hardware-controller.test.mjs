import test from 'node:test';
import assert from 'node:assert/strict';
import {createHardwareController} from './hardware-controller.mjs';
const hooks = () => ({render:()=>{},error:()=>{},busy:()=>{}});
test('hardware scans never imply connect and mutations carry the latest revision',async()=>{
  const requests=[];const c=createHardwareController(async request=>{requests.push(request);return {view:{revision:requests.length},error:null};},hooks());
  await c.send({action:'scan'});await c.send({action:'connect',port:'chosen'});
  assert.deepEqual(requests.map(r=>r.command.action),['scan','connect']);assert.equal(requests[1].expected,1);
});
test('hardware controller does not overlap calls or substitute the current target for a dialog token',async()=>{
  let resolve,count=0;const c=createHardwareController(()=>{count++;return new Promise(r=>{resolve=r;});},hooks());
  const first=c.send({action:'connect',port:'COM1'},42);assert.equal(await c.send({action:'disconnect'}),null);
  resolve({view:{revision:43},error:null});await first;assert.equal(count,1);assert.equal(c.view.revision,43);
});
test('transport failure erases the displayed sample instead of pretending it is still current',async()=>{
  const views=[];let fail=false;const c=createHardwareController(async()=>{if(fail)throw Error('IPC');return {view:{revision:1,sample:[1]},error:null};},{...hooks(),render:v=>views.push(v)});
  await c.send({action:'read'});fail=true;await c.send({action:'read'});assert.equal(c.view,null);assert.equal(views.at(-1),null);
});
