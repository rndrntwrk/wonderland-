//! Small exact-source Mono oracle, separate from the shipping simulation.
//! The extracted classes retain their FreeSO source provenance. Shims supply
//! only content tables/avatar storage; this is not the full FreeSO runtime.
use sim_core::avatars::advertisements::{InteractionCandidate, MotiveAdvertisement};
use sim_core::avatars::autonomy::{score_candidates, AutonomyContext, AutonomyTuning, ScoreCurve};
use sim_core::avatars::events::TimeProperty;
use sim_core::avatars::motives::{
    fractional_multiply, DecayContext, Motive, MotiveDecay, MotiveState, TsoMotiveTuning,
};
use sim_core::avatars::state::PersonData;
use sim_core::avatars::timeline::{
    AnimationCommand, AnimationMetadata, AnimationResult, AnimationTimeline,
};
use sim_core::avatars::AvatarPlatform;
use sim_core::ids::{EntityRef, ObjectId};
use std::fmt::Write as _;

const EXPECTED:&str="TSO_DECAY 400,400,170,420,187,250,27 MOOD=97\nFRAC_MUL_CAST_FIRST -2147483\nTS1_DECAY 0,500,170,420,375,250,55 MOOD=87\nRESTORE_OVER_CAP value=3 fraction_bits=3ff0000000000000\nRESTORE_AFTER_CLEAR value=5 fraction_bits=3fe0000000000000\nANIMATION_TICK_1 frame_bits=3f99999a queue= count=0 end=False\nANIMATION_TICK_2 frame_bits=4019999a queue=7,101,-3 count=2 end=False\nANIMATION_TICK_3 frame_bits=40666667 queue=7,101,-3 count=2 end=True\nANIMATION_SYNTH queue=7,101,-3,2,3 count=4\nAUTONOMY_SCORE bits=3de39000";

