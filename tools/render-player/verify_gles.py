#!/usr/bin/env python3
"""Execute the checked-in GLSL and recorded production binders on native Mesa GLES.
This is software graphics verification, NOT browser, DOM, WASM or device proof.
Requires libEGL, Mesa, Python numpy/Pillow; no network or application modification.
"""
from __future__ import annotations
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import sys
import numpy as np
from PIL import Image

os.environ.setdefault('EGL_PLATFORM', 'surfaceless')
os.environ.setdefault('LIBGL_ALWAYS_SOFTWARE', '1')
E = C.CDLL('libEGL.so.1')
I, U, F, P, B = C.c_int, C.c_uint, C.c_float, C.c_void_p, C.c_ubyte

def egl(name, restype, argtypes):
    f = getattr(E, name); f.restype = restype; f.argtypes = argtypes; return f
get_proc = egl('eglGetProcAddress', P, [C.c_char_p])
def gl(name, restype, args):
    address = get_proc(name.encode())
    if not address: raise RuntimeError(f'Missing GLES entrypoint {name}')
    return C.CFUNCTYPE(restype, *args)(address)

K = dict(LESS=0x201, LEQUAL=0x203, ALWAYS=0x207, EQUAL=0x202,
         KEEP=0x1E00, ZERO=0, REPLACE=0x1E01, FRONT=0x404, BACK=0x405,
         CCW=0x901, STENCIL_TEST=0xB90, BLEND=0xBE2, SRC_ALPHA=0x302,
         ONE_MINUS_SRC_ALPHA=0x303, ONE=1, FUNC_ADD=0x8006)
fns = {
 'glFrontFace': (None,[U]),'glDepthFunc': (None,[U]),'glDepthMask': (None,[B]),
 'glColorMask':(None,[B,B,B,B]),'glEnable':(None,[U]),'glDisable':(None,[U]),
 'glStencilMask':(None,[U]),'glStencilFuncSeparate':(None,[U,U,I,U]),
 'glStencilOpSeparate':(None,[U,U,U,U]),'glBlendEquation':(None,[U]),
 'glBlendFuncSeparate':(None,[U,U,U,U]),'glUniform1f':(None,[I,F]),
 'glUniform1i':(None,[I,I]),'glUniform3f':(None,[I,F,F,F]),
 'glUniformMatrix4fv':(None,[I,I,B,P]),'glGetUniformLocation':(I,[U,C.c_char_p]),
 'glCreateShader':(U,[U]),'glShaderSource':(None,[U,I,C.POINTER(C.c_char_p),P]),
 'glCompileShader':(None,[U]),'glGetShaderiv':(None,[U,U,C.POINTER(I)]),
 'glGetShaderInfoLog':(None,[U,I,P,C.c_char_p]),'glCreateProgram':(U,[]),
 'glAttachShader':(None,[U,U]),'glLinkProgram':(None,[U]),
 'glGetProgramiv':(None,[U,U,C.POINTER(I)]),'glGetProgramInfoLog':(None,[U,I,P,C.c_char_p]),
 'glUseProgram':(None,[U]),'glGetError':(U,[]),'glGetString':(C.c_char_p,[U]),
 'glGenVertexArrays':(None,[I,C.POINTER(U)]),'glBindVertexArray':(None,[U]),
 'glGenBuffers':(None,[I,C.POINTER(U)]),'glBindBuffer':(None,[U,U]),
 'glBufferData':(None,[U,C.c_ssize_t,P,U]),'glEnableVertexAttribArray':(None,[U]),
 'glVertexAttribPointer':(None,[U,I,U,B,I,P]),'glGenTextures':(None,[I,C.POINTER(U)]),
 'glActiveTexture':(None,[U]),'glBindTexture':(None,[U,U]),
 'glTexParameteri':(None,[U,U,I]),'glTexImage2D':(None,[U,I,I,I,I,I,U,U,P]),
 'glPixelStorei':(None,[U,I]),'glViewport':(None,[I,I,I,I]),
 'glClearColor':(None,[F,F,F,F]),'glClearDepthf':(None,[F]),'glClearStencil':(None,[I]),
 'glClear':(None,[U]),'glDrawElements':(None,[U,I,U,P]),
 'glReadPixels':(None,[I,I,I,I,U,U,P]),'glFinish':(None,[]),
 'glDeleteBuffers':(None,[I,C.POINTER(U)]),'glDeleteVertexArrays':(None,[I,C.POINTER(U)]),
 'glDeleteTextures':(None,[I,C.POINTER(U)]),
}
G = {name:gl(name,*definition) for name,definition in fns.items()}
def call(name,*args): return G[name](*args)
def checked(where):
    err=call('glGetError')
    if err: raise RuntimeError(f'{where}: GLES error 0x{err:x}')
