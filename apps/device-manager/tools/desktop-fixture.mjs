// Test-only adapter, never packaged. It cannot enumerate or open OS ports.
export function installDesktopFixture(options={}) {
 const port=(name,category='candidate',serial=name)=>({name,category,kind:category==='other'?'pci':'usb',serial_number:serial,product:'USB シリアル デバイス（表示試験）',manufacturer:'試験用',vid:category==='other'?null:0x1b4f,pid:category==='bootloader'?0x9205:0x9206,reason:'表示試験用の記述子'});
 window.testFeed={revision:1,connection_id:1,connected:options.connected??true,port:'COM9',phase:'receiving',ports:[port('COM3','other'),port('COM8'),port('COM9')],received_frames:1,sample:{received_age_ms:0,values:[2207,61.4,1246.33,2496.67,-2716.6,306.27,49.27]},diagnostics:[],identity_state:'unqueried'};
 if(!window.testFeed.connected){window.testFeed.port=null;window.testFeed.phase='disconnected';window.testFeed.sample=null;}
 window.testMode={developer:false,source:'hardware',epoch:0};window.fixtureRequests=[];const listeners={};
 window.fixtureMenu=id=>listeners['manager-menu']?.({payload:id});
 window.fixtureHoldRead=false;window.fixtureReleaseRead=null;
 const synthetic={id:'srn-11111111111111111111111111111111',role:'left',phase:'not_started',epoch:0,rows:[],operations:[],token:{},steps:['基準','応答・範囲','入力比較','結果確認','記録'].map((label,i)=>({label,state:i?'pending':'current'}))};
 window.__TAURI__={event:{listen:async(name,handler)=>{listeners[name]=handler;return ()=>{};}},core:{invoke:async(name,{request,command}={})=>{
  window.fixtureRequests.push({name,request:structuredClone(request)});
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
  if(name==='application_info')return {product_name:'Selfrionette Device Manager',version:options.version??'表示試験',publisher:'Xpotato.net',homepage:'https://xpotato.net'};
  if(name==='application_command'){
   if(command.action==='ports')return {ports:structuredClone(window.testFeed.ports)};
   if(command.action==='snapshot'||command.action==='disconnect')return {view:{connected:false,ready:false}};
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
  if(c.action==='disconnect'){v.connected=false;v.port=null;v.phase='disconnected';v.sample=null;}
  if(c.action==='read'&&v.sample)v.received_frames++;
  v.revision++;return {view:structuredClone(v),error};
 }}};
}
