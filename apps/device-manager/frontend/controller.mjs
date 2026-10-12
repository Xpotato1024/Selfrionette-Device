// Transport sequencing only. Calibration validity and actions belong to Rust.
export function createController(transport, { render, failure, busy }) {
  let view = null;
  let pending = false;
  const accept = next => {
    if (!next?.token || !Array.isArray(next.operations)) throw Error('Invalid workflow view');
    if (view && next.token.revision < view.token.revision) return false;
    view = next;
    render(view);
    return true;
  };
  return {
    get view() { return view; },
    async send(command) {
      if (pending) return null;
      pending = true;
      busy(true);
      try {
        const reply = await transport({ expected: view?.token ?? null, command });
        if (!accept(reply.view)) return null;
        if (reply.error) failure(reply.error);
        return reply;
      } catch (error) {
        try {
          const reply = await transport({ expected: null, command: { action: 'status' } });
          accept(reply.view);
        } catch {
          view = null;
          render(null);
        }
        failure({ code: 'transport', message: '状態を取得できませんでした。状態を再取得してから操作してください。', detail: String(error) });
        return null;
      } finally {
        pending = false;
        busy(false);
      }
    },
  };
}
