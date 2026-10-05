// Literal layouts exercise the browser's real upload and export path without
// redistributing original game assets or depending on the implementation writer.
export const upgrades = Buffer.from('{ "Version":2,"Files":[{"Name":"chair.iff","Subs":[],"Groups":[],"Upgrades":[{"Name":"1","Price":"$50","Ad":"comfort:50","Subs":[]}],"Config":[],"Future":{"retain":7}}]}');
export const neighborhoods = Buffer.from('[{"GUID":"a","Name":"Garden","Location":{"X":10,"Y":10},"DistanceMul":9.0,"Future":7},{"GUID":"b","Name":"Harbor","Location":{"X":12,"Y":10},"DistanceMul":0.01}]');
export function purchase() {
  const bytes = Buffer.alloc(24);
  [1,0,8,0x12345678].forEach((v,i)=>bytes.writeUInt32BE(v,i*4));
  // An ID outside JavaScript's safe integer range must survive even when
  // another field is changed through Rust's exact JSON parser.
  bytes.writeBigUInt64BE(0xfedcba9876543210n,16);
  return bytes;
}
export function cityBmp() {
  const width=512,height=512,offset=54,size=offset+width*height*3;
  const bytes=Buffer.alloc(size);bytes.write('BM');bytes.writeUInt32LE(size,2);
  bytes.writeUInt32LE(offset,10);bytes.writeUInt32LE(40,14);bytes.writeInt32LE(width,18);bytes.writeInt32LE(height,22);
  bytes.writeUInt16LE(1,26);bytes.writeUInt16LE(24,28);bytes.writeUInt32LE(width*height*3,34);
  return bytes;
}
const u16=value=>{const b=Buffer.alloc(2);b.writeUInt16LE(value);return b;};
const u32=value=>{const b=Buffer.alloc(4);b.writeUInt32LE(value);return b;};
function text(value){const b=Buffer.from(value);if(b.length>127)throw new Error('fixture string too long');return Buffer.concat([Buffer.from([b.length]),b]);}
function chunk(kind,id,data){
  const header=Buffer.alloc(76);header.write(kind);header.writeUInt32BE(76+data.length,4);header.writeUInt16BE(id,8);
  header.write('Fixture resource',12);return Buffer.concat([header,data]);
}
function iff(chunks){const header=Buffer.alloc(64);header.write('IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1');return Buffer.concat([header,...chunks]);}
export const patchSource = iff([chunk('ZZZZ',1,Buffer.from('abc')),chunk('ZZZZ',2,Buffer.from('untouched'))]);
export function patch(value) {
  const data=Buffer.from(value);if(data.length>127)throw new Error('fixture patch too long');
  const descriptor=Buffer.concat([
    u16(2),text('Source.iff'),text('Browser fixture'),u16(1),Buffer.from('ZZZZ'),u16(1),text(''),Buffer.from([0]),text('Patched resource'),u16(17),u16(1),u32(data.length),u32(2),
    Buffer.from([0,3,0]),Buffer.from([0,data.length,1]),data,
  ]);
  return iff([chunk('PIFF',256,descriptor)]);
}
