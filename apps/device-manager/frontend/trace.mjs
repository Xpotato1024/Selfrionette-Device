import {createDisplayHistory} from './readout.mjs';
// Read-only rendering; this module has no IPC, device commands, timers or persistence.
export function createTrace(document,clock=()=>performance.now()) {
 const history=createDisplayHistory(),$=id=>document.getElementById(id);let now=0;
 function draw(){
  const points=history.points,ch=Number($('trace-channel').value);$('trace-empty').hidden=points.length>0;
  if(!points.length){$('trace-line').setAttribute('d','');$('trace-top').textContent='—';$('trace-bottom').textContent='—';return;}
  const maximum=Math.max(...points.map(p=>Math.abs(p.values[ch])),1),scale=maximum;
  $('trace-top').textContent='+'+scale.toPrecision(3);$('trace-bottom').textContent='−'+scale.toPrecision(3);
  $('trace-line').setAttribute('d',points.map((p,i)=>(i?'L':'M')+Math.max(0,Math.min(640,(p.time-(now-30000))/30000*640)).toFixed(2)+' '+(150-p.values[ch]/scale*120).toFixed(2)).join(' '));
 }
 $('trace-channel').onchange=draw;
 return {update(view){now=clock();history.add(view,now);draw();},clear(){history.clear();draw();}};
}
