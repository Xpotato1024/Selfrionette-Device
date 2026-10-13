import { createDesignPreview } from './design-preview.mjs';
import { createController } from './controller.mjs';
const native = typeof window.__TAURI__?.core?.invoke === 'function';
const transport = native ? request => window.__TAURI__.core.invoke('device_command', { request }) : createDesignPreview();
const $ = id => document.getElementById(id);
const phaseNames = { not_started: '未開始', baseline: '基準の確認待ち', assessment: '応答の確認待ち', preview: '入力の比較待ち', confirmation: '結果の確認待ち', record: '合成結果を確認済み' };
const numbers = new Intl.NumberFormat('ja-JP', { maximumFractionDigits: 4 });
const number = value => value === null || value === undefined ? '—' : numbers.format(value);
const normalized = row => !row ? '—' : row.normalized === null ? '未測定' : number(row.normalized);
const status = row => !row ? '未比較' : row.saturated ? '表示範囲超過' : row.normalized === null ? '逆方向未測定' : '—';
let currentPage = 'device', selectedChannel = 0, currentView = null, isBusy = false, lastTarget = null, pendingConfirmation = null;
$('environment').textContent = native ? 'Rust coreによる合成試験 / 実機校正・制御利用不可' : 'ブラウザーの固定表示 / Rust coreの判定・確認・保存は実行しません';
function clearError() {
  $('error-panel').hidden = true;
  $('reason').removeAttribute('aria-invalid'); $('flip-channel').removeAttribute('aria-invalid');
  $('reason-error').hidden = true;
}
function failure(error) {
  $('message').textContent = '';
  $('error-panel').hidden = false;
  $('error-message').textContent = error.message;
  $('error-detail').textContent = error.code + ': ' + error.detail;
  if (error.field && $(error.field)) {
    $('retry-controls').open = true; $('manual-controls').open = true;
    showPage('adjust', false);
    $(error.field).setAttribute('aria-invalid', 'true');
    $('reason-error').hidden = false;
    $('reason-error').textContent = error.field === 'reason' ? '手動変更の根拠を入力してください。空欄・512 UTF-8 bytes超過では変更できません。' : '受信chは0〜6を指定してください。';
    $(error.field).focus();
  } else {
    $('error-title').focus();
  }
}
function controls() {
  const permissions = new Map((currentView?.operations ?? []).map(o => [o.id, o]));
  document.querySelectorAll('[data-command]').forEach(button => {
    const permission = permissions.get(button.dataset.command);
    button.disabled = isBusy || !permission?.enabled;
    const help = $(button.id + '-help');
    if (help) help.textContent = permission?.reason ?? '';
    const primary = button.id === currentView?.next_action;
    button.classList.toggle('primary', primary);
  });
  $('role').disabled = isBusy || !currentView;
  $('refresh').disabled = isBusy;
  $('dialog-apply').disabled = isBusy || !currentView;
  $('workspace').setAttribute('aria-busy', String(isBusy));
}
function showPage(page, focus = true) {
  if (!['device','adjust','diagnostics'].includes(page)) return;
  currentPage = page;
  document.querySelectorAll('[data-page]').forEach(button => {
    if (button.closest('nav')) {
      if (button.dataset.page === page) button.setAttribute('aria-current', 'page');
      else button.removeAttribute('aria-current');
    }
  });
  for (const name of ['device','adjust','diagnostics']) $('page-' + name).hidden = !currentView || name !== page;
  if (focus && currentView) $(page + '-title').focus({ preventScroll: true });
}
function cell(row, text, className = '') {
  const td = document.createElement('td'); td.textContent = text; td.className = className; row.append(td); return td;
}
function compactRows(target, rows, interactive) {
  const body = $(target); body.replaceChildren();
  for (let ch = 0; ch < 7; ch++) {
    const row = rows.find(r => r.wire === ch), tr = document.createElement('tr');
    const name = cell(tr, '');
    if (interactive) {
      const button = document.createElement('button'); button.type = 'button'; button.className = 'channel-select';
      button.textContent = 'ch ' + ch; button.setAttribute('aria-pressed', String(selectedChannel === ch));
      button.onclick = () => { selectedChannel = ch; renderInspector(); compactRows('input-rows', currentView?.rows ?? [], true); $('input-rows').querySelectorAll('button')[ch].focus(); };
      name.append(button);
    } else name.textContent = 'ch ' + ch;
    const reading = cell(tr, '', 'reading');
    const value = document.createElement('span'); value.className = 'numeric'; value.textContent = normalized(row);
    reading.append(value);
    if (row?.displayed !== null && row?.displayed !== undefined) {
      const meter = document.createElement('meter'); meter.min = 0; meter.max = 1; meter.value = Math.abs(row.displayed);
      meter.setAttribute('aria-label', 'ch ' + ch + ' の表示入力の大きさ');
      meter.title = '表示値 ' + number(row.displayed) + '（範囲0〜1）';
      reading.append(meter);
    }
    cell(tr, status(row), row?.saturated ? 'warn' : ''); body.append(tr);
  }
}
function renderInspector() {
  const row = currentView?.rows.find(r => r.wire === selectedChannel);
  $('channel-title').textContent = 'ch ' + selectedChannel + ' の詳細';
  const values = [['元入力', row?.firmware], ['今回の基準', row?.baseline], ['差分', row?.delta], ['極性', row?.sign], ['向き補正後', row?.oriented], ['正方向範囲', row?.positive_span], ['表示値', row?.displayed]];
  $('channel-values').replaceChildren();
  for (const [name, value] of values) {
    const dt = document.createElement('dt'), dd = document.createElement('dd');
    dt.textContent = name; dd.textContent = number(value); dd.className = 'numeric'; $('channel-values').append(dt, dd);
  }
}
function render(view) {
  const previousPhase = currentView?.phase;
  currentView = view;
  if (view?.next_action === 'preview' || view?.phase === 'preview') $('comparison-controls').open = true;
  else if (view?.phase !== previousPhase) $('comparison-controls').open = false;
  $('unavailable').hidden = !!view;
  if (view && view.id !== lastTarget) {
    selectedChannel = 0; lastTarget = view.id;
    $('reason').value = ''; $('scenario').value = 'good'; $('level').value = '0.5';
    $('retry-controls').open = false; $('manual-controls').open = false;
  }
  if (view) $('role').value = view.role;
  $('device-id').textContent = view?.id ?? '状態不明';
  $('role-summary').textContent = view ? (view.role === 'left' ? '左入力用の合成個体' : '右入力用の合成個体') : '対象を取得できません';
  $('device-phase').textContent = phaseNames[view?.phase] ?? '状態不明';
  $('device-record').textContent = view?.saved_current ? '現在の結果を保存済み' : '現在の結果は未保存';
  $('device-guidance').textContent = view?.guidance ?? '';
  $('step-guidance').textContent = view?.guidance ?? '';
  $('source').textContent = view ? 'synthetic-v2 / fixture-assembly-1 / session ' + view.epoch + ' / 設定revision ' + (view.core_revision ?? '—') : '状態不明';
  $('record-state').textContent = view?.saved_current ? '現在の合成結果は保存済みです。保存しても実機校正済みにはなりません。' : '現在の合成結果は確認済み・未保存です。実機校正済みのprofileにはなりません。';
  $('scenario-summary').textContent = '品質条件：' + $('scenario').selectedOptions[0].textContent + '（変更は「診断・記録」の合成試験から）';
  $('steps').replaceChildren();
  for (const [i, step] of (view?.steps ?? []).entries()) {
    const li = document.createElement('li'); li.textContent = (i + 1) + ' ' + step.label;
    if (step.state === 'current') li.setAttribute('aria-current', 'step');
    const label = document.createElement('span'); label.className = 'step-state'; label.textContent = { current: '現在', complete: '完了', pending: '未開始' }[step.state]; li.append(label); $('steps').append(li);
  }
  document.querySelectorAll('[data-phases]').forEach(panel => { panel.hidden = !view || !panel.dataset.phases.split(' ').includes(view.phase); });
  const rows = view?.rows ?? [];
  compactRows('input-rows', rows, true); compactRows('adjust-rows', rows, false); renderInspector();
  $('rows').replaceChildren();
  for (const row of rows) {
    const tr = document.createElement('tr');
    for (const value of ['ch ' + row.wire, number(row.firmware), number(row.baseline), number(row.delta), number(row.sign), number(row.oriented), number(row.positive_span), normalized(row), number(row.displayed) + (row.saturated ? ' / 範囲超過' : '')]) cell(tr, value);
    if (row.saturated) tr.lastChild.className = 'warn'; $('rows').append(tr);
  }
  $('evidence').textContent = rows.map(r => 'ch ' + r.wire + ': ' + r.evidence).join('\n') || '今回の比較結果はありません。';
  $('saved-profile').textContent = view?.history ? JSON.stringify(view.history, null, 2) : '記録は表示していません。';
  $('history-summary').textContent = view?.history ? '表示中：' + view.history.physical.id + ' / 記録revision ' + view.history.physical.revision + '。比較専用・現在へ未適用。' : '記録は表示していません。';
  controls(); showPage(currentPage, false);
}
const controller = createController(transport, { render, failure, busy: value => { isBusy = value; controls(); } });
function commandFor(action) {
  if (action === 'assess') return { action, scenario: $('scenario').value };
  if (action === 'preview') return { action, level: $('level').value };
  if (action === 'flip') return { action, channel: Number($('flip-channel').value), reason: $('reason').value };
  return { action };
}
async function dispatch(command) {
  clearError();
  const reply = await controller.send(command);
  if (!reply || reply.error) return;
  $('message').textContent = reply.message || '';
  $('announcer').textContent = reply.message || '';
  if (command.action === 'resume') showPage('adjust');
  if (currentPage === 'adjust' && ['begin','baseline','assess','preview','confirm','resume'].includes(command.action)) {
    const next = reply.view.next_action && $(reply.view.next_action);
    if (next && !next.disabled && next.checkVisibility()) next.focus();
    else $('step-guidance').focus();
  }
  if (reply.export) {
    const url = URL.createObjectURL(new Blob([reply.export], { type: 'application/json' }));
    const a = document.createElement('a'); a.href = url; a.download = 'selfrionette-unverified-draft.json'; document.body.append(a); a.click(); a.remove(); URL.revokeObjectURL(url);
  }
}
document.querySelectorAll('[data-page]').forEach(button => { button.onclick = () => showPage(button.dataset.page); });
document.querySelectorAll('[data-command]').forEach(button => {
  button.onclick = () => {
    const command = commandFor(button.dataset.command);
    if (button.dataset.confirm) {
      pendingConfirmation = { command, token: controller.view?.token };
      $('dialog-target').textContent = $('role-summary').textContent + ' / ' + (controller.view?.id ?? '不明');
      $('dialog-message').textContent = button.dataset.confirm === 'resume' ? '現在の基準・確認を失効させ、表示中の記録と条件を照合して新しい調整を開始します。過去の値は自動適用しません。' : 'この個体の現在の基準・候補・確認を破棄し、新しい合成調整を開始します。保存記録は削除しません。';
      $('confirm-dialog').showModal();
    } else void dispatch(command);
  };
});
$('dialog-cancel').onclick = () => { pendingConfirmation = null; $('confirm-dialog').close(); };
$('confirm-dialog').oncancel = () => { pendingConfirmation = null; };
$('dialog-apply').onclick = () => {
  const pending = pendingConfirmation; pendingConfirmation = null; $('confirm-dialog').close();
  if (!pending || JSON.stringify(pending.token) !== JSON.stringify(controller.view?.token)) {
    failure({ code: 'stale_target', message: '確認中に対象または状態が変わりました。操作し直してください。', detail: 'dialog target changed' }); return;
  }
  void dispatch(pending.command);
};
$('role').onchange = () => { void dispatch({ action: 'select', role: $('role').value }); };
$('reason').oninput = () => { $('reason').removeAttribute('aria-invalid'); $('reason-error').hidden = true; };
$('scenario').onchange = () => { if (currentView) render(currentView); };
$('refresh').onclick = () => { void dispatch({ action: 'status' }); };
render(null);
void dispatch({ action: 'status' });
