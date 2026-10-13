import {createDisplayHistory} from './readout.mjs';
// Display samples only. No command, timer, file write or physical calibration.
export function createTrace(document,clock=()=>performance.now()) {
 const history=createDisplayHistory(),$=id=>document.getElementById(id);let now=0,channel=0,fixed={},connection=null;
 function draw(){
  const points=history.points;$('trace-title').textContent='ch '+channel+' の履歴';$('trace-empty').hidden=points.length>0;
  $('trace-end').textContent=history.live?'現在':'最終受信';
  if(!points.length){$('trace-line').setAttribute('d','');$('trace-top').textContent='—';$('trace-bottom').textContent='—';$('trace-state').textContent='入力待ち';return;}
  const maximum=Math.max(...points.map(p=>Math.abs(p.values[channel])),1);
  const isFixed=$('trace-scale').value==='fixed';
  if(isFixed&&!fixed[channel])fixed[channel]=maximum;
  const scale=isFixed?fixed[channel]:maximum;
  $('trace-top').textContent='+'+scale.toPrecision(3);$('trace-bottom').textContent='−'+scale.toPrecision(3);
  $('trace-state').textContent=(history.live?'受信中':'更新停止')+(maximum>scale?' · 固定範囲外の値あり':'');
  const end=history.live?now:history.end;
  $('trace-line').setAttribute('d',points.map((p,i)=>{
   const move=i===0||p.segment!==points[i-1].segment;
   const x=Math.max(0,Math.min(640,(p.time-(end-30000))/30000*640));
   const y=Math.max(-300,Math.min(600,150-p.values[channel]/scale*150));
   return (move?'M':'L')+x.toFixed(2)+' '+y.toFixed(2);
  }).join(' '));
 }
 $('trace-scale').onchange=()=>{fixed={};draw();};
 return {update(view){now=clock();history.add(view,now);if(connection!==history.connection){connection=history.connection;fixed={};}draw();},
   select(value){channel=value;draw();},pause(){history.pause();draw();},clear(){history.clear();connection=null;fixed={};draw();}};
}