fn join(values: impl IntoIterator<Item = i16>) -> String {
    values
        .into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn avatar_rust_matches_extracted_mono_vectors() {
    let mut output = String::new();
    let mut state = MotiveState::default();
    let tuning = TsoMotiveTuning {
        flat_sim: [2, 400, 80, 170, 150, 300, 300, 180000, 16, 250, 55, 0],
        category_weights: [[1000; 7]; 11],
    };
    let mut decay = MotiveDecay::tso(Some(tuning));
    decay
        .tick(
            &mut state,
            &DecayContext {
                minute: 1,
                room_score: 80,
                ..Default::default()
            },
        )
        .unwrap();
    writeln!(
        output,
        "TSO_DECAY {} MOOD={}",
        join(*decay.fractions()),
        state.get(Motive::Mood)
    )
    .unwrap();
    writeln!(
        output,
        "FRAC_MUL_CAST_FIRST {}",
        fractional_multiply(1073741824, 6)
    )
    .unwrap();
    let mut state = MotiveState::default();
    state.changes[7].per_hour_change = 1;
    let mut decay = MotiveDecay::ts1();
    decay
        .tick(
            &mut state,
            &DecayContext {
                minute: 2,
                active_personality: 666,
                ..Default::default()
            },
        )
        .unwrap();
    writeln!(
        output,
        "TS1_DECAY {} MOOD={}",
        join(*decay.fractions()),
        state.get(Motive::Mood)
    )
    .unwrap();
    let mut state = MotiveState::default();
    state.set(Motive::Hunger, 0);
    state.changes[7].per_hour_change = 9000;
    state.changes[7].max_value = 2;
    state.tick_changes(AvatarPlatform::Tso);
    state.tick_changes(AvatarPlatform::Tso);
    state.set(Motive::Hunger, 3);
    state.tick_changes(AvatarPlatform::Tso);
    writeln!(
        output,
        "RESTORE_OVER_CAP value={} fraction_bits={:016x}",
        state.get(Motive::Hunger),
        state.changes[7].fractional.to_bits()
    )
    .unwrap();
    state.clear_changes();
    state.changes[7].per_hour_change = 4500;
    state.changes[7].max_value = 100;
    state.set(Motive::Hunger, 0);
    for _ in 0..9 {
        state.tick_changes(AvatarPlatform::Tso);
    }
    writeln!(
        output,
        "RESTORE_AFTER_CLEAR value={} fraction_bits={:016x}",
        state.get(Motive::Hunger),
        state.changes[7].fractional.to_bits()
    )
    .unwrap();
    let metadata = AnimationMetadata {
        resource: "fixture.anim".into(),
        num_frames: 3,
        time_properties: vec![
            TimeProperty::xevt(60, 7),
            TimeProperty::xevt(0, 101),
            TimeProperty::xevt(20, -3),
        ],
    };
    let mut command = AnimationCommand::play("fixture.anim");
    let mut timeline = AnimationTimeline::default();
    timeline.apply(&command, Some(&metadata)).unwrap();
    for i in 1..=3 {
        timeline.tick().unwrap();
        let state = &timeline.animations[0];
        writeln!(
            output,
            "ANIMATION_TICK_{} frame_bits={:08x} queue={} count={} end={}",
            i,
            state.current_frame.to_bits(),
            join(state.event_queue.iter().copied()),
            state.events_run,
            if state.end_reached { "True" } else { "False" }
        )
        .unwrap();
    }
    command.expected_events = 4;
    let first = match timeline.apply(&command, Some(&metadata)).unwrap() {
        AnimationResult::Event(code) => code,
        other => panic!("expected first event, got {other:?}"),
    };
    let state = &timeline.animations[0];
    writeln!(
        output,
        "ANIMATION_SYNTH queue={} count={}",
        join(std::iter::once(first).chain(state.event_queue.iter().copied())),
        state.events_run
    )
    .unwrap();
    let curve = ScoreCurve::new(vec![(-100, -100), (100, 100)]).unwrap();
    let tuning = AutonomyTuning {
        adult_curves: std::array::from_fn(|_| curve.clone()),
        child_curves: std::array::from_fn(|_| curve.clone()),
    };
    let mut motives = MotiveState::default();
    motives.set(Motive::Hunger, -50);
    let context = AutonomyContext {
        platform: AvatarPlatform::Ts1,
        motives,
        person_data: PersonData::default(),
        x: 0,
        y: 0,
        level: 1,
        queued: Vec::new(),
    };
    let mut candidate = InteractionCandidate::new(
        EntityRef {
            object_id: ObjectId(2),
            generation: 1,
        },
        0,
        1,
    );
    candidate.advertisements.push(MotiveAdvertisement {
        motive: Motive::Hunger,
        minimum: 0,
        delta: 1000,
        personality_modifier: 0,
    });
    let score = score_candidates(&context, &[candidate], &tuning).unwrap()[0].score;
    write!(output, "AUTONOMY_SCORE bits={:08x}", score.to_bits()).unwrap();
    assert_eq!(output, EXPECTED);
}

#[test]
#[ignore = "Requires mcs and mono; execute explicitly as the source oracle gate"]
fn avatar_extracted_mono_reference() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temporary = std::env::temp_dir().join(format!(
        "wonderland-avatar-reference-{}",
        std::process::id()
    ));
    fs::create_dir(&temporary).unwrap();
    let paths = [
        "TSOClient/tso.simantics/Model/VMMotive.cs",
        "TSOClient/tso.simantics/Model/VMPersonDataVariable.cs",
        "TSOClient/tso.simantics/Model/VMMotiveChange.cs",
        "TSOClient/tso.simantics/Entities/VMAvatarMotiveDecay.cs",
        "TSOClient/tso.simantics/Entities/VMTS1MotiveDecay.cs",
        "TSOClient/tso.simantics/Model/VMAnimationState.cs",
        "TSOClient/tso.common/TS1/TS1Curve.cs",
    ];
    let mut files = Vec::new();
    for path in paths {
        let destination = temporary.join(Path::new(path).file_name().unwrap());
        fs::copy(root.join(path), &destination).unwrap();
        files.push(destination);
    }
    fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start = source.find(start).unwrap();
        let end = start + source[start..].find(end).unwrap();
        &source[start..end]
    }
    let avatar =
        fs::read_to_string(root.join("TSOClient/tso.simantics/Entities/VMAvatar.cs")).unwrap();
    let tick = between(
        &avatar,
        "            float totalWeight = 0f;",
        "            UpdateHeadSeek();",
    );
    let handler = format!(
        "{}        }}\n",
        between(
            &avatar,
            "        private void HandleTimePropsEvent",
            "            var rhevt ="
        )
    );
    let animator =
        fs::read_to_string(root.join("TSOClient/tso.vitaboy.engine/Animator.cs")).unwrap();
    let silent = between(
        &animator,
        "        public static AnimationStatus SilentFrameProgress",
        "        /// <summary>",
    );
    let primitive =
        fs::read_to_string(root.join("TSOClient/tso.simantics/Primitives/VMAnimateSim.cs"))
            .unwrap();
    let synthesis = between(
        &primitive,
        "                        if (cAnim.EndReached)",
        "                        if (cAnim.EventQueue.Count > 0)",
    );
    let probe = CSHARP_PROBE
        .replace("__HANDLER__", &handler)
        .replace("__TICK__", tick)
        .replace("__SILENT__", silent)
        .replace("__SYNTH__", synthesis);
    let probe_path = temporary.join("Probe.cs");
    fs::write(&probe_path, probe).unwrap();
    files.push(probe_path);
    let executable = temporary.join("Probe.exe");
    let mut compiler = Command::new("mcs");
    compiler
        .arg("-checked-")
        .arg(format!("-out:{}", executable.display()))
        .args(&files);
    let compiled = compiler
        .output()
        .expect("mcs must be installed for this explicit oracle test");
    assert!(
        compiled.status.success(),
        "reference compilation: {}{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    let result = Command::new("mono")
        .arg(executable)
        .output()
        .expect("mono must be installed for this explicit oracle test");
    assert!(
        result.status.success(),
        "reference execution: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert_eq!(stdout.trim(), EXPECTED);
    fs::remove_dir_all(temporary).unwrap();
}

const CSHARP_PROBE: &str = r###"
using System;
using System.Linq;
using System.IO;
using System.Collections.Generic;
using FSO.SimAntics.Model;
using FSO.SimAntics.Entities;
using FSO.Vitaboy;
namespace FSO.Files { public struct TuningEntry { public Dictionary<string,float> Data; public static TuningEntry DEFAULT; public int KeyValueCount {get {return Data==null?0:Data.Count;}} public float GetNum(string key){return Data[key];} } }
namespace FSO.Content { public class Tuning {public Dictionary<string,FSO.Files.TuningEntry> EntriesByName=new Dictionary<string,FSO.Files.TuningEntry>();} public class Anims {public Animation Get(string name){return null;}} public class Content {static Content instance=new Content();public bool TS1;public Tuning GlobalTuning=new Tuning(); public Anims AvatarAnimations=new Anims();public static Content Get(){return instance;}} }
namespace FSO.SimAntics.NetPlay.Model {public interface VMSerializable {void SerializeInto(BinaryWriter writer);void Deserialize(BinaryReader reader);}}
namespace FSO.SimAntics.Model {public enum VMStackObjectVariable {Hidden}}
namespace FSO.SimAntics.Marshals {public class VMAnimationStateMarshal {public string Anim;public float CurrentFrame;public short[] EventQueue;public byte EventsRun;public bool EndReached,PlayingBackwards;public float Speed,Weight;public bool Loop;}}
namespace FSO.Vitaboy {public class Avatar {} public enum AnimationStatus {IN_PROGRESS,COMPLETED} public class TimePropertyListItem {public uint ID;public Dictionary<string,string> Properties=new Dictionary<string,string>();} public class TimeProperty {public List<TimePropertyListItem> Items=new List<TimePropertyListItem>();} public class Motion {public List<TimeProperty> TimeProperties=new List<TimeProperty>();} public class Animation {public string Name="fixture";public int NumFrames;public List<Motion> Motions=new List<Motion>();} public class Animator {__SILENT__}}
namespace Microsoft.Xna.Framework {public struct Point {public int X,Y;public Point(int x,int y){X=x;Y=y;}}}
namespace FSO.SimAntics {
 public class Clock {public int Minutes,Hours;}
 public class Lot {public int PropertyCategory;}
 public class Limits {public int GetLimit(VMMotive m){return 100;}}
 public class VM {public bool TS1;public Lot TSOState=new Lot();public Limits TuningCache=new Limits();}
 public class VMContext {public Clock Clock=new Clock();public VM VM=new VM();public short RoomScore=100;public int GetRoomAt(int position){return 0;}public short GetRoomScore(int room){return RoomScore;}}
 public class VMAvatar {
  public short[] Motives=Enumerable.Repeat((short)100,16).ToArray();public short[] PersonData=new short[101];public bool[] HasChange=new bool[16];public int Position;public short Hidden;
  public List<VMAnimationState> Animations=new List<VMAnimationState>();public VMAnimationState CurrentAnimationState {get{return Animations.FirstOrDefault();}}public Avatar Avatar=new Avatar();
  public VMAvatar(){Motives[11]=0;}
  public short GetMotiveData(VMMotive m){return Motives[(int)m];}
  public void SetMotiveData(VMMotive m,short value){var old=Motives[(int)m];Motives[(int)m]=(short)Math.Max(Math.Min(value,Math.Max((int)old,100)),-100);}
  public short GetPersonData(VMPersonDataVariable p){return PersonData[(int)p];}
  public short GetValue(VMStackObjectVariable p){return Hidden;}
  public bool HasMotiveChange(VMMotive m){return HasChange[(int)m];}
  __HANDLER__
  public void TickAnimation(){VMAvatar avatar=this;__TICK__}
  public void Synthesize(byte expected){var cAnim=CurrentAnimationState;var operand=new Expected {ExpectedEventCount=expected};__SYNTH__}
 }
 public class Expected {public byte ExpectedEventCount;}
}
class Probe {
 static uint Bits(float f){return BitConverter.ToUInt32(BitConverter.GetBytes(f),0);}
 static ulong Fraction(VMMotiveChange m){var s=new MemoryStream();m.SerializeInto(new BinaryWriter(s));return BitConverter.ToUInt64(s.ToArray(),5);}
 static Animation Anim(){var a=new Animation {NumFrames=3};var m=new Motion();var t=new TimeProperty();uint[] ms={60,0,20};short[] codes={7,101,-3};for(int i=0;i<3;i++){var p=new TimePropertyListItem {ID=ms[i]};p.Properties["xevt"]=codes[i].ToString();t.Items.Add(p);}m.TimeProperties.Add(t);a.Motions.Add(m);return a;}
 static void Main(){
  var c=FSO.Content.Content.Get();var values=new Dictionary<string,float>();string[] keys={"HungerDecrementRatio","ComfortDecrementActive","HygieneDecrementAsleep","HygieneDecrementAwake","BladderDecrementAsleep","BladderDecrementAwake","HungerToBladderMultiplier","EnergySpan","WakeHours","EntDecrementAwake","SocialDecrementBase","SocialDecrementMultiplier"};float[] nums={.0021f,.4f,.08f,.17f,.15f,.3f,.3f,180f,16f,.25f,.055f,0f};for(int i=0;i<keys.Length;i++)values[keys[i]]=nums[i];c.GlobalTuning.EntriesByName["simmotives"]=new FSO.Files.TuningEntry {Data=values};var lots=new Dictionary<string,float>();foreach(var category in VMAvatarMotiveDecay.CategoryNames)foreach(var motive in VMAvatarMotiveDecay.LotMotiveNames)lots[category+"_"+motive+"Weight"]=1f;c.GlobalTuning.EntriesByName["lotmotives"]=new FSO.Files.TuningEntry {Data=lots};
  var avatar=new FSO.SimAntics.VMAvatar();var context=new FSO.SimAntics.VMContext();context.Clock.Minutes=1;context.RoomScore=80;var tso=new VMAvatarMotiveDecay();tso.Tick(avatar,context);Console.WriteLine("TSO_DECAY "+string.Join(",",tso.MotiveFractions)+" MOOD="+avatar.GetMotiveData(VMMotive.Mood));Console.WriteLine("FRAC_MUL_CAST_FIRST "+tso.FracMul(1073741824,6));
  var ts1=new VMTS1MotiveDecay();avatar=new FSO.SimAntics.VMAvatar();avatar.PersonData[3]=666;avatar.HasChange[7]=true;context.Clock.Minutes=2;context.RoomScore=100;ts1.Tick(avatar,context);Console.WriteLine("TS1_DECAY "+string.Join(",",ts1.MotiveFractions)+" MOOD="+avatar.GetMotiveData(VMMotive.Mood));
  avatar=new FSO.SimAntics.VMAvatar();avatar.SetMotiveData(VMMotive.Hunger,0);var change=new VMMotiveChange {Motive=VMMotive.Hunger,PerHourChange=9000,MaxValue=2};change.Tick(avatar);change.Tick(avatar);avatar.SetMotiveData(VMMotive.Hunger,3);change.Tick(avatar);Console.WriteLine("RESTORE_OVER_CAP value="+avatar.GetMotiveData(VMMotive.Hunger)+" fraction_bits="+Fraction(change).ToString("x16"));change.Clear();change.PerHourChange=4500;change.MaxValue=100;avatar.SetMotiveData(VMMotive.Hunger,0);for(int i=0;i<9;i++)change.Tick(avatar);Console.WriteLine("RESTORE_AFTER_CLEAR value="+avatar.GetMotiveData(VMMotive.Hunger)+" fraction_bits="+Fraction(change).ToString("x16"));
  avatar=new FSO.SimAntics.VMAvatar();var state=new VMAnimationState(Anim(),false);state.Speed=30/25f;avatar.Animations.Add(state);for(int i=1;i<=3;i++){avatar.TickAnimation();Console.WriteLine("ANIMATION_TICK_"+i+" frame_bits="+Bits(state.CurrentFrame).ToString("x8")+" queue="+string.Join(",",state.EventQueue)+" count="+state.EventsRun+" end="+state.EndReached);}avatar.Synthesize(4);Console.WriteLine("ANIMATION_SYNTH queue="+string.Join(",",state.EventQueue)+" count="+state.EventsRun);
  var curve=new FSO.Common.TS1.TS1Curve("(-100;-100) (100;100)");float[] parts={curve.GetPoint(100)*(1f/9f),curve.GetPoint(100)*(1f/9f),curve.GetPoint(-50)*(1f/9f),curve.GetPoint(100)*(1f/9f),curve.GetPoint(100)*(1f/9f),curve.GetPoint(100)*(1f/9f),curve.GetPoint(100)*(1f/9f),curve.GetPoint(100)*(1f/9f),curve.GetPoint(100)*(1f/9f)};float b=parts.Sum();float score=b;score-=parts[2];score+=curve.GetPoint(-50+(1000*1f)/1000f)*(1f/9f);score-=b;Console.WriteLine("AUTONOMY_SCORE bits="+Bits(score).ToString("x8"));
 }
}
"###;
