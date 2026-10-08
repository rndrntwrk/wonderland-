import assert from 'node:assert/strict';
import {inflateSync} from 'node:zlib';
export function png(bytes){
  assert.ok(bytes.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])));
  let width,height,channels,offset=8,end=false;const data=[];
  while(offset+12<=bytes.length){
    const n=bytes.readUInt32BE(offset),kind=bytes.toString('ascii',offset+4,offset+8);assert.ok(n<=64*1024*1024&&offset+n+12<=bytes.length);
    const chunk=bytes.subarray(offset+8,offset+8+n);
    if(kind==='IHDR'){assert.equal(n,13);width=chunk.readUInt32BE(0);height=chunk.readUInt32BE(4);assert.ok(width>0&&height>0&&width<=4096&&height<=4096);assert.equal(chunk[8],8);assert.ok([2,6].includes(chunk[9]));assert.equal(chunk[12],0);channels=chunk[9]===6?4:3;}
    if(kind==='IDAT')data.push(chunk);if(kind==='IEND'){end=true;break;}offset+=n+12;
  }
  assert.ok(end&&channels);const stride=width*channels,raw=inflateSync(Buffer.concat(data),{maxOutputLength:(stride+1)*height});assert.equal(raw.length,(stride+1)*height);
  const pixels=new Uint8Array(width*height*4);let previous=new Uint8Array(stride);
  for(let y=0;y<height;y++){
    const filter=raw[y*(stride+1)],row=new Uint8Array(stride);assert.ok(filter<=4);
    for(let x=0;x<stride;x++){
      const a=x>=channels?row[x-channels]:0,b=previous[x],c=x>=channels?previous[x-channels]:0;let prediction=0;
      if(filter===1)prediction=a;else if(filter===2)prediction=b;else if(filter===3)prediction=Math.floor((a+b)/2);
      else if(filter===4){const p=a+b-c,pa=Math.abs(p-a),pb=Math.abs(p-b),pc=Math.abs(p-c);prediction=pa<=pb&&pa<=pc?a:pb<=pc?b:c;}
      row[x]=(raw[y*(stride+1)+1+x]+prediction)&255;
    }
    for(let x=0;x<width;x++){pixels.set(row.subarray(x*channels,x*channels+3),(y*width+x)*4);pixels[(y*width+x)*4+3]=channels===4?row[x*channels+3]:255;}previous=row;
  }
  return {width,height,pixels};
}
