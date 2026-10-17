// One bounded I/O request at a time; one pending user operation. Polling never disables controls.
export function createHardwareController(invoke, {render, error, busy}) {
  let view=null, running=null, pending=null;
  const passive=action=>['snapshot','scan','read'].includes(action);
  function reject(message) { error(message); return Promise.resolve(null); }
  async function execute(task) {
    running=task;
    try {
      if (!task.passive && (task.command.action!=='connect'||task.queued) && task.session!==view?.connection_id) {
        error('操作待ちの間に接続対象が変わりました。選び直してください。'); task.resolve(null); return;
      }
      const reply=await invoke({expected:task.queued ? view?.revision ?? null : task.expected ?? view?.revision ?? null,command:task.command});
      view=reply.view;render(view);error(reply.error ?? '');task.resolve(reply);
    } catch (failure) {
      view=null;render(null);error('接続状態を取得できません。再検索してください。'+String(failure));task.resolve(null);
    } finally {
      running=null;
      if(pending) {const next=pending;pending=null;void execute(next);}
      else if(!task.passive)busy(false,task.command.action);
    }
  }
  return {
    get view(){return view;},
    get pendingOperation(){return pending?.command.action ?? (running&&!running.passive?running.command.action:null);},
    send(command,expected) {
      const background=passive(command.action);
      if(background&&(running||pending))return Promise.resolve(null);
      if(!background&&running&&!running.passive)return reject('別の操作を処理中です。');
      if(pending) {
        if(pending.command.action===command.action)return pending.promise;
        if(command.action!=='disconnect')return reject('別の操作の完了を待っています。');
        pending.resolve({cancelled:true});pending=null;
      }
      let resolve;const promise=new Promise(r=>{resolve=r;});
      const task={command,expected,queued:!!running,passive:background,session:view?.connection_id,resolve,promise};
      if(!background)busy(true,command.action);
      if(running)pending=task;else void execute(task);
      return promise;
    }
  };
}
