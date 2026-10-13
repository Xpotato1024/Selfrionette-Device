import {createTrace} from './trace.mjs';
import test from 'node:test';import assert from 'node:assert/strict';import {readingParts,createDisplayHistory,createReadout} from './readout.mjs';
test('readout reserves decimal and exponent fields across sign and magnitude',()=>{for(const v of [0,-0,1,-1,2207,133411.67,999999999.999,-13945.6,1e12,1e-300,Number.MAX_VALUE]){const p=readingParts(v);assert.match(p.fraction,/^\.\d{2}$/);assert(p.integer.length<=11);assert(p.exponent.length<=6);}assert.equal(readingParts(2207).integer,'2,207');assert.equal(readingParts(-0).sign,'');assert.notEqual(readingParts(1e-300).integer,'0');});
test('invalid or stale values are absence, never zero',()=>{for(const v of [undefined,null,NaN,Infinity])assert.equal(readingParts(v).integer,'—');});
test('display history bounds count/time, rejects duplicate display frames and drops disconnected data',()=>{const h=createDisplayHistory(3,100),v={connected:true,port:'test',received_frames:1,sample:{values:[1,2,3,4,5,6,7]}};h.add(v,0);h.add(v,1);assert.equal(h.points.length,1);for(let n=2;n<=6;n++)h.add({...v,received_frames:n},n);assert.equal(h.points.length,3);h.add({...v,received_frames:7},200);assert.equal(h.points.length,1);h.add({...v,port:'other'},201);assert.equal(h.points.length,1);h.add(null,202);assert.equal(h.points.length,0);});
test('readout update preserves node identity',()=>{const doc={createElement(){return {className:'',textContent:'',children:[],append(node){this.children.push(node);},setAttribute(){}};}};const r=createReadout(doc),nodes=[...r.element.children];r.update(1);r.update(-123456.78);assert.equal(r.element.children[1].textContent,'−123,456');assert.equal(r.element.children[0].textContent,'');r.update(null);assert.deepEqual(r.element.children,nodes);assert.equal(r.element.children[1].textContent,'—');});

test('trace coordinates agree with the labelled positive and negative limits',()=>{
 const nodes=Object.fromEntries(['trace-channel','trace-empty','trace-line','trace-top','trace-bottom'].map(id=>[id,{value:'0',textContent:'',setAttribute(key,value){this[key]=value;}}]));
 let now=0;const trace=createTrace({getElementById:id=>nodes[id]},()=>now);
 const view={connected:true,port:'fixture',received_frames:1,sample:{values:[2,0,0,0,0,0,0]}};
 trace.update(view);assert.match(nodes['trace-line'].d,/640\.00 0\.00$/);assert.equal(nodes['trace-top'].textContent,'+2.00');
 now=100;trace.update({...view,received_frames:2,sample:{values:[-2,0,0,0,0,0,0]}});assert.match(nodes['trace-line'].d,/640\.00 300\.00$/);assert.equal(nodes['trace-bottom'].textContent,'−2.00');
 trace.clear();assert.equal(nodes['trace-line'].d,'');
});
