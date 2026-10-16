import test from 'node:test';
import assert from 'node:assert/strict';
import {createApplicationController} from './application-controller.mjs';
function fixture(invoke){const rendered=[],errors=[],busy=[];return {rendered,errors,busy,controller:createApplicationController(invoke,{render:v=>rendered.push(v),error:e=>errors.push(e),busy:b=>busy.push(b)})};}
test('capture start is never queued behind a stream tick',async()=>{
 let finish;const calls=[];const f=fixture(c=>{calls.push(c);return new Promise(r=>finish=r);});
 const tick=f.controller.send({action:'tick'});assert.equal(await f.controller.send({action:'capture_start'}),null);
 finish({view:{revision:1}});await tick;assert.deepEqual(calls,[{action:'tick'}]);assert.match(f.errors[0],/予約されません/);
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

test('polling exposes its lock to controls instead of accepting a silently dropped click',async()=>{
 let finish;const f=fixture(()=>new Promise(resolve=>{finish=resolve;}));
 const polling=f.controller.send({action:'stream_tick'});
 assert.equal(f.controller.busy,true);assert.deepEqual(f.busy,[true]);
 finish({view:{connected:true}});await polling;
 assert.equal(f.controller.busy,false);assert.deepEqual(f.busy,[true,false]);
});
