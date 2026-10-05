// Bounded PNG decoder for Chromium's 8-bit RGB/RGBA screenshot output.
// This is screenshot readback analysis, not an engine asynchronous ID-buffer API.
import {inflateSync,deflateSync} from 'node:zlib';

function validateRgba(image){
  const {width,height,pixels}=image;
  if(!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1||width>4096||height>4096||!(pixels instanceof Uint8Array)||pixels.length!==width*height*4)throw new Error('Invalid bounded RGBA image');
}

export function capturePixelBounds(measurement,scale,expected={width:640,height:480}){
  const {viewport,dpr,documentWidth,documentHeight}=measurement;
  if(!['css','device'].includes(scale)||![1,2].includes(dpr))throw new Error('Unsupported capture scale or DPR');
  if(viewport.cssWidth!==expected.width||viewport.cssHeight!==expected.height)throw new Error('Canvas CSS dimensions differ from requested size');
  if(![viewport.documentX,viewport.documentY,documentWidth,documentHeight].every(Number.isInteger))throw new Error('Document and canvas origin must align with CSS pixels');
  const factor=scale==='css'?1:dpr;
  const bounds={x:viewport.documentX*factor,y:viewport.documentY*factor,width:viewport.cssWidth*factor,height:viewport.cssHeight*factor};
  if(!Object.values(bounds).every(Number.isInteger)||bounds.x<0||bounds.y<0||bounds.width<1||bounds.height<1||bounds.x+bounds.width>documentWidth*factor||bounds.y+bounds.height>documentHeight*factor)throw new Error('Canvas pixel rectangle exceeds document');
  if(scale==='device'&&(bounds.width!==viewport.width||bounds.height!==viewport.height))throw new Error('ID capture must equal the actual canvas backing dimensions');
  return {bounds,factor,fullWidth:documentWidth*factor,fullHeight:documentHeight*factor};
}

// Exact row copies preserve discrete ID bytes. Cropping must never resample.
export function extractPixels(image,{x,y,width,height}){
  validateRgba(image);
  if(![x,y,width,height].every(Number.isInteger)||x<0||y<0||width<1||height<1||x+width>image.width||y+height>image.height)throw new Error('Invalid integer pixel extraction bounds');
  const pixels=new Uint8Array(width*height*4);
  for(let row=0;row<height;row++){
    const start=((y+row)*image.width+x)*4;
    pixels.set(image.pixels.subarray(start,start+width*4),row*width*4);
  }
  return {width,height,pixels};
}

export function rgbaPng(image){
  validateRgba(image);
  function chunk(type,data){
    const name=Buffer.from(type),length=Buffer.alloc(4),crc=Buffer.alloc(4);length.writeUInt32BE(data.length);
    let value=0xffffffff;
    for(const byte of Buffer.concat([name,data])){value^=byte;for(let i=0;i<8;i++)value=(value>>>1)^((value&1)?0xedb88320:0);}
    crc.writeUInt32BE((value^0xffffffff)>>>0);return Buffer.concat([length,name,data,crc]);
  }
  const header=Buffer.alloc(13);header.writeUInt32BE(image.width,0);header.writeUInt32BE(image.height,4);header[8]=8;header[9]=6;
  const stride=image.width*4,rows=Buffer.alloc((stride+1)*image.height);
  for(let y=0;y<image.height;y++)rows.set(image.pixels.subarray(y*stride,(y+1)*stride),y*(stride+1)+1);
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(rows)),chunk('IEND',Buffer.alloc(0))]);
}

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
  if(gpu.width!==cpu.width||gpu.height!==cpu.height)throw new Error(`Comparison image dimensions differ: GPU ${gpu.width}x${gpu.height}, CPU ${cpu.width}x${cpu.height}`);
  let absolute=0,squared=0,maximum=0,over8=0;const n=cpu.width*cpu.height*3;
  for(let i=0;i<cpu.width*cpu.height;i++)for(let c=0;c<3;c++){
    const d=Math.abs(gpu.pixels[i*4+c]-cpu.pixels[i*3+c]);absolute+=d;squared+=d*d;maximum=Math.max(maximum,d);if(d>8)over8++;
  }
  return {meanAbsoluteByteError:absolute/n,rootMeanSquareByteError:Math.sqrt(squared/n),maximumByteError:maximum,channelFractionOver8:over8/n};
}

