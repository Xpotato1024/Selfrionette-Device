// Candidate classification belongs to Rust; selection only preserves the exact descriptor snapshot.
export const targetKeys=['name','kind','serial_number','product','manufacturer','vid','pid'];
export function portTarget(port){return Object.fromEntries(targetKeys.map(key=>[key,port[key] ?? null]));}
export function sameTarget(a,b){return !!a&&!!b&&targetKeys.every(key=>a[key]===b[key]);}
export function visiblePorts(ports,showOther){return ports.filter(p=>showOther||p.category==='candidate');}
export function canSelect(port){return !!port&&port.category!=='bootloader';}
