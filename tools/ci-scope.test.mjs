import test from 'node:test';import assert from 'node:assert/strict';import {classifyChanges as scope} from './ci-scope.mjs';
test('CSS-only changes use fast rendering tests, not a Rust build',()=>assert.deepEqual(scope(['apps/device-manager/frontend/style.css']),{frontend:true,core:false,native:false}));
test('test harness changes do not imply a product rebuild',()=>assert.deepEqual(scope(['apps/device-manager/tools/native-smoke.mjs']),{frontend:true,core:false,native:false}));
test('core changes exercise both host platforms and native boundary',()=>assert.deepEqual(scope(['crates/selfrionette-serial/src/lib.rs']),{frontend:false,core:true,native:true}));
test('shell/config changes require native acceptance',()=>assert.equal(scope(['apps/device-manager/src-tauri/tauri.conf.json']).native,true));
test('unknown paths, contracts, manual full runs fail toward more checking',()=>{assert.deepEqual(scope(['new-module/file']),{frontend:true,core:true,native:true});assert.equal(scope(['docs/contracts/gui-live-monitor.md']).native,true);assert.deepEqual(scope([],true),{frontend:true,core:true,native:true});});

test('UI IPC bindings select the real native boundary, not only a mock renderer',()=>{for(const p of ['apps/device-manager/frontend/app.mjs','apps/device-manager/frontend/hardware-ui.mjs','apps/device-manager/frontend/hardware-controller.mjs']){const result=scope([p]);assert(result.frontend);assert(result.native);}});
test('quality firmware and application service bindings cannot skip native validation',()=>{for(const p of ['firmware/loadcell_7ch/src/main.cpp','firmware/shared/quality.h','apps/device-manager/frontend/application-ui.mjs','apps/device-manager/frontend/application-controller.mjs']){assert(scope([p]).native,p);}});
