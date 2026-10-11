// TEST ONLY. Authored sine-wave sample and minimal original-format HIT/FWAV
// metadata; no original commercial sound assets are distributed or substituted.
import {mkdir,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
export async function audioFixture(directory){
 await mkdir(directory,{recursive:true});
 const hit=Buffer.alloc(36);hit.write('HIT!');hit.writeUInt32LE(1,4);hit.write('TSO!ENTP',12);hit.writeUInt32LE(7,20);hit.writeUInt32LE(32,24);hit.write('EENT',28);hit.set([96,0,11,12],32);
 const rate=8000,count=16000,data=Buffer.alloc(count*2);
 for(let i=0;i<count;i++)data.writeInt16LE(Math.round(18000*Math.sin(2*Math.PI*440*i/rate)),i*2);
 const wav=Buffer.alloc(44+data.length);wav.write('RIFF');wav.writeUInt32LE(wav.length-8,4);wav.write('WAVEfmt ',8);wav.writeUInt32LE(16,16);wav.writeUInt16LE(1,20);wav.writeUInt16LE(1,22);wav.writeUInt32LE(rate,24);wav.writeUInt32LE(rate*2,28);wav.writeUInt16LE(2,32);wav.writeUInt16LE(16,34);wav.write('data',36);wav.writeUInt32LE(data.length,40);data.copy(wav,44);
 const iff=Buffer.alloc(64+76+8);iff.write('IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1');iff.write('FWAV',64);iff.writeUInt32BE(84,68);iff.writeUInt16BE(5,72);iff.write('fixture\0',140);
 const manifest={version:1,groups:[{kind:'new_main',hit:'sound.hit',events:'sound.evt'}],tracks:[{instance:7,file:'sound.trk'}],samples:[{id:8,file:'sound.wav',group:'fx'}],fwav:[{scope:0x04786aed,file:'sound.iff'}]};
 const files={'sound.hit':hit,'sound.evt':'fixture,1,7,0,0,0,0\r\n','sound.trk':'TKDT,1,fixture,8,7,ETKD','sound.wav':wav,'sound.iff':iff,'wonderland-audio.json':JSON.stringify(manifest)};
 for(const [name,bytes] of Object.entries(files))await writeFile(join(directory,name),bytes);
 return Object.keys(files).map(name=>join(directory,name));
}
