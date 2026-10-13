// Display only; rounded values never feed the device/core.
const fixed=new Intl.NumberFormat('en-US',{minimumFractionDigits:2,maximumFractionDigits:2});
export function readingParts(value) {
 if(value===null||value===undefined||!Number.isFinite(value))return {sign:'',integer:'—',fraction:'',exponent:'',raw:''};
 if(Object.is(value,-0)||value===0)value=0;
 const m=Math.abs(value),scientific=m>=1e9-0.005||(m>0&&m<0.005);
 const [mantissa,exponent]=scientific?m.toExponential(2).split('e'):[fixed.format(m),''];
 const [integer,fraction]=mantissa.split('.');
 return {sign:value<0?'−':'',integer,fraction:'.'+fraction,exponent:exponent?'e'+exponent:'',raw:String(value)};
}
export function createReadout(document) {
 const root=document.createElement('span');root.className='readout';const fields=['sign','integer','fraction','exponent'];
 const nodes=Object.fromEntries(fields.map(name=>{const node=document.createElement('span');node.className='readout-'+name;root.append(node);return [name,node];}));
 return {element:root,update(value){const p=readingParts(value);const text={...p,sign:'',integer:p.sign+p.integer};for(const k of fields)if(nodes[k].textContent!==text[k])nodes[k].textContent=text[k];root.title=p.raw;root.setAttribute('aria-label',p.raw||'未受信');}};
}
export function createDisplayHistory(limit=200,windowMs=30000) {
 let points=[],lastFrame=null,connection=null,gap=true,segment=0,live=false,end=0;
 return {add(view,now){
  if(!view?.connected){live=false;gap=true;return;}
  const id=String(view.connection_id??view.port);
  if(id!==connection){points=[];lastFrame=null;gap=true;connection=id;segment=0;}
  const age=view.sample?.received_age_ms??0;
  if(!view.sample||age>500){live=false;gap=true;return;}
  const time=now-Math.max(0,age);
  if(view.received_frames!==lastFrame){
   if(gap||time-end>500)segment++;
   points.push({time,segment,values:[...view.sample.values]});lastFrame=view.received_frames;end=time;
   points=points.filter(p=>p.time>=time-windowMs).slice(-limit);
  }
  gap=false;live=true;
 },get points(){return points;},get live(){return live;},get end(){return end;},get connection(){return connection;},
 pause(){live=false;gap=true;},clear(){points=[];lastFrame=null;connection=null;gap=true;segment=0;live=false;end=0;}};
}
