import { createHardwareUI } from './hardware-ui.mjs';
import { createDesignPreview } from './design-preview.mjs';
import { createController } from './controller.mjs';
import { createStepNavigation, stepKeys, currentStep, canVisit } from './navigation.mjs';
const stepNavigation = createStepNavigation();
const native = typeof window.__TAURI__?.core?.invoke === 'function';
const designPreview = !native && new URLSearchParams(location.search).get('design-preview') === '1';
const transport = native ? request => window.__TAURI__.core.invoke('device_command', { request }) : createDesignPreview();
const $ = id => document.getElementById(id);
let sourceMode = 'hardware', sessionMode = null, modeQueue = Promise.resolve();
const hardware = createHardwareUI(request => window.__TAURI__.core.invoke('hardware_command', {request}));
const phaseNames = { not_started: '未開始', baseline: '基準の確認待ち', assessment: '応答の確認待ち', preview: '入力の比較待ち', confirmation: '結果の確認待ち', record: '合成結果を確認済み' };
const numbers = new Intl.NumberFormat('ja-JP', { maximumFractionDigits: 4 });
const number = value => value === null || value === undefined ? '—' : numbers.format(value);
const normalized = row => !row ? '—' : row.normalized === null ? '未測定' : number(row.normalized);
const status = row => !row ? '未比較' : row.saturated ? '表示範囲超過' : row.normalized === null ? '逆方向未測定' : '—';
let currentPage = 'device', selectedChannel = 0, currentView = null, isBusy = false, lastTarget = null, pendingConfirmation = null;
$('environment').textContent = native ? '未接続' : '表示見本 · 判定と保存は無効';
function clearError() {
  $('error-panel').hidden = true;
  $('reason').removeAttribute('aria-invalid'); $('flip-channel').removeAttribute('aria-invalid');
  $('reason-error').hidden = true;
}
function failure(error) {
  $('error-panel').hidden = !!error.field;
  $('error-message').textContent = error.message;
  $('error-detail').textContent = error.code + ': ' + error.detail;
  if (error.field && $(error.field)) {
    stepNavigation.visit(currentView, 'assessment');
    renderNavigation(); showPage('adjust', false); $('manual-controls').open = true;
    $(error.field).setAttribute('aria-invalid', 'true');
    $('reason-error').hidden = false;
    $('reason-error').textContent = error.field === 'reason' ? '根拠を入力してください（512 UTF-8 bytes以内）。' : '受信chは0〜6です。';
    $(error.field).focus();
  } else $('error-title').focus();
}
function controls() {
  const permissions = new Map((currentView?.operations ?? []).map(o => [o.id, o]));
  const position = stepNavigation.position(currentView);
  let primary = position === 'baseline' ? (currentView?.phase === 'not_started' ? 'begin' : 'baseline') : {assessment:'assess',preview:'preview',confirmation:'confirm',record:'save'}[position];
  if (position === 'confirmation' && !permissions.get('confirm')?.enabled && currentView?.rows.some(row => row.saturated)) primary = 'back-to-comparison';
  document.querySelectorAll('[data-command]').forEach(button => {
    const permission = permissions.get(button.dataset.command);
    button.disabled = isBusy || !permission?.enabled;
    const help = $(button.id + '-help');
    if (help) help.textContent = button.hidden || (button.id === 'save' && currentView?.saved_current) ? '' : permission?.reason ?? '';
    button.classList.toggle('primary', button.id === primary && !button.disabled);
  });
  $('back-to-comparison').classList.toggle('primary', primary === 'back-to-comparison');
  $('role').disabled = isBusy || !currentView;
  $('refresh').disabled = isBusy;
  $('dialog-apply').disabled = isBusy || !currentView;
  $('workspace').setAttribute('aria-busy', String(isBusy));
}
function renderNavigation() {
  const position = stepNavigation.position(currentView);
  const active = currentStep(currentView);
  $('steps').replaceChildren();
  for (const [index, step] of (currentView?.steps ?? []).entries()) {
    const key = stepKeys[index], li = document.createElement('li'), button = document.createElement('button');
    button.type = 'button'; button.dataset.stepLink = key;
    button.setAttribute('aria-disabled', String(!canVisit(currentView, key)));
    if (position === key) button.setAttribute('aria-current', 'step');
    const number = document.createElement('span'); number.className = 'step-number'; number.textContent = String(index + 1); number.setAttribute('aria-hidden','true');
    const title = document.createElement('span'); title.className = 'step-label'; title.textContent = step.label;
    const state = document.createElement('span'); state.className = 'step-state'; state.textContent = currentView.phase === 'not_started' && index === 0 ? '開始前' : {complete:'完了',current:'未完了',pending:'未到達'}[step.state];
    button.setAttribute('aria-label', (index+1)+' '+step.label+' '+state.textContent);
    button.append(number,title,state); button.onclick = () => visitStep(key);
    li.append(button); $('steps').append(li);
  }
  document.querySelectorAll('[data-step]').forEach(panel => { panel.hidden = !currentView || panel.dataset.step !== position; });
  $('begin').hidden = currentView?.phase !== 'not_started';
  $('baseline').hidden = currentView?.phase === 'not_started';
  $('restart').hidden = !currentView || ['not_started','baseline'].includes(currentView.phase);
  $('back-to-comparison').hidden = !currentView?.rows.some(row => row.saturated);
  const previous = stepKeys.indexOf(position) - 1;
  $('previous-step').hidden = previous < 0 || (position === 'confirmation' && currentView?.rows.some(row => row.saturated));
  $('previous-step').textContent = position === 'confirmation' ? '入力を変えて再比較' : '前の手順';
  $('previous-step').onclick = () => visitStep(stepKeys[previous]);
  $('return-current').hidden = position === active || !!currentView?.saved_current;
  $('return-current').onclick = () => visitStep(active);
  $('step-footer').hidden = $('previous-step').hidden && $('return-current').hidden;
  $('manual-controls').hidden = !(currentView?.operations.some(operation => operation.id === 'flip' && operation.enabled));
  controls();
}
function visitStep(key) {
  if (!stepNavigation.visit(currentView,key)) return;
  renderNavigation();

  document.querySelector('[data-step="'+key+'"] h2')?.focus({preventScroll:true});
}
function showPage(page, focus = true) {
  if (!['device','adjust','diagnostics'].includes(page)) return;
  currentPage = page;
  hardware.showPage(page);
  document.querySelectorAll('[role="tab"]').forEach(button => {
    const selected = button.dataset.page === page;
    button.setAttribute('aria-controls', (sourceMode === 'hardware' ? 'hardware-page-' : 'page-') + button.dataset.page);
    button.setAttribute('aria-selected', String(selected)); button.tabIndex = selected ? 0 : -1;
  });
  for (const name of ['device','adjust','diagnostics']) $('page-' + name).hidden = sourceMode !== 'synthetic' || !currentView || name !== page;
  if (sourceMode === 'hardware') {
    if (focus) $('hardware-page-' + page).querySelector('h1')?.focus({preventScroll:true});
    return;
  }
  if (focus && currentView) {
    const heading = page === 'adjust' ? document.querySelector('[data-step="'+stepNavigation.position(currentView)+'"] h2') : $(page+'-title');
    heading?.focus({preventScroll:true});
  }
}
function cell(row, text, className = '') {
  const td = document.createElement('td'); td.textContent = text; td.className = className; row.append(td); return td;
}
function compactRows(target, rows, interactive) {
  const body = $(target); body.replaceChildren();
  const table = body.closest('table');
  table.classList.toggle('hide-notes', !rows.some(row => row.saturated || row.normalized === null));
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
  const previous = currentView; currentView = view;
  if (view && previous?.id === view.id && (previous.phase !== view.phase || previous.epoch !== view.epoch)) stepNavigation.follow(view);
  $('unavailable').hidden = sourceMode !== 'synthetic' || !!view;
  if (view && view.id !== lastTarget) {
    selectedChannel = 0; lastTarget = view.id;
    $('reason').value = ''; $('scenario').value = 'good'; $('level').value = '0.5'; $('manual-controls').open = false;
  }
  if (view) $('role').value = view.role;
  $('device-id').textContent = view?.id ?? '状態不明';
  $('device-id').title = view?.id ?? '';
  $('role-summary').textContent = view ? (view.role === 'left' ? '左入力用の合成個体' : '右入力用の合成個体') : '対象を取得できません';
  $('device-phase').textContent = phaseNames[view?.phase] ?? '状態不明';
  $('device-record').textContent = view?.saved_current ? '現在の結果を保存済み' : '未保存';
  $('source').textContent = view ? 'synthetic-v2 / session ' + view.epoch + ' / revision ' + (view.core_revision ?? '—') : '状態不明';
  $('record-state').textContent = view?.saved_current ? '現在の結果は保存済みです。' : '確認した結果と測定の根拠をPCに保存します。装置の設定は変更しません。';
  const rows = view?.rows ?? [];
  compactRows('input-rows', rows, true); compactRows('adjust-rows', rows, false); renderInspector();
  $('input-rows').closest('table').hidden = rows.length === 0;
  $('input-empty').hidden = rows.length !== 0;
  $('channel-detail').hidden = rows.length === 0;
  $('rows').replaceChildren();
  for (const row of rows) {
    const tr = document.createElement('tr');
    for (const value of ['ch ' + row.wire, number(row.firmware), number(row.baseline), number(row.delta), number(row.sign), number(row.oriented), number(row.positive_span), normalized(row), number(row.displayed) + (row.saturated ? ' / 範囲超過' : '')]) cell(tr, value);
    if (row.saturated) tr.lastChild.className = 'warn'; $('rows').append(tr);
  }
  $('evidence').textContent = rows.map(r => 'ch ' + r.wire + ': ' + r.evidence).join('\n') || '比較結果はありません。';
  $('saved-profile').textContent = view?.history ? JSON.stringify(view.history, null, 2) : '';
  $('history-summary').textContent = view?.history ? '表示中：記録revision '+view.history.physical.revision+' · 比較専用（現在へ未適用）' : '記録は表示していません。';
  renderNavigation(); showPage(currentPage, false);
}

