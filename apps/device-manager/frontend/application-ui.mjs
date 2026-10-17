import {createApplicationController} from './application-controller.mjs';
import {portTarget,sameTarget} from './port-selection.mjs';

const $=id=>document.getElementById(id);
const num=v=>v===null||v===undefined?'—':Number(v).toLocaleString('ja-JP',{maximumFractionDigits:4});
const quality=f=>f===1?'fresh':f===undefined?'品質不明':[f&2?'timeout':'',f&4?'saturated':'',f&8?'held':'',f&16?'rejected':''].filter(Boolean).join(' / ')||'欠測';
function element(tag,text,attributes={}){const node=document.createElement(tag);if(text!==undefined)node.textContent=text;for(const[k,v]of Object.entries(attributes))node.setAttribute(k,String(v));return node;}
export function explainFailure(reason){const text=String(reason);for(const[pattern,message]of [
 [/quality|non-fresh|usable|flags|timeout|stream stopped/i,'取得品質を確認できません。荷重を抜き、接続・全7chの状態を確認して再測定してください。'],
 [/WeakSignal|weak|never activated/i,'指示した方向の応答が小さすぎます。配線・部位を確認し、痛みのない範囲で再測定してください。'],
 [/UnstableRest|BaselineDrift|drift|rest range/i,'静止状態または除荷後の戻りが不安定です。姿勢と支持を整えて中立値から取り直してください。'],
 [/RepeatDisagreement|PolarityDisagreement|direction|intent/i,'方向または繰返し結果が一致しません。対応ch・取付向き・指示方向を確認してください。'],
 [/incomplete|missing|IncompleteRepeats|coverage/i,'必要な記録が不足しています。未測定の方向・反復・検証手順を確認して再測定してください。'],
 [/context|operator|posture|grip|binding|compatible|generation|revision|identity|epoch/i,'装置または使用条件が記録と一致しません。対象・取付設定・操作者を確認し、互換profileと新しい中立値を用意してください。'],
 [/neutral/i,'今回の中立値が必要です。自然な使用姿勢で中立値を取り直してください。'],
 [/coupled|condition|noise gain/i,'候補Bの条件を満たしません。候補Aと観測結果を比較し、別試行で確認してください。'],
 [/capture|another|busy/i,'現在の測定を完了または中止してから操作してください。開始は予約されません。'],
 [/port|serial|connect/i,'接続を確認してください。別の利用アプリを終了し、対象を選び直してください。'],
 ])if(pattern.test(text))return message;return /[ぁ-んァ-ン一-龯]/.test(text)?text:'操作を完了できませんでした。入力と手順を確認して再試行してください。';}
export function wizardPlan(channels) {
 const steps=[{phase:'neutral',axis:null,direction:1,repeat:0,level:1,instruction:'自然な使用姿勢で、意図的な入力をせず静かに保持してください。'}];
 for(let axis=0;axis<7;axis++)for(const direction of [1,-1])for(let repeat=0;repeat<2;repeat++){
  const target=channels.find(c=>Number(c.logical_axis)===axis),name=target?.location||'未設定の部位',dir=target?.direction||'未設定の方向';
  const prefix=`${name} · ${direction>0?dir:'その逆方向'} · ${repeat+1}/2回目`;
  for(const [phase,help]of [['trial_neutral','意図的な入力をせず、自然な使用姿勢へ戻してください。'],['ramp','痛みのない快適な範囲までゆっくり力を増やしてください。最大努力は不要です。'],['hold','その快適な力を保ち、準備ができたら測定してください。'],['release','力を抜き、最初の自然な姿勢に戻してください。']])steps.push({phase,axis,direction,repeat,level:1,instruction:prefix+'：'+help});
 }
 return steps;
}

// These are current service receipts, never the saved profile's passed flag.
export function calibrationRecovery(view,method='diagonal') {
 const verified=!!(view?.fit&&view?.heldout?.passed&&view.heldout.trials>0&&view.heldout.method===method);
 const available=!!view?.connected&&!view?.capture?.active;
 const neutralAllowed=available&&verified;
 const applyAllowed=neutralAllowed&&!!view.session_neutral_valid;
 const message=!view?.connected?'接続先を選び、装置と使用条件を確認してください。':
  view.capture?.active?'取得完了を待ってください。中断した区間は取り直してください。':
  !view.fit?'個人校正を測定・計算するか、現在の装置・使用条件に適合する保存記録を読み込んで再検証してください。':
  !verified?'個人校正画面で不足・中断した別試行を取得し、選択した補正方法で検証してください。':
  !view.session_neutral_valid?'別試行の検証済み証拠は保持されています。新しい中立値を取得してから、入力設定を明示的に適用してください。':
  view.ready?'入力設定を適用中です。':'中立値を確認し、入力設定を明示的に適用してください。';
 return {neutralAllowed,applyAllowed,saveAllowed:neutralAllowed,message};
}

