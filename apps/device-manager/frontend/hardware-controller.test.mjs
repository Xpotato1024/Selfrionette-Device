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

test('a disconnect during a poll is queued once and uses the latest revision in the same connection',async()=>{
  const requests=[];let resolveRead;const states=[];
  const c=createHardwareController(async req=>{requests.push(req);if(req.command.action==='read')return new Promise(resolve=>{resolveRead=resolve;});return {view:{revision:1,connection_id:7,connected:true},error:null};},{...hooks(),busy:(busy,action)=>states.push([busy,action])});
  await c.send({action:'scan'});const poll=c.send({action:'read'});assert.deepEqual(states,[]);
  const stop=c.send({action:'disconnect'});assert.equal(requests.length,2);assert.equal(c.pendingOperation,'disconnect');
  resolveRead({view:{revision:2,connection_id:7,connected:true},error:null});await poll;await stop;
  assert.deepEqual(requests.map(r=>r.command.action),['scan','read','disconnect']);assert.equal(requests[2].expected,2);
  assert.deepEqual(states,[[true,'disconnect'],[false,'disconnect']]);
});
test('a queued operation is rejected instead of retargeted when the connection session changes',async()=>{
  let resolveRead;const requests=[],messages=[];
  const c=createHardwareController(async req=>{requests.push(req);if(req.command.action==='read')return new Promise(resolve=>{resolveRead=resolve;});return {view:{revision:1,connection_id:7},error:null};},{...hooks(),error:message=>messages.push(message)});
  await c.send({action:'scan'});const poll=c.send({action:'read'}),stop=c.send({action:'disconnect'});
  resolveRead({view:{revision:2,connection_id:8},error:null});await poll;assert.equal(await stop,null);assert.equal(requests.length,2);assert(messages.some(m=>m.includes('接続対象')));
});