const controller = createController(transport, { render, failure, busy: value => { isBusy = value; controls(); } });
function commandFor(action) {
  if (action === 'assess') return { action, scenario: $('scenario').value };
  if (action === 'preview') return { action, level: $('level').value };
  if (action === 'flip') return { action, channel: Number($('flip-channel').value), reason: $('reason').value };
  return { action };
}
async function dispatch(command) {
  if (sourceMode !== 'synthetic') return;
  clearError();
  const restoreTargetFocus = command.action === 'select' && document.activeElement === $('role');
  const reply = await controller.send(command);
  if (!reply || reply.error) return;
  if (restoreTargetFocus && document.activeElement === document.body) $('role').focus({preventScroll:true});
  $('announcer').textContent = reply.message || '';
  if (command.action === 'resume') showPage('adjust');
  if (['begin','baseline','assess','preview','confirm','resume','invalidate'].includes(command.action)) {
    stepNavigation.follow(reply.view); renderNavigation();
    if (currentPage === 'adjust') document.querySelector('[data-step="'+stepNavigation.position(reply.view)+'"] h2')?.focus({preventScroll:true});
  }

  if (reply.export) {
    const url = URL.createObjectURL(new Blob([reply.export], { type: 'application/json' }));
    const a = document.createElement('a'); a.href = url; a.download = 'selfrionette-unverified-draft.json'; document.body.append(a); a.click(); a.remove(); URL.revokeObjectURL(url);
  }
}
document.querySelectorAll('[data-page]').forEach(button => { button.onclick = () => showPage(button.dataset.page, button.getAttribute('role') !== 'tab'); });
document.querySelectorAll('[data-command]').forEach(button => {
  button.onclick = () => {
    const command = commandFor(button.dataset.command);
    const repeated = ['baseline','assess'].includes(command.action) && currentView?.steps[command.action === 'baseline' ? 0 : 1]?.state === 'complete';
    if (button.dataset.confirm || repeated) {
      pendingConfirmation = { command, token: controller.view?.token };
      $('dialog-target').textContent = $('role-summary').textContent + ' / ' + (controller.view?.id ?? '不明');
      $('dialog-message').textContent = button.dataset.confirm === 'resume' ? '現在の基準・確認を失効させ、表示中の記録と条件を照合して新しい調整を開始します。過去の値は自動適用しません。' : repeated ? 'この手順を再実行すると、依存する後続の比較・確認が失効します。保存記録は削除しません。' : '現在の基準・候補・確認を破棄してやり直します。保存記録は削除しません。';
      $('dialog-apply').textContent = repeated ? (command.action === 'baseline' ? '基準を再取得' : '応答を再確認') : command.action === 'resume' ? '新しく調整を開始' : 'やり直す';
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

$('refresh').onclick = () => { void dispatch({ action: 'status' }); };
document.querySelectorAll('[data-step-link]').forEach(button => { button.onclick = () => visitStep(button.dataset.stepLink); });
const tabs = [...document.querySelectorAll('[role="tab"]')];
for (const tab of tabs) tab.addEventListener('keydown', event => {
  const index = tabs.indexOf(tab);
  const target = {ArrowRight:(index+1)%tabs.length, ArrowLeft:(index+tabs.length-1)%tabs.length, Home:0, End:tabs.length-1}[event.key];
  if (target !== undefined) { event.preventDefault(); tabs[target].focus(); }
});
// Product information is read from the bundled config, never from device state.
async function showAbout() {
  for (const key of ['product','version','publisher','homepage']) $('about-'+key).textContent = '—';
  $('about-note').textContent = '発行元の名称は製品情報です。電子署名による本人性の検証とは異なります。';
  $('about-dialog').showModal();
  try {
    const info = native ? await window.__TAURI__.core.invoke('application_info') : {
      product_name: document.title, version: 'ブラウザー表示見本', publisher: '—', homepage: '—'
    };
    $('about-product').textContent = info.product_name;
    $('about-version').textContent = info.version;
    $('about-publisher').textContent = info.publisher ?? '未設定';
    $('about-homepage').textContent = info.homepage ?? '未設定';
  } catch {
    $('about-note').textContent = '製品情報を取得できませんでした。配布物のBUILD-INFO.jsonを確認してください。';
  }
};
$('about-close').onclick = () => $('about-dialog').close();
async function applyMode(mode) {
  if(sessionMode && mode.epoch <= sessionMode.epoch)return;
  const changed=!sessionMode||mode.source!==sourceMode;sessionMode=mode;
  sourceMode=mode.source;clearError();
  $('developer-badge').hidden=!mode.developer;
  $('synthetic-target').hidden=sourceMode!=='synthetic';$('unavailable').hidden=true;
  if(changed) {
    await hardware.activate(sourceMode==='hardware',currentPage);
    if(sourceMode==='synthetic')await dispatch({action:'status'});
  }
  if(sourceMode==='synthetic')$('environment').textContent=native?'合成試験 · 実機未接続':'表示見本 · 判定と保存は無効';
  $('workspace').dataset.source=sourceMode;$('workspace').dataset.developer=String(mode.developer);
  showPage(currentPage,false);
}
function queueMode(mode){modeQueue=modeQueue.then(()=>applyMode(mode));return modeQueue;}
async function changeMode(command) {
  if(hardware.busy||isBusy){failure({code:'busy',message:'現在の操作が完了してから切り替えてください。',detail:'mode transition blocked while operation is pending'});return;}
  try {
    const mode=await window.__TAURI__.core.invoke('mode_command',{request:{expected:sessionMode?.epoch??null,command}});
    await queueMode(mode);
  } catch(error) {
    failure({code:'mode_rejected',message:String(error),detail:'Rust mode gate'});
    const current=await window.__TAURI__.core.invoke('mode_command',{request:{expected:null,command:{action:'snapshot'}}}).catch(()=>null);
    if(current)await queueMode(current);
  }
}
function menuAction(id) {
  if(document.querySelector('dialog[open]'))return;
  if(id.startsWith('view-')){showPage(id.slice(5));return;}
  switch(id) {
    case 'choose-device': void hardware.choose();break;
    case 'disconnect': void hardware.disconnect();break;
    case 'about': void showAbout();break;
    case 'help': $('help-dialog').showModal();break;
    case 'enable-developer': $('developer-dialog').showModal();break;
    case 'disable-developer': void changeMode({action:'disable'});break;
    case 'source-hardware': void changeMode({action:'select',source:'hardware'});break;
    case 'source-synthetic': void changeMode({action:'select',source:'synthetic'});break;
  }
}
$('developer-cancel').onclick=()=>$('developer-dialog').close();
$('developer-enable').onclick=async()=>{$('developer-dialog').close();await changeMode({action:'enable'});};
$('help-close').onclick=()=>$('help-dialog').close();
render(null);
async function initialize(){
  if(native) {
    await window.__TAURI__.event.listen('manager-menu',event=>menuAction(event.payload));
    await window.__TAURI__.event.listen('manager-mode',event=>{void queueMode(event.payload);});
    const mode=await window.__TAURI__.core.invoke('mode_command',{request:{expected:null,command:{action:'snapshot'}}});
    await queueMode(mode);
  } else if(designPreview) await queueMode({developer:true,source:'synthetic',epoch:0});
  else throw Error('Native runtime is unavailable. Browser design preview requires an explicit preview launch.');
}
void initialize().catch(error=>failure({code:'startup',message:'画面を初期化できません。再起動してください。',detail:String(error)}));
