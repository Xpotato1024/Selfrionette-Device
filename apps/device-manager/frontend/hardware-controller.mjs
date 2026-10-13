// Serialized commands; scans enumerate but never connect. No device mutation is inferred.
export function createHardwareController(invoke, {render, error, busy}) {
  let view = null, inFlight = false;
  return {
    get view() { return view; },
    async send(command, expected) {
      if (inFlight) return null;
      inFlight = true; busy(true, command.action);
      try {
        const reply = await invoke({ expected: expected ?? view?.revision ?? null, command });
        view = reply.view; render(view);
        error(reply.error ?? ''); return reply;
      } catch (failure) {
        view = null; render(null);
        error('接続状態を取得できません。再検索で状態を確認してください。' + String(failure));
        return null;
      } finally { inFlight = false; busy(false, command.action); }
    }
  };
}
