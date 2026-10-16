import test from 'node:test';
import assert from 'node:assert/strict';
import {createApplicationController} from './application-controller.mjs';
function fixture(invoke){const rendered=[],errors=[],busy=[];return {rendered,errors,busy,controller:createApplicationController(invoke,{render:v=>rendered.push(v),error:e=>errors.push(e),busy:b=>busy.push(b)})};}
test('one deliberate action takes priority after the current bounded read, without retries',async()=>{
 let finish;const calls=[];const f=fixture(c=>{calls.push(c.action);return c.action==='stream_tick'?new Promise(r=>finish=r):Promise.resolve({view:{connected:true}});});
 await f.controller.send({action:'snapshot'});calls.length=0;
 const tick=f.controller.send({action:'stream_tick'}),capture=f.controller.send({action:'capture_start'});
 assert.equal(await f.controller.send({action:'stream_tick'}),null);
 assert.equal(await f.controller.send({action:'save_calibration'}),null);
 finish({view:{connected:true}});await tick;assert(await capture);
 assert.deepEqual(calls,['stream_tick','capture_start']);
});
test('one user operation owns IPC and background ticks cannot race it',async()=>{
 let finish;const calls=[];const f=fixture(c=>{calls.push(c);return new Promise(r=>finish=r);});
 const capture=f.controller.send({action:'capture_start'});assert.equal(await f.controller.send({action:'tick'}),null);
 finish({view:{phase:'capturing'}});await capture;assert.equal(calls.length,1);assert.deepEqual(f.busy,[true,false]);
});
test('invalidated connection cannot restore an in-flight reply',async()=>{
 let finish;const f=fixture(()=>new Promise(r=>finish=r));const request=f.controller.send({action:'snapshot'});
 f.controller.invalidate();finish({view:{ready:true}});assert.equal(await request,null);assert.equal(f.controller.view,null);assert.deepEqual(f.rendered,[null]);
});
test('failed command keeps the service error visible and never invents success',async()=>{
 const f=fixture(async()=>({view:{ready:false},error:'quality_unknown'}));await f.controller.send({action:'capture_start'});
 assert.equal(f.controller.view.ready,false);assert.equal(f.errors.at(-1),'quality_unknown');
});
test('successful background polling cannot erase a failed user operation explanation',async()=>{
 let failed=true;const f=fixture(async()=>failed?{view:{ready:false},error:'quality_unknown'}:{view:{ready:false}});await f.controller.send({action:'capture_start'});failed=false;await f.controller.send({action:'tick'});assert.equal(f.errors.at(-1),'quality_unknown');
});

test('polling does not disable user controls',async()=>{
 let finish;const f=fixture(()=>new Promise(resolve=>{finish=resolve;}));
 const polling=f.controller.send({action:'stream_tick'});
 assert.equal(f.controller.busy,false);assert.deepEqual(f.busy,[]);
 finish({view:{connected:true}});await polling;
 assert.equal(f.controller.busy,false);assert.deepEqual(f.busy,[]);
});

test('context changes during a read reject the waiting action without invoking it',async()=>{
 let finish;const calls=[];const f=fixture(c=>{calls.push(c.action);return calls.length===1?Promise.resolve({view:{connected:true,info:{generation:1}}}):new Promise(r=>finish=r);});
 await f.controller.send({action:'snapshot'});
 const poll=f.controller.send({action:'stream_tick'}),action=f.controller.send({action:'tare',confirmed:true});
 finish({view:{connected:true,info:{generation:2}}});await poll;
 assert.equal(await action,null);assert.deepEqual(calls,['snapshot','stream_tick']);assert.match(f.errors.at(-1),/context/);
});

test('a read that outlives the readiness deadline cannot schedule a later capture',async()=>{
 let finish,now=0;const calls=[],errors=[],operations=[];
 const controller=createApplicationController(c=>{calls.push(c.action);return new Promise(r=>finish=r);},{render(){},error:e=>errors.push(e),busy(){},operation:o=>operations.push(o),now:()=>now});
 const poll=controller.send({action:'stream_tick'}),capture=controller.send({action:'capture_start'});
 now=251;finish({view:{}});await poll;assert.equal(await capture,null);
 assert.deepEqual(calls,['stream_tick']);assert.match(errors.at(-1),/再度/);assert.equal(operations.at(-1).status,'error');
});

test('IPC failure clears a previously ready snapshot and acknowledges failure',async()=>{
 let fail=false;
 const f=fixture(async()=>{if(fail)throw Error('IPC unavailable');return {view:{ready:true}};});
 await f.controller.send({action:'snapshot'});fail=true;
 assert.equal(await f.controller.send({action:'apply_calibration'}),null);
 assert.equal(f.controller.view,null);assert.equal(f.rendered.at(-1),null);assert.match(f.errors.at(-1),/IPC unavailable/);
});

test('a failed persistent write is invoked once and keeps its error through later reads',async()=>{
 const calls=[];const f=fixture(async command=>{calls.push(command.action);return {view:{connected:true},...(command.action==='save_device_settings'?{error:'readback failed'}:{})};});
 const reply=await f.controller.send({action:'save_device_settings'});assert.equal(reply.error,'readback failed');
 await f.controller.send({action:'stream_tick'});assert.deepEqual(calls,['save_device_settings','stream_tick']);assert.equal(f.errors.at(-1),'readback failed');
});
