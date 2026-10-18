// Firmware識別と進捗の表示専用変換。書込み/成功判定はRustが所有する。
const stages={idle:'更新待ち',checking:'対象と保護記録を確認中',resetting:'書込み待機モードへ切替中',bootloader:'書込み先の再接続を確認中',awaiting_confirmation:'同じ実物か確認してください',programming:'Firmwareを書込み中',verifying:'書込み内容を照合中',reconnecting:'再接続・Firmwareと設定を確認中',complete:'更新確認が完了しました',needs_recovery:'要復旧：更新の確認が必要です',failed:'更新を完了できませんでした'};
export function progressView(snapshot){
 const stage=Object.hasOwn(stages,snapshot?.stage)?snapshot.stage:'checking';
 const percent=Number.isInteger(snapshot?.percent)&&snapshot.percent>=0&&snapshot.percent<=100&&['programming','verifying','complete'].includes(stage)?snapshot.percent:null;
 return {title:stage==='programming'&&percent===null?'Firmwareの書込み・照合を実行中':stages[stage],percent,active:!!snapshot?.active,
  detail:(snapshot?.target?'対象: '+snapshot.target+'。 ':'')+(stage==='complete'?'全域照合と更新後の確認を完了しました。':snapshot?.active?'USBを抜かず、アプリを終了しないでください。割合は現在の工程内の進捗です。':'検査結果・技術情報を確認してください。')};
}
export function createProgressTracker(operationId,baseline=-1){
 let sequence=baseline;
 return snapshot=>{if(snapshot?.operation_id!==operationId||!Number.isSafeInteger(snapshot.sequence)||snapshot.sequence<=sequence)return null;sequence=snapshot.sequence;return progressView(snapshot);};
}
const familyNames={modern:'Selfrionette Managed',legacy_7ch:'Selfrionette Original',compatible_7ch:'Selfrionette Compatible'};
export function packageIdentityRows(manifest){
 const m=manifest??{},name=m.firmware_name??familyNames[m.family??'modern']??'未確認';
 return [['書込むFirmware名',name],['Firmware配布版',m.distribution_version??'旧package・配布版未宣言'],
 ['本体Firmware版',m.firmware_version||'取得機能なし（配布版とは別）'],
 ['対応通信プロトコル',m.supported_protocols?.length?m.supported_protocols.map(p=>'Protocol '+p).join(' / '):m.protocol_schema===3?'Protocol 2 / Protocol 3':'Legacy 7ch（版番号なし／v1相当）'],
 ['同梱するアプリ版',m.release_version??'未確認']];
}
export function observedIdentityRows(observation){
 const o=observation??{},info=o.info;
 return [['更新前に確認したFirmware版',info?.firmware_version||'未取得・不明'],['更新前に確認した通信',o.kind==='protocol3'?'Protocol 3（情報応答を照合）':o.kind==='legacy'?'Protocol 2（情報応答を照合）':'未識別（vectorだけでは版を判別できません）']];
}
