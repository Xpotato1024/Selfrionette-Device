import test from 'node:test';
import assert from 'node:assert/strict';
import { createDesignPreview } from './design-preview.mjs';
test('static browser fixture cannot evaluate, approve, save, resume or export', async () => {
  const call = createDesignPreview();
  const before = (await call({ command: { action: 'status' } })).view;
  assert.equal(before.hardware_verified, false); assert.equal(before.control_ready, false);
  assert.equal(before.rows.length, 7); assert(before.rows.every(r => r.physical === null));
  assert(before.operations.every(o => !o.enabled));
  for (const action of ['begin','baseline','assess','flip','preview','confirm','save','export','read_history','resume','invalidate']) await assert.rejects(call({ command: { action } }));
  assert.deepEqual((await call({ command: { action: 'status' } })).view, before);
});
test('browser selection changes only the fixed presentation target', async () => {
  const call = createDesignPreview();
  const v = (await call({ command: { action: 'select', role: 'right' } })).view;
  assert.equal(v.role, 'right'); assert.match(v.id, /2222/); assert.equal(v.epoch, 0);
  assert.equal(v.rows[0].normalized, 0.5);
  await assert.rejects(call({ command: { action: 'select', role: 'other' } }));
});
