import test from 'node:test';
import assert from 'node:assert/strict';
import {installDesktopFixture} from '../tools/desktop-fixture.mjs';
test('display fixture mirrors the real stream_tick without inventing calibration',async()=>{
 const previousWindow=globalThis.window,previousStorage=globalThis.localStorage;
 const store=new Map();globalThis.window={};globalThis.localStorage={getItem:key=>store.get(key)??null,setItem:(key,value)=>store.set(key,value)};
 try{
  installDesktopFixture({connected:false});const invoke=command=>window.__TAURI__.core.invoke('application_command',{command});
  const target=window.testFeed.ports.find(port=>port.name==='COM8');
  await invoke({action:'connect',target});await invoke({action:'stream_start'});
  for(let i=0;i<3;i++){const {view}=await invoke({action:'stream_tick'});assert.equal(view.connected,true);assert.equal(view.owns_handle,true);assert.equal(view.ready,false);}
  const paused=await invoke({action:'pause'});assert.equal(paused.view.owns_handle,false);assert.equal(paused.view.ready,false);
  const resumed=await invoke({action:'reconnect'});assert.equal(resumed.view.candidate.name,'COM8');assert.equal(resumed.view.ready,false);
  await assert.rejects(invoke({action:'capture_tick'}),/表示fixture/);
 }finally{if(previousWindow===undefined)delete globalThis.window;else globalThis.window=previousWindow;if(previousStorage===undefined)delete globalThis.localStorage;else globalThis.localStorage=previousStorage;}
});
