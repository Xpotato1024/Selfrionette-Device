// Synthetic input only. These coefficients are NOT Selfrionette measurements.
const n = 7, epoch = 42;
let timestamp = 0;
const mixing = Array.from({length:n},(_,i)=>Array.from({length:n},(_,j)=>i===j?1:0));
mixing[1][1]=-1; mixing[0][1]=0.3; mixing[1][0]=-0.2; mixing[5][4]=0.15;
function values(intent) {
  return mixing.map(row=>10+row.reduce((sum,a,j)=>sum+a*intent[j]*(intent[j]>=0?1000+j*100:600+j*50),0));
}
function sample(values) {timestamp+=10;return {timestamp_ms:timestamp,epoch,values,quality:'usable'};}
function window(base) {return Array.from({length:8},(_,i)=>sample(base.map(v=>v+(i%2?0.1:-0.1))));}
const context={device_id:'srn-0123456789abcdef0123456789abcdef',firmware:'synthetic-input-v1',
  assembly:'fixture-1',epoch,user:'synthetic-operator',posture:'synthetic-posture'};
const recording={schema_version:1,recording_id:'synthetic-seven-axis-v1',sample_domain:'firmware_v2_vector',
  origin:'synthetic',context,axes:Array.from({length:n},(_,channel)=>({channel,positive_direction:'synthetic-axis-'+channel+'-positive'})),
  limits:{noise_floor_counts:Array(n).fill(0.2),max_rest_range_counts:Array(n).fill(1),max_drift_counts:Array(n).fill(2),
    min_signal_to_noise:5,repeat_tolerance:0.05,direction_tolerance:0.05,max_condition_number:20,max_noise_gain:10,max_sample_gap_ms:100},
  signal:{time_constant_ms:40,noise_multiplier:3,release_ratio:0.5,max_deadzone_fraction:0.1},
  neutral:window(Array(n).fill(10)),trials:[],replay:[]};
for(let axis=0;axis<n;axis++)for(const sign of [1,-1])for(let repeat=0;repeat<2;repeat++) {
  const level=sign*(repeat?0.5:1),intent=Array(n).fill(0);intent[axis]=level;
  recording.trials.push({id:'trial-'+axis+'-'+sign+'-'+repeat,axis,level,instruction:'synthetic instructed effort '+level,
    neutral:window(Array(n).fill(10)),loaded:window(values(intent)),released:window(Array(n).fill(10))});
}
const sequence=[Array(n).fill(0),[0.4,-0.7,0.25,-0.3,0.55,0.2,-0.15],Array(n).fill(0),
  [-0.5,0.2,-0.3,0.7,-0.2,0.4,0.3],Array(n).fill(0)];
for(const intent of sequence)for(let i=0;i<64;i++) {
  recording.replay.push({sample:sample(values(intent).map(v=>v+(i%2?0.1:-0.1))),intent});
}
process.stdout.write(JSON.stringify(recording)+'\n');
