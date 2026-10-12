import test from 'node:test';
import assert from 'node:assert/strict';
import { createController } from './controller.mjs';
const view = revision => ({ token: { id: 'fixture', epoch: 1, revision }, operations: [] });
function observer() {
  const events = { renders: [], errors: [], pending: [] };
  const handlers = { render: v => events.renders.push(v), failure: e => events.errors.push(e), busy: b => events.pending.push(b) };
  return { events, handlers };
}
test('rapid target changes and double clicks cannot send overlapping mutations', async () => {
  const { events, handlers } = observer(); let release; let calls = 0;
  const c = createController(() => { calls++; return new Promise(r => { release = r; }); }, handlers);
  const first = c.send({ action: 'status' });
  assert.equal(await c.send({ action: 'select', role: 'right' }), null); assert.equal(calls, 1);
  release({ view: view(1) }); await first;
  assert.equal(events.renders.length, 1); assert.deepEqual(events.pending, [true, false]);
});
test('commands carry the latest target token; stale replies cannot replace current view', async () => {
  const { events, handlers } = observer(); const requests = [];
  const revisions = [4, 3, 5];
  const c = createController(async request => { requests.push(request); return { view: view(revisions.shift()) }; }, handlers);
  await c.send({ action: 'status' }); await c.send({ action: 'begin' }); await c.send({ action: 'baseline' });
  assert.equal(requests[1].expected.revision, 4); assert.equal(requests[2].expected.revision, 4);
  assert.deepEqual(events.renders.map(v => v.token.revision), [4, 5]);
});
test('domain errors render the authoritative failed state rather than the old approval', async () => {
  const { events, handlers } = observer();
  const error = { code: 'inconclusive', message: 'failed' };
  const c = createController(async () => ({ view: view(2), error }), handlers);
  await c.send({ action: 'assess', scenario: 'timeout' });
  assert.equal(c.view.token.revision, 2); assert.deepEqual(events.errors, [error]);
});
test('transport failure recovers status; unavailable status clears the view and actions', async () => {
  const { events, handlers } = observer(); let calls = 0;
  const c = createController(async () => { if (++calls <= 1) return { view: view(1) }; throw Error('offline'); }, handlers);
  await c.send({ action: 'status' }); await c.send({ action: 'save' });
  assert.equal(c.view, null); assert.equal(events.renders.at(-1), null); assert.equal(events.errors.at(-1).code, 'transport');
});