def one(name):
    value=U();call(name,1,C.byref(value));return value.value

def shader(kind,text):
    s=call('glCreateShader',kind);p=C.c_char_p(text.encode())
    call('glShaderSource',s,1,C.byref(p),None);call('glCompileShader',s)
    ok=I();call('glGetShaderiv',s,0x8B81,C.byref(ok))
    if not ok.value:
        msg=C.create_string_buffer(16384);call('glGetShaderInfoLog',s,len(msg),None,msg)
        raise RuntimeError(msg.value.decode())
    return s

def texture(image):
    t=one('glGenTextures');call('glActiveTexture',0x84C0);call('glBindTexture',0xDE1,t)
    for key,value in [(0x2801,0x2600),(0x2800,0x2600),(0x2802,0x812F),(0x2803,0x812F)]:
        call('glTexParameteri',0xDE1,key,value)
    a=np.asarray(image['pixels'],dtype=np.uint8)
    call('glTexImage2D',0xDE1,0,0x8058,image['width'],image['height'],0,0x1908,0x1401,a.ctypes.data)
    return t

def run(inputs,directory):
    d=egl('eglGetDisplay',P,[P])(None);ma,mi=I(),I()
    assert egl('eglInitialize',U,[P,C.POINTER(I),C.POINTER(I)])(d,C.byref(ma),C.byref(mi))
    assert egl('eglBindAPI',U,[U])(0x30A0)
    attrs=(I*21)(0x3024,8,0x3023,8,0x3022,8,0x3021,8,0x3025,24,0x3026,8,0x3033,1,0x3040,0x40,0x3038,0,0,0,0)
    config=P();count=I()
    assert egl('eglChooseConfig',U,[P,C.POINTER(I),C.POINTER(P),I,C.POINTER(I)])(d,attrs,C.byref(config),1,C.byref(count)) and count.value
    ctx=egl('eglCreateContext',P,[P,P,P,C.POINTER(I)])(d,config,None,(I*3)(0x3098,3,0x3038));assert ctx
    surface=egl('eglCreatePbufferSurface',P,[P,P,C.POINTER(I)])(d,config,(I*5)(0x3057,256,0x3056,192,0x3038));assert surface
    assert egl('eglMakeCurrent',U,[P,P,P,P])(d,surface,surface,ctx)
    program=call('glCreateProgram')
    for kind,text in [(0x8B31,inputs['vertex']),(0x8B30,inputs['fragment'])]:call('glAttachShader',program,shader(kind,text))
    call('glLinkProgram',program);ok=I();call('glGetProgramiv',program,0x8B82,C.byref(ok))
    if not ok.value:
        msg=C.create_string_buffer(16384);call('glGetProgramInfoLog',program,len(msg),None,msg);raise RuntimeError(msg.value.decode())
    call('glUseProgram',program)
    location=lambda name:call('glGetUniformLocation',program,name.encode())
    def matrix(name,data):
        a=np.asarray(data,dtype=np.float32);call('glUniformMatrix4fv',location(name),1,False,a.ctypes.data)
    output={'scope':inputs['scope'],'sourceSha256':inputs['sourceSha256'],'version':call('glGetString',0x1F02).decode(),'renderer':call('glGetString',0x1F01).decode(),'scenes':[],'failures':[]}
    for entry in inputs['frames']:
        scene,frame=entry['scene'],entry['frame'];width,height=frame['width'],frame['height']
        assert (width,height)==(256,192)
        arrays=[];buffers=[];textures=[texture(t) for t in frame['textures']]
        white=texture({'width':1,'height':1,'pixels':[[255]*4]});textures.append(white)
        for mesh in frame['meshes']:
            va=one('glGenVertexArrays');call('glBindVertexArray',va);arrays.append(va)
            for target,data,dtype in [(0x8892,mesh['vertices'],np.float32),(0x8893,mesh['indices'],np.uint32)]:
                buf=one('glGenBuffers');buffers.append(buf);call('glBindBuffer',target,buf)
                a=np.asarray(data,dtype=dtype);call('glBufferData',target,a.nbytes,a.ctypes.data,0x88E4)
            for index,size,offset in [(0,3,0),(1,2,12),(2,4,20)]:
                call('glEnableVertexAttribArray',index);call('glVertexAttribPointer',index,size,0x1406,False,36,P(offset))
        images=[]
        for picking in [False,True]:
            call('glViewport',0,0,width,height);call('glDisable',0xB44);call('glDisable',0xBD0)
            call('glEnable',0xB71);call('glDepthMask',True);call('glStencilMask',255);call('glColorMask',True,True,True,True)
            call('glClearColor',*([0,0,0,0] if picking else [229/255,235/255,222/255,1]))
            call('glClearDepthf',1);call('glClearStencil',0);call('glClear',0x4000|0x100|0x400)
            call('glUniform1i',location('uPick'),int(picking));call('glUniform1i',location('uImage'),0);call('glUniform1i',location('uLight'),1)
            for draw,commands in zip(frame['draws'],entry['pickCalls' if picking else 'colorCalls'],strict=True):
                matrix('uMatrix',draw['matrix']);i=draw['pick_id'];call('glUniform3f',location('uId'),(i&255)/255,((i>>8)&255)/255,((i>>16)&255)/255)
                call('glActiveTexture',0x84C0);call('glBindTexture',0xDE1,textures[draw['texture']] if draw['texture'] is not None else white)
                light=draw.get('light');call('glUniform1i',location('uHasLight'),int(bool(light)))
                if light:
                    call('glActiveTexture',0x84C1);call('glBindTexture',0xDE1,textures[light['texture']]);matrix('uLightMatrix',light['matrix'])
                for cmd in commands:
                    name='gl'+cmd['name'][0].upper()+cmd['name'][1:];args=[]
                    for a in cmd['args']:args.append(location(a) if isinstance(a,str) and a.startswith('u') else K.get(a,a) if isinstance(a,str) else a)
                    if os.environ.get('WONDERLAND_GLES_FORCE_CLAMP')=='1' and cmd['name']=='uniform1i' and cmd['args'][0]=='uWrap':args[1]=0
                    call(name,*args)
                call('glBindVertexArray',arrays[draw['mesh']]);call('glDrawElements',4,len(frame['meshes'][draw['mesh']]['indices']),0x1405,None)
            call('glFinish');pixels=np.empty((height,width,4),dtype=np.uint8)
            call('glReadPixels',0,0,width,height,0x1908,0x1401,pixels.ctypes.data);checked(scene['name']);images.append(pixels[::-1].copy())
        cpu=np.frombuffer((directory/(scene['name']+'.rgba')).read_bytes(),dtype=np.uint8).reshape(height,width,4)
        diff=np.abs(cpu[:,:,:3].astype(float)-images[0][:,:,:3]);mae=float(diff.mean());rms=float(np.sqrt((diff*diff).mean()));over=float((diff>8).mean())
        ids=images[1].astype(np.uint32);ids=ids[:,:,0]|ids[:,:,1]<<8|ids[:,:,2]<<16
        mismatches=[p for p in scene['picks'] if ids[p['y'],p['x']]!=p['index']]
        object_centers=[p for p in scene['picks'] if p.get('avatar')] if scene['name'].startswith('wrapped-avatar') else []
        color_centers=[p for p in object_centers if diff[p['y'],p['x']].max()>8]
        if scene['name'].startswith('wrapped-avatar') and not object_centers:
            raise RuntimeError('Avatar witness contains no stable visible object centers')
        row={'name':scene['name'],'color':{'mae':mae,'rms':rms,'fractionOver8':over},'id_samples':len(scene['picks']),'id_mismatches':len(mismatches),'object_color_samples':len(object_centers),'object_color_mismatches':len(color_centers),'pixelsSha256':hashlib.sha256(images[0].tobytes()).hexdigest()}
        output['scenes'].append(row)
        if mae>4 or rms>12 or over>.03 or mismatches or color_centers:output['failures'].append(row)
        Image.fromarray(images[0]).save(directory/(scene['name']+('.clamp' if os.environ.get('WONDERLAND_GLES_FORCE_CLAMP')=='1' else '')+'.gles.png'))
        for name,values in [('glDeleteBuffers',buffers),('glDeleteVertexArrays',arrays),('glDeleteTextures',textures)]:
            if values:call(name,len(values),(U*len(values))(*values))
    output['status']='failed' if output['failures'] else 'passed'
    path=directory/('gles-clamp-mutation.json' if os.environ.get('WONDERLAND_GLES_FORCE_CLAMP')=='1' else 'gles-report.json')
    path.write_text(json.dumps(output,indent=2)+'\n');print(json.dumps(output))
    egl('eglMakeCurrent',U,[P,P,P,P])(d,None,None,None)
    egl('eglDestroyContext',U,[P,P])(d,ctx);egl('eglDestroySurface',U,[P,P])(d,surface);egl('eglTerminate',U,[P])(d)
    return not output['failures']
if __name__=='__main__':
    directory=Path(sys.argv[1] if len(sys.argv)>1 else 'tests/output/source-gpu')
    sys.exit(0 if run(json.loads((directory/'gles-input.json').read_text()),directory) else 1)