// Interaction remains in the immutable logical 640x480 fixture coordinate system.
// Choose its test point independently of physical screenshot samples.
export function logicalSelectionPoint(ids){
  if(ids.length!==640*480*8)throw new Error('Logical selection ID dimensions differ');
  const identity=(x,y)=>{const i=(y*640+x)*8;return [ids.readUInt32LE(i),ids.readUInt32LE(i+4)];};
  for(let y=3;y<477;y+=5)for(let x=3;x<637;x+=5){
    const [object_id,generation]=identity(x,y);if(!object_id&&!generation)continue;
    let interior=true;
    for(let dy=-2;dy<=2&&interior;dy++)for(let dx=-2;dx<=2;dx++){
      const other=identity(x+dx,y+dy);if(other[0]!==object_id||other[1]!==generation){interior=false;break;}
    }
    if(interior)return {x,y,object_id,generation};
  }
  return null;
}

export function compareIds(gpu,ids,depths,idMap,dimensions={width:640,height:480}){
  const {width,height}=dimensions;
  validateRgba(gpu);
  if(!((width===640&&height===480)||(width===1280&&height===960))||gpu.width!==width||gpu.height!==height||ids.length!==width*height*8||depths.length!==width*height*4)throw new Error('ID comparison dimensions differ');
  const map=new Map(),indices=new Set();
  for(const item of idMap){
    const {objectId,generation,index}=item,key=`${objectId}:${generation}`;
    if(![objectId,generation].every(v=>Number.isInteger(v)&&v>=0&&v<=0xffffffff)||generation===0||!Number.isInteger(index)||index<1||index>0xffffff||map.has(key)||indices.has(index))throw new Error('GPU ID map must have unique valid identities and positive unique 24-bit indices');
    map.set(key,index);indices.add(index);
  }
  const groups=new Map();let checked=0,mismatched=0,ownerlessOcclusionChecked=0;const examples=[];
  const identity=(x,y)=>{const i=(y*width+x)*8;return `${ids.readUInt32LE(i)}:${ids.readUInt32LE(i+4)}`;};
  for(let y=3,row=0;y<height-3;y+=5,row++)for(let x=3,column=0;x<width-3;x+=5,column++){
    const key=identity(x,y);let interior=true;
    for(let dy=-2;dy<=2&&interior;dy++)for(let dx=-2;dx<=2;dx++)if(identity(x+dx,y+dy)!==key){interior=false;break;}
    if(!interior)continue;
    const depth=depths.readFloatLE((y*width+x)*4),zero=key==='0:0';
    // Zero IDs over geometry must still occlude. Pure background is sampled too, with fewer points.
    if(zero&&(!Number.isFinite(depth)||depth>=1)&&(column+row)%5!==0)continue;
    if(zero&&Number.isFinite(depth)&&depth<1)ownerlessOcclusionChecked++;
    const expected=zero?0:map.get(key);if(expected===undefined)throw new Error(`GPU ID map omitted stable object ${key}`);
    const i=(y*width+x)*4,actual=gpu.pixels[i]|gpu.pixels[i+1]<<8|gpu.pixels[i+2]<<16;
    checked++;let group=groups.get(key);if(!group){group={identity:key,checked:0,mismatched:0,points:[]};groups.set(key,group);}group.checked++;
    if(group.points.length<3)group.points.push({x,y});
    if(actual!==expected){mismatched++;group.mismatched++;if(examples.length<24)examples.push({x,y,expected,actual,identity:key,rgba:Array.from(gpu.pixels.slice(i,i+4))});}
  }
  return {method:'GPU ID visualization at physical pixel centers; fixed 5x5 physical CPU interiors',width,height,checked,mismatched,ownerlessOcclusionChecked,groups:[...groups.values()],examples};
}
