// 試験専用。模擬センサの値投入だけを再試行し、実機操作・校正命令は受け付けない。
export async function setScriptedInput(invoke,raw,flags,{attempts=50,wait=()=>new Promise(resolve=>setTimeout(resolve,20))}={}){
 for(let attempt=0;attempt<attempts;attempt++){
  try{return await invoke({action:'fixture_input',raw,flags});}
  catch(error){if(!String(error).includes('別の処理中です。操作は予約されません。')||attempt+1===attempts)throw error;await wait();}
 }
 throw Error('scripted input requires a positive attempt limit');
}
