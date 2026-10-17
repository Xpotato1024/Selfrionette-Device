import {createReadout} from './readout.mjs';
import {createTrace} from './trace.mjs';
import {createHardwareController,monitorBackgroundCommand} from './hardware-controller.mjs';
import {portTarget,sameTarget,visiblePorts,canSelect} from './port-selection.mjs';
export function createHardwareUI(invoke,{onOwner=()=>{}}={}) {
  let ownerConnection=null;
  const $=id=>document.getElementById(id);
  let active=false,busy=false,externalOwner=false,action='',page='device',timer=null,expiry=null,selected=null,selectedChannel=0,portSignature='',selectionError='';
  let candidate=null;
  const names={disconnected:'未接続',paused:'一時停止（接続を解放）',waiting:'受信待ち',receiving:'受信可能・設定未確認',stale:'入力更新なし',error:'未接続／再接続待ち'};
  function remember(port){candidate=portTarget(port);void window.__TAURI__.core.invoke('connection_candidate',{target:candidate}).catch(()=>{});}
  const trace=createTrace(document),readouts=[],channelButtons=[];
  function chooseChannel(channel,focus=false) {
    selectedChannel=channel;
    channelButtons.forEach((button,index)=>{const chosen=index===channel;button.setAttribute('aria-pressed',String(chosen));button.tabIndex=chosen?0:-1;button.closest('tr').setAttribute('aria-selected',String(chosen));});
    trace.select(channel);if(focus)channelButtons[channel].focus();
  }
  for(let channel=0;channel<7;channel++) {
    const row=document.createElement('tr'),name=document.createElement('td'),value=document.createElement('td'),button=document.createElement('button'),reading=createReadout(document);
    button.type='button';button.className='channel-select';button.textContent='ch '+channel;button.id='hardware-ch-'+channel;
    button.onclick=()=>chooseChannel(channel);
    button.onkeydown=event=>{const next={ArrowDown:(channel+1)%7,ArrowUp:(channel+6)%7,Home:0,End:6}[event.key];if(next!==undefined){event.preventDefault();chooseChannel(next,true);}};
    name.append(button);value.className='value-cell';value.append(reading.element);row.append(name,value);
    $('hardware-value-rows').append(row);reading.update(null);readouts.push(reading);channelButtons.push(button);
  }
  chooseChannel(0);
  const controller=createHardwareController(invoke,{render,error:text=>{
    const message=text||controller.view?.failure||selectionError;
    const inDialog=$('hardware-connect-dialog').open;
    $('port-error').textContent=inDialog?message:'';$('port-error').hidden=!inDialog||!message;
    $('hardware-error').textContent=inDialog?'':message;$('hardware-error').hidden=!active||inDialog||!message;
  },busy:(value,current)=>{busy=value;action=current;controls();}});
  function controls() {
    const view=controller.view;
    $('hardware-choose').disabled=!active||busy||!!view?.connected||externalOwner;
    $('hardware-disconnect').disabled=!active||busy||!(view?.connected||externalOwner);
    $('hardware-reconnect').disabled=!active||busy||externalOwner||!candidate;
    $('hardware-scan').disabled=busy;
    $('hardware-connect-confirm').disabled=busy||!selected||!view||view.connected;
    $('hardware-connect-cancel').disabled=busy&&action==='connect';
    $('show-other-ports').disabled=busy;$('query-on-connect').disabled=busy||!selected;
    $('hardware-info').disabled=busy||!view?.connected||view.info_pending;
  }
  function renderPortDetails(port) {
    $('port-details').replaceChildren();
    const hex=n=>n===null||n===undefined?'—':'0x'+n.toString(16).padStart(4,'0');
    for(const [key,value]of [['接続先',port?.name],['種別',port?.kind],['VID / PID',port?hex(port.vid)+' / '+hex(port.pid):null],['USB serial',port?.serial_number],['メーカー',port?.manufacturer],['候補の根拠',port?.reason]]){
      const dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=key;dd.textContent=value||'—';$('port-details').append(dt,dd);
    }
  }
  function selectPort(port) {
    selectionError='';selected=portTarget(port);$('query-on-connect').checked=false;renderPortDetails(port);
    for(const row of $('port-rows').children)row.setAttribute('aria-selected',String(row.dataset.name===port.name));
    $('port-error').hidden=true;controls();
  }
  function renderPorts(force=false) {
    if(!$('hardware-connect-dialog').open)return;
    const ports=controller.view?.ports??[],showOther=$('show-other-ports').checked;
    const signature=JSON.stringify([ports,showOther]);if(!force&&signature===portSignature)return;portSignature=signature;
    const present=ports.find(p=>sameTarget(portTarget(p),selected));
    if(selected&&(!present||!visiblePorts(ports,showOther).includes(present))) {selectionError='接続先の情報が変わりました。選び直してください。';selected=null;renderPortDetails(null);$('port-error').textContent='接続先の情報が変わりました。選び直してください。';$('port-error').hidden=false;}
    const candidateCount=ports.filter(p=>p.category==='candidate').length;
    $('port-count').textContent='対応基板の候補 '+candidateCount+'件 ／ 全ポート '+ports.length+'件';
    const shown=visiblePorts(ports,showOther);$('port-empty').hidden=shown.length!==0;
    $('port-empty').textContent=ports.length?'候補が見つかりません。「その他のポートも表示」から確認できます。':'ポートが見つかりません。USB接続後に再検索してください。';
    $('port-rows').replaceChildren();
    const labels={candidate:'対応基板の候補',other:'その他',unknown:'情報不足',bootloader:'書込み待機用'};
    for(const [index,port]of shown.entries()) {
      const row=document.createElement('tr');row.dataset.name=port.name;row.setAttribute('aria-selected',String(sameTarget(selected,portTarget(port))));
      const radio=document.createElement('input');radio.type='radio';radio.name='connection-target';radio.id='port-option-'+index;radio.disabled=!canSelect(port);radio.checked=sameTarget(selected,portTarget(port));radio.onchange=()=>selectPort(port);
      const choice=document.createElement('td');choice.append(radio);
      const name=document.createElement('td'),label=document.createElement('label');label.htmlFor=radio.id;label.textContent=port.name;name.append(label);
      const product=document.createElement('td');product.textContent=port.product||'名称なし';product.title=port.reason||'';
      const state=document.createElement('td');state.textContent=labels[port.category]??'情報不足';
      row.append(choice,name,product,state);row.onclick=event=>{if(canSelect(port)&&event.target!==radio&&event.target.tagName!=='LABEL'){radio.checked=true;selectPort(port);radio.focus();}};
      $('port-rows').append(row);
    }
    controls();
  }
  function render(view) {
    if(view?.connected&&view.connection_id!==ownerConnection){ownerConnection=view.connection_id;onOwner();}
    clearTimeout(expiry);
    if(view?.sample)expiry=setTimeout(()=>{
      readouts.forEach(reading=>reading.update(null));trace.pause();$('hardware-empty').textContent='更新停止。現在値は無効です。';if(active)$('environment').textContent='入力更新なし';
    },Math.max(1,501-view.sample.received_age_ms));
    if(active&&!externalOwner)$('environment').textContent=names[view?.phase]??'状態不明';
    if(view?.candidate)candidate=portTarget(view.candidate);
    if(!candidate&&view?.port){const p=view.ports?.find(p=>p.name===view.port);if(p)candidate=portTarget(p);}
    if(!externalOwner){$('device-summary').textContent=view?.connected?(view.reported_id&&view.reported_id!=='unprovisioned'?'個体 …'+view.reported_id.slice(-8)+' · '+view.port:view.port+' · 個体未識別'):candidate?candidate.name+' · 再接続待ち':'—';$('device-summary').title=view?.reported_id||'';}
    $('hardware-empty').textContent=!view?'接続状態を取得できません。':view.failure?'接続先を確認して接続し直してください。':
      view.phase==='disconnected'?'「接続先を選ぶ」から対象を指定します。':view.phase==='waiting'?'7ch入力を待っています。':view.phase==='stale'?'更新停止。現在値は無効です。':'';
    readouts.forEach((reading,index)=>reading.update(view?.sample?.values[index]??null));trace.update(view);
    $('hardware-id').textContent=view?.reported_id==='unprovisioned'?'対応機器・個体ID未登録':view?.reported_id??({querying:'照会中',no_response:'応答なし（個体未識別）'}[view?.identity_state]||'未照会');
    $('hardware-firmware').textContent=view?.firmware??'—';$('hardware-protocol').textContent=view?.protocol??'—';$('hardware-frame-count').textContent=view?.received_frames??0;
    $('hardware-diagnostics').textContent=view?.diagnostics?.join('\n')||'診断メッセージはありません。';
    renderPorts();controls();
  }
  async function tick() {
    if(!active)return;
    const command=monitorBackgroundCommand({connected:!!controller.view?.connected,externalOwner,choosing:$('hardware-connect-dialog').open,pendingOperation:controller.pendingOperation});
    if(command)await controller.send(command);
    if(active)timer=setTimeout(tick,controller.view?.connected?150:2000);
  }
  function showPage(name){page=name;for(const key of ['device','adjust','diagnostics'])$('hardware-page-'+key).hidden=!active||key!==page;}
  async function choose() {
    if(!active||busy||externalOwner||controller.view?.connected)return;
    selected=candidate;selectionError='';portSignature='';$('show-other-ports').checked=false;$('query-on-connect').checked=false;renderPortDetails(null);$('port-error').hidden=true;
    $('hardware-connect-dialog').showModal();controls();await controller.send({action:'scan'});renderPorts(true);
  }
  function disconnect(){if(active&&controller.view?.connected)return controller.send({action:'disconnect'});return Promise.resolve(null);}
  async function reconnect(){if(!candidate||busy||externalOwner)return;if(!window.confirm('同じ実物を再接続したことを確認してください。接続情報を再照合し、既知の個体IDが異なる場合は停止します。'))return;const metadata_supported=!!controller.view?.expected_id;return controller.view?.candidate?controller.send({action:'reconnect',confirmed:true,metadata_supported}):controller.send({action:'connect',target:candidate,query_info:false,metadata_supported:false});}
  $('hardware-choose').onclick=()=>{void choose();};
  $('hardware-scan').onclick=async()=>{await controller.send({action:'scan'});renderPorts(true);};
  $('show-other-ports').onchange=()=>renderPorts(true);
  $('hardware-connect-cancel').onclick=()=>{selected=null;$('hardware-connect-dialog').close();};
  $('hardware-connect-dialog').oncancel=event=>{if(busy&&action==='connect')event.preventDefault();else selected=null;};
  $('hardware-connect-confirm').onclick=async()=>{
    if(!selected||busy)return;const target=structuredClone(selected),query_info=$('query-on-connect').checked;
    const reply=await controller.send({action:'connect',target,query_info,metadata_supported:query_info},controller.view?.revision);
    if(reply&&!reply.error&&!reply.cancelled){remember(target);selected=null;$('hardware-connect-dialog').close();$('hardware-disconnect').focus();}
  };
  $('hardware-disconnect').onclick=()=>{void disconnect();};
  $('hardware-reconnect').onclick=()=>{void reconnect();};
  $('hardware-info').onclick=()=>{void controller.send({action:'info',metadata_supported:true});};
  return {
    get connected(){return !!controller.view?.connected;},get busy(){return busy;},get candidate(){return candidate;},remember,choose,disconnect,reconnect,showPage,
    setExternalOwner(value){if(externalOwner===value)return;externalOwner=value;if(!value)render(controller.view);else controls();},
    async activate(value,name=page){active=value;clearTimeout(timer);clearTimeout(expiry);showPage(name);$('hardware-target').hidden=!active;
      if(!active){$('hardware-error').hidden=true;trace.clear();}
      if(active){if(!candidate)candidate=await window.__TAURI__.core.invoke('connection_candidate',{target:null}).catch(()=>null);await controller.send({action:'scan'});if(active)timer=setTimeout(tick,150);}controls();
    }
  };
}
