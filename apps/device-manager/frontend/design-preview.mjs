// A fixed presentation fixture, not a second calibration implementation.
export function createDesignPreview() {
  let role = 'left', revision = 0;
  const ids = { left: 'srn-11111111111111111111111111111111', right: 'srn-22222222222222222222222222222222' };
  const snapshot = () => ({
    token: { id: ids[role], epoch: 0, revision }, id: ids[role], role, epoch: 0, core_revision: null,
    phase: 'confirmation', guidance: '固定の表示例です。校正判定・確認・保存はnative GUIで行ってください。', next_action: null,
    operations: ['begin','baseline','assess','flip','preview','confirm','save','export','read_history','resume','invalidate'].map(id => ({ id, enabled: false, reason: '固定表示専用です。Rust coreは実行していません。' })),
    steps: ['基準','応答・範囲','入力比較','結果確認','記録'].map((label, i) => ({ label, state: i < 3 ? 'complete' : i === 3 ? 'current' : 'pending' })),
    devices: Object.entries(ids).map(([role, id]) => ({ role, id, phase: 'confirmation' })),
    rows: Array.from({ length: 7 }, (_, wire) => ({ wire, physical: null, firmware: 20, baseline: 10, delta: 10, sign: 1, oriented: 10, positive_span: 20, normalized: 0.5, displayed: 0.5, saturated: false, evidence: '固定の合成表示。Rust判定ではありません。' })),
    history: null, saved_current: false, hardware_verified: false, control_ready: false,
  });
  return async ({ command }) => {
    if (command.action === 'select' && Object.hasOwn(ids, command.role)) { role = command.role; revision++; }
    else if (command.action !== 'status') throw Error('Presentation only: no calibration, approval, persistence, or device I/O');
    return { view: snapshot(), error: null, export: null, message: '' };
  };
}
