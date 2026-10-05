#!/usr/bin/env python3
"""Extract source methods from a read-only baseline; compile with its checked DLL.
No original payloads or source excerpts are installed into shipping Rust crates.
Usage: source_probe.py SOURCE_CHECKOUT OUTPUT_DIR
"""
import pathlib, subprocess, os, sys, hashlib
source=pathlib.Path(sys.argv[1]).resolve();out=pathlib.Path(sys.argv[2]).resolve();out.mkdir(parents=True,exist_ok=True)
model=source/'TSOClient/tso.vitaboy.model';engine=source/'TSOClient/tso.vitaboy.engine'
assembly=source/'TSOClient/tso.client/Monogame/MacOS/MonoGame.Framework.dll'
for rel in ['TSOClient/tso.vitaboy.model/Skeleton.cs','TSOClient/tso.vitaboy.engine/Animator.cs']:
    original=subprocess.check_output(['git','-C',str(source),'show','4c6b3e8f5835b228723caea3c9f683c62f244f73:'+rel],text=True)
    if (source/rel).read_text(encoding='utf-8-sig')!=original.lstrip('\ufeff'):
        sys.exit('source differs from pinned baseline: '+rel)
def method(path,signature):
    text=path.read_text(encoding='utf-8-sig');start=text.index(signature);body=text.index('{',start);depth=1;end=body+1
    while depth:
        if text[end]=='{':depth+=1
        if text[end]=='}':depth-=1
        end+=1
    return text[start:end]
compute=method(model/'Skeleton.cs','public void ComputeBonePositions')
render=method(engine/'Animator.cs','public static AnimationStatus RenderFrame')
seek=method(engine/'Animator.cs','public static Quaternion CalculateHeadSeek')
program='''using System;
using System.Collections.Generic;
using System.Globalization;
using Microsoft.Xna.Framework;
public enum AnimationStatus{COMPLETED,IN_PROGRESS}
public class Bone{public string Name,ParentName;public Vector3 Translation,AbsolutePosition;public Quaternion Rotation=Quaternion.Identity;public Matrix AbsoluteMatrix;public List<Bone> Children=new List<Bone>();}
public class Skeleton{public List<Bone>Bones=new List<Bone>();public Bone GetBone(string name){return Bones.Find(b=>b.Name==name);} COMPUTE}
public class Avatar{public Skeleton Skeleton=new Skeleton();}
public class Motion{public string BoneName;public int FrameCount,FirstTranslationIndex,FirstRotationIndex;public bool HasTranslation,HasRotation;}
public class Animation{public int NumFrames;public List<Motion>Motions=new List<Motion>();public Vector3[]Translations;public Quaternion[]Rotations;}
public class Animator{RENDER SEEK}
public class Probe{
 static void V(string n,Vector3 v){Console.WriteLine(n+" "+v.X.ToString("R")+" "+v.Y.ToString("R")+" "+v.Z.ToString("R"));}
 static void Q(string n,Quaternion q){Console.WriteLine(n+" "+q.X.ToString("R")+" "+q.Y.ToString("R")+" "+q.Z.ToString("R")+" "+q.W.ToString("R"));}
 public static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
 Console.WriteLine("assembly "+typeof(Quaternion).Assembly.FullName);
 var avatar=new Avatar();var root=new Bone{Name="ROOT",ParentName="NULL",Translation=new Vector3(1,0,0),Rotation=Quaternion.CreateFromAxisAngle(Vector3.UnitZ,(float)Math.PI/2)};var head=new Bone{Name="HEAD",ParentName="ROOT",Translation=new Vector3(0,2,0)};root.Children.Add(head);avatar.Skeleton.Bones.Add(root);avatar.Skeleton.Bones.Add(head);avatar.Skeleton.ComputeBonePositions(root,Matrix.Identity);V("root",root.AbsolutePosition);V("child",head.AbsolutePosition);
 var anim=new Animation{NumFrames=3,Translations=new[]{new Vector3(2,0,0),new Vector3(8,0,0)}};anim.Motions.Add(new Motion{BoneName="HEAD",FrameCount=2,HasTranslation=true});Animator.RenderFrame(avatar,anim,0,-0.2f,1);V("negative_fraction",head.Translation);Animator.RenderFrame(avatar,anim,2,0.8f,1);V("track_end",head.Translation);head.Translation=new Vector3(0,0,0);Animator.RenderFrame(avatar,anim,1,0,0.75f);V("prefix_mix",head.Translation);
 root.Translation=Vector3.Zero;root.Rotation=Quaternion.Identity;avatar.Skeleton.ComputeBonePositions(root,Matrix.Identity);Q("seek_vertical",Animator.CalculateHeadSeek(avatar,new Vector3(10,0,1),(float)Math.PI));Q("seek_horizontal",Animator.CalculateHeadSeek(avatar,new Vector3(0,10,1),(float)Math.PI));
 Q("slerp_endpoint",Quaternion.Slerp(Quaternion.Identity,Quaternion.CreateFromAxisAngle(Vector3.UnitY,1),1));Q("slerp_antipode",Quaternion.Slerp(Quaternion.Identity,new Quaternion(0,0,0,-1),0.5f));Q("slerp_near",Quaternion.Slerp(Quaternion.Identity,Quaternion.CreateFromAxisAngle(Vector3.UnitY,0.01f),0.37f));Q("slerp_half_turn",Quaternion.Slerp(Quaternion.Identity,new Quaternion(0,1,0,0),0.5f));Q("slerp_nonunit",Quaternion.Slerp(new Quaternion(0,0,0,2),new Quaternion(0,1,0,0),0.5f));
 var primary=Matrix.CreateTranslation(1,0,0);var secondary=Matrix.CreateTranslation(0,1,0);V("dual_local",Vector3.Transform(new Vector3(1,0,0),primary)*0.75f+Vector3.Transform(new Vector3(0,2,0),secondary)*0.25f);
 }
}
'''.replace('COMPUTE',compute).replace('RENDER',render).replace('SEEK',seek)
cs=out/'AvatarSourceProbe.cs';exe=out/'AvatarSourceProbe.exe';cs.write_text(program)
subprocess.run(['mcs','-r:'+str(assembly),'-out:'+str(exe),str(cs)],check=True)
env=dict(os.environ);env['MONO_PATH']=str(assembly.parent)
result=subprocess.run(['mono',str(exe)],env=env,check=True,text=True,capture_output=True)
metadata='source_baseline 4c6b3e8f5835b228723caea3c9f683c62f244f73\nassembly_sha256 '+hashlib.sha256(assembly.read_bytes()).hexdigest()+'\nprobe_method_sha256 '+hashlib.sha256((compute+render+seek).encode()).hexdigest()+'\n'
print(metadata+result.stdout,end='');(out/'source-probe-results.txt').write_text(metadata+result.stdout)
