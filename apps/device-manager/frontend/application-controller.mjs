// A user-ready capture is never queued behind polling or another action.
export function createApplicationController(invoke, {render, error, busy}) {
  let current=null, running=false, generation=0;
  return {
    get view(){return current;},
    get busy(){return running;},
    invalidate(){generation++;current=null;render(null);},
    async send(command) {
      if(running) {
        if(!['tick','snapshot','stream_tick','capture_tick'].includes(command.action))error('処理中です。準備ができたらもう一度押してください。開始は予約されません。');
        return null;
      }
      const passive=['tick','snapshot','stream_tick','capture_tick'].includes(command.action), epoch=generation;
      running=true;busy(true);
      try {
        const reply=await invoke(command,current);
        if(epoch!==generation)return null;
        current=reply.view??reply;render(current);if(reply.error||!passive)error(reply.error??'');return reply;
      } catch(reason) {
        if(epoch===generation){error(String(reason));}
        return null;
      } finally {running=false;busy(false);}
    }
  };
}
