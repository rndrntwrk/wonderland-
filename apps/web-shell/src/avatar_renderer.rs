//! Textured hardware/software presentation for source-composed, CPU-skinned Vitaboy meshes.
use wasm_bindgen::prelude::*;
#[wasm_bindgen(inline_js = r#"
const sessions = new WeakMap();
const sampledColors = new WeakMap();
function sampledColor(bitmap,u,v){
  let colors=sampledColors.get(bitmap);if(!colors){colors=new Map();sampledColors.set(bitmap,colors);}const key=u+','+v;
  if(!colors.has(key)){const sample=document.createElement('canvas');sample.width=sample.height=1;const context=sample.getContext('2d');context.drawImage(bitmap,u,v,1,1,0,0,1,1);colors.set(key,Array.from(context.getImageData(0,0,1,1).data));}return colors.get(key);
}
export function dropAvatar(canvas) {
  const s=sessions.get(canvas);if(!s)return;s.cancelled=true;cancelAnimationFrame(s.frame);
  if(s.gl){
    canvas.removeEventListener('webglcontextlost',s.lost);canvas.removeEventListener('webglcontextrestored',s.restored);
    for(const p of s.parts){s.gl.deleteVertexArray(p.vao);s.gl.deleteTexture(p.texture);for(const b of p.buffers)s.gl.deleteBuffer(b);}
    if(s.program)s.gl.deleteProgram(s.program);s.gl.clearColor(0,0,0,0);s.gl.clear(s.gl.COLOR_BUFFER_BIT|s.gl.DEPTH_BUFFER_BIT);
  }else{
    for(const p of s.parts){if(p.bitmap)p.bitmap.close();}
    s.ctx.setTransform(1,0,0,1,0,0);s.ctx.clearRect(0,0,canvas.width,canvas.height);
  }
  sessions.delete(canvas);
}
export function disposeAvatar(canvas){const s=sessions.get(canvas);dropAvatar(canvas);if(s&&s.gl){const ext=s.gl.getExtension("WEBGL_lose_context");if(ext)ext.loseContext();}}
function modeLabel(canvas,label){const el=canvas.parentElement&&canvas.parentElement.querySelector('.avatar-render-mode');if(el)el.textContent=label;}
async function renderSoftware(canvas,parts,reduced){
  const ctx=canvas.getContext('2d',{alpha:true});if(!ctx)throw new Error('Neither WebGL 2 nor Canvas 2D is available in this browser.');
  const s={ctx,parts:[],cancelled:false,frame:0,yaw:0};sessions.set(canvas,s);let phase='software texture decode';
  try{
    const min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
    for(const input of parts){
      const bitmap=await createImageBitmap(new Blob([input.bytes],{type:input.mime}));
      if(s.cancelled||sessions.get(canvas)!==s){bitmap.close();return false;}
      if(bitmap.width*bitmap.height>16777216){bitmap.close();throw new Error('Source texture exceeds the browser preview memory budget.');}
      const shades=[];
      if(bitmap.width*bitmap.height<=1048576){
        for(let k=0;k<8;k++){const image=document.createElement('canvas');image.width=bitmap.width;image.height=bitmap.height;const context=image.getContext('2d');context.drawImage(bitmap,0,0);context.globalCompositeOperation='source-atop';context.fillStyle='rgba(0,0,0,'+(1-(0.65+0.35*k/7))+')';context.fillRect(0,0,image.width,image.height);shades.push(image);}
      }
      s.parts.push({input,bitmap,shades});
      for(let i=0;i<input.positions.length;i+=3)for(let a=0;a<3;a++){min[a]=Math.min(min[a],input.positions[i+a]);max[a]=Math.max(max[a],input.positions[i+a]);}
    }
    if(!s.parts.length)throw new Error('Selected source has no complete textured mesh.');
    const center=min.map((v,i)=>(v+max[i])/2),radius=Math.max(...max.map((v,i)=>v-min[i]))/2||1;
    s.draw=()=>{
      if(s.cancelled)return;
      const dpr=Math.min(devicePixelRatio||1,1.5),w=Math.max(1,Math.round(canvas.clientWidth*dpr)),h=Math.max(1,Math.round(canvas.clientHeight*dpr));
      if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;}
      ctx.setTransform(1,0,0,1,0,0);ctx.clearRect(0,0,w,h);ctx.imageSmoothingEnabled=true;
      const scale=Math.min(w,h)/(2*radius*1.35),co=Math.cos(s.yaw),si=Math.sin(s.yaw),triangles=[];
      for(const part of s.parts){
        const input=part.input,points=[],normals=[];
        for(let v=0;v<input.positions.length/3;v++){
          const x=input.positions[v*3]-center[0],y=input.positions[v*3+1]-center[1],z=input.positions[v*3+2]-center[2];
          points.push([w/2+(co*x+si*z)*scale,h/2-y*scale,-si*x+co*z]);
          const nx=input.normals[v*3],ny=input.normals[v*3+1],nz=input.normals[v*3+2];normals.push([co*nx+si*nz,ny,-si*nx+co*nz]);
        }
        for(let i=0;i<input.indices.length;i+=3){
          const ids=[input.indices[i],input.indices[i+1],input.indices[i+2]],p=ids.map(v=>points[v]);
          if(p.some(v=>!v||v.some(n=>!Number.isFinite(n))))continue;
          if(Math.abs((p[1][0]-p[0][0])*(p[2][1]-p[0][1])-(p[2][0]-p[0][0])*(p[1][1]-p[0][1]))<0.01)continue;
          const uv=ids.map(v=>[input.uvs[v*2]*part.bitmap.width,input.uvs[v*2+1]*part.bitmap.height]);
          const n=[0,0,0];for(const v of ids)for(let a=0;a<3;a++)n[a]+=normals[v][a];
          const len=Math.hypot(...n)||1,light=0.65+0.35*Math.max((-0.4*n[0]+0.7*n[1]+n[2])/(len*Math.hypot(-0.4,0.7,1)),0);
          triangles.push({p,uv,bitmap:part.shades.length?part.shades[Math.max(0,Math.min(7,Math.round((light-0.65)/0.35*7)))]:part.bitmap,depth:(p[0][2]+p[1][2]+p[2][2])/3});
        }
      }
      // Painter ordering is an approximation for intersecting triangles. Every
      // triangle, UV coordinate and texture comes from the composed source mesh.
      triangles.sort((a,b)=>a.depth-b.depth);
      for(const t of triangles){
        const [p0,p1,p2]=t.p,[t0,t1,t2]=t.uv,du1=t1[0]-t0[0],dv1=t1[1]-t0[1],du2=t2[0]-t0[0],dv2=t2[1]-t0[1],det=du1*dv2-du2*dv1;
        if(!Number.isFinite(det))continue;
        if(Math.abs(det)<1e-8){
          // A collapsed UV face still has real geometry. Use its clamped
          // representative source texel instead of making that face disappear.
          const u=Math.max(0,Math.min(t.bitmap.width-1,Math.floor((t0[0]+t1[0]+t2[0])/3))),v=Math.max(0,Math.min(t.bitmap.height-1,Math.floor((t0[1]+t1[1]+t2[1])/3)));
          const rgba=sampledColor(t.bitmap,u,v);
          ctx.save();ctx.beginPath();ctx.moveTo(p0[0],p0[1]);ctx.lineTo(p1[0],p1[1]);ctx.lineTo(p2[0],p2[1]);ctx.closePath();ctx.fillStyle='rgba('+rgba[0]+','+rgba[1]+','+rgba[2]+','+(rgba[3]/255)+')';ctx.fill();ctx.restore();continue;
        }
        const dx1=p1[0]-p0[0],dy1=p1[1]-p0[1],dx2=p2[0]-p0[0],dy2=p2[1]-p0[1];
        const a=(dx1*dv2-dx2*dv1)/det,b=(dy1*dv2-dy2*dv1)/det,c=(dx2*du1-dx1*du2)/det,d=(dy2*du1-dy1*du2)/det;
        ctx.save();ctx.beginPath();ctx.moveTo(p0[0],p0[1]);ctx.lineTo(p1[0],p1[1]);ctx.lineTo(p2[0],p2[1]);ctx.closePath();ctx.clip();
        ctx.setTransform(a,b,c,d,p0[0]-a*t0[0]-c*t0[1],p0[1]-b*t0[0]-d*t0[1]);ctx.drawImage(t.bitmap,0,0);
        // Match CLAMP_TO_EDGE without allocating a padded texture. The existing
        // triangle clip bounds all stretched source border strips and corners.
        const iw=t.bitmap.width,ih=t.bitmap.height,minU=Math.min(t0[0],t1[0],t2[0]),maxU=Math.max(t0[0],t1[0],t2[0]),minV=Math.min(t0[1],t1[1],t2[1]),maxV=Math.max(t0[1],t1[1],t2[1]);
        if(minU<0)ctx.drawImage(t.bitmap,0,0,1,ih,minU,0,-minU,ih);
        if(maxU>iw)ctx.drawImage(t.bitmap,iw-1,0,1,ih,iw,0,maxU-iw,ih);
        if(minV<0)ctx.drawImage(t.bitmap,0,0,iw,1,0,minV,iw,-minV);
        if(maxV>ih)ctx.drawImage(t.bitmap,0,ih-1,iw,1,0,ih,iw,maxV-ih);
        if(minU<0&&minV<0)ctx.drawImage(t.bitmap,0,0,1,1,minU,minV,-minU,-minV);
        if(maxU>iw&&minV<0)ctx.drawImage(t.bitmap,iw-1,0,1,1,iw,minV,maxU-iw,-minV);
        if(minU<0&&maxV>ih)ctx.drawImage(t.bitmap,0,ih-1,1,1,minU,ih,-minU,maxV-ih);
        if(maxU>iw&&maxV>ih)ctx.drawImage(t.bitmap,iw-1,ih-1,1,1,iw,ih,maxU-iw,maxV-ih);
        ctx.restore();
      }
    };
    phase='software textured triangle draw';canvas.removeAttribute('data-render-error');modeLabel(canvas,'Software textured 3D preview');s.draw();
    if(!reduced){let previous,lastDraw=0;const tick=time=>{if(s.cancelled)return;const dt=previous===undefined?0:Math.min(time-previous,100);previous=time;s.yaw+=dt*0.00018;if(time-lastDraw>=50){s.draw();lastDraw=time;}s.frame=requestAnimationFrame(tick);};s.frame=requestAnimationFrame(tick);}
    return true;
  }catch(error){const reported=new Error(phase+': '+(error&&error.message?error.message:String(error)));console.error('Wonderland original avatar software render',reported,error);if(sessions.get(canvas)===s)dropAvatar(canvas);throw reported;}
}
export function textureUrl(bytes,mime){return URL.createObjectURL(new Blob([bytes],{type:mime}));}
export function revokeTextureUrl(url){URL.revokeObjectURL(url);}
export function turnAvatar(canvas,degrees){const s=sessions.get(canvas);if(s){s.yaw+=degrees*Math.PI/180;if(s.draw)s.draw();}}
export async function renderAvatar(canvas,parts,reduced) {
  dropAvatar(canvas);
  const gl=canvas.getContext('webgl2',{alpha:true,antialias:true,premultipliedAlpha:false});
  if(!gl)return renderSoftware(canvas,parts,reduced);
  modeLabel(canvas,'Hardware textured 3D preview');
  const s={gl,parts:[],cancelled:false,frame:0,yaw:0,program:null};sessions.set(canvas,s);
  s.lost=e=>{e.preventDefault();s.cancelled=true;cancelAnimationFrame(s.frame);canvas.setAttribute('data-render-error','lost');if(canvas.nextElementSibling)canvas.nextElementSibling.textContent='Graphics context was lost. Change a selection to retry.';};
  s.restored=()=>{canvas.setAttribute('data-render-error','restored');if(canvas.nextElementSibling)canvas.nextElementSibling.textContent='Graphics context restored. Change a selection to rebuild the avatar.';};
  canvas.addEventListener('webglcontextlost',s.lost);canvas.addEventListener('webglcontextrestored',s.restored);
  const shader=(type,source)=>{const sh=gl.createShader(type);gl.shaderSource(sh,source);gl.compileShader(sh);if(!gl.getShaderParameter(sh,gl.COMPILE_STATUS)){const err=gl.getShaderInfoLog(sh);gl.deleteShader(sh);throw new Error(err);}return sh;};
  let phase="vertex shader";
  try {
    const vs=shader(gl.VERTEX_SHADER,`#version 300 es
      in vec3 position;in vec3 normal;in vec2 uv;uniform mat4 mvp;uniform mat4 model;out vec2 texcoord;out vec3 n;
      void main(){gl_Position=mvp*vec4(position,1.0);texcoord=uv;n=mat3(model)*normal;}`);
    phase="fragment shader";
    const fs=shader(gl.FRAGMENT_SHADER,`#version 300 es
      precision highp float;in vec2 texcoord;in vec3 n;uniform sampler2D u_texture;out vec4 color;
      void main(){vec4 t=texture(u_texture,texcoord);if(t.a<0.05)discard;float light=0.65+0.35*max(dot(normalize(n),normalize(vec3(-0.4,0.7,1.0))),0.0);color=vec4(t.rgb*light,t.a);}`);
    phase="shader linking";
    const program=gl.createProgram();s.program=program;gl.attachShader(program,vs);gl.attachShader(program,fs);gl.linkProgram(program);gl.deleteShader(vs);gl.deleteShader(fs);
    if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(program));
    let min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
    for(const input of parts){
      if(s.cancelled||sessions.get(canvas)!==s)return false;
      for(let i=0;i<input.positions.length;i+=3)for(let a=0;a<3;a++){min[a]=Math.min(min[a],input.positions[i+a]);max[a]=Math.max(max[a],input.positions[i+a]);}
      const p={vao:gl.createVertexArray(),texture:gl.createTexture(),buffers:[],count:input.indices.length};s.parts.push(p);gl.bindVertexArray(p.vao);
      for(const [name,array,size] of [['position',input.positions,3],['normal',input.normals,3],['uv',input.uvs,2]]){const b=gl.createBuffer();p.buffers.push(b);gl.bindBuffer(gl.ARRAY_BUFFER,b);gl.bufferData(gl.ARRAY_BUFFER,array,gl.STATIC_DRAW);const location=gl.getAttribLocation(program,name);gl.enableVertexAttribArray(location);gl.vertexAttribPointer(location,size,gl.FLOAT,false,0,0);}
      const ib=gl.createBuffer();p.buffers.push(ib);gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,input.indices,gl.STATIC_DRAW);
      phase="texture decode ("+input.mime+", "+input.bytes.length+" bytes, part "+s.parts.length+")";
      const bitmap=await createImageBitmap(new Blob([input.bytes],{type:input.mime}));
      if(s.cancelled||sessions.get(canvas)!==s){bitmap.close();return false;}
      if(bitmap.width>gl.getParameter(gl.MAX_TEXTURE_SIZE)||bitmap.height>gl.getParameter(gl.MAX_TEXTURE_SIZE)||bitmap.width*bitmap.height>16777216){bitmap.close();throw new Error('Avatar texture dimensions exceed this browser’s graphics limit.');}
      phase="texture upload";
      gl.bindTexture(gl.TEXTURE_2D,p.texture);gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,false);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,gl.RGBA,gl.UNSIGNED_BYTE,bitmap);bitmap.close();gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    }
    if(!s.parts.length)throw new Error('The selected content has no complete renderable mesh.');
    const center=min.map((v,i)=>(v+max[i])/2);const radius=Math.max(...max.map((v,i)=>v-min[i]))/2||1;
    const multiply=(a,b)=>{const c=new Float32Array(16);for(let col=0;col<4;col++)for(let row=0;row<4;row++)for(let k=0;k<4;k++)c[col*4+row]+=a[k*4+row]*b[col*4+k];return c;};
    s.draw=()=>{
      if(s.cancelled)return;
      const dpr=Math.min(devicePixelRatio||1,2);const w=Math.max(1,Math.round(canvas.clientWidth*dpr));const h=Math.max(1,Math.round(canvas.clientHeight*dpr));if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;}
      gl.viewport(0,0,w,h);gl.clearColor(0,0,0,0);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);gl.enable(gl.DEPTH_TEST);gl.disable(gl.CULL_FACE);gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);gl.useProgram(program);
      const c=Math.cos(s.yaw),sn=Math.sin(s.yaw);const model=new Float32Array([c,0,-sn,0,0,1,0,0,sn,0,c,0,-c*center[0]-sn*center[2],-center[1],sn*center[0]-c*center[2],1]);
      const aspect=w/h,extent=radius*1.35*Math.max(1,1/aspect);const projection=new Float32Array([1/(extent*aspect),0,0,0,0,1/extent,0,0,0,0,-1/(radius*4),0,0,0,0,1]);
      gl.uniformMatrix4fv(gl.getUniformLocation(program,'mvp'),false,multiply(projection,model));gl.uniformMatrix4fv(gl.getUniformLocation(program,'model'),false,model);gl.uniform1i(gl.getUniformLocation(program,'u_texture'),0);
      for(const p of s.parts){gl.bindVertexArray(p.vao);gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,p.texture);gl.drawElements(gl.TRIANGLES,p.count,gl.UNSIGNED_INT,0);}
    };
    phase="avatar draw";
    canvas.removeAttribute('data-render-error');s.draw();
    if(!reduced){let previous;const tick=time=>{if(s.cancelled)return;const dt=previous===undefined?0:Math.min(time-previous,100);previous=time;s.yaw+=dt*0.00018;s.draw();s.frame=requestAnimationFrame(tick);};s.frame=requestAnimationFrame(tick);}
    return true;
  }catch(error){const detail=error&&error.message?error.message:String(error);const reported=new Error(phase+": "+detail);console.error("Wonderland original avatar render",reported,error);if(sessions.get(canvas)===s)dropAvatar(canvas);throw reported;}
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=textureUrl)]
    pub fn texture_url(bytes: &js_sys::Uint8Array, mime: &str) -> String;
    #[wasm_bindgen(js_name=revokeTextureUrl)]
    pub fn revoke_texture_url(url: &str);
    #[wasm_bindgen(js_name=renderAvatar)]
    pub fn render_avatar(
        canvas: &web_sys::HtmlCanvasElement,
        parts: &js_sys::Array,
        reduced: bool,
    ) -> js_sys::Promise;
    #[wasm_bindgen(js_name=disposeAvatar)]
    pub fn dispose_avatar(canvas: &web_sys::HtmlCanvasElement);
    #[wasm_bindgen(js_name=dropAvatar)]
    pub fn drop_avatar(canvas: &web_sys::HtmlCanvasElement);
    #[wasm_bindgen(js_name=turnAvatar)]
    pub fn turn_avatar(canvas: &web_sys::HtmlCanvasElement, degrees: f64);
}
