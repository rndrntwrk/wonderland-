// Bounded PNG decoder for Chromium's 8-bit RGB/RGBA screenshot output.
// This is screenshot readback analysis, not an engine asynchronous ID-buffer API.
import {inflateSync} from 'node:zlib';

export function readPng(input){
  const bytes=Buffer.from(input);
  if(bytes.length<33||!bytes.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])))throw new Error('Invalid PNG signature');
  let offset=8,width,height,channels,seenEnd=false;const compressed=[];
  while(offset+12<=bytes.length){
    const length=bytes.readUInt32BE(offset),type=bytes.toString('ascii',offset+4,offset+8);
    if(length>64*1024*1024||offset+12+length>bytes.length)throw new Error('Invalid PNG chunk length');
    const data=bytes.subarray(offset+8,offset+8+length);
    if(type==='IHDR'){
      if(length!==13||width!==undefined)throw new Error('Invalid PNG header');
      width=data.readUInt32BE(0);height=data.readUInt32BE(4);
      if(width<1||height<1||width>4096||height>4096||data[8]!==8||![2,6].includes(data[9])||data[10]!==0||data[11]!==0||data[12]!==0)throw new Error('Only bounded non-interlaced 8-bit RGB/RGBA PNG is supported');
      channels=data[9]===6?4:3;
    }else if(type==='IDAT')compressed.push(data);
    else if(type==='IEND'){seenEnd=true;break;}
    offset+=length+12;
  }
  if(!width||!seenEnd||!compressed.length)throw new Error('Incomplete PNG');
  const stride=width*channels,expected=(stride+1)*height;
  const raw=inflateSync(Buffer.concat(compressed),{maxOutputLength:expected});
  if(raw.length!==expected)throw new Error('PNG scanline length mismatch');
  const pixels=new Uint8Array(width*height*4);let previous=new Uint8Array(stride);
  for(let y=0;y<height;y++){
    const filter=raw[y*(stride+1)];if(filter>4)throw new Error('Unknown PNG filter');
    const row=new Uint8Array(stride);
    for(let x=0;x<stride;x++){
      const a=x>=channels?row[x-channels]:0,b=previous[x],c=x>=channels?previous[x-channels]:0;
      let predictor=0;
      if(filter===1)predictor=a;else if(filter===2)predictor=b;else if(filter===3)predictor=Math.floor((a+b)/2);
      else if(filter===4){const p=a+b-c,pa=Math.abs(p-a),pb=Math.abs(p-b),pc=Math.abs(p-c);predictor=pa<=pb&&pa<=pc?a:pb<=pc?b:c;}
      row[x]=(raw[y*(stride+1)+1+x]+predictor)&255;
    }
    for(let x=0;x<width;x++){const dst=(y*width+x)*4,src=x*channels;pixels[dst]=row[src];pixels[dst+1]=row[src+1];pixels[dst+2]=row[src+2];pixels[dst+3]=channels===4?row[src+3]:255;}
    previous=row;
  }
  return {width,height,pixels};
}

export function readPpm(input){
  const bytes=Buffer.from(input);const match=/^P6\n(\d+) (\d+)\n255\n/.exec(bytes.toString('ascii',0,80));
  if(!match)throw new Error('Expected fixture P6 PPM');
  const width=Number(match[1]),height=Number(match[2]),start=Buffer.byteLength(match[0]);
  if(width!==640||height!==480||bytes.length-start!==width*height*3)throw new Error('Invalid fixture PPM dimensions');
  return {width,height,pixels:bytes.subarray(start)};
}

export function colorDifference(gpu,cpu){
  if(gpu.width!==cpu.width||gpu.height!==cpu.height)throw new Error('Comparison image dimensions differ');
  let absolute=0,squared=0,maximum=0,over8=0;const n=cpu.width*cpu.height*3;
  for(let i=0;i<cpu.width*cpu.height;i++)for(let c=0;c<3;c++){
    const d=Math.abs(gpu.pixels[i*4+c]-cpu.pixels[i*3+c]);absolute+=d;squared+=d*d;maximum=Math.max(maximum,d);if(d>8)over8++;
  }
  return {meanAbsoluteByteError:absolute/n,rootMeanSquareByteError:Math.sqrt(squared/n),maximumByteError:maximum,channelFractionOver8:over8/n};
}

export function compareIds(gpu,ids,depths,idMap){
  if(gpu.width!==640||gpu.height!==480||ids.length!==640*480*8||depths.length!==640*480*4)throw new Error('ID comparison dimensions differ');
  const map=new Map(idMap.map(i=>[`${i.objectId}:${i.generation}`,i.index]));
  const groups=new Map();let checked=0,mismatched=0,ownerlessOcclusionChecked=0;const examples=[];
  const identity=(x,y)=>{const i=(y*640+x)*8;return `${ids.readUInt32LE(i)}:${ids.readUInt32LE(i+4)}`;};
  for(let y=3;y<477;y+=5)for(let x=3;x<637;x+=5){
    const key=identity(x,y);let interior=true;
    for(let dy=-2;dy<=2&&interior;dy++)for(let dx=-2;dx<=2;dx++)if(identity(x+dx,y+dy)!==key){interior=false;break;}
    if(!interior)continue;
    const depth=depths.readFloatLE((y*640+x)*4),zero=key==='0:0';
    // Zero IDs over geometry must still occlude. Pure background is sampled too, with fewer points.
    if(zero&&(!Number.isFinite(depth)||depth>=1)&&(x+y)%25!==0)continue;
    if(zero&&Number.isFinite(depth)&&depth<1)ownerlessOcclusionChecked++;
    const expected=zero?0:map.get(key);if(expected===undefined)throw new Error(`GPU ID map omitted stable object ${key}`);
    const i=(y*640+x)*4,actual=gpu.pixels[i]|gpu.pixels[i+1]<<8|gpu.pixels[i+2]<<16;
    checked++;let group=groups.get(key);if(!group){group={identity:key,checked:0,mismatched:0,points:[]};groups.set(key,group);}group.checked++;
    if(group.points.length<3)group.points.push({x,y});
    if(actual!==expected){mismatched++;group.mismatched++;if(examples.length<24)examples.push({x,y,expected,actual,identity:key,rgba:Array.from(gpu.pixels.slice(i,i+4))});}
  }
  return {method:'GPU canvas screenshot of ID visualization; 5x5 stable CPU interiors',checked,mismatched,ownerlessOcclusionChecked,groups:[...groups.values()],examples};
}
