import {spawnSync} from 'node:child_process';import {fileURLToPath} from 'node:url';
const labels={'choose-device':'接続先を選ぶ…','about':'アプリ情報','help':'使い方','enable-developer':'開発者向け機能を有効にする…','source-synthetic':'合成試験','source-hardware':'実機画面','disable-developer':'開発者モードを終了'};
export function nativeMenu(pid,action,capturePath) {
 const args=['-NoProfile','-File',fileURLToPath(new URL('./native-menu.ps1',import.meta.url)),'-AppProcess',String(pid)];
 if(capturePath)args.push('-CapturePath',capturePath);
 if(action){if(!labels[action])throw Error('Unknown test menu action');args.push('-ItemText',labels[action]);}
 const result=spawnSync('powershell.exe',args,{encoding:'utf8',windowsHide:true,timeout:15000});
 if(result.error)throw result.error;if(result.status!==0)throw Error('Native menu test failed: '+result.stdout+result.stderr);
 return JSON.parse(result.stdout.trim());
}
export async function enableSynthetic(page,menu) {
 const snapshot=()=>page.evaluate(()=>window.__TAURI__.core.invoke('mode_command',{request:{expected:null,command:{action:'snapshot'}}}));
 let mode=await snapshot();
 if(!mode.developer){await menu('enable-developer');await page.locator('#developer-enable').click();await page.waitForFunction(()=>document.getElementById('workspace').dataset.developer==='true');mode=await snapshot();}
 if(mode.source!=='synthetic'){await menu('source-synthetic');}
 await page.waitForFunction(()=>document.getElementById('workspace').dataset.source==='synthetic'&&!document.getElementById('role').disabled);
}
