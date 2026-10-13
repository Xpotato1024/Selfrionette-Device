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
 let points=[],lastFrame=null,lastConnection=null;
 return {add(view,now){
  if(!view?.sample||!view.connected){points=[];lastFrame=null;lastConnection=null;return;}
  if(view.port!==lastConnection){points=[];lastFrame=null;lastConnection=view.port;}
  if(view.received_frames!==lastFrame){points.push({time:now,values:[...view.sample.values]});lastFrame=view.received_frames;}
  points=points.filter(p=>p.time>=now-windowMs).slice(-limit);
 },get points(){return points;},clear(){points=[];lastFrame=null;lastConnection=null;}};
}
