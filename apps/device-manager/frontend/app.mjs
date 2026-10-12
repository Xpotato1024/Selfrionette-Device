import { createDesignPreview } from './design-preview.mjs';
const native = typeof window.__TAURI__?.core?.invoke === 'function';
const designPreview = createDesignPreview();
const call = native ? (action, argument='') => window.__TAURI__.core.invoke('fixture_action', {action, argument}) : designPreview;
const $ = id => document.getElementById(id);
$('environment').textContent = native ? 'Rust coreを使うoffline GUI。合成fixtureのみ。実機は未接続です。' : 'ブラウザーの表示確認モード：固定の合成表示です。Rust coreの校正判定は実行しません。';
const buttons = ['baseline','assess','flip','preview','confirm','export','invalidate'];
function render(view) {
  $('device-id').textContent = view.id || '未選択';
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
  [...$('steps').children].forEach((li,i)=>li.classList.toggle('active',view.stage===i+1));
  const stage = view.stage;
  $('baseline').disabled=stage<1; $('assess').disabled=stage<2; $('flip').disabled=stage<3;
  $('preview').disabled=stage<3; $('confirm').disabled=stage!==4; $('export').disabled=stage!==5; $('invalidate').disabled=stage<1;
  $('message').textContent=view.message; $('message').className='';
  return view;
}
async function action(name, argument='') {
  try { buttons.forEach(id=>$(id).disabled=true); return render(await call(name,argument)); }
  catch(error) {$('message').textContent='判定不可: '+String(error);$('message').className='error'; $('baseline').disabled=false;$('assess').disabled=false;$('invalidate').disabled=false;return null;}
}
$('begin').onclick=()=>action('begin',$('role').value);
$('baseline').onclick=()=>action('baseline');
$('assess').onclick=()=>action('assess',$('scenario').value);
$('flip').onclick=()=>action('flip',$('flip-channel').value+':'+$('reason').value);
$('preview').onclick=()=>action('preview',$('level').value);
$('confirm').onclick=()=>action('confirm');
$('invalidate').onclick=()=>action('invalidate');
$('role').onchange=()=>action('invalidate');
$('export').onclick=async()=>{const view=await action('export');if(!view?.export)return;const url=URL.createObjectURL(new Blob([view.export],{type:'application/json'}));const a=document.createElement('a');a.href=url;a.download='selfrionette-unverified-draft.json';a.click();URL.revokeObjectURL(url);};
render({stage:0,id:'',epoch:0,provenance:'未接続',rows:[],message:'起動だけではserialを開きません。合成fixtureを明示選択してください。'});
