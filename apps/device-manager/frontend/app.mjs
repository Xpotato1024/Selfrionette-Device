import { createDesignPreview } from './design-preview.mjs';
const native = typeof window.__TAURI__?.core?.invoke === 'function';
const designPreview = createDesignPreview();
const call = native ? (action, argument='') => window.__TAURI__.core.invoke('fixture_action', {action, argument}) : designPreview;
const $ = id => document.getElementById(id);
$('environment').textContent = native ? 'Rust coreを使うoffline GUI。合成fixtureのみ。実機は未接続です。' : 'ブラウザーの表示確認モード：固定の合成表示です。Rust coreの校正判定は実行しません。';
const buttons = ['baseline','assess','flip','preview','confirm','export','save','load','invalidate'];
const stepLabels = ['1 個体と対応','2 基準を確認','3 既知方向・繰返し','4 補正値を比較','5 draft確認・保存'];
const guidance = ['対象を選び、合成試験を開始してください。','現在1/5：対象側と合成IDを確認し、合成基準を作成してください。','現在2/5：基準を確認できました。品質条件を選び、符号候補・操作範囲を確認してください。','現在3/5：候補を確認できました。元入力と補正値を比較してください。','現在4/5：全7chと未測定・飽和を確認してから、このプレビューを確認してください。','現在5/5：未検証draftを保存・書き出せます。実機の校正済みprofileにはなりません。'];
function clearInputError() {
  $('reason').removeAttribute('aria-invalid');
  $('reason-error').hidden=true; $('reason-error').textContent='';
  $('error-details').hidden=true; $('error-detail').textContent='';
}
function render(view) {
  $('device-id').textContent = view.id || '未選択';
  $('role-summary').textContent = '対象：'+(view.role==='right'?'右（Right）':view.role==='left'?'左（Left）':'未開始')+' / 合成試験';
  $('saved-profile').textContent = view.saved_profile || '再読込していません。';
  $('source').textContent = view.provenance + ' / epoch ' + view.epoch;
  $('rows').replaceChildren();
  for (const row of view.rows) {
    const tr = document.createElement('tr');
    const values = [row.wire+' → '+row.physical,row.firmware,row.baseline,row.delta,row.sign,row.oriented,row.positive_span,row.normalized ?? '逆方向span未測定',String(row.displayed ?? '不明')+(row.saturated?' / SATURATED':'')];
    for (const value of values) {const td=document.createElement('td');td.textContent=String(value);tr.append(td);}
    if(row.saturated) tr.lastChild.className='warn';
    $('rows').append(tr);
  }
  $('evidence').textContent = view.rows.map(r=>'wirech'+r.wire+': '+r.evidence).join('\n') || '合成fixtureのみ。実番号・個体・極性は未確認です。';
  const stage = view.stage;
  [...$('steps').children].forEach((li,i)=>{
    const current=stage===i+1;
    li.classList.toggle('active',current);
    if(current) li.setAttribute('aria-current','step'); else li.removeAttribute('aria-current');
    li.textContent=stepLabels[i];
    const state=document.createElement('span'); state.className='step-state';
    state.textContent=current?'現在':stage>i+1?'完了':'未開始';li.append(state);
  });
  $('step-guidance').textContent=guidance[stage] || guidance[0];
  const next=['begin','baseline','assess','preview','confirm','save'][stage];
  ['begin',...buttons].forEach(id=>$(id).classList.toggle('current-action',id===next));
  $('baseline').disabled=stage<1; $('assess').disabled=stage<2; $('flip').disabled=stage<3;
  $('preview').disabled=stage<3; $('confirm').disabled=stage!==4; $('export').disabled=stage!==5; $('invalidate').disabled=stage<1; $('save').disabled=!native||stage!==5; $('load').disabled=!native||stage<1;
  $('message').textContent=view.message; $('message').className='';
  return view;
}
async function action(name, argument='') {
  clearInputError();
  try { buttons.forEach(id=>$(id).disabled=true); return render(await call(name,argument)); }
  catch(error) {
    try { render(await call('status')); }
    catch { render({stage:0,id:'',epoch:0,provenance:'状態を取得できません',rows:[],message:''}); }
    $('message').textContent='判定不可：操作に必要な条件を満たしていません。現在の工程と技術情報を確認してください。';
    if(name==='flip'&&!$('reason').value.trim()) {
      const explanation='＊手動変更の根拠を入力してください。未確認の場合は反転せず、確認をやり直してください。';
      $('reason-error').textContent=explanation; $('reason-error').hidden=false;
      $('reason').setAttribute('aria-invalid','true'); $('reason').focus();
      $('message').textContent='判定不可：手動変更の根拠が未入力です。入力欄の説明を確認してください。';
    } else if(name==='assess') {
      $('message').textContent='判定不可：品質または既知方向の条件を満たしていません。条件を確認し、基準・観測をやり直してください。';
    }
    $('error-details').hidden=false; $('error-detail').textContent=String(error);
    $('message').className='error';return null;
  }
}
$('reason').oninput=()=>{ $('reason').removeAttribute('aria-invalid'); $('reason-error').hidden=true; };
$('begin').onclick=()=>action('begin',$('role').value);
$('baseline').onclick=()=>action('baseline');
$('assess').onclick=()=>action('assess',$('scenario').value);
$('flip').onclick=()=>action('flip',$('flip-channel').value+':'+$('reason').value);
$('preview').onclick=()=>action('preview',$('level').value);
$('confirm').onclick=()=>action('confirm');
$('save').onclick=()=>action('save');
$('load').onclick=()=>action('load');
$('invalidate').onclick=()=>action('invalidate');
$('role').onchange=()=>action('begin',$('role').value);
$('export').onclick=async()=>{const view=await action('export');if(!view?.export)return;const url=URL.createObjectURL(new Blob([view.export],{type:'application/json'}));const a=document.createElement('a');a.href=url;a.download='selfrionette-unverified-draft.json';a.click();URL.revokeObjectURL(url);};
render({stage:0,id:'',epoch:0,provenance:'未接続',rows:[],message:'起動だけではserialを開きません。合成fixtureを明示選択してください。'});
