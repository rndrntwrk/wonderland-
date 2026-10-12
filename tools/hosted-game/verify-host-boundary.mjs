// Executes the actual native host process with a declared authored cooked release.
// The process has no baked-in actor/content/account fixture. This is local service evidence.
import {spawn,execFileSync} from 'node:child_process';
import {mkdtemp,readFile,writeFile,mkdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {createInterface} from 'node:readline';
import assert from 'node:assert/strict';
const root=await mkdtemp(join(tmpdir(),'wonderland-host-'));
const release=join(root,'release');
const exe=resolve('target/debug/wonderland-native-lot-host');
const helper=resolve('target/debug/examples/host-boundary-fixture');
const run=(file,args)=>execFileSync(file,args,{encoding:'utf8',timeout:60000});
run(resolve('target/debug/examples/host-cooked-fixture'),[release]);
run(resolve('target/debug/runtime-bridge'),['release-prepare',join(release,'draft.json'),'--release-dir',release,'--output',join(release,'binding.json')]);
run(helper,['prepare',release]);
const reports=[];let process;
function launch(config){
  const p=spawn(exe,[config],{stdio:['pipe','pipe','pipe']});let stderr='';p.stderr.on('data',d=>stderr+=d);
  const queue=[],waiters=[];let ended=false;
  createInterface({input:p.stdout}).on('line',line=>{const value=JSON.parse(line);if(waiters.length)waiters.shift().resolve(value);else queue.push(value);});
  p.on('exit',()=>{ended=true;for(const waiter of waiters.splice(0))waiter.reject(Error(stderr||'host exited'));});
  const next=()=>queue.length?Promise.resolve(queue.shift()):ended?Promise.reject(Error(stderr||'host exited')):new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('host reply timeout')),10000);waiters.push({resolve:v=>{clearTimeout(timer);resolve(v)},reject:e=>{clearTimeout(timer);reject(e)}});});
  return {p,next,async ask(value){p.stdin.write(JSON.stringify(value)+'\n');return next();},stderr:()=>stderr};
}
try {
 process=launch(join(release,'host.json'));
 const initial=await process.next();assert.equal(initial.kind,'committed_state');assert.equal(initial.cursor.completed_tick,'0');
 const commands_hex=(await readFile(join(release,'commands.txt'),'utf8')).trim();
 const request={kind:'prepare',batch_id:'1',binding:initial.binding,predecessor:initial.cursor,commands_hex};
 const proposal=await process.ask(request);assert.equal(proposal.kind,'proposal');assert.equal(proposal.payload.result.completed_tick,'2');
 const unchanged=await process.ask({kind:'inspect'});assert.deepEqual(unchanged.cursor,initial.cursor);assert.equal(unchanged.checkpoint_hex,initial.checkpoint_hex);
 assert.deepEqual(await process.ask(request),proposal);
 assert.equal((await process.ask({...request,commands_hex:commands_hex+'00'})).kind,'denied');
 assert.equal((await process.ask({...request,batch_id:'2'})).kind,'denied');
 assert.deepEqual(await process.ask({kind:'resolve',batch_id:'1',digest:proposal.digest,decision:{status:'unknown'}}),proposal);
 assert.equal((await process.ask({kind:'resolve',batch_id:'1',digest:proposal.digest,decision:{status:'committed',cursor:initial.cursor}})).kind,'denied');
 const receipt=await process.ask({kind:'resolve',batch_id:'1',digest:proposal.digest,decision:{status:'committed',cursor:proposal.payload.result}});assert.equal(receipt.kind,'committed');
 assert.deepEqual(await process.ask({kind:'lookup',batch_id:'1'}),receipt);
 assert.deepEqual(await process.ask(request),receipt);
 const committed=await process.ask({kind:'inspect'});assert.deepEqual(committed.cursor,proposal.payload.result);
 await writeFile(join(release,'initial.json'),JSON.stringify(initial));await writeFile(join(release,'receipt.json'),JSON.stringify(receipt));
 const replay=run(helper,['replay',release]);assert.match(replay,/attribute 41/);
 reports.push('Actual cooked behavior executes; committed WLR1 replay matches hash and attribute 41');
 reports.push('Tentative state hidden; stable retry/unknown/lookup; no duplicate execution');
 const emptyTick='01000000000000000000000000000000';
 const next={...request,batch_id:'2',predecessor:committed.cursor,commands_hex:emptyTick};
 const rejected=await process.ask(next);assert.equal(rejected.kind,'proposal');
 assert.equal((await process.ask({kind:'resolve',batch_id:'2',digest:rejected.digest,decision:{status:'rejected'}})).kind,'rejected');
 assert.deepEqual((await process.ask({kind:'inspect'})).cursor,committed.cursor);
 for(const invalid of [
 {...next,batch_id:3}, {...next,batch_id:'03'}, {...next,batch_id:'18446744073709551616'},
 {...next,batch_id:'3',binding:{...next.binding,lot_incarnation:'2'}},
 {...next,batch_id:'3',binding:{...next.binding,binding_sha256:'0'.repeat(64)}},
 {...next,batch_id:'3',predecessor:initial.cursor},
 {...next,batch_id:'3',commands_hex:'ff'.repeat(262145)},
 {...next,batch_id:'3',commands_hex:'0000000000000000'},
 {...next,batch_id:'3',commands_hex:'ffffffffffffffff'},
 {...next,batch_id:'3',commands_hex:'0100000000000000ffffffffffffffff'},
 ]) assert.equal((await process.ask(invalid)).kind,'denied');
 reports.push('Rejected candidate unchanged; decimal identities, content/incarnation, predecessor and byte/count bounds rejected');

 const buildInput={...next,batch_id:'3',commands_hex:(await readFile(join(release,'build-commands.txt'),'utf8')).trim()};
 const buildProposal=await process.ask(buildInput);assert.equal(buildProposal.kind,'proposal');
 assert.notEqual(buildProposal.payload.events_hex,'0000000000000000');
 assert.equal((await process.ask({kind:'resolve',batch_id:'3',digest:buildProposal.digest,decision:{status:'rejected'}})).kind,'rejected');
 assert.equal((await process.ask({kind:'inspect'})).checkpoint_hex,committed.checkpoint_hex);
 const acceptedBuild=await process.ask({...buildInput,batch_id:'4'});assert.equal(acceptedBuild.kind,'proposal');
 const buildReceipt=await process.ask({kind:'resolve',batch_id:'4',digest:acceptedBuild.digest,decision:{status:'committed',cursor:acceptedBuild.payload.result}});
 assert.equal(buildReceipt.kind,'committed');assert.equal(buildReceipt.events_hex,acceptedBuild.payload.events_hex);
 assert.deepEqual(await process.ask({kind:'lookup',batch_id:'4'}),buildReceipt);
 await writeFile(join(release,'build-receipt.json'),JSON.stringify(buildReceipt));assert.match(run(helper,['events',release]),/exact u64/);
 reports.push('Real BeginBuild retained through commit/lookup; rejection leaves checkpoint unchanged; operation beyond JS safe integer remains exact');

 const afterBuild=await process.ask({kind:'inspect'});
 const effectInput={...request,batch_id:'5',predecessor:afterBuild.cursor,commands_hex:(await readFile(join(release,'effect-commands.txt'),'utf8')).trim()};
 const effectProposal=await process.ask(effectInput);assert.equal(effectProposal.kind,'proposal');assert.notEqual(effectProposal.payload.effects_hex,'0000000000000000');
 assert.equal((await process.ask({kind:'inspect'})).checkpoint_hex,afterBuild.checkpoint_hex);
 await process.ask({kind:'resolve',batch_id:'5',digest:effectProposal.digest,decision:{status:'rejected'}});
 assert.equal((await process.ask({kind:'inspect'})).checkpoint_hex,afterBuild.checkpoint_hex);
 const effectAgain=await process.ask({...effectInput,batch_id:'6'});
 const effectReceipt=await process.ask({kind:'resolve',batch_id:'6',digest:effectAgain.digest,decision:{status:'committed',cursor:effectAgain.payload.result}});assert.equal(effectReceipt.kind,'committed');
 assert.deepEqual(await process.ask({kind:'lookup',batch_id:'6'}),effectReceipt);
 await writeFile(join(release,'effect-receipt.json'),JSON.stringify(effectReceipt));assert.match(run(helper,['effects',release]),/exact epochs/);
 reports.push('Actual dialog effect is withheld on rejection and retained on commit with epochs beyond JS safe integer');
 process.p.stdin.end();
 const config=JSON.parse(await readFile(join(release,'host.json'),'utf8'));
 await writeFile(join(release,'bad.json'),JSON.stringify({...config,binding_sha256:'0'.repeat(64)}));
 const bad=launch(join(release,'bad.json'));await assert.rejects(bad.next(),/digest mismatch/);
 reports.push('Wrong independently pinned cooked binding prevents startup');
 const oversized=launch(join(release,'host.json'));await oversized.next();oversized.p.stdin.on('error',()=>{});oversized.p.stdin.write(' '.repeat(1048577));await assert.rejects(oversized.next(),/1 MiB/);
 reports.push('Overlong input closes host without unbounded line allocation');
 await mkdir('tests/output/native-host',{recursive:true});await writeFile('tests/output/native-host/report.json',JSON.stringify({passed:true,scope:'Actual local host and real runtime with declared authored cooker fixture; no hosted durability/auth qualification',checks:reports},null,2)+'\n');
 console.log(JSON.stringify({passed:true,checks:reports},null,2));
}finally{process?.p.kill();}