export function createApplicationUI({invoke,hardware,onOwner=()=>{}}) {
 const root=$('application-workflow');
 root.innerHTML=`
 <p class="caption-note">入力確認は校正不要です。新版の補正入力は Firmware → 装置設定 → 個人校正 → 保存した入力設定の順に準備します。</p>
 <div class="live-workflow">
 <nav class="live-outline" aria-label="実機設定の工程">
  <button data-live-page="setup" aria-current="step">1 装置と取付方向</button>
  <button data-live-page="calibrate">2 個人校正</button>
  <button data-live-page="profiles">3 保存と現在入力</button>
  <button data-live-page="update">Firmware更新</button>
 </nav>
 <div class="live-content">
 <p id="live-error" class="field-error" role="alert" hidden></p>
 <details id="live-error-details" hidden><summary>技術情報</summary><pre id="live-error-detail"></pre></details>
 <p id="live-state" role="status">品質付きFirmwareに接続してください。</p>
 <p id="live-command-status" role="status" aria-live="polite"></p>
 <section data-live-panel="setup">
  <h2>対象を確認する</h2><p class="caption-note">新版へ安全な情報照会（?3）を送ります。非対応の旧版・preview.1は先にFirmware更新へ進んでください。通常の入力確認は受信専用です。</p>
  <div class="live-form-line"><label>接続先<select id="live-port"><option value="">接続先を選択</option></select></label><button id="live-scan">再検索</button><button id="live-connect" class="primary">新版に接続</button><button id="live-disconnect" disabled>一時停止（接続を解放）</button><button id="live-reconnect" disabled>再開・再接続</button></div>
  <p class="caption-note">旧Firmwareは「入力確認」で品質不明の受信のみ可能です。初回の対応Firmware導入は「Firmware更新」で基板を手動確認します。</p>
  <details><summary>装置情報</summary><dl id="live-info" class="about-values"></dl></details>
  <label class="check-line"><input id="live-provision-confirm" type="checkbox">この対象が未登録であることを確認しました</label><button id="live-provision">個体IDを新規登録</button>
  <h2>実際の配線・取付方向</h2>
  <p>一つの部位・既知の方向だけを動かし、全7chを観察します。極性だけで軸入替えや取付違いを解決したと判断しないでください。</p>
  <div class="live-form-line"><label>組立revision<input id="live-assembly" type="number" min="1" value="1"></label><label>構成revision<input id="live-config" type="number" min="1" value="1"></label></div>
  <div class="live-channel-scroll"><table class="live-setup-table"><thead><tr><th scope="col">wire</th><th scope="col">論理軸</th><th scope="col">指・部位</th><th scope="col">正方向の説明</th><th scope="col">極性</th><th scope="col">対応・向きを独立確認</th></tr></thead><tbody id="live-setup-rows"></tbody></table></div>
  <div class="live-form-line"><label>既知方向の観測対象<select id="live-observe-channel"></select></label><button id="live-observe-rest">静止状態を記録</button><button id="live-observe-direction">既知の正方向を記録・極性候補を表示</button></div>
  <p id="live-observation" class="caption-note">極性候補はraw差分から提案します。別の試行で実際の部位・方向・全ch応答を確認し、各行の確認欄を選択してください。</p>
  <button id="live-setup-save" class="primary">確認済み設定を装置へ保存・確認</button>
  <h2>今回のゼロ点調整</h2><p>装置を無荷重にして実行します。個人の使用姿勢の中立値とは別です。失敗時は以前の準備完了を失効させます。</p>
  <label class="check-line"><input id="live-tare-confirm" type="checkbox">対象と無荷重状態を確認しました</label><button id="live-tare">全7chのゼロ点調整を実行</button>
  <h2>全7chの取得状態</h2><p class="caption-note">raw＝符号付きADC count。oriented＝装置極性と今回のゼロ点調整適用後（wire順）。精度の認証ではありません。</p>
  <table><thead><tr><th scope="col">wire</th><th scope="col">raw count</th><th scope="col">oriented count</th><th scope="col">取得状態</th></tr></thead><tbody id="live-sample-rows"></tbody></table>
 </section>
 <section data-live-panel="calibrate" hidden>
  <h2>操作者と使用姿勢</h2>
  <div class="live-context"><label>操作者名<input id="live-operator" maxlength="128" autocomplete="off"></label><label>姿勢・支持方法<input id="live-posture" maxlength="128"></label><label>握り方・接触位置<input id="live-grip" maxlength="128"></label></div>
  <button id="live-context-save">この条件で校正を開始</button>
  <details><summary>実験用の手順閾値・filter設定</summary><p>以下は調整可能な手順上の閾値です。普遍的な物理合格値ではありません。変更した値はprofileに記録します。</p><div id="live-thresholds" class="live-context"></div></details>
  <h2 id="live-instruction-title">測定の準備</h2><p id="live-instruction">装置設定・ゼロ点調整・操作者条件を確認してください。</p>
  <label>測定済み手順を確認・再試行<select id="live-measure-step"><option value="">校正を開始してください</option></select></label>
  <p id="live-capture-progress" role="status"></p>
  <label class="check-line"><input id="live-reverse-safe" type="checkbox">逆方向も無理なく動かせます（できない方向はスキップ）</label>
  <div class="live-form-line"><button id="live-capture-start" class="primary">この手順を測定</button><button id="live-capture-retry">この手順を再試行</button><button id="live-capture-skip">この方向をスキップ</button><button id="live-capture-abort">校正を中止</button></div>
  <p id="live-missing" class="field-error" hidden>未測定の必須方向があります。保存・適用はできません。該当手順を再測定してください。</p>
  <div class="live-form-line"><button id="live-fit">候補A / Bを計算・比較</button><label class="sr-only" for="live-method">採用する補正方法</label><select id="live-method" aria-label="採用する補正方法"><option value="diagonal">A：独立軸</option><option value="coupled">B：干渉補償（条件を満たす場合のみ）</option></select></div>
  <div id="live-fit-summary"></div>
  <h2>別の操作で校正結果を確認</h2><p>今回の中立値から、正逆方向・弱い入力・同時入力・除荷を別の記録で確認します。測定時の記録を再利用しません。</p>
  <label>検証する操作<select id="live-heldout-kind"><option value="positive">単独の正方向</option><option value="negative">単独の逆方向</option><option value="weak">弱い入力</option><option value="combined">二つ以上の同時入力</option></select></label>
  <label>対象論理軸<select id="live-heldout-axis"></select></label>
  <label>同時入力の第二論理軸<select id="live-heldout-second-axis"></select></label>
  <p id="live-heldout-instruction">最初に自然な中立値を測定します。各操作で中立値 → ゆっくり増加 → 保持 → 除荷を別々に記録してください。</p>
  <div class="live-form-line"><button id="live-heldout-start">この操作を測定</button><button id="live-heldout-verify">検証結果を確認</button></div><p id="live-heldout-result"></p>
 </section>
 <section data-live-panel="profiles" hidden>
  <h2>入力設定の保存と再利用</h2><p>操作範囲などを保存し、同じ装置・姿勢で再利用できます。再接続後は中立値を取り直してください。</p>
  <label>保存版<input id="live-profile-revision" type="number" min="1" value="1"></label>
  <label class="check-line"><input id="live-profile-confirm" type="checkbox">装置・操作者条件、候補比較と別試行の検証結果を確認しました</label>
  <div class="live-form-line"><button id="live-profile-save">入力設定を保存</button><button id="live-profile-apply" class="primary">今回の入力に適用</button></div>
  <div class="live-form-line"><label>保存した入力設定<select id="live-profiles"><option value="">保存記録を選択</option></select></label><button id="live-profile-list">一覧を更新</button><button id="live-profile-load">互換性を確認して読み込む</button></div>
  <button id="live-neutral-start">中立値を取り直す</button><p id="live-profile-result" role="status"></p>
  <h2>現在の正規化入力</h2><table><thead><tr><th scope="col">論理軸</th><th scope="col">正規化入力（制限前）</th><th scope="col">状態</th></tr></thead><tbody id="live-normalized-rows"></tbody></table>
  <p class="caption-note">同梱CLIからも保存設定を使った入力を取得できます。Xpotato-Simとの接続は別途必要です。</p>
 </section>
 <section data-live-panel="update" hidden>
  <h2>Firmware更新</h2><p>接続先とFirmwareを確認してから書き込みます。アプリのインストールだけでは、機器のFirmwareは変わりません。</p>
  <div class="live-form-line"><label>接続先<select id="update-port"><option value="">接続先を選択</option></select></label><button id="update-scan">再検索</button></div>
  <label class="check-line"><input id="update-source-supported" type="checkbox" checked>機器情報を確認する（対応Firmwareのみ）</label><p class="caption-note">情報を取得できない機器では、実物の基板を確認してください。</p>
  <label>書き込むFirmware<select id="update-family"><option value="modern">新版（個体設定・個人校正対応）</option><option value="legacy_7ch">旧版（初期保管ソース）</option></select></label>
  <label>入手元<select id="update-package"><option value="bundled">このアプリの同梱版</option><option value="local">ローカルのmanifestとHEX</option></select></label>
  <div id="update-local" hidden><label>manifest JSON<input id="update-manifest" type="file" accept=".json"></label><label>Firmware IntelHEX<input id="update-hex" type="file" accept=".hex"></label><p>hashは破損検出です。配布元を別に確認してください。</p></div>
  <label class="check-line"><input id="update-board-confirm" type="checkbox">基板がPro Micro（ATmega32U4・5V／16MHz）であることを確認しました</label>
  <button id="update-handoff">受信を停止して更新へ</button><p id="update-family-note" class="caption-note"></p><button id="update-prepare">対象とFirmwareを検査</button>
  <details><summary>検査結果・出典</summary><dl id="update-plan" class="about-values"></dl></details>
  <label class="check-line"><input id="update-write-confirm" type="checkbox">接続先と書き込むFirmware、設定の退避結果を確認しました</label>
  <div id="update-bootloader-confirmation" hidden><label>再列挙された書込み待機の接続先<select id="update-bootloader-target"><option value="">実物を確認して選択</option></select></label><label class="check-line"><input id="update-bootloader-confirm" type="checkbox">この書込み待機の接続先が先ほどの同じ実物であることを確認しました</label></div>
  <p id="update-disabled-reason" class="caption-note"></p><button id="update-apply" class="primary" disabled>この接続先へ書き込む</button><p id="update-result" role="status"></p>
  <button id="update-recover">中断した更新を確認する</button><details><summary>更新の技術情報</summary><pre id="update-detail"></pre></details>
  <p class="caption-note">曖昧なbootloader候補は自動選択しません。失敗・中断は再接続してversion・ID・設定を確認するまで「要復旧」です。個体IDの新規登録は更新と別操作です。</p>
 </section>
 </div></div>`;
 let owned=false;
 let page='setup',active=false,timer=null,view=null,ports=[],busy=false,plan=[],position=0,furthest=0,missing=new Set(),lastCapture=null,rest=null,updaterPlan=null,setupSignature='',operatorSignature='',contextSignature='';
 const observations=new Map();
 let heldoutStep=0;
 let bootloaderCandidates=[];let updateGeneration=0;
 const thresholds=[['noise_floor_counts','noise floor / count',1],['max_rest_range_counts','静止range上限 / count',2000],['max_drift_counts','drift上限 / count',1000],['min_signal_to_noise','最小signal/noise',5],['repeat_tolerance','繰返し許容率',0.3],['direction_tolerance','方向許容率',0.4],['max_condition_number','条件数上限',100],['max_noise_gain','noise増幅上限',10],['max_sample_gap_ms','sample間隔上限 / ms',250],['time_constant_ms','filter時定数 / ms',30],['noise_multiplier','deadzone noise倍率',3],['release_ratio','hysteresis release比率',0.5],['max_deadzone_fraction','deadzone/span上限',0.25]];
 for(const[key,label,value]of thresholds){const l=element('label',label),input=element('input',undefined,{id:'live-limit-'+key,type:'number',step:'any',min:0,value});l.append(input);$('live-thresholds').append(l);}
 for(let ch=0;ch<7;ch++){
  const row=element('tr'),axis=element('select',undefined,{id:'live-axis-'+ch,'aria-label':'wire '+ch+' の論理軸'});axis.append(element('option','未確認',{value:''}));for(let n=0;n<7;n++)axis.append(element('option','軸 '+n,{value:n}));
  const sign=element('select',undefined,{id:'live-polarity-'+ch,'aria-label':'wire '+ch+' の極性'});for(const[value,label]of [['','未確認'],['1','+1'],['-1','−1']])sign.append(element('option',label,{value}));
  const location=element('input',undefined,{id:'live-location-'+ch,maxlength:128,'aria-label':'wire '+ch+' の部位'}),direction=element('input',undefined,{id:'live-direction-'+ch,maxlength:128,'aria-label':'wire '+ch+' の正方向'}),confirmed=element('input',undefined,{id:'live-confirm-'+ch,type:'checkbox','aria-label':'wire '+ch+' の独立した対応・向き確認'});
  for(const cell of [element('span','CH'+ch),axis,location,direction,sign,confirmed]){const td=element('td');if(cell.id)td.append(element('label',cell.getAttribute('aria-label'),{for:cell.id,class:'sr-only'}));td.append(cell);row.append(td);}$('live-setup-rows').append(row);
  $('live-observe-channel').append(element('option','wire CH'+ch,{value:ch}));$('live-heldout-axis').append(element('option','軸 '+ch,{value:ch}));$('live-heldout-second-axis').append(element('option','軸 '+ch,{value:ch}));
  for(const target of ['live-sample-rows','live-normalized-rows']){const r=element('tr');r.dataset.channel=ch;for(let n=0;n<(target==='live-sample-rows'?4:3);n++)r.append(element('td',n===0?'CH'+ch:'—'));$(target).append(r);}
   for(const input of [axis,sign,location,direction])input.addEventListener('change',()=>{$('live-confirm-'+ch).checked=false;observations.delete(ch);updaterPlan=null;});
 }
 $('live-heldout-second-axis').value='1';
 const report=text=>{$('live-error').textContent=text?explainFailure(text):'';$('live-error').hidden=!text;$('live-error-details').hidden=!text;$('live-error-detail').textContent=text;};
 const controller=createApplicationController((command)=>invoke(command),{render,error:report,busy:value=>{busy=value;controls();},operation:({id,action,status})=>{
  root.dataset.commandId=String(id);root.dataset.commandAction=action;root.dataset.commandStatus=status;
  root.setAttribute('aria-busy',String(status==='pending'));
  $('live-command-status').textContent=status==='pending'?'操作を実行中…':'';
 }});
 function setup(){return {assembly_revision:Number($('live-assembly').value),config_revision:Number($('live-config').value),channels:Array.from({length:7},(_,wire)=>({wire,logical_axis:$('live-axis-'+wire).value===''?null:Number($('live-axis-'+wire).value),location:$('live-location-'+wire).value.trim(),direction:$('live-direction-'+wire).value.trim(),polarity:$('live-polarity-'+wire).value===''?null:Number($('live-polarity-'+wire).value),confirmed:$('live-confirm-'+wire).checked,evidence:observations.get(wire)??null}))};}
 function showPage(name){page=name;$('live-state').hidden=name==='update';if(name==='update')void scan(hardware.connected?hardware.candidate:view?.candidate).catch(e=>report(String(e)));for(const panel of root.querySelectorAll('[data-live-panel]'))panel.hidden=panel.dataset.livePanel!==page;for(const b of root.querySelectorAll('[data-live-page]')){if(b.dataset.livePage===page)b.setAttribute('aria-current','step');else b.removeAttribute('aria-current');}}
 for(const b of root.querySelectorAll('[data-live-page]'))b.onclick=()=>showPage(b.dataset.livePage);
 function controls(){
  for(const button of root.querySelectorAll('button:not([data-live-page])'))button.disabled=busy;
  for(const input of root.querySelectorAll('input,select'))input.disabled=busy||!!view?.capture?.active;
  for(const id of ['update-port','update-family','update-package','update-manifest','update-hex','update-board-confirm','update-write-confirm','update-bootloader-confirm','update-bootloader-target'])$(id).disabled=busy;
  $('live-connect').disabled=busy||!!view?.owns_handle||!!view?.connected||!$('live-port').value;
  $('live-disconnect').disabled=busy||!(view?.owns_handle||view?.connected);$('live-reconnect').disabled=busy||!view?.candidate||hardware.connected;
  for(const id of ['live-provision','live-tare','live-setup-save','live-context-save','live-capture-start','live-capture-retry','live-capture-skip','live-capture-abort','live-fit','live-heldout-start','live-heldout-verify','live-neutral-start','live-profile-save','live-profile-apply','live-profile-list','live-profile-load'])$(id).disabled=busy||!view?.connected;
  $('live-provision').disabled ||= !($('live-provision-confirm').checked && (view?.info?.device_id==='unprovisioned'||view?.info?.identity==='unprovisioned'));
  $('live-tare').disabled ||= !$('live-tare-confirm').checked;
  $('live-capture-start').disabled ||= !plan[position]||!!view?.capture?.active||(plan[position]?.direction<0&&!$('live-reverse-safe').checked);
  for(const id of ['live-heldout-start','live-neutral-start','live-context-save','live-setup-save','live-fit','live-heldout-verify','live-profile-save','live-profile-load','live-profile-apply'])$(id).disabled ||= !!view?.capture?.active;
  $('live-profile-save').disabled ||= missing.size>0||!$('live-profile-confirm').checked;
  $('live-profile-apply').disabled ||= missing.size>0||!$('live-profile-confirm').checked;
  const recovery=calibrationRecovery(view,$('live-method').value);
  $('live-neutral-start').disabled ||= !recovery.neutralAllowed;
  $('live-profile-apply').disabled ||= !recovery.applyAllowed;
  $('live-profile-save').disabled ||= !recovery.saveAllowed;
  $('live-profile-result').textContent=recovery.message+(view?.saved_path?' 保存記録: '+view.saved_path:'');
  $('update-prepare').disabled=busy||!selectedPort('update-port')||hardware.connected||!!view?.owns_handle;$('update-handoff').disabled=busy||!(hardware.connected||view?.owns_handle);$('update-recover').disabled=busy||hardware.connected||!!view?.owns_handle;
  $('update-apply').disabled=busy||!updaterPlan?.apply_enabled||!$('update-write-confirm').checked;
  if(!$('update-bootloader-confirmation').hidden)$('update-apply').disabled ||= !$('update-bootloader-confirm').checked||$('update-bootloader-target').value==='';$('update-disabled-reason').textContent=hardware.connected||view?.owns_handle?'「受信を停止して更新へ」で接続を解放してください。':!selectedPort('update-port')?'接続先を選んでください。':!updaterPlan?.apply_enabled?'対象とFirmwareを検査してください。':!$('update-write-confirm').checked?'検査結果を確認し、書込みの確認欄を選択してください。':'';
 }
 function infoRows(target,rows){$(target).replaceChildren();for(const[key,value]of rows){$(target).append(element('dt',key),element('dd',value??'—'));}}
 function render(next){if(next?.connected&&!owned)onOwner();owned=!!next?.connected;view=next;
  root.dataset.connected=String(!!view?.connected);root.dataset.ready=String(!!view?.ready);root.dataset.sampleSequence=String(view?.sample?.sequence??'');
  hardware.setExternalOwner?.(!!(view?.owns_handle||view?.connected));
  if(view?.owns_handle||view?.connected){$('environment').textContent=view.scripted?'模擬transport · 実機未接続':view.connected?(view.ready?'入力設定を適用中':'品質付き接続'):'接続失効 · 切断が必要';$('device-summary').textContent=(view.info?.device_id&&view.info.device_id!=='unprovisioned'?'個体 …'+view.info.device_id.slice(-8):'個体ID未登録')+' · '+(view.port??'');$('device-summary').title=view.info?.device_id??'';}
  const contextKey=JSON.stringify([view?.connected,view?.owns_handle,view?.port,view?.info?.device_id,view?.info?.generation,view?.info?.source_sha,view?.info?.tare_epoch]);if(contextKey!==contextSignature){contextSignature=contextKey;observations.clear();rest=null;setupSignature='';operatorSignature='';for(let wire=0;wire<7;wire++)$('live-confirm-'+wire).checked=false;lastCapture=null;plan=[];position=0;furthest=0;missing.clear();heldoutStep=0;updaterPlan=null;updateInstruction();heldoutInstruction();}
  const setupKey=JSON.stringify(view?.setup??null);if(view?.setup&&setupKey!==setupSignature){setupSignature=setupKey;const s=view.setup;$('live-assembly').value=s.assembly_revision;$('live-config').value=s.config_revision;for(const c of s.channels){$('live-axis-'+c.wire).value=c.logical_axis??'';$('live-location-'+c.wire).value=c.location??'';$('live-direction-'+c.wire).value=c.direction??'';$('live-polarity-'+c.wire).value=c.polarity??'';$('live-confirm-'+c.wire).checked=c.confirmed;if(c.evidence)observations.set(c.wire,c.evidence);}}
  const operatorKey=JSON.stringify(view?.operator??null);if(view?.operator&&operatorKey!==operatorSignature){operatorSignature=operatorKey;$('live-operator').value=view.operator.operator??'';$('live-posture').value=view.operator.posture??'';$('live-grip').value=view.operator.grip??'';}
  $('live-state').textContent=page==='update'?'接続先とFirmwareを検査してから書き込みます。':!view?.connected?(hardware.connected?'受信可能・設定未確認':view?.paused?'一時停止（接続を解放）':'新版の装置設定は接続先を選んで開始します。'):view.ready?'入力設定を適用中':'接続中 · 装置設定、ゼロ点調整、使用条件と今回の中立値を確認してください';
  const info=view?.info;
  infoRows('live-info',[['個体ID',info?.device_id??info?.identity],['Firmware',info?.firmware_version??info?.version],['ソース版',info?.source_sha??info?.source_commit],['設定の保存世代',info?.generation],['ゼロ点調整',info?`${info.tare_epoch} / ${info.tare_valid?'有効':'未完了・失効'}`:null]]);
  const sample=view?.sample;
  for(let ch=0;ch<7;ch++){
   const cells=$('live-sample-rows').children[ch].children;cells[1].textContent=num(sample?.raw?.[ch]);cells[2].textContent=num(sample?.oriented?.[ch]);cells[3].textContent=quality(sample?.flags?.[ch]);
   const r=$('live-normalized-rows').children[ch].children,output=view?.normalized??view?.output;r[1].textContent=num(output?.normalized?.[ch]);r[2].textContent=output?.span_exceeded?.[ch]?'操作範囲を超過':output?.unsupported_direction?.[ch]?'方向未測定':output?.normalized?.[ch]===null||!output?'欠測・未適用':'入力あり';
  }
  const capture=view?.capture;
  root.dataset.captureActive=String(!!capture?.active);root.dataset.captureCompleted=String(!!capture?.completed);
  $('live-capture-progress').textContent=capture?`${capture.active?'取得中':'取得終了'}：${capture.done??capture.samples??capture.count??0} samples · ${capture.reason??''}`:'';
  if(lastCapture&&capture&&!capture.active&&capture.completed===true){
   if(lastCapture==='wizard'){missing.delete(position);position=Math.min(position+1,plan.length);furthest=Math.max(furthest,position);updateInstruction();}
   if(lastCapture==='heldout'){heldoutStep=(heldoutStep+1)%4;heldoutInstruction();}
   lastCapture=null;
  }
  else if(lastCapture&&capture&&!capture.active&&capture.complete&&!capture.completed){report(capture.reason||'capture rejected');lastCapture=null;}
  $('live-fit-summary').replaceChildren();
  if(view?.fit){const f=view.fit;infoRows('live-fit-summary',[['候補A',f.diagonal_available===false?'不可':f.diagonal_reason??'算出結果を確認'],['候補B',f.coupled_available?'比較可能':f.coupled_rejection??'条件未達・未計測'],['正方向の操作範囲 / count',f.positive_span_counts?.map(num).join(' / ')],['逆方向の操作範囲 / count',f.negative_span_counts?.map(num).join(' / ')],['中立時の変動 / count',f.noise_envelope_counts?.map(num).join(' / ')]]);$('live-method').querySelector('[value="coupled"]').disabled=!f.coupled_available;}
  $('live-heldout-result').textContent=view?.heldout?.reason??(view?.heldout?.passed?'別試行の検証を通過':'別試行は未検証');
  if(view?.profiles){const prior=$('live-profiles').value;$('live-profiles').replaceChildren(element('option','保存記録を選択',{value:''}));for(const p of view.profiles)$('live-profiles').append(element('option',p.label??p.path,{value:p.path??p}));$('live-profiles').value=prior;}
  controls();
 }
 function updateInstruction(){$('live-instruction-title').textContent=position<plan.length?`手順 ${position+1} / ${plan.length}`:'測定記録を比較する';$('live-instruction').textContent=plan[position]?.instruction??'測定終了。候補A/Bを計算し、別試行で検証してください。';$('live-missing').hidden=missing.size===0;$('live-measure-step').replaceChildren();for(const[index,step]of plan.entries()){const option=element('option',`${index+1} ${step.instruction}`,{value:index});option.disabled=index>furthest;$('live-measure-step').append(option);}$('live-measure-step').value=String(position);controls();}
 async function send(command){return controller.send(command);}
 function selectedPort(id,list=ports){const v=$(id).value;return v!==''&&/^(0|[1-9][0-9]*)$/.test(v)?list[Number(v)]??null:null;}
 function invalidateUpdate(){invalidation=invalidation.then(()=>window.__TAURI__.core.invoke('updater_invalidate')).catch(e=>report(String(e)));updateGeneration++;updaterPlan=null;$('update-write-confirm').checked=false;$('update-bootloader-confirm').checked=false;$('update-bootloader-confirmation').hidden=true;bootloaderCandidates=[];}
 let invalidation=Promise.resolve(),scanGeneration=0;
 async function scan(preferred=null){
  const prior=Object.fromEntries(['live-port','update-port'].map(id=>[id,selectedPort(id)]));
  if(preferred&&!sameTarget(portTarget(preferred),prior['update-port']))invalidateUpdate();
  const scanId=++scanGeneration,revision=updateGeneration;
  const r=await invoke({action:'ports'});if(scanId!==scanGeneration||revision!==updateGeneration)return;ports=r.ports??[];
  if(prior['update-port']&&!ports.some(p=>sameTarget(portTarget(p),portTarget(prior['update-port']))))invalidateUpdate();
  for(const id of ['live-port','update-port']){
   const target=preferred??prior[id]??view?.candidate??hardware.candidate;
   $(id).replaceChildren(element('option','接続先を選択',{value:''}));
   for(const[pindex,p]of ports.entries())$(id).append(element('option',`${p.name} · ${p.product||p.category||'要確認'}`,{value:pindex}));
   let index=ports.findIndex(p=>sameTarget(portTarget(p),target));
   if(index<0&&target)$(id).firstChild.textContent=`${target.name} · 未接続／再接続待ち`;
   $(id).value=index<0?'':String(index);
  }controls();
 }
 $('live-scan').onclick=$('update-scan').onclick=()=>{void scan().catch(e=>report(String(e)));};
 function invalidateWizard(){lastCapture=null;plan=[];position=0;furthest=0;missing.clear();heldoutStep=0;rest=null;observations.clear();updateInstruction();heldoutInstruction();}
 $('live-port').onchange=()=>{const p=selectedPort('live-port');if(p)hardware.remember(p);invalidateUpdate();controls();};
 $('live-connect').onclick=async()=>{if(hardware.connected)await hardware.disconnect();const target=selectedPort('live-port');if(!target){report('接続先を選択してください。');return;}hardware.remember(target);const r=await send({action:'connect',target:portTarget(target)});if(r&&!r.error)await send({action:'stream_start'});};
 async function pause(){invalidateWizard();await send({action:'disconnect'});}
 async function reconnect(){if(!view?.candidate||!window.confirm('同じ実物を再接続したことを確認してください。安全な情報照会で個体IDを照合します。中立値は取り直します。'))return;const r=await send({action:'reconnect',confirmed:true,metadata_supported:true});if(r&&!r.error)await send({action:'stream_start'});}
 $('live-disconnect').onclick=pause;$('live-reconnect').onclick=reconnect;
 $('live-provision-confirm').onchange=$('live-tare-confirm').onchange=$('live-profile-confirm').onchange=$('update-write-confirm').onchange=$('live-reverse-safe').onchange=$('live-method').onchange=controls;
 $('live-provision').onclick=()=>send({action:'provision',confirmed:$('live-provision-confirm').checked});
 $('live-tare').onclick=async()=>{lastCapture=null;await send({action:'tare',confirmed:$('live-tare-confirm').checked});$('live-tare-confirm').checked=false;controls();};
 $('live-observe-rest').onclick=()=>{rest=view?.sample?.flags?.every(f=>f===1)?structuredClone(view.sample):null;$('live-observation').textContent=rest?'静止rawを記録しました。対象の既知の正方向だけを操作してください。':'freshなrawを受信してから再試行してください。';};
 $('live-observe-direction').onclick=()=>{
  const wire=Number($('live-observe-channel').value),sample=view?.sample;if(!rest||sample?.flags?.[wire]!==1||sample.raw?.[wire]===null){report('freshな静止・方向観測が必要です。取得状態を確認してやり直してください。');return;}
  if(sample.sequence===rest.sequence||sample.generation!==rest.generation||sample.tare_epoch!==rest.tare_epoch){report('別のfresh試行で観測してください。設定やtareが変わった場合は静止状態から取り直してください。');return;}
  const deltas=sample.raw.map((v,n)=>v===null||rest.raw[n]===null?null:v-rest.raw[n]),delta=deltas[wire];if(delta===null||delta===0){report('対象chの変化が確認できません。配線と方向を確認してください。');return;}
  observations.set(wire,`v3 seq ${rest.sequence}->${sample.sequence}; raw delta [${deltas.join(',')}] count; independent direction confirmation`);
  $('live-polarity-'+wire).value=delta>0?'1':'-1';$('live-confirm-'+wire).checked=false;$('live-observation').textContent=`CH${wire}の極性候補 ${delta>0?'+1':'−1'}。全ch raw差分：${deltas.map(num).join(' / ')} count。候補は未確認です。別試行で対応と向きを独立確認してください。`;
 };
 $('live-setup-save').onclick=async()=>{const s=setup();let known_mask=0,negative_mask=0;for(const c of s.channels){if(c.confirmed&&c.polarity!==null){known_mask|=1<<c.wire;if(c.polarity<0)negative_mask|=1<<c.wire;}}const a=await send({action:'setup',setup:s});if(a?.error||!a)return;await send({action:'save_device_settings',expected_generation:a.view.info.generation,known_mask,negative_mask});};
 $('live-context-save').onclick=async()=>{const r=await send({action:'set_operator',operator:$('live-operator').value.trim(),posture:$('live-posture').value.trim(),grip:$('live-grip').value.trim()});if(r&&!r.error){plan=wizardPlan(setup().channels);position=0;furthest=0;missing=new Set();updateInstruction();}};
 $('live-measure-step').onchange=()=>{if(view?.capture?.active){report('現在の測定を中止してから手順を選んでください。');$('live-measure-step').value=String(position);return;}position=Number($('live-measure-step').value);updateInstruction();};
 async function start(step,type='wizard'){if(!step)return;const r=await send({action:'capture_start',sample_count:16,...step});if(r&&!r.error)lastCapture=type;}
 $('live-capture-start').onclick=()=>start(plan[position]);
 $('live-capture-retry').onclick=async()=>{await send({action:'capture_abort'});lastCapture=null;if(position>=plan.length)position=plan.length-1;while(position>0&&plan[position]?.phase!=='trial_neutral')position--;updateInstruction();};
 $('live-capture-skip').onclick=async()=>{lastCapture=null;const stopped=await send({action:'capture_abort'});if(!stopped||stopped.error)return;if(plan[position]?.direction<0){const old=plan[position],skipped=await send({action:'skip_direction',axis:old.axis,direction:-1});if(!skipped||skipped.error)return;while(position<plan.length&&plan[position].axis===old.axis&&plan[position].direction<0)position++;$('live-capture-progress').textContent='逆方向は欠測として保持します。その方向の入力は利用できません。';}else{missing.add(position);position=Math.min(position+1,plan.length);}furthest=Math.max(furthest,position);updateInstruction();};
 $('live-capture-abort').onclick=async()=>{lastCapture=null;await send({action:'capture_abort'});plan=[];position=0;updateInstruction();};
 $('live-fit').onclick=()=>{
  const values=Object.fromEntries(thresholds.map(([key])=>[key,Number($('live-limit-'+key).value)])),limits={};for(const[key]of thresholds.slice(0,9))limits[key]=key.endsWith('_counts')?Array(7).fill(values[key]):values[key];const signal_settings=Object.fromEntries(thresholds.slice(9).map(([key])=>[key,values[key]]));return send({action:'fit',limits,signal_settings,method:$('live-method').value});
 };
 function heldoutInstruction(){const messages=['自然な使用姿勢で、意図的な入力をせず保持してください。','指定した操作を、痛みのない快適な範囲までゆっくり増やしてください。','指定した方向と力を保って測定してください。','力を抜いて、最初の自然な使用姿勢へ戻してください。'];$('live-heldout-instruction').textContent=`検証 ${heldoutStep+1}/4：${messages[heldoutStep]}`;}
 for(const id of ['live-heldout-kind','live-heldout-axis','live-heldout-second-axis'])$(id).onchange=()=>{heldoutStep=0;heldoutInstruction();};
 $('live-heldout-start').onclick=()=>{const kind=$('live-heldout-kind').value,axis=Number($('live-heldout-axis').value),second=Number($('live-heldout-second-axis').value),intent=Array(7).fill(0);if(kind==='combined'&&axis===second){report('同時入力では異なる二つの軸を指定してください。');return;}if(heldoutStep===1||heldoutStep===2){intent[axis]=kind==='negative'?-1:kind==='weak'?0.25:kind==='combined'?0.5:1;if(kind==='combined')intent[second]=0.5;}return start({phase:['held_out_neutral','held_out_ramp','held_out_hold','held_out_release'][heldoutStep],axis,direction:kind==='negative'?-1:1,repeat:0,level:Math.max(...intent.map(Math.abs)),intent,instruction:`${kind}-axis${axis}-second${kind==='combined'?second:'none'}`},'heldout');};
 $('live-heldout-verify').onclick=()=>send({action:'verify_held_out',method:$('live-method').value});
 $('live-profile-save').onclick=()=>send({action:'save_calibration',profile_revision:Number($('live-profile-revision').value),method:$('live-method').value,confirmed:$('live-profile-confirm').checked});
 $('live-profile-apply').onclick=()=>send({action:'apply_calibration',confirmed:$('live-profile-confirm').checked,method:$('live-method').value});
 $('live-profile-list').onclick=()=>send({action:'list_calibrations'});
 $('live-profile-load').onclick=()=>send({action:'load_calibration',record_path:$('live-profiles').value});
 $('live-neutral-start').onclick=()=>start({phase:'session_neutral',axis:null,direction:1,repeat:0,level:0,instruction:'今回の自然な使用姿勢で意図的な入力をせず保持してください。'},'neutral');
 $('update-family').onchange=$('update-package').onchange=()=>{$('update-family-note').textContent=$('update-family').value==='legacy_7ch'?'保管7ch版：個体設定・個人校正には非対応。ID・版情報は取得できません。設定保持は新版に戻して確認します。先輩の実機と同じFirmwareとは未確認です。':'';invalidateUpdate();$('update-local').hidden=$('update-package').value!=='local';controls();};
 $('update-port').onchange=()=>{const p=selectedPort('update-port');if(p)hardware.remember(p);invalidateUpdate();controls();};
 $('update-source-supported').onchange=$('update-board-confirm').onchange=$('update-manifest').onchange=$('update-hex').onchange=()=>{invalidateUpdate();controls();};
 $('update-handoff').onclick=async()=>{if(hardware.connected)await hardware.disconnect();if(view?.connected||view?.owns_handle)await pause();await scan();controls();};
 $('update-bootloader-confirm').onchange=$('update-bootloader-target').onchange=controls;
 $('update-prepare').onclick=async()=>{
  const target=selectedPort('update-port');if(!target){$('update-result').textContent='接続先を選択してください。';return;}invalidateUpdate();const generation=updateGeneration;
  if(hardware.connected||view?.owns_handle){$('update-result').textContent='先に受信を停止してください。';return;}
  await invalidation;try{busy=true;controls();let selected=null;if($('update-package').value==='local'){const m=$('update-manifest').files[0],h=$('update-hex').files[0];if(!m||!h||m.size>32768||h.size>262144)throw Error('manifestとHEXを選択してください（上限32KiB / 256KiB）。');selected={manifest_json:await m.text(),firmware_hex:await h.text()};}
   const p=await window.__TAURI__.core.invoke('updater_prepare',{request:{target:portTarget(target),package:selected,family:$('update-family').value,metadata_supported:$('update-source-supported').checked,manual_board_confirmation:$('update-board-confirm').checked}});if(generation!==updateGeneration)throw Error('更新条件が変更されました。対象とパッケージを再検査してください。');updaterPlan=p;$('update-write-confirm').checked=false;
   infoRows('update-plan',[['状態',p.status],['対象',p.target?.name],['現在のID',p.observation?.info?.device_id??'未識別（初回導入）'],['現在のFirmware',p.observation?.info?.firmware_version??'未識別'],['更新先',p.manifest?.release_version],['source',p.manifest?.source_sha],['image SHA256',p.firmware_sha256],['配布元',p.source_trust],['保護記録',p.backup_id??p.backup_status]]);$('update-result').textContent='検査結果を確認してください。書込みはまだ行っていません。';
  }catch(e){updaterPlan=null;$('update-detail').textContent=String(e);$('update-result').textContent='検査できませんでした。接続先・基板・Firmwareを確認してください。';}finally{busy=false;controls();}
 };
 $('update-apply').onclick=async()=>{if(!updaterPlan||!$('update-write-confirm').checked)return;try{busy=true;controls();const awaiting=!$('update-bootloader-confirmation').hidden,bootloader_target=awaiting?bootloaderCandidates[Number($('update-bootloader-target').value)]:null;const r=await window.__TAURI__.core.invoke('updater_apply',{request:{plan_id:updaterPlan.plan_id,operator_confirmed:true,bootloader_target,manual_bootloader_confirmation:awaiting&&$('update-bootloader-confirm').checked}});if(r.status==='awaiting_bootloader_confirmation'){bootloaderCandidates=r.bootloader_candidates??[];$('update-bootloader-target').replaceChildren(element('option','実物を確認して選択',{value:''}));for(const[pindex,p]of bootloaderCandidates.entries())$('update-bootloader-target').append(element('option',`${p.name} · ${p.product??'書込み待機用'}`,{value:pindex}));$('update-bootloader-confirm').checked=false;$('update-bootloader-confirmation').hidden=false;$('update-result').textContent='書込み前の対象確認が必要です。同じ実物の書込み待機portを確認してください。';}else{$('update-detail').textContent=r.message??r.status;$('update-result').textContent=r.needs_recovery?'要復旧：接続先を変えず、更新を確認してください。':r.status==='image_verified_legacy_identity_unavailable'?'画像確認済み。旧版のID・版情報は取得不可、設定保持は未確認です。':'Firmware・個体ID・設定を確認しました。';invalidateUpdate();}}catch(e){$('update-detail').textContent=String(e);$('update-result').textContent='確認が必要です。接続先を変えず、中断した更新を確認してください。';invalidateUpdate();}finally{busy=false;controls();}};
 $('update-recover').onclick=async()=>{if(hardware.connected||view?.owns_handle)return;try{busy=true;controls();const r=await window.__TAURI__.core.invoke('updater_recover',{metadataSupported:$('update-source-supported').checked});$('update-detail').textContent=r.message??r.status;$('update-result').textContent=r.needs_recovery?'要復旧：接続先と技術情報を確認してください。':r.status==='image_verified_legacy_identity_unavailable'?'画像確認済み。旧版のID・版情報は取得不可、設定保持は未確認です。':r.status==='none'?'中断した更新はありません。':'更新状態を確認しました。';}catch(e){$('update-detail').textContent=String(e);$('update-result').textContent='確認が必要です。接続先を変えず、中断した更新を確認してください。';}finally{busy=false;controls();}};
 async function tick(){if(!active)return;if(view?.connected&&!controller.busy)await send({action:view?.capture?.active?'capture_tick':'stream_tick'});timer=setTimeout(tick,50);}
 controls();
 return {get ownsHandle(){return !!view?.owns_handle;},get candidate(){return view?.candidate??null;},get connected(){return !!view?.connected;},get busy(){return busy||controller.busy;},pause,reconnect,async activate(value){active=value;clearTimeout(timer);if(value){await scan().catch(e=>report(String(e)));await send({action:'snapshot'});timer=setTimeout(tick,100);}},disconnect:()=>send({action:'disconnect'})};
}
