import {createHardwareController} from './hardware-controller.mjs';
export function createHardwareUI(invoke) {
  const $ = id => document.getElementById(id);
  let active = false, busy = false, action = '', page = 'device', timer = null, expiry = null, pending = null;
  const names = {disconnected:'未接続', waiting:'接続済み · 受信待ち', receiving:'7ch受信中', stale:'入力更新なし', error:'接続停止'};
  const numbers = new Intl.NumberFormat('ja-JP', {maximumFractionDigits:3});
  const controller = createHardwareController(invoke, {render, error: text => {
    const message = text || controller.view?.failure || '';
    $('hardware-error').textContent = message; $('hardware-error').hidden = !active || !message;
  }, busy: (value, current) => {busy = value; action = current; controls();}});
  function controls() {
    const view = controller.view;
    $('hardware-scan').disabled = busy;
    $('hardware-connect').disabled = busy || !view || view.connected || !$('hardware-port').value;
    $('hardware-disconnect').disabled = busy || !view?.connected;
    $('hardware-port').disabled = !!view?.connected || (busy && action === 'connect');
    $('hardware-info').disabled = busy || !view?.connected || view.info_pending;
    if (active) $('source-mode').disabled = !view || view.connected || (busy && action === 'connect');
  }
  function render(view) {
    clearTimeout(expiry);
    if (view?.sample) expiry = setTimeout(() => {
      $('hardware-values').hidden = true; $('hardware-empty').hidden = false;
      $('hardware-empty').textContent = '新しい入力を確認できないため、以前の値を消しています。';
      if (active) $('environment').textContent = '入力更新なし';
    }, Math.max(1, 501 - view.sample.received_age_ms));
    const selected = $('hardware-port').value;
    const ports = view?.ports ?? [];
    const signature = JSON.stringify(ports);
    if ($('hardware-port').dataset.options !== signature) {
      const select = $('hardware-port'); select.replaceChildren(new Option('接続先を選択', ''));
      for (const port of ports) select.add(new Option(port.name + (port.product ? ' · ' + port.product : ''), port.name));
      select.dataset.options = signature;
      select.value = view?.connected ? view.port : selected;
    }
    if (active) $('environment').textContent = names[view?.phase] ?? '状態不明';
    $('hardware-empty').textContent = !view ? '接続状態を取得できません。' : view.failure ? '接続先を確認して接続し直してください。' :
      view.phase === 'disconnected' ? ports.length ? '接続先を選び「接続」を押してください。OSのポート一覧であり、すべてがSelfrionetteとは限りません。' : 'シリアルポートが見つかりません。USB接続後に再検索してください。' :
      view.phase === 'waiting' ? '7ch入力を待っています。値が届かない場合は、接続先・通信形式・Firmwareを確認してください。' :
      view.phase === 'stale' ? '500ms以上、新しい時刻の入力が届いていないため、以前の値を消しています。' : '';
    $('hardware-empty').hidden = !view || !!view.sample;
    $('hardware-values').hidden = !view?.sample;
    $('hardware-value-rows').replaceChildren();
    if (view?.sample) for (const [channel,value] of view.sample.values.entries()) {
      const row = document.createElement('tr');
      for (const text of ['ch ' + channel, Math.abs(value) >= 1e8 ? value.toExponential(3) : numbers.format(value)]) {
        const cell = document.createElement('td'); cell.textContent = text; row.append(cell);
      }
      $('hardware-value-rows').append(row);
    }
    $('hardware-id').textContent = view?.reported_id === 'unprovisioned' ? '未登録' : view?.reported_id ?? '未照会 / 応答なし';
    $('hardware-firmware').textContent = view?.firmware ?? '—';
    $('hardware-protocol').textContent = view?.protocol ?? '—';
    $('hardware-frame-count').textContent = view?.received_frames ?? 0;
    $('hardware-diagnostics').textContent = view?.diagnostics.join('\n') || '受信した診断メッセージはありません。';
    controls();
  }
  async function tick() {
    if (!active) return;
    if (!$('hardware-connect-dialog').open) await controller.send({action:controller.view?.connected ? 'read' : 'scan'});
    if (active) timer = setTimeout(tick, controller.view?.connected ? 150 : 2000);
  }
  function showPage(name) {
    page = name;
    for (const key of ['device','adjust','diagnostics']) $('hardware-page-' + key).hidden = !active || key !== page;
  }
  $('hardware-scan').onclick = () => { void controller.send({action:'scan'}); };
  $('hardware-port').onchange = controls;
  $('hardware-connect').onclick = () => {
    if (!controller.view || controller.view.connected) return;
    pending = {port:$('hardware-port').value, revision:controller.view.revision};
    $('hardware-connect-target').textContent = $('hardware-port').selectedOptions[0]?.textContent ?? '';
    $('hardware-connect-dialog').showModal();
  };
  $('hardware-connect-cancel').onclick = () => {pending = null; $('hardware-connect-dialog').close();};
  $('hardware-connect-dialog').oncancel = () => {pending = null;};
  $('hardware-connect-confirm').onclick = () => {
    const target = pending; pending = null; $('hardware-connect-dialog').close();
    if (target) void controller.send({action:'connect',port:target.port},target.revision);
  };
  $('hardware-disconnect').onclick = () => { void controller.send({action:'disconnect'}); };
  $('hardware-info').onclick = () => { void controller.send({action:'info'}); };
  return {
    get connected() { return !!controller.view?.connected; },
    async activate(value, name = page) {
      active = value; clearTimeout(timer); clearTimeout(expiry); showPage(name);
      $('hardware-target').hidden = !active;
      if (!active) $('hardware-error').hidden = true;
      if (active) { await controller.send({action:'scan'}); if (active) timer = setTimeout(tick,2000); }
    },
    showPage,
  };
}
