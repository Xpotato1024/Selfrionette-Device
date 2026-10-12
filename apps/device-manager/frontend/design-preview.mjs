// Presentation-only adapter. It never evaluates calibration or accesses hardware.
export function createDesignPreview() {
  let stage=0, epoch=0, role='left';
  const view=()=>({stage,epoch,role,id:role==='left'?'srn-11111111111111111111111111111111':'srn-22222222222222222222222222222222',
    provenance:'static synthetic design preview',hardware_verified:false,control_ready:false,rows:stage>=4?Array.from({length:7},(_,i)=>{
      const span=i%2===0?20+i*5:40+i*5, sign=i%2===0?1:-1;
      return {wire:i,physical:i+1,firmware:10+sign*span*0.5,baseline:10,delta:sign*span*0.5,sign,oriented:span*0.5,positive_span:span,normalized:0.5,displayed:0.5,saturated:false,evidence:'固定の合成表示。Rust判定ではありません。'};
    }):[],export:null,message:'ブラウザー表示確認。校正判定・実値保存・実機接続は行いません。'});
  return async(action,argument='')=>{
    if(action==='status')return view();
    if(action==='begin'){role=argument;epoch++;stage=1;}
    else if(action==='baseline') stage=2;
    else if(action==='assess'){stage=2;if(argument!=='good')throw Error('表示用の判定不可ケース');stage=3;}
    else if(action==='preview') {if(argument!=='0.5')throw Error('固定previewは0.5のみ。Rust GUIで他条件を検証してください。');stage=4;}
    else if(action==='flip') {stage=3;throw Error('手動反転の判定はRust GUIで行ってください。');}
    else if(action==='confirm'||action==='export')throw Error('ブラウザー表示確認ではdraft保存できません。');
    else if(action==='invalidate') stage=1;
    else throw Error('unknown presentation action');
    return view();
  };
}
