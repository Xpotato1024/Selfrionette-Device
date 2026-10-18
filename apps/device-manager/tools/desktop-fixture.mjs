// Test-only adapter, never packaged. It cannot enumerate or open OS ports.
export function installDesktopFixture(options={}) {
 const port=(name,category='candidate',serial=name)=>({name,category,kind:category==='other'?'pci':'usb',serial_number:serial,product:'USB シリアル デバイス（表示試験）',manufacturer:'試験用',vid:category==='other'?null:0x1b4f,pid:category==='bootloader'?0x9205:0x9206,reason:'表示試験用の記述子'});
 window.testFeed={revision:1,connection_id:1,connected:options.connected??true,port:'COM9',phase:'receiving',ports:[port('COM3','other'),port('COM8'),port('COM9')],received_frames:1,sample:{received_age_ms:0,values:[2207,61.4,1246.33,2496.67,-2716.6,306.27,49.27]},diagnostics:[],identity_state:'unqueried'};
 if(!window.testFeed.connected){window.testFeed.port=null;window.testFeed.phase='disconnected';window.testFeed.sample=null;}
 window.testMode={developer:false,source:'hardware',epoch:0};window.fixtureRequests=[];const listeners={};
 let candidate=null,updatePlan=null;try{candidate=JSON.parse(localStorage.getItem('fixture-candidate'));}catch{}
 window.testModern={connected:false,owns_handle:false,ready:false,candidate:null};
 if(options.emptyPorts)window.testFeed.ports=[];
 window.fixtureMenu=id=>listeners['manager-menu']?.({payload:id});
 window.fixtureHoldRead=false;window.fixtureReleaseRead=null;
 const synthetic={id:'srn-11111111111111111111111111111111',role:'left',phase:'not_started',epoch:0,rows:[],operations:[],token:{},steps:['基準','応答・範囲','入力比較','結果確認','記録'].map((label,i)=>({label,state:i?'pending':'current'}))};
 window.__TAURI__={event:{listen:async(name,handler)=>{listeners[name]=handler;return ()=>{};}},core:{invoke:async(name,{request,command,target}={})=>{
  window.fixtureRequests.push({name,request:structuredClone(request),command:structuredClone(command)});
  if(name==='connection_candidate'){if(target){candidate=target;localStorage.setItem('fixture-candidate',JSON.stringify(target));}return candidate;}
  if(name==='updater_invalidate'){updatePlan=null;return null;}
  if(name==='updater_prepare'){if(window.testFeed.connected)throw Error('接続を解放してください');updatePlan={status:'ready',plan_id:'fixture-plan',apply_enabled:true,target:request.target,manifest:{release_version:'1.0.0',family:request.family,firmware_name:({modern:'Selfrionette Managed',legacy_7ch:'Selfrionette Original',compatible_7ch:'Selfrionette Compatible'})[request.family],distribution_version:'1.0.0',firmware_version:request.family==='modern'?'1.0.0':'',supported_protocols:request.family==='modern'?[2,3]:[],protocol_schema:request.family==='modern'?3:0},source_trust:'test_only'};return updatePlan;}
  if(name==='updater_progress')return structuredClone(window.testUpdateProgress??{operation_id:'',sequence:0,stage:'idle',active:false,percent:null});
  if(name==='updater_apply'&&options.holdUpdater){window.testUpdateProgress={operation_id:request.plan_id,sequence:1,stage:'resetting',target:request.target?.name??'COM9',active:true,percent:null};return await new Promise(resolve=>{window.fixtureFinishUpdate=resolve;});}
  if(name==='updater_apply'){if(!updatePlan||request.plan_id!==updatePlan.plan_id||!request.operator_confirmed)throw Error('unapproved');return {status:'verified',needs_recovery:false};}
  if(name==='mode_command'){
   const c=request.command,m=window.testMode;
   if(c.action!=='snapshot'){
    if(request.expected!==m.epoch)throw Error('stale mode');
    if(c.action==='enable')m.developer=true;
    if(c.action==='disable'){m.developer=false;m.source='hardware';}
    if(c.action==='select'){if(c.source==='synthetic'&&!m.developer)throw Error('developer required');if(c.source!==m.source&&window.testFeed.connected)throw Error('先に実機との接続を切断してください。');m.source=c.source;}
    m.epoch++;listeners['manager-mode']?.({payload:structuredClone(m)});
   }
   return structuredClone(m);
  }
  if(name==='application_info')return {product_name:'Selfrionette Device Manager',version:options.version??'表示試験',firmware_editions:options.editions,publisher:'Xpotato.net',homepage:'https://xpotato.net'};
  if(name==='application_command'){
   if(command.action==='ports')return {ports:structuredClone(window.testFeed.ports)};
   const m=window.testModern;
   if(command.action==='connect'||command.action==='reconnect'){
    if(window.testFeed.connected)throw Error('monitor owns handle');
    const target=command.target??m.candidate;if(!target)throw Error('missing candidate');
    Object.assign(m,{connected:true,owns_handle:true,ready:false,candidate:target,port:target.name,info:{device_id:'srn-11111111111111111111111111111111'},sample:null,output:null});
   }
   if(command.action==='disconnect'||command.action==='pause')Object.assign(m,{connected:false,owns_handle:false,ready:false,paused:true,sample:null,output:null});
   if(['snapshot','disconnect','pause','connect','reconnect','stream_start','stream_tick'].includes(command.action))return {view:structuredClone(m)};
   throw Error('表示fixtureでは装置設定・実個人校正は実行できません。');
  }
  if(name==='device_command'){if(!window.testMode.developer||window.testMode.source!=='synthetic')throw Error('synthetic gate');return {view:structuredClone(synthetic),error:null};}
  if(name!=='hardware_command')throw Error('Unexpected test command '+name);
  const c=request.command,v=window.testFeed;
  if(c.action==='read'&&window.fixtureHoldRead)await new Promise(resolve=>{window.fixtureReleaseRead=()=>{window.fixtureHoldRead=false;resolve();};});
  let error=null;
  if(c.action==='connect'){
   const keys=['name','kind','serial_number','product','manufacturer','vid','pid'];const target=v.ports.find(p=>keys.every(k=>(p[k]??null)===(c.target[k]??null)));
   if(!target||target.category==='bootloader')error='選択時からポートの情報が変わりました。';
   else {v.connected=true;v.connection_id++;v.port=c.target.name;v.phase='waiting';v.identity_state=c.query_info?'querying':'unqueried';}
  }
  if(c.action==='disconnect'){v.candidate=v.ports.find(p=>p.name===v.port)??v.candidate;v.connected=false;v.phase='paused';v.sample=null;}
  if(c.action==='reconnect'){v.connected=true;v.connection_id++;v.phase='waiting';v.sample=null;}
  if(c.action==='read'&&v.sample)v.received_frames++;
  v.revision++;return {view:structuredClone(v),error};
 }}};
}
