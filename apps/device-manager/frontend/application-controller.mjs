// Only the current bounded read may precede a deliberate action. A changed
// context or readiness deadline rejects it; no later dispatch or retry survives.
const passiveActions=new Set(['tick','snapshot','stream_tick','capture_tick']);
const contextOf=view=>JSON.stringify([view?.connected,view?.owns_handle,view?.port,view?.info,view?.setup,view?.operator]);
export function createApplicationController(invoke, {render, error, busy, operation=()=>{}, now=()=>performance.now()}) {
  let current=null, running=false, reading=null, generation=0, sequence=0;
  async function request(command,epoch,passive) {
    try {
      const reply=await invoke(command,current);
      if(epoch!==generation)return null;
      current=reply.view??reply;render(current);
      if(reply.error||!passive)error(reply.error??'');
      return reply;
    } catch(reason) {
      if(epoch===generation){current=null;render(null);error(String(reason));}
      return null;
    }
  }
  return {
    get view(){return current;},
    get busy(){return running;},
    invalidate(){generation++;current=null;render(null);},
    async send(command) {
      const passive=passiveActions.has(command.action),epoch=generation;
      if(passive) {
        if(running||reading)return null;
        reading=request(command,epoch,true);
        try{return await reading;}finally{reading=null;}
      }
      if(running) {
        error('処理中です。準備ができたらもう一度押してください。開始は予約されません。');
        return null;
      }
      const id=++sequence,context=contextOf(current),started=now();
      running=true;busy(true);
      operation({id,action:command.action,status:'pending'});
      let result=null,deadline;
      try {
        if(reading) {
          await Promise.race([reading,new Promise(resolve=>{deadline=setTimeout(resolve,250);})]);
          if(reading||now()-started>250)throw Error('読取り待機が長すぎます。準備を確認して再度操作してください。開始は予約されません。');
          if(contextOf(current)!==context)throw Error('context changed during read; confirm the current device and repeat the action');
        }
        if(epoch!==generation)throw Error('context invalidated; repeat the action');
        result=await request(command,epoch,false);
        return result;
      } catch(reason) {
        error(String(reason));
        return null;
      } finally {
        clearTimeout(deadline);running=false;busy(false);
        operation({id,action:command.action,status:result&&!result.error?'success':'error'});
      }
    }
  };
}
