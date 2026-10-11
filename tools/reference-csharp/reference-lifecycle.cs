// Template expanded by reference-lifecycle.py with unchanged pinned C# methods.
// The scheduler and NotifyOutOfIdle compile as their complete original files.
// Tick's BHAV body is a fixture executor; this is not a full original VM oracle.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Reflection;
using FSO.SimAntics;
using FSO.SimAntics.Engine;
using FSO.SimAntics.Primitives;

namespace FSO.Files.Utils {
    public enum ByteOrder { LITTLE_ENDIAN }
    public sealed class IoBuffer : IDisposable {
        public static IoBuffer FromBytes(byte[] bytes, ByteOrder order) { return new IoBuffer(); }
        public void Dispose() { }
    }
}
namespace FSO.SimAntics {
    public enum VMStackObjectVariable { Room, PlacementFlags }
    [Flags] public enum VMPlacementFlags { OnFloor = 1, InAir = 512 }
    public enum VMGameObjectDisableFlags { None = 0, ForSale = 2, Disabled = 3 }
    public struct FloorTile { public ushort Pattern; }
    public struct LotTilePos {
        public short TileX, TileY; public sbyte Level;
        public static readonly LotTilePos OUT_OF_WORLD = new LotTilePos { TileX = -32768, TileY = -32768, Level = 1 };
        public static bool operator ==(LotTilePos a, LotTilePos b) { return a.TileX == b.TileX && a.TileY == b.TileY && a.Level == b.Level; }
        public static bool operator !=(LotTilePos a, LotTilePos b) { return !(a == b); }
        public override bool Equals(object obj) { return obj is LotTilePos && this == (LotTilePos)obj; }
        public override int GetHashCode() { return TileX ^ TileY ^ Level; }
    }
    public sealed class ArchitectureShim {
        public FloorTile GetFloor(short x, short y, sbyte level) { return new FloorTile(); }
        public void SetFloor(short x, short y, sbyte level, FloorTile floor, bool force) { throw new InvalidOperationException("entrypoint 11 is outside this oracle"); }
    }
    public sealed class ObjectDefinitionShim { public int StackSize = 8; public int LevelOffset; }
    public sealed class GameObject { public ObjectDefinitionShim OBJ = new ObjectDefinitionShim(); }
    public sealed class OBJfFunctionEntry { public ushort ActionFunction; }
    public sealed class VMBHAVOwnerPair { public VMRoutine routine; public GameObject owner; }
    public sealed class MultitileShim { public List<VMEntity> Objects = new List<VMEntity>(); }
    public sealed class WorldUiShim { public int IdleFrames; public object Headline; }
    public sealed class HeadlineRendererShim {
        public bool Update() { return true; } public void Dispose() { }
        public object DrawFrame(object world) { return null; }
    }
    public sealed class VMSandboxRestoreState { public List<VMEntity> Entities; }
    public sealed class VM {
        public static bool UseWorld; public bool Aborting;
        public VMContext Context; public VMScheduler Scheduler;
        public List<VMEntity> Entities = new List<VMEntity>();
        public List<FrameRecord> Frames = new List<FrameRecord>();
        public List<string> Events = new List<string>(), Ticks = new List<string>(), Deletions = new List<string>();
        public VM(ulong seed = 123) { Context = new VMContext { VM = this, RandomSeed = seed }; Scheduler = new VMScheduler(this); }
        public VMEntity GetObjectById(short id) { return Entities.FirstOrDefault(entity => entity.ObjectID == id); }
        public void AddEntity(VMEntity entity) { AddToObjList(Entities, entity); }
        public VMSandboxRestoreState Sandbox() { Events.Add("sandbox"); return new VMSandboxRestoreState { Entities = new List<VMEntity>(Entities) }; }
        public void SandboxRestore(VMSandboxRestoreState state) { Entities = state.Entities; Events.Add("restore"); }
        @@VM_ADD_TO_OBJ_LIST@@
    }
    public sealed class VMContext {
        public VM VM; public ulong RandomSeed; public object World;
        public ArchitectureShim Architecture = new ArchitectureShim();
        @@VM_NEXT_RANDOM@@
    }
    public class VMEntity {
        public short ObjectID, MainParam, MainStackOBJ, Room; public bool GhostImage, Dead;
        public VMThread Thread; public GameObject Object = new GameObject();
        public OBJfFunctionEntry[] EntryPoints = Enumerable.Range(0, 33).Select(i => new OBJfFunctionEntry()).ToArray();
        public Dictionary<ushort, VMRoutine> Routines = new Dictionary<ushort, VMRoutine>();
        public short[] ObjectData = new short[2]; public LotTilePos Position;
        public MultitileShim MultitileGroup = new MultitileShim();
        public WorldUiShim WorldUI = new WorldUiShim(); public object Headline;
        public HeadlineRendererShim HeadlineRenderer; public bool UseWorld { get { return VM.UseWorld; } }
        public Action<VMThread> FixtureTick; private bool InReset;
        public void FetchTreeByName(VMContext context) { context.VM.Events.Add("fetch:" + ObjectID); }
        public void UpdateTuning(VM vm) { vm.Events.Add("tuning:" + ObjectID); }
        public void SetValue(VMStackObjectVariable field, short value) {
            if (field != VMStackObjectVariable.Room) throw new InvalidOperationException("unmodeled entity field"); Room = value;
        }
        public VMBHAVOwnerPair GetRoutineWithOwner(ushort id, VMContext context) {
            VMRoutine routine; return Routines.TryGetValue(id, out routine) ? new VMBHAVOwnerPair { routine = routine, owner = Object } : null;
        }
        public void Delete(bool cleanup, VMContext context) {
            context.VM.Deletions.Add(ObjectID + ":" + context.RandomSeed + ":" + context.VM.Entities.Count + ":" + context.VM.Scheduler.RunningNow);
            Dead = true; context.VM.Entities.Remove(this);
        }
        @@VM_ENTITY_TICK@@
        @@VM_RUN_EVERY_FRAME@@
        @@VM_ENTITY_INIT@@
        @@VM_ENTITY_RESET@@
        @@VM_ENTRYPOINT_3@@
        @@VM_ENTRYPOINT_4@@
        @@VM_ENTRYPOINT_5@@
        @@VM_GENERIC_ENTRYPOINT@@
    }
    public sealed class VMGameObject : VMEntity {
        public VMGameObjectDisableFlags Disabled; public int LightRefreshes;
        public void RefreshLight() { LightRefreshes++; }
    }
    public sealed class VMAvatar : VMEntity { public bool IsPet; }
}
namespace FSO.SimAntics.Engine {
    public sealed class VMRoutine {
        public ushort ID; public int Locals = 2, Arguments = 4;
        public object[] Instructions = new object[] { new object() };
        public VMPrimitiveExitCode FixtureExit = VMPrimitiveExitCode.RETURN_TRUE;
        public Action<VMThread, VMStackFrame> FixtureAction;
    }
    public sealed class VMQueuedAction { }
    public sealed class VMPieMenuInteraction { public object MotiveAdChanges; }
    public enum VMThreadBreakMode { Active }
    public sealed class VMStackFrame {
        public VMThread Thread; public VMRoutine Routine; public byte InstructionPointer;
        public VMEntity Caller, Callee; public GameObject CodeOwner; public short[] Locals, Args;
        private VMEntity _StackObject; public short _StackObjectID;
        public VM VM { get { return Thread.Context.VM; } }
        @@VM_STACK_OBJECT@@
        @@VM_STACK_OBJECT_ID@@
    }
    public sealed class VMThread {
        public VMContext Context; public VMEntity Entity;
        public List<VMStackFrame> Stack; public List<VMQueuedAction> Queue;
        public bool QueueDirty, IsCheck, Interrupt; public sbyte ActiveQueueBlock = -1;
        public object BlockingState, EODConnection, MotiveAdChanges;
        public int DialogCooldown, TicksThisFrame; public uint ScheduleIdleStart, ScheduleIdleEnd;
        public short[] TempRegisters = new short[20]; public int[] TempXL = new int[2];
        public VMPrimitiveExitCode LastStackExitCode = VMPrimitiveExitCode.GOTO_FALSE;
        public VMThreadBreakMode ThreadBreak; public List<VMPieMenuInteraction> ActionStrings;
        @@VM_THREAD_CTOR@@
        @@VM_THREAD_PUSH@@
        @@VM_CHECK_3@@
        @@VM_CHECK_4@@
        @@VM_CHECK_5@@
        public void Tick() {
            // Fixture seam: these actions stand in for BHAV instructions.
            if (!IsCheck) {
                Context.VM.Ticks.Add(Context.VM.Scheduler.CurrentTickID + ":" + Entity.ObjectID);
                if (Entity.FixtureTick != null) Entity.FixtureTick(this);
            }
            // Only this empty-stack block is copied from the original Tick.
            @@VM_EMPTY_STACK_MAIN@@
            var frame = Stack[Stack.Count - 1];
            Context.VM.Frames.Add(new FrameRecord(frame, IsCheck));
            if (frame.Routine.FixtureAction != null) frame.Routine.FixtureAction(this, frame);
            LastStackExitCode = frame.Routine.FixtureExit; Stack.Remove(frame);
        }
    }
}
public sealed class FrameRecord {
    public readonly ushort Routine; public readonly short Caller, Callee, Stack;
    public readonly short[] Args; public readonly bool Check; public readonly int LocalCount;
    public FrameRecord(VMStackFrame frame, bool check) {
        Routine = frame.Routine.ID; Caller = frame.Caller.ObjectID; Callee = frame.Callee.ObjectID;
        Stack = frame.StackObjectID; Args = (short[])frame.Args.Clone(); Check = check; LocalCount = frame.Locals.Length;
    }
    public string Fields() {
        return "routine=" + Routine + "\tcaller=" + Caller + "\tcallee=" + Callee + "\tstack=" + Stack
            + "\targs=" + String.Join(",", Args) + "\tlocals=" + LocalCount + "\tcheck=" + Check;
    }
}
public static class ReferenceLifecycle {
    private static void Emit(string name, params string[] values) { Console.WriteLine(name + "\t" + String.Join("\t", values)); }
    private static VMGameObject Object(VM vm, short id, bool entries = false, bool register = true) {
        var entity = new VMGameObject { ObjectID = id }; entity.MultitileGroup.Objects.Add(entity);
        if (entries) foreach (var entry in new int[] { 0, 8, 1, 3 }) {
            var routine = new VMRoutine { ID = (ushort)(4096 + entry) };
            entity.EntryPoints[entry].ActionFunction = routine.ID; entity.Routines.Add(routine.ID, routine);
        }
        if (register) vm.AddEntity(entity); return entity;
    }
    private static VMGameObject Scheduled(VM vm, short id) {
        var entity = Object(vm, id); entity.Thread = new VMThread(vm.Context, entity, 8); return entity;
    }
    private static VMPrimitiveExitCode Notify(VM vm, VMEntity target) {
        var caller = vm.Entities.First(entity => entity.Thread != null);
        return new VMNotifyOutOfIdle().Execute(new VMStackFrame { Thread = caller.Thread, StackObject = target }, new VMNotifyOutOfIdleOperand());
    }
    private static string Calendar(VM vm) {
        var field = typeof(VMScheduler).GetField("TickSchedule", BindingFlags.Instance | BindingFlags.NonPublic);
        var calendar = (Dictionary<uint, List<VMEntity>>)field.GetValue(vm.Scheduler);
        return String.Join(";", calendar.OrderBy(item => item.Key).Where(item => item.Value.Count > 0)
            .Select(item => item.Key + ":" + String.Join(",", item.Value.Select(entity => entity.ObjectID))));
    }
    private static void Initialization(string name, short parameter, short stack, string mainKind) {
        var vm = new VM(); var entity = Object(vm, 7, true); Object(vm, 9);
        entity.MainParam = parameter; entity.MainStackOBJ = stack;
        if (mainKind == "missing") entity.EntryPoints[1].ActionFunction = 0;
        if (mainKind == "unresolved") entity.Routines.Remove(4097);
        if (mainKind == "empty") entity.Routines[4097].Instructions = new object[0];
        entity.Init(vm.Context);
        Emit(name, "checks=" + String.Join(",", vm.Frames.Select(frame => frame.Routine)),
            "queued=" + entity.Thread.Stack.Count, "main_param=" + entity.MainParam, "main_stack=" + entity.MainStackOBJ,
            "idle_end=" + entity.Thread.ScheduleIdleEnd, "events=" + String.Join(",", vm.Events));
        foreach (var record in vm.Frames) Emit(name + ".check", record.Fields());
        if (entity.Thread.Stack.Count > 0) {
            Emit(name + ".queued", new FrameRecord(entity.Thread.Stack.Last(), false).Fields());
            entity.Thread.Tick(); Emit(name + ".first", vm.Frames.Last().Fields());
            entity.Thread.Tick(); Emit(name + ".restart", vm.Frames.Last().Fields());
        }
    }
    private static void GhostAndReset() {
        var vm = new VM(); Object(vm, 9); var ghost = Object(vm, 7, true, false);
        ghost.GhostImage = true; ghost.MainParam = 321; ghost.MainStackOBJ = 9; ghost.Init(vm.Context);
        Emit("init.ghost", "checks=" + String.Join(",", vm.Frames.Select(frame => frame.Routine)),
            "has_thread=" + (ghost.Thread != null), "main_param=" + ghost.MainParam, "main_stack=" + ghost.MainStackOBJ,
            "room=" + ghost.Room, "light_refreshes=" + ghost.LightRefreshes, "entities=" + vm.Entities.Count,
            "calendar=" + Calendar(vm), "events=" + String.Join(",", vm.Events));
        vm = new VM(); var entity = Object(vm, 7, true); entity.Init(vm.Context);
        entity.MainParam = 55; entity.MainStackOBJ = 9; entity.Thread.Interrupt = true; entity.Thread.TempRegisters[0] = 22;
        vm.Frames.Clear(); entity.Reset(vm.Context);
        Emit("reset", "checks=" + String.Join(",", vm.Frames.Select(frame => frame.Routine)),
            "main_param=" + entity.MainParam, "main_stack=" + entity.MainStackOBJ,
            "interrupt=" + entity.Thread.Interrupt, "temp0=" + entity.Thread.TempRegisters[0], "calendar=" + Calendar(vm));
        Emit("reset.queued", new FrameRecord(entity.Thread.Stack.Last(), false).Fields());
    }
    private static void ResetTemps(bool running) {
        var vm = new VM(); var entity = Object(vm, 7, true); entity.Init(vm.Context);
        entity.Thread.TempRegisters[0] = 11; entity.Thread.TempXL[0] = 22;
        entity.Routines[4099].FixtureAction = (thread, frame) => {
            thread.TempRegisters[0] = 77; thread.TempXL[0] = 88;
        };
        vm.Scheduler.RunningNow = running;
        entity.Reset(vm.Context);
        Emit("reset.temps", "running=" + running, "temp0=" + entity.Thread.TempRegisters[0],
            "xl0=" + entity.Thread.TempXL[0], "queued=" + entity.Thread.Stack.Count);
    }
    private static void CheckTemps(bool running, bool abort) {
        var vm = new VM(); var entity = Scheduled(vm, 7);
        entity.Thread.TempRegisters[0] = 11; entity.Thread.TempXL[0] = 22; vm.Scheduler.RunningNow = running;
        var routine = new VMRoutine { ID = 4100 };
        routine.FixtureAction = (thread, frame) => { thread.TempRegisters[0] = 77; thread.TempXL[0] = 88; if (abort) vm.Aborting = true; };
        var result = VMThread.EvaluateCheck(vm.Context, entity, new VMStackFrame { Caller = entity, Callee = entity, CodeOwner = entity.Object, Routine = routine, Args = new short[4] });
        Emit("check.temps", "running=" + running, "aborting=" + abort, "result=" + (byte)result,
            "temp0=" + entity.Thread.TempRegisters[0], "xl0=" + entity.Thread.TempXL[0]);
    }
    private static void SortedAndNotify() {
        var vm = new VM(100); var entities = new Dictionary<short, VMEntity>();
        foreach (short id in new short[] { 7, 1, 5, 2 }) entities[id] = Scheduled(vm, id);
        foreach (short id in new short[] { 7, 1, 5, 1 }) vm.Scheduler.ScheduleTick(entities[id], 1);
        vm.Scheduler.ScheduleTick(entities[2], 5); vm.Scheduler.BeginTick(1); vm.Scheduler.RunTick();
        Emit("scheduler.sorted", "ticks=" + String.Join(",", vm.Ticks), "calendar=" + Calendar(vm), "rng=" + vm.Context.RandomSeed,
            "current_object=" + vm.Scheduler.CurrentObjectID, "running=" + vm.Scheduler.RunningNow);
        vm = new VM(100); var five = Scheduled(vm, 5); var lower = Scheduled(vm, 2); var higher = Scheduled(vm, 8);
        vm.Scheduler.ScheduleTick(five, 1); vm.Scheduler.ScheduleTick(lower, 10); vm.Scheduler.ScheduleTick(higher, 10);
        string during = null;
        five.FixtureTick = thread => {
            var highResult = Notify(vm, higher); var lowResult = Notify(vm, lower); Notify(vm, higher);
            during = "higher=" + higher.Thread.ScheduleIdleEnd + ",lower=" + lower.Thread.ScheduleIdleEnd + ",results=" + (byte)highResult + "," + (byte)lowResult;
        };
        vm.Scheduler.BeginTick(1); vm.Scheduler.RunTick();
        Emit("notify.active", "during=" + during, "ticks=" + String.Join(",", vm.Ticks),
            "higher_interrupt=" + higher.Thread.Interrupt, "lower_interrupt=" + lower.Thread.Interrupt,
            "calendar=" + Calendar(vm), "rng=" + vm.Context.RandomSeed);
        vm = new VM(); Scheduled(vm, 7); var target = Scheduled(vm, 9); var noThread = Object(vm, 11);
        Emit("notify.null", "result=" + (byte)Notify(vm, null), "calendar=" + Calendar(vm));
        Emit("notify.no_thread", "result=" + (byte)Notify(vm, noThread), "calendar=" + Calendar(vm));
        Emit("notify.unscheduled", "result=" + (byte)Notify(vm, target), "interrupt=" + target.Thread.Interrupt, "idle_end=" + target.Thread.ScheduleIdleEnd, "calendar=" + Calendar(vm));
        target.Thread.Interrupt = false; vm.Scheduler.BeginTick(1); target.Thread.ScheduleIdleEnd = 1;
        Emit("notify.current", "result=" + (byte)Notify(vm, target), "interrupt=" + target.Thread.Interrupt, "idle_end=" + target.Thread.ScheduleIdleEnd, "calendar=" + Calendar(vm));
        vm.Scheduler.ScheduleTick(target, 20); target.Thread.Interrupt = false;
        Emit("notify.outside_run", "result=" + (byte)Notify(vm, target), "interrupt=" + target.Thread.Interrupt, "idle_end=" + target.Thread.ScheduleIdleEnd, "calendar=" + Calendar(vm));
    }
    private static void SchedulerEdges() {
        var vm = new VM(); vm.Scheduler.CurrentTickID = 40;
        var ordinary = Scheduled(vm, 1); var headline = Scheduled(vm, 2); var disabled = Scheduled(vm, 3);
        headline.Headline = new object(); disabled.Disabled = VMGameObjectDisableFlags.Disabled;
        foreach (var item in new VMEntity[] { ordinary, headline, disabled }) vm.Scheduler.ScheduleTickIn(item, 90);
        Emit("scheduler.every_frame", "ordinary=" + ordinary.Thread.ScheduleIdleEnd, "headline=" + headline.Thread.ScheduleIdleEnd, "disabled=" + disabled.Thread.ScheduleIdleEnd);
        vm = new VM(); var wrap = Scheduled(vm, 7); vm.Scheduler.CurrentTickID = UInt32.MaxValue; vm.Scheduler.ScheduleTickIn(wrap, 1);
        Emit("scheduler.uint32_wrap", "idle_end=" + wrap.Thread.ScheduleIdleEnd, "calendar=" + Calendar(vm));
        vm = new VM(100); var one = Scheduled(vm, 1); var seven = Scheduled(vm, 7);
        vm.Scheduler.ScheduleTick(one, 1); vm.Scheduler.ScheduleTick(seven, 1); vm.Scheduler.BeginTick(100);
        Emit("scheduler.resync_before", "calendar=" + Calendar(vm), "one_idle=" + one.Thread.ScheduleIdleEnd, "seven_idle=" + seven.Thread.ScheduleIdleEnd);
        vm.Scheduler.RunTick(); Emit("scheduler.resync_after", "ticks=" + String.Join(",", vm.Ticks), "calendar=" + Calendar(vm), "rng=" + vm.Context.RandomSeed);
        vm = new VM(123); var entity = Scheduled(vm, 1);
        entity.FixtureTick = thread => { vm.Context.NextRandom(1); vm.Scheduler.ScheduleTickIn(entity, 100); };
        vm.Scheduler.ScheduleTick(entity, 2); vm.Scheduler.ScheduleTick(entity, 5);
        Emit("scheduler.multiple_wakes_before", "calendar=" + Calendar(vm), "idle_end=" + entity.Thread.ScheduleIdleEnd);
        for (uint tick = 1; tick <= 5; tick++) { vm.Scheduler.BeginTick(tick); vm.Scheduler.RunTick(); }
        Emit("scheduler.multiple_wakes_after", "ticks=" + String.Join(",", vm.Ticks), "calendar=" + Calendar(vm), "rng=" + vm.Context.RandomSeed);
        vm = new VM(100); var caller = Scheduled(vm, 1); Object(vm, 2); var victim = Object(vm, 3);
        caller.FixtureTick = thread => { vm.Scheduler.Delete(victim); vm.Scheduler.Delete(victim); };
        vm.Scheduler.ScheduleTick(caller, 1); vm.Scheduler.BeginTick(1); vm.Scheduler.RunTick();
        Emit("scheduler.deferred_delete", "rng=" + vm.Context.RandomSeed, "entities=" + vm.Entities.Count,
            "deletions=" + String.Join(",", vm.Deletions), "pending=" + vm.Scheduler.PendingDeletion.Count);
    }
    public static void Main() {
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
        Initialization("init.default", 0, 0, "normal"); Initialization("init.creation", 321, 9, "normal");
        Initialization("init.unresolved_stack", -123, 99, "normal"); Initialization("init.missing_main", 321, 9, "missing");
        Initialization("init.unresolved_main", 321, 9, "unresolved"); Initialization("init.empty_main", 321, 9, "empty");
        GhostAndReset(); ResetTemps(false); ResetTemps(true);
        CheckTemps(false, false); CheckTemps(true, false); CheckTemps(true, true);
        SortedAndNotify(); SchedulerEdges();
    }
}
